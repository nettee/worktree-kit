use crate::auxiliary::{self, AuxiliaryRefStatus, AuxiliaryWorktreeState, WorktreeEntry};
use crate::clipboard::ClipboardProvider;
use crate::gitexec::{Git, RepoContext, absolute_path, is_git_exit, resolve, same_path};
use crate::list::{self, AuxiliaryRefDetail, AuxiliaryRefSummary, ListOptions};
use crate::output;
use crate::paths::default_path;
use crate::{AppResult, Error};
use serde::Serialize;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
#[cfg(unix)]
use std::os::unix::fs as unix_fs;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(windows)]
use std::os::windows::fs as windows_fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const IGNORED_ENV_SNAPSHOT_PREFIX: &str = "wtk-init-worktree-snapshot-";
const IGNORED_ENV_SNAPSHOT_MARKER: &str = ".wtk-ignored-env-snapshot";
const IGNORED_SEND_OUT_ACTIVE_SPEC_PATH: &str = "specs/change/active";

#[derive(Debug, Default, Clone)]
pub struct Options {
    pub branch: String,
    pub path: String,
    pub base: String,
    pub from_current: bool,
    pub delete_branch: bool,
    pub no_clipboard: bool,
    pub auxiliary_groups: Vec<String>,
}

pub struct Session<'a> {
    pub cwd: PathBuf,
    pub out: &'a mut dyn Write,
    pub clipboard: &'a mut dyn ClipboardProvider,
    pub git: Git,
    pub style_enabled: bool,
}

impl<'a> Session<'a> {
    pub fn new(
        cwd: PathBuf,
        out: &'a mut dyn Write,
        clipboard: &'a mut dyn ClipboardProvider,
        style_enabled: bool,
    ) -> Session<'a> {
        Session {
            cwd,
            out,
            clipboard,
            git: Git,
            style_enabled,
        }
    }
}

#[derive(Serialize)]
struct StatusOutput {
    cwd: PathBuf,
    current_root: PathBuf,
    main_root: PathBuf,
    git_common_dir: PathBuf,
    current_is_main: bool,
}

#[derive(Serialize)]
struct AuxiliaryStatusOutput {
    mode: &'static str,
    primary_worktree: PathBuf,
    primary_main_worktree: PathBuf,
    branch: String,
    current_is_main: bool,
    state: PathBuf,
    auxiliaries: BTreeMap<String, AuxiliaryStatusEntry>,
}

#[derive(Serialize)]
struct AuxiliaryStatusEntry {
    repository: PathBuf,
    worktree: PathBuf,
    ref_path: PathBuf,
    current_target: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct SnapshotFile {
    relative: PathBuf,
    kind: SnapshotFileKind,
}

#[derive(Debug, Clone)]
pub(crate) enum SnapshotFileKind {
    File {
        contents: Vec<u8>,
        permissions: fs::Permissions,
    },
    Symlink {
        target: PathBuf,
    },
}

pub struct SendOutWorktreeInit {
    ignored_env_files: Vec<SnapshotFile>,
    ignored_active_spec: Option<SnapshotFile>,
}

pub enum AsyncPnpmInstall {
    Started,
    Skipped,
}

pub fn create(session: &mut Session<'_>, opts: Options) -> AppResult<()> {
    if !opts.auxiliary_groups.is_empty() {
        return create_with_auxiliaries(session, opts);
    }
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
    let ignored_env_files =
        snapshot_ignored_env_files(session, &repo.main_root).map_err(|error| {
            Error::message(format!(
                "worktree created, but failed to snapshot ignored .env files: {error}"
            ))
        })?;
    let ignored_env_snapshot_root = write_ignored_env_snapshot(&ignored_env_files, &path)?;
    cleanup_ignored_env_snapshot_on_error(
        finish(
            session,
            opts.no_clipboard,
            path.display().to_string(),
            format!("created worktree at {}", path.display()),
        ),
        &ignored_env_snapshot_root,
    )?;
    cleanup_ignored_env_snapshot_on_error(
        start_async_init_worktree(session, &repo.main_root, &path, &ignored_env_snapshot_root),
        &ignored_env_snapshot_root,
    )
}

fn create_with_auxiliaries(session: &mut Session<'_>, opts: Options) -> AppResult<()> {
    let repo = repo(session)?;
    if opts.branch.is_empty() {
        return Err(Error::message("branch is required"));
    }
    if !opts.path.is_empty() {
        return Err(Error::message(
            "--path is not supported with Auxiliary Groups; paths are derived from the branch name",
        ));
    }
    let selections = auxiliary::expand_groups(
        &session.git,
        &repo.main_root,
        &repo.git_common_dir,
        &opts.auxiliary_groups,
    )?;
    if selections.is_empty() {
        return Err(Error::message(
            "at least one Auxiliary Group with auxiliaries is required",
        ));
    }

    let primary_path = create_target_path(&repo, &opts.branch, "")?;
    let primary_base = prepare_create_base(session, &repo, &opts)?;
    if branch_exists(&session.git, &repo.main_root, &opts.branch)? {
        return Err(Error::message(format!(
            "branch already exists in Primary Repository: {}",
            opts.branch
        )));
    }

    let mut auxiliary_bases = BTreeMap::new();
    let mut auxiliary_paths = BTreeMap::new();
    for selection in &selections {
        let base = prepare_create_base(session, &selection.repo, &opts)?;
        if branch_exists(&session.git, &selection.repo.main_root, &opts.branch)? {
            return Err(Error::message(format!(
                "branch already exists in auxiliary repository {}: {}",
                selection.name, opts.branch
            )));
        }
        let path = default_path(&selection.repo.main_root, &opts.branch);
        if path.exists() {
            return Err(Error::message(format!(
                "target path already exists for auxiliary repository {}: {}",
                selection.name,
                path.display()
            )));
        }
        ensure_creatable_parent(&path)?;
        auxiliary_bases.insert(selection.name.clone(), base);
        auxiliary_paths.insert(selection.name.clone(), path);
    }

    let mut created = Vec::<(PathBuf, PathBuf, String)>::new();
    let previous_state = auxiliary::read_state(&repo.main_root, &repo.git_common_dir)?;
    let result = (|| {
        create_git_worktree(
            session,
            &repo.main_root,
            &primary_path,
            &opts.branch,
            &primary_base,
        )?;
        created.push((
            repo.main_root.clone(),
            primary_path.clone(),
            opts.branch.clone(),
        ));

        for selection in &selections {
            let path = auxiliary_paths
                .get(&selection.name)
                .ok_or_else(|| Error::message("missing auxiliary target path"))?;
            let base = auxiliary_bases
                .get(&selection.name)
                .ok_or_else(|| Error::message("missing auxiliary base"))?;
            create_git_worktree(session, &selection.repo.main_root, path, &opts.branch, base)?;
            created.push((
                selection.repo.main_root.clone(),
                path.clone(),
                opts.branch.clone(),
            ));
        }

        for selection in &selections {
            let path = auxiliary_paths
                .get(&selection.name)
                .ok_or_else(|| Error::message("missing auxiliary target path"))?;
            auxiliary::write_ref(&primary_path.join("refs").join(&selection.name), path)?;
        }

        let primary_env = snapshot_ignored_env_files(session, &repo.main_root)?;
        print_copied_ignored_env_files(session, copy_snapshot_files(&primary_env, &primary_path)?)?;
        for selection in &selections {
            let path = auxiliary_paths
                .get(&selection.name)
                .ok_or_else(|| Error::message("missing auxiliary target path"))?;
            worktree_init_without_pnpm(session, &selection.repo.main_root, path)?;
        }

        let mut state = previous_state.clone();
        let auxiliaries = selections
            .iter()
            .map(|selection| {
                let worktree = auxiliary_paths
                    .get(&selection.name)
                    .ok_or_else(|| Error::message("missing auxiliary target path"))?;
                Ok((
                    selection.name.clone(),
                    AuxiliaryWorktreeState {
                        repository: selection.repository.clone(),
                        worktree: worktree.clone(),
                    },
                ))
            })
            .collect::<AppResult<BTreeMap<_, _>>>()?;
        state.worktrees.insert(
            absolute_path(&primary_path),
            WorktreeEntry {
                branch: opts.branch.clone(),
                auxiliaries,
            },
        );
        auxiliary::write_state(&repo.git_common_dir, &state)?;
        Ok(())
    })();

    if let Err(error) = result {
        for (repo_root, path, branch) in created.iter().rev() {
            let _ = session.git.run(
                repo_root,
                ["worktree", "remove", "--force", &path.display().to_string()],
            );
            let _ = session.git.run(repo_root, ["branch", "-D", branch]);
        }
        let _ = auxiliary::write_state(&repo.git_common_dir, &previous_state);
        return Err(error);
    }

    finish(
        session,
        opts.no_clipboard,
        primary_path.display().to_string(),
        format!("created coordinated worktree at {}", primary_path.display()),
    )?;
    for selection in &selections {
        if let Some(path) = auxiliary_paths.get(&selection.name) {
            start_worktree_async_pnpm_install(session, path, "worktree initialized")?;
        }
    }
    start_worktree_async_pnpm_install(session, &primary_path, "worktree initialized").map(|_| ())
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
        copy_snapshot_files(&ignored_env_files, &path).map_err(|error| {
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

pub fn status(session: &mut Session<'_>) -> AppResult<()> {
    let repo = repo(session)?;
    let state = auxiliary::read_state(&repo.main_root, &repo.git_common_dir)?;
    if let Some(entry) = auxiliary::worktree_entry(&state, &repo.current_root) {
        let worktree = repo.worktree_by_path(&repo.current_root).ok_or_else(|| {
            Error::message(format!(
                "worktree at {} is missing from git worktree list",
                repo.current_root.display()
            ))
        })?;
        auxiliary::validate_primary_worktree_branch(worktree, &entry.branch, &repo.current_root)?;
        let refs = auxiliary::validate_refs(&session.git, &repo.current_root, entry)?;
        let payload = AuxiliaryStatusOutput {
            mode: "coordinated",
            primary_worktree: repo.current_root.clone(),
            primary_main_worktree: repo.main_root.clone(),
            branch: entry.branch.clone(),
            current_is_main: repo.current_is_main,
            state: auxiliary::state_path(&repo.main_root, &repo.git_common_dir),
            auxiliaries: auxiliary_status_entries(&repo.current_root, entry, &refs),
        };
        serde_yaml::to_writer(&mut *session.out, &payload).map_err(|error| {
            Error::message(format!("failed to serialize status as YAML: {error}"))
        })?;
        writeln!(session.out)?;
        return Ok(());
    }
    let payload = StatusOutput {
        cwd: repo.cwd.clone(),
        current_root: repo.current_root.clone(),
        main_root: repo.main_root.clone(),
        git_common_dir: repo.git_common_dir.clone(),
        current_is_main: repo.current_is_main,
    };

    serde_yaml::to_writer(&mut *session.out, &payload)
        .map_err(|error| Error::message(format!("failed to serialize status as YAML: {error}")))?;
    writeln!(session.out)?;
    Ok(())
}

pub fn list(session: &mut Session<'_>, options: ListOptions) -> AppResult<()> {
    let repo = repo(session)?;
    let state = auxiliary::read_state(&repo.main_root, &repo.git_common_dir)?;
    let updated_at_by_head =
        list::commit_timestamps_by_head(&session.git, &repo.main_root, &repo.worktrees);
    let mut rows = Vec::with_capacity(repo.worktrees.len());
    for worktree in &repo.worktrees {
        let mut row = list::repository_row(&session.git, &repo, worktree, &updated_at_by_head);
        if let Some(entry) = auxiliary::worktree_entry(&state, &worktree.path) {
            let ignored_refs = auxiliary::ignored_ref_paths(entry);
            row = list::repository_row_with_options(
                &session.git,
                &repo,
                worktree,
                Some(&ignored_refs),
                &updated_at_by_head,
            );
            row.kind = "primary_worktree";
            if let Err(error) =
                auxiliary::validate_primary_worktree_branch(worktree, &entry.branch, &worktree.path)
            {
                row.diagnostics.push(error.to_string());
                if !row.labels.iter().any(|label| label == "error") {
                    row.labels.push("error".to_string());
                }
            }
            row.auxiliary_refs = Some(auxiliary_ref_summary(&session.git, &worktree.path, entry));
        }
        rows.push(row);
    }
    let payload = list::ListOutput {
        mode: "repository",
        worktrees: list::sorted_rows(rows),
    };
    list::render(
        session.out,
        &payload,
        options,
        output::Style::new(session.style_enabled && !options.json),
    )
}

pub fn init_worktree(
    session: &mut Session<'_>,
    source_root: &Path,
    worktree_path: &Path,
    ignored_env_snapshot_root: Option<&Path>,
) -> AppResult<()> {
    copy_ignored_env_files_for_init(
        session,
        source_root,
        worktree_path,
        ignored_env_snapshot_root,
    )?;
    maybe_run_pnpm_install(session, worktree_path, "worktree initialized")
}

pub fn init_worktree_with_async_pnpm(
    session: &mut Session<'_>,
    source_root: &Path,
    worktree_path: &Path,
    ignored_env_snapshot_root: Option<&Path>,
) -> AppResult<()> {
    prepare_worktree_for_async_pnpm(
        session,
        source_root,
        worktree_path,
        ignored_env_snapshot_root,
    )?;
    start_worktree_async_pnpm_install(session, worktree_path, "worktree initialized").map(|_| ())
}

fn worktree_init_without_pnpm(
    session: &mut Session<'_>,
    source_root: &Path,
    worktree_path: &Path,
) -> AppResult<()> {
    let ignored_env_files = snapshot_ignored_env_files(session, source_root)?;
    print_copied_ignored_env_files(
        session,
        copy_snapshot_files(&ignored_env_files, worktree_path)?,
    )
}

fn remove_with_auxiliaries(
    session: &mut Session<'_>,
    opts: Options,
    repo: RepoContext,
    target: PathBuf,
    worktree: crate::gitexec::Worktree,
    state: &mut auxiliary::WorktreesState,
    entry: WorktreeEntry,
) -> AppResult<()> {
    auxiliary::validate_primary_worktree_branch(&worktree, &entry.branch, &target)?;
    if let Some(reason) = &worktree.locked {
        let detail = if reason.is_empty() {
            String::new()
        } else {
            format!(": {reason}")
        };
        return Err(Error::message(format!(
            "Primary worktree at {} is locked{}",
            target.display(),
            detail
        )));
    }
    auxiliary::validate_refs(&session.git, &target, &entry)?;
    require_clean_ignoring_refs(session, &target, &entry)?;
    validate_auxiliary_worktrees_removable(&session.git, &entry)?;
    for auxiliary in entry.auxiliaries.values() {
        require_clean(session, &auxiliary.worktree)?;
    }
    preflight_branch_deletions(
        &session.git,
        &repo.main_root,
        &worktree,
        opts.delete_branch,
        &entry,
    )?;

    for auxiliary in entry.auxiliaries.values() {
        remove_git_worktree(session, &auxiliary.repository, &auxiliary.worktree)?;
    }
    remove_git_worktree_force(session, &repo.main_root, &target)?;

    if opts.delete_branch {
        for auxiliary in entry.auxiliaries.values() {
            if let Err(error) = delete_branch(session, &auxiliary.repository, &entry.branch) {
                return Err(Error::message(format!(
                    "coordinated worktrees removed, but auxiliary branch deletion failed; coordinated state remains in {}: {}",
                    auxiliary::state_path(&repo.main_root, &repo.git_common_dir).display(),
                    error
                )));
            }
        }
        if worktree.branch.is_empty() {
            return Err(Error::message(
                "cannot delete branch for detached linked worktree",
            ));
        }
        if let Err(error) = delete_branch(session, &repo.main_root, &worktree.branch) {
            return Err(Error::message(format!(
                "coordinated worktrees removed, but primary branch deletion failed; coordinated state remains in {}: {}",
                auxiliary::state_path(&repo.main_root, &repo.git_common_dir).display(),
                error
            )));
        }
    }
    auxiliary::remove_worktree_entry(state, &target);
    auxiliary::write_state(&repo.git_common_dir, state)?;

    finish(
        session,
        opts.no_clipboard,
        target.display().to_string(),
        format!("removed coordinated worktree {}", target.display()),
    )
}

fn preflight_branch_deletions(
    git: &Git,
    primary_repo_root: &Path,
    worktree: &crate::gitexec::Worktree,
    delete_branch: bool,
    entry: &WorktreeEntry,
) -> AppResult<()> {
    if !delete_branch {
        return Ok(());
    }
    for auxiliary in entry.auxiliaries.values() {
        if let Err(error) = require_branch_deletable(git, &auxiliary.repository, &entry.branch) {
            return Err(Error::message(format!(
                "cannot remove coordinated worktree with --delete-branch because auxiliary branch deletion would fail in {}: {}",
                auxiliary.repository.display(),
                error
            )));
        }
    }
    if worktree.branch.is_empty() {
        return Err(Error::message(
            "cannot delete branch for detached linked worktree",
        ));
    }
    if let Err(error) = require_branch_deletable(git, primary_repo_root, &worktree.branch) {
        return Err(Error::message(format!(
            "cannot remove coordinated worktree with --delete-branch because primary branch deletion would fail in {}: {}",
            primary_repo_root.display(),
            error
        )));
    }
    Ok(())
}

fn require_branch_deletable(git: &Git, repo_root: &Path, branch: &str) -> AppResult<()> {
    let branch_ref = format!("refs/heads/{branch}");
    let delete_target = branch_delete_target(git, repo_root, &branch_ref)?;
    match git.run(
        repo_root,
        ["merge-base", "--is-ancestor", &branch_ref, &delete_target],
    ) {
        Ok(_) => Ok(()),
        Err(error) if is_git_exit(&error, 1) => Err(Error::message(format!(
            "branch {branch} is not fully merged into {delete_target}"
        ))),
        Err(error) => Err(error),
    }
}

fn branch_delete_target(git: &Git, repo_root: &Path, branch_ref: &str) -> AppResult<String> {
    let upstream = git
        .run(
            repo_root,
            ["for-each-ref", "--format=%(upstream:short)", branch_ref],
        )?
        .stdout
        .trim()
        .to_string();
    if upstream.is_empty() {
        Ok("HEAD".to_string())
    } else {
        Ok(upstream)
    }
}

fn validate_auxiliary_worktrees_removable(git: &Git, entry: &WorktreeEntry) -> AppResult<()> {
    for (name, auxiliary) in &entry.auxiliaries {
        let repo = resolve(git, &auxiliary.worktree)?;
        let worktree = repo.worktree_by_path(&auxiliary.worktree).ok_or_else(|| {
            Error::message(format!(
                "Auxiliary worktree {name} is missing from git worktree list at {}",
                auxiliary.repository.display()
            ))
        })?;
        if let Some(reason) = &worktree.locked {
            let detail = if reason.is_empty() {
                String::new()
            } else {
                format!(": {reason}")
            };
            return Err(Error::message(format!(
                "Auxiliary worktree {name} at {} is locked{}",
                auxiliary.worktree.display(),
                detail
            )));
        }
    }
    Ok(())
}

pub fn prepare_worktree_for_async_pnpm(
    session: &mut Session<'_>,
    source_root: &Path,
    worktree_path: &Path,
    ignored_env_snapshot_root: Option<&Path>,
) -> AppResult<()> {
    copy_ignored_env_files_for_init(
        session,
        source_root,
        worktree_path,
        ignored_env_snapshot_root,
    )
}

pub fn start_worktree_async_pnpm_install(
    session: &mut Session<'_>,
    worktree_path: &Path,
    partial_success_prefix: &str,
) -> AppResult<AsyncPnpmInstall> {
    start_async_pnpm_install(session, worktree_path, partial_success_prefix)
}

fn copy_ignored_env_files_for_init(
    session: &mut Session<'_>,
    source_root: &Path,
    worktree_path: &Path,
    ignored_env_snapshot_root: Option<&Path>,
) -> AppResult<()> {
    let (ignored_env_files, snapshot_root) = match ignored_env_snapshot_root {
        Some(snapshot_root) => match snapshot_ignored_env_files_from_root(snapshot_root) {
            Ok(ignored_env_files) => (ignored_env_files, Some(snapshot_root)),
            Err(error) => {
                return match remove_ignored_env_snapshot_root(snapshot_root) {
                    Ok(()) => Err(error),
                    Err(cleanup_error) => {
                        Err(Error::message(format!("{error}; also {cleanup_error}")))
                    }
                };
            }
        },
        None => (snapshot_ignored_env_files(session, source_root)?, None),
    };
    let copy_result = copy_snapshot_files(&ignored_env_files, worktree_path)
        .map_err(|error| Error::message(format!("ignored .env copy failed: {error}")))
        .and_then(|copied| print_copied_ignored_env_files(session, copied));
    let cleanup_result = snapshot_root.map_or(Ok(()), remove_ignored_env_snapshot_root);
    match (copy_result, cleanup_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(cleanup_error)) => {
            Err(Error::message(format!("{error}; also {cleanup_error}")))
        }
    }
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

    let worktree = repo.worktree_by_path(&target).cloned().ok_or_else(|| {
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
    let mut state = auxiliary::read_state(&repo.main_root, &repo.git_common_dir)?;
    if let Some(entry) = auxiliary::worktree_entry(&state, &target).cloned() {
        return remove_with_auxiliaries(
            session,
            opts,
            repo,
            target,
            worktree.clone(),
            &mut state,
            entry,
        );
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
    reject_auxiliary_state(&repo, &repo.current_root, "send-out")?;
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
    let ignored_active_spec = snapshot_ignored_send_out_active_spec(session, &repo.main_root)?;

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
        copy_snapshot_files(&ignored_env_files, &path).map_err(|error| {
            Error::message(format!(
                "main worktree switched to {base} and linked worktree created, but ignored .env copy failed: {error}"
            ))
        })?,
    )?;
    if let Some(active_spec) = ignored_active_spec {
        print_copied_ignored_files(
            session,
            "copied ignored file",
            copy_snapshot_files(&[active_spec], &path).map_err(|error| {
                Error::message(format!(
                    "main worktree switched to {base} and linked worktree created, but ignored file copy failed: {error}"
                ))
            })?,
        )?;
    }
    finish(
        session,
        opts.no_clipboard,
        path.display().to_string(),
        format!("sent {} out to {}", branch.trim(), path.display()),
    )?;
    start_async_pnpm_install(
        session,
        &path,
        &format!("main worktree switched to {base} and linked worktree created"),
    )
    .map(|_| ())
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
    reject_auxiliary_state(&repo, &target, "bring-in")?;

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

fn create_git_worktree(
    session: &mut Session<'_>,
    repo_root: &Path,
    path: &Path,
    branch: &str,
    base: &str,
) -> AppResult<()> {
    let args = vec![
        "worktree".to_string(),
        "add".to_string(),
        "-b".to_string(),
        branch.to_string(),
        path.display().to_string(),
        base.to_string(),
    ];
    output::git(session.out, repo_root, &args)?;
    session
        .git
        .run(repo_root, args.iter().map(String::as_str))?;
    Ok(())
}

fn remove_git_worktree(session: &mut Session<'_>, repo_root: &Path, path: &Path) -> AppResult<()> {
    let args = vec![
        "worktree".to_string(),
        "remove".to_string(),
        path.display().to_string(),
    ];
    output::git(session.out, repo_root, &args)?;
    session
        .git
        .run(repo_root, args.iter().map(String::as_str))?;
    Ok(())
}

fn remove_git_worktree_force(
    session: &mut Session<'_>,
    repo_root: &Path,
    path: &Path,
) -> AppResult<()> {
    let args = vec![
        "worktree".to_string(),
        "remove".to_string(),
        "--force".to_string(),
        path.display().to_string(),
    ];
    output::git(session.out, repo_root, &args)?;
    session
        .git
        .run(repo_root, args.iter().map(String::as_str))?;
    Ok(())
}

fn delete_branch(session: &mut Session<'_>, repo_root: &Path, branch: &str) -> AppResult<()> {
    let args = vec!["branch".to_string(), "-d".to_string(), branch.to_string()];
    output::git(session.out, repo_root, &args)?;
    session
        .git
        .run(repo_root, args.iter().map(String::as_str))?;
    Ok(())
}

fn branch_exists(git: &Git, repo_root: &Path, branch: &str) -> AppResult<bool> {
    match git.run(
        repo_root,
        [
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    ) {
        Ok(_) => Ok(true),
        Err(error) if is_git_exit(&error, 1) => Ok(false),
        Err(error) => Err(error),
    }
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

fn require_clean_ignoring_refs(
    session: &Session<'_>,
    dir: &Path,
    entry: &WorktreeEntry,
) -> AppResult<()> {
    let status = session
        .git
        .run(dir, ["status", "--porcelain=v1", "--untracked-files=all"])?;
    let ignored = auxiliary::ignored_ref_paths(entry);
    let visible = status
        .stdout
        .lines()
        .filter(|line| !auxiliary::status_line_ignored(line, &ignored))
        .collect::<Vec<_>>();
    if visible.is_empty() {
        Ok(())
    } else {
        Err(Error::message(format!(
            "worktree is dirty at {}:\n{}",
            dir.display(),
            visible.join("\n")
        )))
    }
}

fn reject_auxiliary_state(
    repo: &RepoContext,
    primary_worktree: &Path,
    command: &str,
) -> AppResult<()> {
    let state = auxiliary::read_state(&repo.main_root, &repo.git_common_dir)?;
    if auxiliary::worktree_entry(&state, primary_worktree).is_some() {
        return Err(Error::message(format!(
            "{command} is not supported for worktrees with auxiliary state"
        )));
    }
    Ok(())
}

fn auxiliary_status_entries(
    primary_worktree: &Path,
    entry: &WorktreeEntry,
    refs: &[AuxiliaryRefStatus],
) -> BTreeMap<String, AuxiliaryStatusEntry> {
    refs.iter()
        .filter_map(|reference| {
            let auxiliary = entry.auxiliaries.get(&reference.name)?;
            Some((
                reference.name.clone(),
                AuxiliaryStatusEntry {
                    repository: auxiliary.repository.clone(),
                    worktree: auxiliary.worktree.clone(),
                    ref_path: primary_worktree.join("refs").join(&reference.name),
                    current_target: reference.current_target.clone(),
                },
            ))
        })
        .collect()
}

fn auxiliary_ref_summary(
    git: &Git,
    primary_worktree: &Path,
    entry: &WorktreeEntry,
) -> AuxiliaryRefSummary {
    let details = entry
        .auxiliaries
        .iter()
        .map(|(name, auxiliary)| {
            let mut diagnostics = Vec::new();
            let current_target =
                match auxiliary::read_ref(&primary_worktree.join("refs").join(name)) {
                    Ok(target) => {
                        if !same_path(&target, &auxiliary.worktree) {
                            diagnostics.push(format!(
                                "Auxiliary Ref points to {}, expected {}",
                                target.display(),
                                auxiliary.worktree.display()
                            ));
                        }
                        if let Err(error) =
                            auxiliary::validate_worktree_branch(git, name, &entry.branch, auxiliary)
                        {
                            diagnostics.push(error.to_string());
                        }
                        Some(target)
                    }
                    Err(error) => {
                        diagnostics.push(error.to_string());
                        None
                    }
                };
            AuxiliaryRefDetail {
                name: name.clone(),
                ok: diagnostics.is_empty(),
                expected_target: auxiliary.worktree.clone(),
                current_target,
                diagnostics,
            }
        })
        .collect::<Vec<_>>();
    let ok = details.iter().filter(|detail| detail.ok).count();
    let total = details.len();
    AuxiliaryRefSummary {
        total,
        ok,
        broken: total.saturating_sub(ok),
        details,
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

fn copy_snapshot_files(
    ignored: &[SnapshotFile],
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
            SnapshotFileKind::File {
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
            SnapshotFileKind::Symlink {
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
) -> AppResult<Vec<SnapshotFile>> {
    let mut ignored = Vec::new();
    for relative in ignored_env_files(session, main_root)? {
        if let Some(snapshot) = snapshot_file(main_root, relative)? {
            ignored.push(snapshot);
        }
    }
    Ok(ignored)
}

fn snapshot_ignored_send_out_active_spec(
    session: &Session<'_>,
    main_root: &Path,
) -> AppResult<Option<SnapshotFile>> {
    snapshot_ignored_exact_file(session, main_root, IGNORED_SEND_OUT_ACTIVE_SPEC_PATH)
}

pub fn snapshot_send_out_worktree_init(
    session: &Session<'_>,
    main_root: &Path,
) -> AppResult<SendOutWorktreeInit> {
    Ok(SendOutWorktreeInit {
        ignored_env_files: snapshot_ignored_env_files(session, main_root)?,
        ignored_active_spec: snapshot_ignored_send_out_active_spec(session, main_root)?,
    })
}

pub fn apply_send_out_worktree_init(
    session: &mut Session<'_>,
    worktree_path: &Path,
    init: &SendOutWorktreeInit,
) -> AppResult<()> {
    print_copied_ignored_env_files(
        session,
        copy_snapshot_files(&init.ignored_env_files, worktree_path)
            .map_err(|error| Error::message(format!("ignored .env copy failed: {error}")))?,
    )?;
    if let Some(active_spec) = &init.ignored_active_spec {
        print_copied_ignored_files(
            session,
            "copied ignored file",
            copy_snapshot_files(std::slice::from_ref(active_spec), worktree_path)
                .map_err(|error| Error::message(format!("ignored file copy failed: {error}")))?,
        )?;
    }
    Ok(())
}

fn snapshot_ignored_exact_file(
    session: &Session<'_>,
    main_root: &Path,
    relative: &str,
) -> AppResult<Option<SnapshotFile>> {
    let relative_path = PathBuf::from(relative);
    if !ignored_exact_file(session, main_root, relative)? {
        return Ok(None);
    }
    snapshot_file(main_root, relative_path)
}

fn snapshot_ignored_env_files_from_root(root: &Path) -> AppResult<Vec<SnapshotFile>> {
    let mut ignored = Vec::new();
    collect_ignored_env_files_from_root(root, root, &mut ignored)?;
    ignored.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(ignored)
}

fn collect_ignored_env_files_from_root(
    root: &Path,
    current: &Path,
    ignored: &mut Vec<SnapshotFile>,
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
        if let Some(snapshot) = snapshot_file(root, relative.to_path_buf())? {
            ignored.push(snapshot);
        }
    }

    Ok(())
}

fn snapshot_file(main_root: &Path, relative: PathBuf) -> AppResult<Option<SnapshotFile>> {
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
        return Ok(Some(SnapshotFile {
            relative,
            kind: SnapshotFileKind::Symlink { target },
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
    Ok(Some(SnapshotFile {
        relative,
        kind: SnapshotFileKind::File {
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

fn ignored_exact_file(session: &Session<'_>, main_root: &Path, relative: &str) -> AppResult<bool> {
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
            relative,
        ],
    )?;
    Ok(output
        .stdout
        .split(|byte| *byte == b'\0')
        .filter(|path| !path.is_empty())
        .map(path_buf_from_git_bytes)
        .any(|path| path == Path::new(relative)))
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
    print_copied_ignored_files(session, "copied ignored .env", copied)
}

fn print_copied_ignored_files(
    session: &mut Session<'_>,
    label: &str,
    copied: Vec<PathBuf>,
) -> AppResult<()> {
    for relative in copied {
        writeln!(session.out, "{label}: {}", relative.display())?;
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
    ignored_env_files: &[SnapshotFile],
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
        "{}{}-{}-{}",
        IGNORED_ENV_SNAPSHOT_PREFIX,
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
    write_ignored_env_snapshot_marker(&snapshot_root)?;
    cleanup_ignored_env_snapshot_on_error(
        copy_snapshot_files(ignored_env_files, &snapshot_root)
            .map_err(|error| {
                Error::message(format!(
                    "worktree created, but failed to snapshot ignored .env files in {}: {error}",
                    snapshot_root.display()
                ))
            })
            .map(|_| ()),
        &snapshot_root,
    )?;
    Ok(snapshot_root)
}

fn write_ignored_env_snapshot_marker(snapshot_root: &Path) -> AppResult<()> {
    fs::write(
        snapshot_root.join(IGNORED_ENV_SNAPSHOT_MARKER),
        b"managed by wtk\n",
    )
    .map_err(|error| {
        Error::message(format!(
            "worktree created, but failed to mark ignored .env snapshot {}: {error}",
            snapshot_root.display()
        ))
    })
}

fn remove_ignored_env_snapshot_root(snapshot_root: &Path) -> AppResult<()> {
    validate_ignored_env_snapshot_root(snapshot_root)?;
    fs::remove_dir_all(snapshot_root).map_err(|error| {
        Error::message(format!(
            "failed to remove ignored .env snapshot {}: {error}",
            snapshot_root.display()
        ))
    })
}

fn validate_ignored_env_snapshot_root(snapshot_root: &Path) -> AppResult<()> {
    let file_name = snapshot_root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            Error::message(format!(
                "refusing to remove unmanaged ignored .env snapshot {}",
                snapshot_root.display()
            ))
        })?;
    let expected_parent = std::env::temp_dir();
    if snapshot_root.parent() != Some(expected_parent.as_path())
        || !file_name.starts_with(IGNORED_ENV_SNAPSHOT_PREFIX)
        || !snapshot_root.join(IGNORED_ENV_SNAPSHOT_MARKER).is_file()
    {
        return Err(Error::message(format!(
            "refusing to remove unmanaged ignored .env snapshot {}",
            snapshot_root.display()
        )));
    }
    Ok(())
}

fn cleanup_ignored_env_snapshot_on_error(
    result: AppResult<()>,
    snapshot_root: &Path,
) -> AppResult<()> {
    match result {
        Ok(()) => Ok(()),
        Err(error) => match remove_ignored_env_snapshot_root(snapshot_root) {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(Error::message(format!("{error}; also {cleanup_error}"))),
        },
    }
}

fn async_init_stdio(worktree_path: &Path) -> AppResult<(Stdio, Stdio, Option<PathBuf>)> {
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
    let stdout = open_async_init_log(&log_path)?;
    let stderr = stdout.try_clone().map_err(|error| {
        Error::message(format!(
            "worktree created, but failed to duplicate async initialization log {}: {error}",
            log_path.display()
        ))
    })?;
    Ok((Stdio::from(stdout), Stdio::from(stderr), Some(log_path)))
}

fn open_async_init_log(log_path: &Path) -> AppResult<File> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(log_path).map_err(|error| {
        Error::message(format!(
            "worktree created, but failed to open async initialization log {}: {error}",
            log_path.display()
        ))
    })
}

pub fn maybe_run_pnpm_install(
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

fn start_async_pnpm_install(
    session: &mut Session<'_>,
    worktree_path: &Path,
    partial_success_prefix: &str,
) -> AppResult<AsyncPnpmInstall> {
    if !should_run_pnpm_install(worktree_path) {
        return Ok(AsyncPnpmInstall::Skipped);
    }

    output::info(
        session.out,
        &format!(
            "running pnpm install asynchronously in {}",
            worktree_path.display()
        ),
    )?;
    let (stdout, stderr, log_path) = async_pnpm_install_stdio(worktree_path)?;
    if let Some(log_path) = log_path {
        output::info(
            session.out,
            &format!(
                "async pnpm install output will be written to {}",
                log_path.display()
            ),
        )?;
    }

    Command::new("pnpm")
        .arg("install")
        .current_dir(worktree_path)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map(|_| AsyncPnpmInstall::Started)
        .map_err(|error| {
            Error::message(format!(
                "{partial_success_prefix}, but failed to start async pnpm install: {error}"
            ))
        })
}

fn async_pnpm_install_stdio(worktree_path: &Path) -> AppResult<(Stdio, Stdio, Option<PathBuf>)> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let worktree_name = worktree_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("worktree");
    let log_path = std::env::temp_dir().join(format!(
        "wtk-pnpm-install-{}-{}-{}.log",
        std::process::id(),
        worktree_name,
        nonce
    ));
    let stdout = open_async_pnpm_install_log(&log_path)?;
    let stderr = stdout.try_clone().map_err(|error| {
        Error::message(format!(
            "worktree created, but failed to duplicate async pnpm install log {}: {error}",
            log_path.display()
        ))
    })?;
    Ok((Stdio::from(stdout), Stdio::from(stderr), Some(log_path)))
}

fn open_async_pnpm_install_log(log_path: &Path) -> AppResult<File> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(log_path).map_err(|error| {
        Error::message(format!(
            "worktree created, but failed to open async pnpm install log {}: {error}",
            log_path.display()
        ))
    })
}

fn should_run_pnpm_install(worktree_path: &Path) -> bool {
    worktree_path.join("pnpm-lock.yaml").is_file()
        || worktree_path.join("pnpm-workspace.yaml").is_file()
}

pub(crate) fn finish(
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
    use super::{
        async_init_stdio, async_pnpm_install_stdio, cleanup_ignored_env_snapshot_on_error, finish,
        open_async_init_log, remove_ignored_env_snapshot_root, should_run_pnpm_install,
        write_ignored_env_snapshot_marker,
    };
    use crate::clipboard::ClipboardProvider;
    use crate::{AppResult, Error};
    use std::io;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
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
        let mut session = super::Session::new(PathBuf::from("."), &mut out, &mut clipboard, false);
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
    fn cleanup_ignored_env_snapshot_on_error_removes_snapshot_root() {
        let snapshot_root = std::env::temp_dir().join(format!(
            "{}{}-{}",
            super::IGNORED_ENV_SNAPSHOT_PREFIX,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&snapshot_root).unwrap();
        write_ignored_env_snapshot_marker(&snapshot_root).unwrap();
        std::fs::write(snapshot_root.join(".env"), "SECRET=value\n").unwrap();

        let error = cleanup_ignored_env_snapshot_on_error(
            Err(Error::message(
                "operation succeeded, but clipboard copy failed",
            )),
            &snapshot_root,
        )
        .expect_err("cleanup helper should preserve the original error");

        assert!(
            error
                .to_string()
                .contains("operation succeeded, but clipboard copy failed")
        );
        assert!(!snapshot_root.exists());
    }

    #[test]
    fn remove_ignored_env_snapshot_root_rejects_unmanaged_paths() {
        let snapshot_root = std::env::temp_dir().join(format!(
            "{}{}-{}",
            super::IGNORED_ENV_SNAPSHOT_PREFIX,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&snapshot_root).unwrap();
        std::fs::write(snapshot_root.join(".env"), "SECRET=value\n").unwrap();

        let error = remove_ignored_env_snapshot_root(&snapshot_root)
            .expect_err("unmanaged snapshot root should not be deleted");

        assert!(error.to_string().contains("refusing to remove unmanaged"));
        assert!(snapshot_root.exists());
        std::fs::remove_dir_all(&snapshot_root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn open_async_init_log_creates_owner_only_log_file() {
        let log_path = std::env::temp_dir().join(format!(
            "wtk-async-init-log-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let log = open_async_init_log(&log_path).expect("log file creation should succeed");
        let mode = std::fs::metadata(&log_path).unwrap().permissions().mode() & 0o777;

        assert_eq!(mode, 0o600);

        drop(log);
        std::fs::remove_file(&log_path).unwrap();
    }

    #[test]
    fn async_init_stdio_writes_to_log_file() {
        let worktree_path = std::env::temp_dir().join(format!(
            "wtk-async-init-stdio-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (stdout, stderr, log_path) =
            async_init_stdio(&worktree_path).expect("async init stdio should open a log file");
        let log_path = log_path.expect("async init output should be redirected to a log file");

        assert!(log_path.is_file());
        assert!(
            log_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("wtk-init-worktree-")
        );

        drop(stdout);
        drop(stderr);
        std::fs::remove_file(log_path).unwrap();
    }

    #[test]
    fn async_pnpm_install_stdio_writes_to_log_file() {
        let worktree_path = std::env::temp_dir().join(format!(
            "wtk-async-pnpm-stdio-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (stdout, stderr, log_path) = async_pnpm_install_stdio(&worktree_path)
            .expect("async pnpm install stdio should open a log file");
        let log_path = log_path.expect("async pnpm output should be redirected to a log file");

        assert!(log_path.is_file());
        assert!(
            log_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("wtk-pnpm-install-")
        );

        drop(stdout);
        drop(stderr);
        std::fs::remove_file(log_path).unwrap();
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
