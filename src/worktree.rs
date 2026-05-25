use crate::clipboard::ClipboardProvider;
use crate::gitexec::{Git, RepoContext, absolute_path, is_git_exit, resolve, same_path};
use crate::output;
use crate::paths::default_path;
use crate::{AppResult, Error};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{IsTerminal, Write};
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
#[cfg(unix)]
use std::os::unix::fs as unix_fs;
#[cfg(windows)]
use std::os::windows::fs as windows_fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

#[derive(Debug, Clone)]
struct IgnoredEnvFile {
    relative: PathBuf,
    kind: IgnoredEnvFileKind,
}

#[derive(Debug, Clone)]
enum IgnoredEnvFileKind {
    File {
        contents: Vec<u8>,
        permissions: fs::Permissions,
    },
    Symlink {
        target: PathBuf,
    },
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
    let ignored_env_files = snapshot_ignored_env_files(session, &repo.main_root)?;
    let ignored_env_snapshot_root = write_ignored_env_snapshot(&ignored_env_files, &path)?;
    finish(
        session,
        opts.no_clipboard,
        path.display().to_string(),
        format!("created worktree at {}", path.display()),
    )?;
    start_async_init_worktree(session, &repo.main_root, &path, &ignored_env_snapshot_root)
}

pub fn checkout(session: &mut Session<'_>, opts: Options) -> AppResult<()> {
    let repo = repo(session)?;
    if opts.branch.is_empty() {
        return Err(Error::message("branch is required"));
    }

    let path = create_target_path(&repo, &opts.branch, &opts.path)?;
    let ignored_env_files = snapshot_ignored_env_files(session, &repo.main_root)?;
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
    print_copied_ignored_env_files(
        session,
        copy_ignored_env_files(&ignored_env_files, &path).map_err(|error| {
            Error::message(format!(
                "worktree created, but ignored .env copy failed: {error}"
            ))
        })?,
    )?;
    maybe_run_pnpm_install(session, &path, "worktree created")?;
    finish(
        session,
        opts.no_clipboard,
        path.display().to_string(),
        format!("created worktree at {}", path.display()),
    )
}

pub fn init_worktree(
    session: &mut Session<'_>,
    source_root: &Path,
    worktree_path: &Path,
    ignored_env_snapshot_root: Option<&Path>,
) -> AppResult<()> {
    let ignored_env_files = match ignored_env_snapshot_root {
        Some(snapshot_root) => snapshot_ignored_env_files_from_root(snapshot_root)?,
        None => snapshot_ignored_env_files(session, source_root)?,
    };
    print_copied_ignored_env_files(
        session,
        copy_ignored_env_files(&ignored_env_files, worktree_path)
            .map_err(|error| Error::message(format!("ignored .env copy failed: {error}")))?,
    )?;
    maybe_run_pnpm_install(session, worktree_path, "worktree initialized")
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

    let ignored_env_files = snapshot_ignored_env_files(session, &repo.main_root)?;

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
    print_copied_ignored_env_files(
        session,
        copy_ignored_env_files(&ignored_env_files, &path).map_err(|error| {
            Error::message(format!(
                "main worktree switched to {base} and linked worktree created, but ignored .env copy failed: {error}"
            ))
        })?,
    )?;
    maybe_run_pnpm_install(
        session,
        &path,
        &format!("main worktree switched to {base} and linked worktree created"),
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

fn copy_ignored_env_files(
    ignored: &[IgnoredEnvFile],
    new_worktree_path: &Path,
) -> AppResult<Vec<PathBuf>> {
    let mut copied = Vec::with_capacity(ignored.len());
    for file in ignored {
        let target = new_worktree_path.join(&file.relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                Error::message(format!(
                    "failed to create target parent {}: {}",
                    parent.display(),
                    error
                ))
            })?;
        }
        match &file.kind {
            IgnoredEnvFileKind::File {
                contents,
                permissions,
            } => {
                fs::write(&target, contents).map_err(|error| {
                    Error::message(format!(
                        "failed to copy {} to {}: {}",
                        file.relative.display(),
                        target.display(),
                        error
                    ))
                })?;
                fs::set_permissions(&target, permissions.clone()).map_err(|error| {
                    Error::message(format!(
                        "failed to set permissions on {}: {}",
                        target.display(),
                        error
                    ))
                })?;
            }
            IgnoredEnvFileKind::Symlink {
                target: symlink_target,
            } => {
                create_symlink(symlink_target, &target).map_err(|error| {
                    Error::message(format!(
                        "failed to copy symlink {} to {}: {}",
                        file.relative.display(),
                        target.display(),
                        error
                    ))
                })?;
            }
        }
        copied.push(file.relative.clone());
    }
    Ok(copied)
}

fn snapshot_ignored_env_files(
    session: &Session<'_>,
    main_root: &Path,
) -> AppResult<Vec<IgnoredEnvFile>> {
    let mut ignored = Vec::new();
    for relative in ignored_env_files(session, main_root)? {
        if let Some(snapshot) = snapshot_ignored_env_file(main_root, relative)? {
            ignored.push(snapshot);
        }
    }
    Ok(ignored)
}

fn snapshot_ignored_env_files_from_root(root: &Path) -> AppResult<Vec<IgnoredEnvFile>> {
    let mut ignored = Vec::new();
    collect_ignored_env_files_from_root(root, root, &mut ignored)?;
    ignored.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(ignored)
}

fn collect_ignored_env_files_from_root(
    root: &Path,
    current: &Path,
    ignored: &mut Vec<IgnoredEnvFile>,
) -> AppResult<()> {
    let mut entries = fs::read_dir(current)
        .map_err(|error| {
            Error::message(format!(
                "failed to read snapshot root {}: {error}",
                current.display()
            ))
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            Error::message(format!(
                "failed to read snapshot root {}: {error}",
                current.display()
            ))
        })?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            Error::message(format!(
                "failed to inspect snapshot entry {}: {error}",
                path.display()
            ))
        })?;
        if file_type.is_dir() {
            collect_ignored_env_files_from_root(root, &path, ignored)?;
            continue;
        }

        if path.file_name().is_none_or(|name| name != ".env") {
            continue;
        }

        let relative = path.strip_prefix(root).map_err(|error| {
            Error::message(format!(
                "failed to compute snapshot path relative to {} for {}: {error}",
                root.display(),
                path.display()
            ))
        })?;
        if let Some(snapshot) = snapshot_ignored_env_file(root, relative.to_path_buf())? {
            ignored.push(snapshot);
        }
    }

    Ok(())
}

fn snapshot_ignored_env_file(
    main_root: &Path,
    relative: PathBuf,
) -> AppResult<Option<IgnoredEnvFile>> {
    let source = main_root.join(&relative);
    let metadata = fs::symlink_metadata(&source).map_err(|error| {
        Error::message(format!(
            "failed to read source {}: {}",
            source.display(),
            error
        ))
    })?;
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(&source).map_err(|error| {
            Error::message(format!(
                "failed to read symlink target {}: {}",
                source.display(),
                error
            ))
        })?;
        return Ok(Some(IgnoredEnvFile {
            relative,
            kind: IgnoredEnvFileKind::Symlink { target },
        }));
    }
    if !metadata.file_type().is_file() {
        return Ok(None);
    }

    let contents = fs::read(&source).map_err(|error| {
        Error::message(format!(
            "failed to read source {}: {}",
            source.display(),
            error
        ))
    })?;
    Ok(Some(IgnoredEnvFile {
        relative,
        kind: IgnoredEnvFileKind::File {
            contents,
            permissions: metadata.permissions(),
        },
    }))
}

#[cfg(unix)]
fn create_symlink(target: &Path, path: &Path) -> std::io::Result<()> {
    unix_fs::symlink(target, path)
}

#[cfg(windows)]
fn create_symlink(target: &Path, path: &Path) -> std::io::Result<()> {
    windows_fs::symlink_file(target, path)
}

fn ignored_env_files(session: &Session<'_>, main_root: &Path) -> AppResult<Vec<PathBuf>> {
    let output = session.git.run_bytes(
        main_root,
        [
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--full-name",
            "-z",
            "--",
            ".env",
            ":(glob)**/.env",
        ],
    )?;
    let mut ignored: Vec<_> = output
        .stdout
        .split(|byte| *byte == b'\0')
        .filter(|path| !path.is_empty())
        .map(path_buf_from_git_bytes)
        .filter(|path| path.file_name().is_some_and(|name| name == ".env"))
        .collect();
    ignored.sort();
    Ok(ignored)
}

#[cfg(unix)]
fn path_buf_from_git_bytes(path: &[u8]) -> PathBuf {
    PathBuf::from(OsString::from_vec(path.to_vec()))
}

#[cfg(not(unix))]
fn path_buf_from_git_bytes(path: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(path).into_owned())
}

fn print_copied_ignored_env_files(
    session: &mut Session<'_>,
    copied: Vec<PathBuf>,
) -> AppResult<()> {
    for relative in copied {
        writeln!(session.out, "copied ignored .env: {}", relative.display())?;
    }
    Ok(())
}

fn start_async_init_worktree(
    session: &mut Session<'_>,
    source_root: &Path,
    worktree_path: &Path,
    ignored_env_snapshot_root: &Path,
) -> AppResult<()> {
    let exe = std::env::current_exe()
        .map_err(|error| Error::message(format!("worktree created, but failed to locate wtk executable for async initialization: {error}")))?;
    output::info(
        session.out,
        &format!(
            "initializing worktree asynchronously: wtk init-worktree {} {}",
            source_root.display(),
            worktree_path.display()
        ),
    )?;
    let (stdout, stderr, log_path) = async_init_stdio(worktree_path)?;
    if let Some(log_path) = log_path {
        output::info(
            session.out,
            &format!(
                "async initialization output will be written to {}",
                log_path.display()
            ),
        )?;
    }
    Command::new(exe)
        .arg("init-worktree")
        .arg(source_root)
        .arg(worktree_path)
        .arg("--snapshot-root")
        .arg(ignored_env_snapshot_root)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|error| {
            Error::message(format!(
                "worktree created, but failed to start async worktree initialization: {error}"
            ))
        })?;
    Ok(())
}

fn write_ignored_env_snapshot(
    ignored_env_files: &[IgnoredEnvFile],
    worktree_path: &Path,
) -> AppResult<PathBuf> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let worktree_name = worktree_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("worktree");
    let snapshot_root = std::env::temp_dir().join(format!(
        "wtk-init-worktree-snapshot-{}-{}-{}",
        std::process::id(),
        worktree_name,
        nonce
    ));
    fs::create_dir_all(&snapshot_root).map_err(|error| {
        Error::message(format!(
            "worktree created, but failed to create ignored .env snapshot {}: {error}",
            snapshot_root.display()
        ))
    })?;
    copy_ignored_env_files(ignored_env_files, &snapshot_root).map_err(|error| {
        Error::message(format!(
            "worktree created, but failed to snapshot ignored .env files in {}: {error}",
            snapshot_root.display()
        ))
    })?;
    Ok(snapshot_root)
}

fn async_init_stdio(worktree_path: &Path) -> AppResult<(Stdio, Stdio, Option<PathBuf>)> {
    if std::io::stdout().is_terminal() && std::io::stderr().is_terminal() {
        return Ok((Stdio::inherit(), Stdio::inherit(), None));
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let worktree_name = worktree_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("worktree");
    let log_path = std::env::temp_dir().join(format!(
        "wtk-init-worktree-{}-{}-{}.log",
        std::process::id(),
        worktree_name,
        nonce
    ));
    let stdout = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&log_path)
        .map_err(|error| {
            Error::message(format!(
                "worktree created, but failed to open async initialization log {}: {error}",
                log_path.display()
            ))
        })?;
    let stderr = stdout.try_clone().map_err(|error| {
        Error::message(format!(
            "worktree created, but failed to duplicate async initialization log {}: {error}",
            log_path.display()
        ))
    })?;
    Ok((Stdio::from(stdout), Stdio::from(stderr), Some(log_path)))
}

fn maybe_run_pnpm_install(
    session: &mut Session<'_>,
    worktree_path: &Path,
    partial_success_prefix: &str,
) -> AppResult<()> {
    if !should_run_pnpm_install(worktree_path) {
        return Ok(());
    }

    output::info(
        session.out,
        &format!("running pnpm install in {}", worktree_path.display()),
    )?;
    let output = Command::new("pnpm")
        .arg("install")
        .current_dir(worktree_path)
        .output()
        .map_err(|error| {
            Error::message(format!(
                "{partial_success_prefix}, but pnpm install failed: {error}"
            ))
        })?;
    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let details = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit status {}", output.status)
    };
    Err(Error::message(format!(
        "{partial_success_prefix}, but pnpm install failed: {details}"
    )))
}

fn should_run_pnpm_install(worktree_path: &Path) -> bool {
    worktree_path.join("pnpm-lock.yaml").is_file()
        || worktree_path.join("pnpm-workspace.yaml").is_file()
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
    use super::{finish, should_run_pnpm_install};
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

    #[test]
    fn should_run_pnpm_install_only_for_pnpm_worktrees() {
        let root = std::env::temp_dir().join(format!(
            "wtk-pnpm-detect-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let non_node = root.join("non-node");
        std::fs::create_dir_all(&non_node).unwrap();
        assert!(!should_run_pnpm_install(&non_node));

        let npm_only = root.join("npm-only");
        std::fs::create_dir_all(&npm_only).unwrap();
        std::fs::write(npm_only.join("package.json"), "{}\n").unwrap();
        assert!(!should_run_pnpm_install(&npm_only));

        let pnpm_lock = root.join("pnpm-lock");
        std::fs::create_dir_all(&pnpm_lock).unwrap();
        std::fs::write(pnpm_lock.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n").unwrap();
        assert!(should_run_pnpm_install(&pnpm_lock));

        let pnpm_workspace = root.join("pnpm-workspace");
        std::fs::create_dir_all(&pnpm_workspace).unwrap();
        std::fs::write(
            pnpm_workspace.join("pnpm-workspace.yaml"),
            "packages:\n  - .\n",
        )
        .unwrap();
        assert!(should_run_pnpm_install(&pnpm_workspace));

        std::fs::remove_dir_all(&root).unwrap();
    }
}
