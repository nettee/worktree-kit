use crate::VERSION;
use crate::clipboard::{DisabledClipboard, SystemClipboard};
use crate::gitexec::Git;
use crate::worktree::{self, Options, Session};
use crate::{AppResult, Error};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

const TOP_LEVEL_COMMANDS: &[&str] = &[
    "create",
    "checkout",
    "init-worktree",
    "remove",
    "send-out",
    "bring-in",
    "completion",
    "help",
];
const SHELLS: &[&str] = &["bash", "zsh", "fish", "powershell"];

enum Parsed {
    Create(Options),
    Checkout(Options),
    InitWorktree {
        source_root: String,
        worktree_path: String,
        ignored_env_snapshot_root: Option<String>,
    },
    Remove(Options),
    SendOut(Options),
    BringIn(Options),
    Completion(String),
    HiddenComplete(Vec<String>),
    Version,
    Help,
    HelpText(&'static str),
}

#[derive(Debug)]
struct UsageError {
    reason: String,
    usage: String,
}

impl UsageError {
    fn new(reason: impl Into<String>, usage: impl Into<String>) -> UsageError {
        UsageError {
            reason: reason.into(),
            usage: usage.into(),
        }
    }
}

pub fn run<I>(args: I, stdout: &mut dyn Write, stderr: &mut dyn Write) -> Result<(), u8>
where
    I: IntoIterator<Item = OsString>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let parsed = match parse_args(&args) {
        Ok(parsed) => parsed,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error.reason);
            let _ = writeln!(stderr);
            let _ = write!(stderr, "{}", error.usage);
            return Err(1);
        }
    };

    match parsed {
        Parsed::Version => {
            let _ = writeln!(stdout, "wtk {}", VERSION);
            Ok(())
        }
        Parsed::Help => {
            let _ = write!(stdout, "{}", root_help());
            Ok(())
        }
        Parsed::HelpText(text) => {
            let _ = write!(stdout, "{text}");
            Ok(())
        }
        Parsed::Completion(shell) => {
            if let Err(error) = print_completion(stdout, &shell) {
                let _ = writeln!(stderr, "{error}");
                return Err(1);
            }
            Ok(())
        }
        Parsed::HiddenComplete(words) => {
            let cwd = match std::env::current_dir() {
                Ok(cwd) => cwd,
                Err(error) => {
                    let _ = writeln!(stderr, "{error}");
                    return Err(1);
                }
            };
            if let Err(error) = print_dynamic_completion(stdout, cwd, &words) {
                let _ = writeln!(stderr, "{error}");
                return Err(1);
            }
            Ok(())
        }
        Parsed::Create(options) => {
            execute_worktree(stdout, stderr, options.no_clipboard, |session| {
                worktree::create(session, options)
            })
        }
        Parsed::Checkout(options) => {
            execute_worktree(stdout, stderr, options.no_clipboard, |session| {
                worktree::checkout(session, options)
            })
        }
        Parsed::InitWorktree {
            source_root,
            worktree_path,
            ignored_env_snapshot_root,
        } => execute_worktree(stdout, stderr, true, |session| {
            worktree::init_worktree(
                session,
                Path::new(&source_root),
                Path::new(&worktree_path),
                ignored_env_snapshot_root.as_deref().map(Path::new),
            )
        }),
        Parsed::Remove(options) => {
            execute_worktree(stdout, stderr, options.no_clipboard, |session| {
                worktree::remove(session, options)
            })
        }
        Parsed::SendOut(options) => {
            execute_worktree(stdout, stderr, options.no_clipboard, |session| {
                worktree::send_out(session, options)
            })
        }
        Parsed::BringIn(options) => {
            execute_worktree(stdout, stderr, options.no_clipboard, |session| {
                worktree::bring_in(session, options)
            })
        }
    }
}

fn execute_worktree<F>(
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    no_clipboard: bool,
    callback: F,
) -> Result<(), u8>
where
    F: FnOnce(&mut Session<'_>) -> AppResult<()>,
{
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            return Err(1);
        }
    };

    let result = if no_clipboard {
        let mut clipboard = DisabledClipboard;
        let mut session = Session::new(cwd, stdout, &mut clipboard);
        callback(&mut session)
    } else {
        let mut clipboard = SystemClipboard;
        let mut session = Session::new(cwd, stdout, &mut clipboard);
        callback(&mut session)
    };

    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            Err(1)
        }
    }
}

fn parse_args(args: &[String]) -> Result<Parsed, UsageError> {
    if args.len() <= 1 {
        return Ok(Parsed::Help);
    }

    let rest = &args[1..];
    match rest[0].as_str() {
        "--version" | "-V" => Ok(Parsed::Version),
        "--help" | "-h" => Ok(Parsed::Help),
        "help" => Ok(Parsed::Help),
        "create" => parse_create(rest),
        "checkout" => parse_checkout(rest),
        "init-worktree" => parse_init_worktree(rest),
        "remove" => parse_remove(rest),
        "send-out" => parse_send_out(rest),
        "bring-in" => parse_bring_in(rest),
        "completion" => parse_completion(rest),
        "__complete" => Ok(Parsed::HiddenComplete(rest[1..].to_vec())),
        other => Err(UsageError::new(
            format!("unknown command: {other}"),
            root_help(),
        )),
    }
}

fn parse_create(args: &[String]) -> Result<Parsed, UsageError> {
    let usage = command_help("create");
    let mut options = Options::default();
    let mut positionals = Vec::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            flag if flag == "--path" || flag.starts_with("--path=") => {
                if let Some(value) = inline_flag_value(flag, "--path") {
                    options.path = value;
                } else {
                    i += 1;
                    options.path = require_flag_value(args, i, "--path", usage)?;
                }
            }
            flag if flag == "--base" || flag.starts_with("--base=") => {
                if let Some(value) = inline_flag_value(flag, "--base") {
                    options.base = value;
                } else {
                    i += 1;
                    options.base = require_flag_value(args, i, "--base", usage)?;
                }
            }
            "--from-current" | "-C" => options.from_current = true,
            "--no-clipboard" => options.no_clipboard = true,
            "--help" | "-h" => return Ok(Parsed::HelpText(usage)),
            flag if flag.starts_with('-') => {
                return Err(UsageError::new(format!("unknown flag: {flag}"), usage));
            }
            value => positionals.push(value.to_string()),
        }
        i += 1;
    }
    if positionals.is_empty() {
        return Err(UsageError::new("missing required argument: branch", usage));
    }
    if positionals.len() > 1 {
        return Err(UsageError::new(
            "too many arguments: expected 1 branch",
            usage,
        ));
    }
    options.branch = positionals.remove(0);
    Ok(Parsed::Create(options))
}

fn parse_checkout(args: &[String]) -> Result<Parsed, UsageError> {
    let usage = command_help("checkout");
    let mut options = Options::default();
    let mut positionals = Vec::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            flag if flag == "--path" || flag.starts_with("--path=") => {
                if let Some(value) = inline_flag_value(flag, "--path") {
                    options.path = value;
                } else {
                    i += 1;
                    options.path = require_flag_value(args, i, "--path", usage)?;
                }
            }
            "--no-clipboard" => options.no_clipboard = true,
            "--help" | "-h" => return Ok(Parsed::HelpText(usage)),
            flag if flag.starts_with('-') => {
                return Err(UsageError::new(format!("unknown flag: {flag}"), usage));
            }
            value => positionals.push(value.to_string()),
        }
        i += 1;
    }
    if positionals.is_empty() {
        return Err(UsageError::new("missing required argument: branch", usage));
    }
    if positionals.len() > 1 {
        return Err(UsageError::new(
            "too many arguments: expected 1 branch",
            usage,
        ));
    }
    options.branch = positionals.remove(0);
    Ok(Parsed::Checkout(options))
}

fn parse_init_worktree(args: &[String]) -> Result<Parsed, UsageError> {
    let usage = command_help("init-worktree");
    if args.len() == 2 && matches!(args[1].as_str(), "--help" | "-h") {
        return Ok(Parsed::HelpText(usage));
    }
    let mut positionals = Vec::new();
    let mut ignored_env_snapshot_root = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            flag if flag == "--snapshot-root" || flag.starts_with("--snapshot-root=") => {
                if let Some(value) = inline_flag_value(flag, "--snapshot-root") {
                    ignored_env_snapshot_root = Some(value);
                } else {
                    i += 1;
                    ignored_env_snapshot_root =
                        Some(require_flag_value(args, i, "--snapshot-root", usage)?);
                }
            }
            "--help" | "-h" => return Ok(Parsed::HelpText(usage)),
            flag if flag.starts_with('-') => {
                return Err(UsageError::new(format!("unknown flag: {flag}"), usage));
            }
            value => positionals.push(value.to_string()),
        }
        i += 1;
    }
    if positionals.len() < 2 {
        return Err(UsageError::new(
            "missing required arguments: source-root and worktree-path",
            usage,
        ));
    }
    if positionals.len() > 2 {
        return Err(UsageError::new(
            "too many arguments: expected source-root and worktree-path",
            usage,
        ));
    }
    Ok(Parsed::InitWorktree {
        source_root: positionals[0].clone(),
        worktree_path: positionals[1].clone(),
        ignored_env_snapshot_root,
    })
}

fn parse_remove(args: &[String]) -> Result<Parsed, UsageError> {
    let usage = command_help("remove");
    let mut options = Options::default();
    let mut positionals = Vec::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--delete-branch" => options.delete_branch = true,
            "--no-clipboard" => options.no_clipboard = true,
            "--help" | "-h" => return Ok(Parsed::HelpText(usage)),
            flag if flag.starts_with('-') => {
                return Err(UsageError::new(format!("unknown flag: {flag}"), usage));
            }
            value => positionals.push(value.to_string()),
        }
        i += 1;
    }
    if positionals.len() > 1 {
        return Err(UsageError::new(
            "too many arguments: expected at most 1 path",
            usage,
        ));
    }
    options.path = positionals.into_iter().next().unwrap_or_default();
    Ok(Parsed::Remove(options))
}

fn parse_send_out(args: &[String]) -> Result<Parsed, UsageError> {
    let usage = command_help("send-out");
    let mut options = Options::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            flag if flag == "--path" || flag.starts_with("--path=") => {
                if let Some(value) = inline_flag_value(flag, "--path") {
                    options.path = value;
                } else {
                    i += 1;
                    options.path = require_flag_value(args, i, "--path", usage)?;
                }
            }
            flag if flag == "--base" || flag.starts_with("--base=") => {
                if let Some(value) = inline_flag_value(flag, "--base") {
                    options.base = value;
                } else {
                    i += 1;
                    options.base = require_flag_value(args, i, "--base", usage)?;
                }
            }
            "--no-clipboard" => options.no_clipboard = true,
            "--help" | "-h" => return Ok(Parsed::HelpText(usage)),
            flag if flag.starts_with('-') => {
                return Err(UsageError::new(format!("unknown flag: {flag}"), usage));
            }
            value => {
                return Err(UsageError::new(
                    format!("unexpected argument: {value}"),
                    usage,
                ));
            }
        }
        i += 1;
    }
    Ok(Parsed::SendOut(options))
}

fn parse_bring_in(args: &[String]) -> Result<Parsed, UsageError> {
    let usage = command_help("bring-in");
    let mut options = Options::default();
    let mut positionals = Vec::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--no-clipboard" => options.no_clipboard = true,
            "--help" | "-h" => return Ok(Parsed::HelpText(usage)),
            flag if flag.starts_with('-') => {
                return Err(UsageError::new(format!("unknown flag: {flag}"), usage));
            }
            value => positionals.push(value.to_string()),
        }
        i += 1;
    }
    if positionals.is_empty() {
        return Err(UsageError::new("missing required argument: branch", usage));
    }
    if positionals.len() > 1 {
        return Err(UsageError::new(
            "too many arguments: expected 1 branch",
            usage,
        ));
    }
    options.branch = positionals.remove(0);
    Ok(Parsed::BringIn(options))
}

fn parse_completion(args: &[String]) -> Result<Parsed, UsageError> {
    let usage = command_help("completion");
    if args.len() == 2 && matches!(args[1].as_str(), "--help" | "-h") {
        return Ok(Parsed::HelpText(usage));
    }
    if args.len() == 1 {
        return Err(UsageError::new("missing required argument: shell", usage));
    }
    if args.len() > 2 {
        return Err(UsageError::new(
            "too many arguments: expected 1 shell",
            usage,
        ));
    }
    let shell = args[1].to_string();
    if !SHELLS.contains(&shell.as_str()) {
        return Err(UsageError::new(
            format!("unsupported shell: {shell}"),
            usage,
        ));
    }
    Ok(Parsed::Completion(shell))
}

fn require_flag_value(
    args: &[String],
    index: usize,
    flag: &str,
    usage: &'static str,
) -> Result<String, UsageError> {
    args.get(index)
        .filter(|value| !value.starts_with('-'))
        .cloned()
        .ok_or_else(|| UsageError::new(format!("missing value for {flag}"), usage))
}

fn inline_flag_value(flag: &str, name: &str) -> Option<String> {
    flag.strip_prefix(name)
        .and_then(|suffix| suffix.strip_prefix('='))
        .map(str::to_string)
}

fn root_help() -> &'static str {
    concat!(
        "Friendly Git worktree workflows\n\n",
        "Usage: wtk <command> [flags]\n\n",
        "Commands:\n",
        "  create      Create a new branch in a linked worktree\n",
        "  checkout    Check out an existing branch or ref in a linked worktree\n",
        "  remove      Remove a linked worktree\n",
        "  send-out    Move the current main-worktree branch to a linked worktree\n",
        "  bring-in    Move a linked worktree branch back into the main worktree\n",
        "  completion  Generate shell completion script\n",
        "  help        Show help\n\n",
        "Flags:\n",
        "  -h, --help       Show help\n",
        "  -V, --version    Show version\n",
    )
}

fn command_help(command: &str) -> &'static str {
    match command {
        "create" => concat!(
            "Usage: wtk create <branch> [flags]\n\n",
            "Flags:\n",
            "      --path <path>\n",
            "      --base <branch>\n",
            "  -C, --from-current\n",
            "      --no-clipboard\n",
        ),
        "checkout" => concat!(
            "Usage: wtk checkout <branch> [flags]\n\n",
            "Flags:\n",
            "      --path <path>\n",
            "      --no-clipboard\n",
        ),
        "init-worktree" => concat!(
            "Usage: wtk init-worktree <source-root> <worktree-path> [flags]\n\n",
            "Advanced command:\n",
            "  Copy ignored .env files from source-root into worktree-path and run pnpm install when needed.\n\n",
            "Flags:\n",
            "  -h, --help\n",
        ),
        "remove" => concat!(
            "Usage: wtk remove [path] [flags]\n\n",
            "Flags:\n",
            "      --delete-branch\n",
            "      --no-clipboard\n",
        ),
        "send-out" => concat!(
            "Usage: wtk send-out [flags]\n\n",
            "Flags:\n",
            "      --path <path>\n",
            "      --base <branch>\n",
            "      --no-clipboard\n",
        ),
        "bring-in" => concat!(
            "Usage: wtk bring-in <branch> [flags]\n\n",
            "Flags:\n",
            "      --no-clipboard\n",
        ),
        "completion" => concat!(
            "Usage: wtk completion <bash|zsh|fish|powershell> [flags]\n\n",
            "Flags:\n",
            "  -h, --help\n",
        ),
        _ => root_help(),
    }
}

fn print_completion(stdout: &mut dyn Write, shell: &str) -> AppResult<()> {
    match shell {
        "bash" => write!(stdout, "{}", bash_completion_script())?,
        "zsh" => write!(stdout, "{}", zsh_completion_script())?,
        "fish" => write!(stdout, "{}", fish_completion_script())?,
        "powershell" => write!(stdout, "{}", powershell_completion_script())?,
        other => return Err(Error::message(format!("unsupported shell: {other}"))),
    }
    Ok(())
}

fn print_dynamic_completion(
    stdout: &mut dyn Write,
    cwd: PathBuf,
    args: &[String],
) -> AppResult<()> {
    let candidates = dynamic_candidates(cwd, args);
    for candidate in candidates {
        writeln!(stdout, "{candidate}")?;
    }
    writeln!(stdout, ":0")?;
    Ok(())
}

fn dynamic_candidates(cwd: PathBuf, args: &[String]) -> Vec<String> {
    let command = args.first().map(String::as_str).unwrap_or_default();
    let to_complete = args.last().map(String::as_str).unwrap_or_default();

    match command {
        "" => filter_prefix(TOP_LEVEL_COMMANDS, to_complete),
        "completion" => filter_prefix(SHELLS, to_complete),
        "remove" => git_lines(&cwd, ["worktree", "list", "--porcelain"])
            .map(|lines| {
                let mut paths = lines
                    .iter()
                    .filter_map(|line| line.strip_prefix("worktree ").map(ToOwned::to_owned))
                    .collect::<Vec<_>>();
                if !paths.is_empty() {
                    paths.remove(0);
                }
                filter_prefix_owned(paths, to_complete)
            })
            .unwrap_or_default(),
        "bring-in" => git_lines(&cwd, ["worktree", "list", "--porcelain"])
            .map(|lines| {
                let mut branches = lines
                    .iter()
                    .filter_map(|line| {
                        line.strip_prefix("branch refs/heads/")
                            .map(ToOwned::to_owned)
                    })
                    .collect::<Vec<_>>();
                if !branches.is_empty() {
                    branches.remove(0);
                }
                filter_prefix_owned(branches, to_complete)
            })
            .unwrap_or_default(),
        "create" | "checkout" => git_lines(&cwd, ["branch", "--format=%(refname:short)"])
            .map(|lines| filter_prefix_owned(lines, to_complete))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn git_lines<I, S>(cwd: &Path, args: I) -> Option<Vec<String>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let git = Git;
    let output = git.run(cwd, args).ok()?;
    if output.stdout.trim().is_empty() {
        return None;
    }
    Some(
        output
            .stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
    )
}

fn filter_prefix(candidates: &[&str], prefix: &str) -> Vec<String> {
    candidates
        .iter()
        .filter(|candidate| candidate.starts_with(prefix))
        .map(|candidate| (*candidate).to_string())
        .collect()
}

fn filter_prefix_owned(candidates: Vec<String>, prefix: &str) -> Vec<String> {
    candidates
        .into_iter()
        .filter(|candidate| candidate.starts_with(prefix))
        .collect()
}

fn bash_completion_script() -> &'static str {
    r#"_wtk() {
  local IFS=$'\n'
  local candidates
  candidates=$(wtk __complete "${COMP_WORDS[@]:1}")
  COMPREPLY=($(printf '%s\n' "$candidates" | grep -v '^:' | grep -v '^Completion ended'))
}
complete -F _wtk wtk
"#
}

fn zsh_completion_script() -> &'static str {
    r#"#compdef wtk
_wtk() {
  local -a candidates
  candidates=("${(@f)$(wtk __complete "${words[@]:2}")}")
  candidates=(${candidates:#:0})
  _describe 'values' candidates
}
compdef _wtk wtk
"#
}

fn fish_completion_script() -> &'static str {
    r#"complete -c wtk -f -a '(wtk __complete (commandline -opc | string sub -s 2) | string match -rv "^:")'
"#
}

fn powershell_completion_script() -> &'static str {
    r#"Register-ArgumentCompleter -Native -CommandName wtk -ScriptBlock {
  param($wordToComplete, $commandAst, $cursorPosition)
  $parts = $commandAst.CommandElements | Select-Object -Skip 1 | ForEach-Object { $_.Extent.Text }
  wtk __complete @parts | Where-Object { $_ -notmatch '^:' } | ForEach-Object {
    [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)
  }
}
"#
}

#[cfg(test)]
mod tests {
    use super::{Options, Parsed, TOP_LEVEL_COMMANDS, dynamic_candidates, parse_args};

    #[test]
    fn parses_version_flag() {
        let parsed = parse_args(&["wtk".to_string(), "--version".to_string()]).unwrap();
        assert!(matches!(parsed, super::Parsed::Version));
    }

    #[test]
    fn parses_help_subcommand() {
        let parsed = parse_args(&["wtk".to_string(), "help".to_string()]).unwrap();
        assert!(matches!(parsed, Parsed::Help));
    }

    #[test]
    fn complete_top_level_commands() {
        let candidates =
            dynamic_candidates(std::env::current_dir().unwrap(), &Vec::<String>::new());
        for command in TOP_LEVEL_COMMANDS {
            assert!(candidates.contains(&command.to_string()));
        }
    }

    #[test]
    fn parses_inline_flag_values_for_create() {
        let parsed = parse_args(&[
            "wtk".to_string(),
            "create".to_string(),
            "--path=../feature".to_string(),
            "--base=main".to_string(),
            "topic".to_string(),
        ])
        .unwrap();

        assert!(matches!(
            parsed,
            Parsed::Create(Options {
                branch,
                path,
                base,
                ..
            }) if branch == "topic" && path == "../feature" && base == "main"
        ));
    }

    #[test]
    fn parses_inline_flag_values_for_checkout_and_send_out() {
        let parsed = parse_args(&[
            "wtk".to_string(),
            "checkout".to_string(),
            "--path=../feature".to_string(),
            "topic".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            parsed,
            Parsed::Checkout(Options {
                branch,
                path,
                ..
            }) if branch == "topic" && path == "../feature"
        ));

        let parsed = parse_args(&[
            "wtk".to_string(),
            "send-out".to_string(),
            "--path=../feature".to_string(),
            "--base=main".to_string(),
        ])
        .unwrap();
        assert!(matches!(
            parsed,
            Parsed::SendOut(Options { path, base, .. }) if path == "../feature" && base == "main"
        ));
    }
}
