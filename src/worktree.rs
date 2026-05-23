use crate::clipboard::ClipboardProvider;
use crate::gitexec::{Git, RepoContext, absolute_path, is_git_exit, resolve, same_path};
use crate::output;
use crate::paths::default_path;
use crate::{AppResult, Error};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Default, Clone)]
pub struct Options {
    pub branch: String,
    pub path: String,
    pub base: String,
    pub from_current: bool,
    pub delete_branch: bool,
    pub no_clipboard: bool,
}

pub struct Session<'a> {
    pub cwd: PathBuf,
    pub out: &'a mut dyn Write,
    pub clipboard: &'a mut dyn ClipboardProvider,
    pub git: Git,
}

impl<'a> Session<'a> {
    pub fn new(
        cwd: PathBuf,
        out: &'a mut dyn Write,
        clipboard: &'a mut dyn ClipboardProvider,
    ) -> Session<'a> {
        Session {
            cwd,
            out,
            clipboard,
            git: Git,
        }
    }
}

pub fn create(session: &mut Session<'_>, opts: Options) -> AppResult<()> {
    let repo = repo(session)?;
    if opts.branch.is_empty() {
        return Err(Error::message("branch is required"));
    }

    let path = create_target_path(&repo, &opts.branch, &opts.path)?;
    let base = prepare_create_base(session, &repo, &opts)?;
    let args = vec![
        "worktree".to_string(),
        "add".to_string(),
        "-b".to_string(),
        opts.branch.clone(),
        path.display().to_string(),
        base,
    ];
    output::git(session.out, &repo.main_root, &args)?;
    session
        .git
        .run(&repo.main_root, args.iter().map(String::as_str))?;
    maybe_run_pnpm_install(
        session.out,
        &path,
        &format!("worktree created at {}", path.display()),
    )?;
    finish(
        session,
        opts.no_clipboard,
        path.display().to_string(),
        format!("created worktree at {}", path.display()),
    )
}

pub fn checkout(session: &mut Session<'_>, opts: Options) -> AppResult<()> {
    let repo = repo(session)?;
    if opts.branch.is_empty() {
        return Err(Error::message("branch is required"));
    }

    let path = create_target_path(&repo, &opts.branch, &opts.path)?;
    let args = vec![
        "worktree".to_string(),
        "add".to_string(),
        path.display().to_string(),
        opts.branch.clone(),
    ];
    output::git(session.out, &repo.main_root, &args)?;
    session
        .git
        .run(&repo.main_root, args.iter().map(String::as_str))?;
    maybe_run_pnpm_install(
        session.out,
        &path,
        &format!("worktree created at {}", path.display()),
    )?;
    finish(
        session,
        opts.no_clipboard,
        path.display().to_string(),
        format!("created worktree at {}", path.display()),
    )
}

pub fn remove(session: &mut Session<'_>, opts: Options) -> AppResult<()> {
    let repo = repo(session)?;
    let target = if opts.path.is_empty() {
        if repo.current_is_main {
            return Err(Error::message(
                "path is required when removing from the main worktree",
            ));
        }
        repo.current_root.clone()
    } else {
        absolute_path(Path::new(&opts.path))
    };

    let worktree = repo.worktree_by_path(&target).ok_or_else(|| {
        Error::message(format!(
            "target is not a linked worktree: {}",
            target.display()
        ))
    })?;
    if same_path(&target, &repo.main_root) {
        return Err(Error::message(format!(
            "target is not a linked worktree: {}",
            target.display()
        )));
    }
    require_clean(session, &target)?;

    let remove_args = vec![
        "worktree".to_string(),
        "remove".to_string(),
        target.display().to_string(),
    ];
    output::git(session.out, &repo.main_root, &remove_args)?;
    session
        .git
        .run(&repo.main_root, remove_args.iter().map(String::as_str))?;

    let mut payload = target.display().to_string();
    if opts.delete_branch {
        if worktree.branch.is_empty() {
            return Err(Error::message(
                "cannot delete branch for detached linked worktree",
            ));
        }
        let branch_args = vec![
            "branch".to_string(),
            "-d".to_string(),
            worktree.branch.clone(),
        ];
        output::git(session.out, &repo.main_root, &branch_args)?;
        if let Err(error) = session
            .git
            .run(&repo.main_root, branch_args.iter().map(String::as_str))
        {
            return Err(Error::message(format!(
                "worktree removed, but branch deletion failed; run git -C {} branch -d {} after resolving the issue: {}",
                repo.main_root.display(),
                worktree.branch,
                error
            )));
        }
        payload.push('\n');
        payload.push_str(&worktree.branch);
    }

    finish(
        session,
        opts.no_clipboard,
        payload,
        format!("removed worktree {}", target.display()),
    )
}

pub fn send_out(session: &mut Session<'_>, opts: Options) -> AppResult<()> {
    let repo = repo(session)?;
    if !repo.current_is_main {
        return Err(Error::message(
            "send-out must be run from the main worktree",
        ));
    }
    require_clean(session, &repo.main_root)?;

    let branch = session
        .git
        .run(&repo.main_root, ["branch", "--show-current"])?
        .stdout;
    if branch.trim().is_empty() {
        return Err(Error::message("send-out requires a named branch"));
    }

    let base = if opts.base.is_empty() {
        detect_main_branch(session, &repo, "")?
    } else {
        opts.base.clone()
    };
    if branch.trim() == base {
        return Err(Error::message(format!(
            "current branch {:?} is the base branch; no task branch to send out",
            branch.trim()
        )));
    }

    let path = create_target_path(&repo, branch.trim(), &opts.path)?;
    ensure_creatable_parent(&path)?;

    let switch_args = vec!["switch".to_string(), base.clone()];
    output::git(session.out, &repo.main_root, &switch_args)?;
    session
        .git
        .run(&repo.main_root, switch_args.iter().map(String::as_str))?;

    let add_args = vec![
        "worktree".to_string(),
        "add".to_string(),
        path.display().to_string(),
        branch.trim().to_string(),
    ];
    output::git(session.out, &repo.main_root, &add_args)?;
    if let Err(error) = session
        .git
        .run(&repo.main_root, add_args.iter().map(String::as_str))
    {
        return Err(Error::message(format!(
            "main worktree switched to {}, but linked worktree creation failed; recover with git -C {} switch {} after resolving the issue: {}",
            base,
            repo.main_root.display(),
            branch.trim(),
            error
        )));
    }

    maybe_run_pnpm_install(
        session.out,
        &path,
        &format!(
            "main worktree switched to {}, and worktree created at {}",
            base,
            path.display()
        ),
    )?;

    finish(
        session,
        opts.no_clipboard,
        path.display().to_string(),
        format!("sent {} out to {}", branch.trim(), path.display()),
    )
}

pub fn bring_in(session: &mut Session<'_>, opts: Options) -> AppResult<()> {
    let repo = repo(session)?;
    if !repo.current_is_main {
        return Err(Error::message(
            "bring-in must be run from the main worktree",
        ));
    }
    if opts.branch.is_empty() {
        return Err(Error::message("branch is required"));
    }

    let target = repo
        .worktrees
        .iter()
        .find(|worktree| {
            worktree.branch == opts.branch && !same_path(&worktree.path, &repo.main_root)
        })
        .map(|worktree| worktree.path.clone())
        .ok_or_else(|| {
            Error::message(format!(
                "branch is not checked out in a linked worktree: {}",
                opts.branch
            ))
        })?;

    require_clean(session, &repo.main_root)?;
    require_clean(session, &target)?;

    if let Err(error) = session
        .git
        .run(&repo.main_root, ["rev-parse", "--verify", &opts.branch])
    {
        return Err(Error::message(format!(
            "branch cannot be checked out in main worktree: {}",
            error
        )));
    }

    let remove_args = vec![
        "worktree".to_string(),
        "remove".to_string(),
        target.display().to_string(),
    ];
    output::git(session.out, &repo.main_root, &remove_args)?;
    session
        .git
        .run(&repo.main_root, remove_args.iter().map(String::as_str))?;

    let switch_args = vec!["switch".to_string(), opts.branch.clone()];
    output::git(session.out, &repo.main_root, &switch_args)?;
    if let Err(error) = session
        .git
        .run(&repo.main_root, switch_args.iter().map(String::as_str))
    {
        return Err(Error::message(format!(
            "worktree removed; failed to switch to {}: {}",
            opts.branch, error
        )));
    }

    finish(
        session,
        opts.no_clipboard,
        opts.branch.clone(),
        format!("brought {} into main worktree", opts.branch),
    )
}

fn repo(session: &Session<'_>) -> AppResult<RepoContext> {
    resolve(&session.git, &session.cwd)
}

fn create_target_path(repo: &RepoContext, branch: &str, explicit_path: &str) -> AppResult<PathBuf> {
    let path = if explicit_path.is_empty() {
        default_path(&repo.main_root, branch)
    } else {
        PathBuf::from(explicit_path)
    };
    let path = absolute_path(&path);
    if path.exists() {
        return Err(Error::message(format!(
            "target path already exists: {}",
            path.display()
        )));
    }
    Ok(path)
}

fn require_clean(session: &Session<'_>, dir: &Path) -> AppResult<()> {
    let status = session.git.run(
        dir,
        ["status", "--porcelain=v1", "--untracked-files=normal"],
    )?;
    if status.stdout.trim().is_empty() {
        Ok(())
    } else {
        Err(Error::message(format!(
            "worktree is dirty at {}:\n{}",
            dir.display(),
            status.stdout
        )))
    }
}

fn detect_main_branch(
    session: &Session<'_>,
    repo: &RepoContext,
    explicit: &str,
) -> AppResult<String> {
    if !explicit.is_empty() {
        return Ok(explicit.to_string());
    }

    match session.git.run(
        &repo.main_root,
        ["config", "--get", "worktree-kit.mainBranch"],
    ) {
        Ok(output) if !output.stdout.trim().is_empty() => {
            return Ok(output.stdout.trim().to_string());
        }
        Ok(_) => {}
        Err(error) if !is_git_exit(&error, 1) => return Err(error),
        Err(_) => {}
    }

    match session.git.run(
        &repo.main_root,
        [
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    ) {
        Ok(output) if output.stdout.starts_with("origin/") => {
            return Ok(output
                .stdout
                .trim_start_matches("origin/")
                .trim()
                .to_string());
        }
        Ok(_) => {}
        Err(error) if !is_git_exit(&error, 1) => return Err(error),
        Err(_) => {}
    }

    let mut found = Vec::new();
    for candidate in ["main", "master", "trunk", "develop"] {
        match session.git.run(
            &repo.main_root,
            [
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{candidate}"),
            ],
        ) {
            Ok(_) => found.push(candidate.to_string()),
            Err(error) if !is_git_exit(&error, 1) => return Err(error),
            Err(_) => {}
        }
    }

    if found.len() == 1 {
        Ok(found.remove(0))
    } else {
        Err(Error::message(
            "cannot determine main branch; pass --base or run git config worktree-kit.mainBranch <branch>",
        ))
    }
}

fn prepare_base(
    session: &mut Session<'_>,
    repo: &RepoContext,
    explicit: &str,
) -> AppResult<String> {
    if !explicit.is_empty() {
        return Ok(explicit.to_string());
    }
    let base = detect_main_branch(session, repo, "")?;

    let fetch_args = vec!["fetch".to_string(), "origin".to_string(), base.clone()];
    output::git(session.out, &repo.main_root, &fetch_args)?;
    session
        .git
        .run(&repo.main_root, fetch_args.iter().map(String::as_str))?;

    let current = session
        .git
        .run(&repo.main_root, ["branch", "--show-current"])?
        .stdout;
    if current.trim() == base {
        let merge_args = vec![
            "merge".to_string(),
            "--ff-only".to_string(),
            format!("origin/{base}"),
        ];
        output::git(session.out, &repo.main_root, &merge_args)?;
        session
            .git
            .run(&repo.main_root, merge_args.iter().map(String::as_str))?;
        return Ok(base);
    }

    match session.git.run(
        &repo.main_root,
        [
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{base}"),
        ],
    ) {
        Ok(_) => {
            let merge_base_args = vec![
                "merge-base".to_string(),
                "--is-ancestor".to_string(),
                base.clone(),
                format!("origin/{base}"),
            ];
            output::git(session.out, &repo.main_root, &merge_base_args)?;
            if session
                .git
                .run(&repo.main_root, merge_base_args.iter().map(String::as_str))
                .is_err()
            {
                return Err(Error::message(format!(
                    "local {} is not an ancestor of origin/{}; refusing to move it without a fast-forward",
                    base, base
                )));
            }
        }
        Err(error) if !is_git_exit(&error, 1) => return Err(error),
        Err(_) => {}
    }

    let force_args = vec![
        "branch".to_string(),
        "-f".to_string(),
        base.clone(),
        format!("origin/{base}"),
    ];
    output::git(session.out, &repo.main_root, &force_args)?;
    if let Err(error) = session
        .git
        .run(&repo.main_root, force_args.iter().map(String::as_str))
    {
        let message = error.to_string();
        if message.contains("checked out") || message.contains("cannot force update") {
            output::warn(
                session.out,
                &format!("{base} is checked out; using origin/{base} as base"),
            )?;
            return Ok(format!("origin/{base}"));
        }
        return Err(error);
    }

    Ok(base)
}

fn prepare_create_base(
    session: &mut Session<'_>,
    repo: &RepoContext,
    opts: &Options,
) -> AppResult<String> {
    if opts.from_current {
        if !opts.base.is_empty() {
            return Err(Error::message(
                "--base and --from-current cannot be used together",
            ));
        }
        let current = session
            .git
            .run(&repo.current_root, ["branch", "--show-current"])?
            .stdout;
        let current = current.trim();
        if current.is_empty() {
            return Err(Error::message(
                "--from-current requires the current worktree to be on a named branch",
            ));
        }
        return Ok(current.to_string());
    }
    prepare_base(session, repo, &opts.base)
}

fn maybe_run_pnpm_install(
    out: &mut dyn Write,
    worktree_path: &Path,
    created_state: &str,
) -> AppResult<()> {
    if !is_pnpm_project(worktree_path) {
        return Ok(());
    }

    let args = vec!["install".to_string()];
    output::command(out, worktree_path, "pnpm", &args)?;
    let output = Command::new("pnpm")
        .current_dir(worktree_path)
        .args(args.iter().map(String::as_str))
        .output();

    match output {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let details = if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                format!("exit code {:?}", output.status.code())
            };
            Err(Error::message(format!(
                "{}, but pnpm install failed; run 'pnpm install' in {} after resolving the issue: {}",
                created_state,
                worktree_path.display(),
                details
            )))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(Error::message(format!(
            "{}, but pnpm project detected and pnpm was not found on PATH",
            created_state
        ))),
        Err(error) => Err(Error::message(format!(
            "{}, but pnpm install failed; run 'pnpm install' in {} after resolving the issue: {}",
            created_state,
            worktree_path.display(),
            error
        ))),
    }
}

fn is_pnpm_project(worktree_path: &Path) -> bool {
    worktree_path.join("pnpm-lock.yaml").is_file()
        || worktree_path.join("pnpm-workspace.yaml").is_file()
}

fn ensure_creatable_parent(path: &Path) -> AppResult<()> {
    let parent = path.parent().ok_or_else(|| {
        Error::message(format!("target parent is unavailable: {}", path.display()))
    })?;
    let metadata = fs::metadata(parent).map_err(|error| {
        Error::message(format!(
            "target parent is unavailable: {}: {}",
            parent.display(),
            error
        ))
    })?;
    if !metadata.is_dir() {
        return Err(Error::message(format!(
            "target parent is not a directory: {}",
            parent.display()
        )));
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let probe_path = parent.join(format!(".wtk-write-check-{}-{}", std::process::id(), nonce));
    let file = File::create(&probe_path).map_err(|error| {
        Error::message(format!(
            "target parent is not writable: {}: {}",
            parent.display(),
            error
        ))
    })?;
    drop(file);
    fs::remove_file(&probe_path)?;
    Ok(())
}

fn finish(
    session: &mut Session<'_>,
    no_clipboard: bool,
    payload: String,
    message: String,
) -> AppResult<()> {
    output::success(session.out, &message)?;
    if no_clipboard {
        return Ok(());
    }
    session.clipboard.write_text(&payload).map_err(|error| {
        Error::message(format!(
            "operation succeeded, but clipboard copy failed: {error}"
        ))
    })?;
    output::info(session.out, &format!("copied to clipboard: {payload}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::finish;
    use crate::clipboard::ClipboardProvider;
    use crate::{AppResult, Error};
    use std::io;
    use std::path::PathBuf;

    struct FailingClipboard;

    impl ClipboardProvider for FailingClipboard {
        fn write_text(&mut self, _value: &str) -> AppResult<()> {
            Err(Error::message("clipboard unavailable"))
        }
    }

    #[test]
    fn finish_reports_clipboard_partial_failure() {
        let mut out = io::sink();
        let mut clipboard = FailingClipboard;
        let mut session = super::Session::new(PathBuf::from("."), &mut out, &mut clipboard);
        let error = finish(
            &mut session,
            false,
            "payload".to_string(),
            "done".to_string(),
        )
        .expect_err("clipboard failure should bubble up");
        assert!(
            error
                .to_string()
                .contains("operation succeeded, but clipboard copy failed")
        );
    }
}
