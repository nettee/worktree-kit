use crate::auxiliary::{self, AuxiliaryRefStatus, AuxiliaryWorktreeState, WorktreeEntry};
use crate::clipboard::ClipboardProvider;
use crate::gitexec::{Git, RepoContext, absolute_path, is_git_exit, resolve, same_path};
use crate::list::{self, AuxiliaryRefDetail, AuxiliaryRefSummary, ListOptions};
use crate::output;
use crate::paths::default_path;
use crate::{AppResult, Error};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
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
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const RECURSIVE_IGNORED_FILE_SNAPSHOT_PREFIX: &str = "wtk-init-worktree-snapshot-";
const RECURSIVE_IGNORED_FILE_SNAPSHOT_MARKER: &str = ".wtk-recursive-ignored-files-snapshot";

#[derive(Debug, Clone)]
struct CopiedIgnoredFiles {
    patterns: Vec<String>,
    matcher: CopyPatternMatcher,
    pathspecs: Vec<String>,
}

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
    ignored_files: Vec<SnapshotFile>,
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
    let copied_files = copied_ignored_files(session, &repo.main_root)?;
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
    let ignored_files = snapshot_copy_pattern_files(session, &repo.main_root, &copied_files)
        .map_err(|error| {
            Error::message(format!(
                "worktree created, but failed to snapshot ignored files: {error}"
            ))
        })?;
    print_copied_file_count(session, ignored_files.len())?;
    let ignored_snapshot_root = write_recursive_ignored_file_snapshot(&ignored_files, &path)?;
    cleanup_recursive_ignored_file_snapshot_on_error(
        finish(
            session,
            opts.no_clipboard,
            path.display().to_string(),
            format!("created worktree at {}", path.display()),
        ),
        &ignored_snapshot_root,
    )?;
    cleanup_recursive_ignored_file_snapshot_on_error(
        start_async_init_worktree(session, &repo.main_root, &path, &ignored_snapshot_root),
        &ignored_snapshot_root,
    )
}

fn create_with_auxiliaries(session: &mut Session<'_>, opts: Options) -> AppResult<()> {
    let repo = repo(session)?;
    if opts.branch.is_empty() {
        return Err(Error::message("branch is required"));
    }
    if opts.from_current && !opts.base.is_empty() {
        return Err(Error::message(
            "--base and --from-current cannot be used together",
        ));
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
    let coordinated_current_branch = if opts.from_current {
        Some(current_branch_name(session, &repo.current_root)?)
    } else {
        None
    };
    let primary_base = match coordinated_current_branch.clone() {
        Some(branch) => branch,
        None => prepare_create_base(session, &repo, &opts)?,
    };
    if branch_exists(&session.git, &repo.main_root, &opts.branch)? {
        return Err(Error::message(format!(
            "branch already exists in Primary Repository: {}",
            opts.branch
        )));
    }

    let mut auxiliary_bases = BTreeMap::new();
    let mut auxiliary_paths = BTreeMap::new();
    for selection in &selections {
        let base = match coordinated_current_branch.clone() {
            Some(branch) => branch,
            None => prepare_create_base(session, &selection.repo, &opts)?,
        };
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
            auxiliary::write_auxiliary_marker(
                &session.git,
                path,
                &auxiliary::AuxiliaryMarker {
                    primary_repository: repo.main_root.clone(),
                    primary_worktree: primary_path.clone(),
                    branch: opts.branch.clone(),
                },
            )?;
        }

        let copied_files = copied_ignored_files(session, &repo.main_root)?;
        let ignored_files_to_copy =
            snapshot_copy_pattern_files(session, &repo.main_root, &copied_files)?;
        print_copied_files(
            session,
            &copied_files,
            copy_snapshot_files(&ignored_files_to_copy, &primary_path)?,
        )?;
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
        let entry = auxiliary::worktree_entry(&state, &primary_path)
            .ok_or_else(|| Error::message("missing coordinated worktree state after creation"))?;
        auxiliary::write_guidance(&primary_path, entry)?;
        auxiliary::install_ref_excludes(&session.git, &primary_path, entry)?;
        auxiliary::write_state(&session.git, &repo.main_root, &state)?;
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
        let _ = auxiliary::write_state(&session.git, &repo.main_root, &previous_state);
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
    let copied_files = copied_ignored_files(session, &repo.main_root)?;
    let ignored_files_to_copy =
        snapshot_copy_pattern_files(session, &repo.main_root, &copied_files)?;
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
    print_copied_files(
        session,
        &copied_files,
        copy_snapshot_files(&ignored_files_to_copy, &path).map_err(|error| {
            Error::message(format!(
                "worktree created, but ignored file copy failed: {error}"
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

#[derive(Debug, Clone)]
struct DeleteCandidate {
    path: PathBuf,
    branch: Option<String>,
    dirty: bool,
    coordinated: bool,
    members: Vec<DeleteMember>,
}

#[derive(Debug, Clone)]
struct DeleteMember {
    path: PathBuf,
    dirty: bool,
}

#[derive(Debug, Clone)]
struct DeleteRow {
    row: list::ListRow,
    non_deletable_reason: Option<String>,
    coordinated: bool,
    members: Vec<DeleteMember>,
}

fn delete_non_deletable_reason(
    session: &Session<'_>,
    worktree: &crate::gitexec::Worktree,
    row: &list::ListRow,
) -> AppResult<Option<String>> {
    if row.is_main {
        return Ok(Some("main worktree cannot be deleted".to_string()));
    }
    if row.is_current {
        return Ok(Some("current worktree cannot be deleted".to_string()));
    }
    if let Some(reason) = &worktree.locked {
        if reason.is_empty() {
            return Ok(Some("locked".to_string()));
        }
        return Ok(Some(format!("locked: {reason}")));
    }
    if !row.diagnostics.is_empty() {
        return Ok(Some(row.diagnostics.join("; ")));
    }
    match auxiliary::read_auxiliary_marker(&session.git, &worktree.path) {
        Ok(Some(_)) => Ok(Some(
            "delete is not supported for auxiliary-side worktrees".to_string(),
        )),
        Ok(None) => Ok(None),
        Err(error) => Err(error),
    }
}

fn render_delete_selection_table(
    out: &mut dyn Write,
    rows: &[DeleteRow],
    style: output::Style,
) -> AppResult<()> {
    let mut next_number = 1usize;
    let rendered_rows = rows
        .iter()
        .map(|delete_row| {
            let selector = if delete_row.non_deletable_reason.is_none() {
                let selector = next_number.to_string();
                next_number += 1;
                selector
            } else {
                "-".to_string()
            };
            let state = list::state_text(&delete_row.row);
            (
                selector,
                delete_row.row.display_name.clone(),
                list::branch_text(&delete_row.row),
                delete_row.row.updated.clone(),
                state,
                delete_row.row.short_head.clone(),
                delete_row.row.is_current,
                delete_row.row.diagnostics.is_empty(),
                delete_row.non_deletable_reason.clone(),
            )
        })
        .collect::<Vec<_>>();

    let selector_width = rendered_rows
        .iter()
        .map(|(selector, _, _, _, _, _, _, _, _)| selector.len())
        .chain(std::iter::once("#".len()))
        .max()
        .unwrap_or("#".len());
    let worktree_width = rendered_rows
        .iter()
        .map(|(_, worktree, _, _, _, _, _, _, _)| worktree.len())
        .chain(std::iter::once("worktree".len()))
        .max()
        .unwrap_or("worktree".len());
    let branch_width = rendered_rows
        .iter()
        .map(|(_, _, branch, _, _, _, _, _, _)| branch.len())
        .chain(std::iter::once("branch".len()))
        .max()
        .unwrap_or("branch".len());
    let updated_width = rendered_rows
        .iter()
        .map(|(_, _, _, updated, _, _, _, _, _)| updated.len())
        .chain(std::iter::once("updated".len()))
        .max()
        .unwrap_or("updated".len());
    let state_width = rendered_rows
        .iter()
        .map(|(_, _, _, _, state, _, _, _, _)| state.len())
        .chain(std::iter::once("state".len()))
        .max()
        .unwrap_or("state".len());

    let header = format!(
        "  {:selector_width$}   {:worktree_width$}  {:branch_width$}  {:updated_width$}  {:state_width$}  head",
        "#", "worktree", "branch", "updated", "state"
    );
    writeln!(out, "{}", style.header(&header))?;
    for (selector, worktree, branch, updated, state, head, is_current, diagnostics_ok, reason) in
        rendered_rows
    {
        let line = format!(
            "  {:selector_width$}   {:worktree_width$}  {:branch_width$}  {:updated_width$}  {:state_width$}  {}",
            selector, worktree, branch, updated, state, head
        );
        if is_current {
            writeln!(out, "{}", style.current(&line))?;
        } else if !diagnostics_ok
            || state.contains("broken")
            || state.contains("error")
            || state.contains("non-deletable")
        {
            writeln!(out, "{}", style.error(&line))?;
        } else if state.contains("dirty") || state.contains("prunable") {
            writeln!(out, "{}", style.warning(&line))?;
        } else {
            writeln!(out, "{line}")?;
        }
        if let Some(reason) = reason {
            writeln!(out, "      non-deletable: {reason}")?;
        }
    }
    Ok(())
}

fn parse_delete_selection(input: &str, candidate_count: usize) -> AppResult<Vec<usize>> {
    let mut selected = BTreeSet::new();
    for token in input.split(',') {
        let token = token.trim();
        if token.is_empty() {
            return Err(Error::message("empty selection token"));
        }
        if let Some((start, end)) = token.split_once('-') {
            let start = parse_delete_selection_number(start.trim(), candidate_count)?;
            let end = parse_delete_selection_number(end.trim(), candidate_count)?;
            if start > end {
                return Err(Error::message(format!("invalid descending range: {token}")));
            }
            for number in start..=end {
                selected.insert(number - 1);
            }
        } else {
            selected.insert(parse_delete_selection_number(token, candidate_count)? - 1);
        }
    }
    Ok(selected.into_iter().collect())
}

fn parse_delete_selection_number(token: &str, candidate_count: usize) -> AppResult<usize> {
    let number = token
        .parse::<usize>()
        .map_err(|_| Error::message(format!("invalid selection token: {token}")))?;
    if number == 0 || number > candidate_count {
        return Err(Error::message(format!(
            "selection number out of range: {number} (valid range: 1-{candidate_count})"
        )));
    }
    Ok(number)
}

pub fn delete_interactive(session: &mut Session<'_>) -> AppResult<()> {
    let repo = repo(session)?;
    let state = auxiliary::read_state(&repo.main_root, &repo.git_common_dir)?;
    let updated_at_by_head =
        list::commit_timestamps_by_head(&session.git, &repo.main_root, &repo.worktrees);
    let mut delete_rows = Vec::new();
    let mut rows = Vec::new();
    for worktree in &repo.worktrees {
        rows.push(list::repository_row(
            &session.git,
            &repo,
            worktree,
            &updated_at_by_head,
        ));
    }
    for row in list::sorted_rows(rows) {
        let Some(worktree) = repo.worktree_by_path(&row.path) else {
            continue;
        };
        let mut row = list::repository_row(&session.git, &repo, worktree, &updated_at_by_head);
        let mut members = vec![DeleteMember {
            path: worktree.path.clone(),
            dirty: row.dirty,
        }];
        let mut coordinated = false;
        if let Some(entry) = auxiliary::worktree_entry(&state, &worktree.path) {
            coordinated = true;
            let ignored_refs = auxiliary::ignored_ref_paths(entry);
            row = list::repository_row_with_options(
                &session.git,
                &repo,
                worktree,
                Some(&ignored_refs),
                &updated_at_by_head,
            );
            row.kind = "primary_worktree";
            let mut diagnostics = Vec::new();
            if let Err(error) =
                auxiliary::validate_primary_worktree_branch(worktree, &entry.branch, &worktree.path)
            {
                diagnostics.push(error.to_string());
            }
            if let Err(error) = auxiliary::validate_refs(&session.git, &worktree.path, entry) {
                diagnostics.push(error.to_string());
            }
            match auxiliary_delete_members(&session.git, entry) {
                Ok(auxiliary_members) => members.extend(auxiliary_members),
                Err(error) => diagnostics.push(error.to_string()),
            }
            row.diagnostics.extend(diagnostics);
        }
        if coordinated && !row.labels.iter().any(|label| label == "coordinated") {
            row.labels.push("coordinated".to_string());
        }
        let non_deletable_reason = delete_non_deletable_reason(session, worktree, &row)?;
        if non_deletable_reason.is_some() {
            if !row.labels.iter().any(|label| label == "non-deletable") {
                row.labels.push("non-deletable".to_string());
            }
            if !row.diagnostics.is_empty() && !row.labels.iter().any(|label| label == "error") {
                row.labels.push("error".to_string());
            }
        }
        delete_rows.push(DeleteRow {
            row,
            non_deletable_reason,
            coordinated,
            members,
        });
    }

    let candidates = delete_rows
        .iter()
        .filter(|delete_row| delete_row.non_deletable_reason.is_none())
        .map(|delete_row| DeleteCandidate {
            path: delete_row.row.path.clone(),
            branch: delete_row.row.branch.clone(),
            dirty: delete_row.row.dirty,
            coordinated: delete_row.coordinated,
            members: delete_row.members.clone(),
        })
        .collect::<Vec<_>>();

    if candidates.is_empty() {
        writeln!(
            session.out,
            "Select worktrees to delete by number. Branches are preserved."
        )?;
        writeln!(session.out)?;
        render_delete_selection_table(
            session.out,
            &delete_rows,
            output::Style::new(session.style_enabled),
        )?;
        writeln!(session.out, "No deletable linked worktrees found.")?;
        return Ok(());
    }

    writeln!(
        session.out,
        "Select worktrees to delete by number. Branches are preserved."
    )?;
    writeln!(session.out)?;
    render_delete_selection_table(
        session.out,
        &delete_rows,
        output::Style::new(session.style_enabled),
    )?;
    let selection = dialoguer::Input::<String>::new()
        .with_prompt("Enter numbers/ranges to delete, e.g. 1,3-5")
        .allow_empty(true)
        .interact_text()
        .map_err(|error| Error::message(format!("interactive selection failed: {error}")))?;
    if selection.trim().is_empty() {
        writeln!(session.out, "No worktrees selected; cancelled.")?;
        return Ok(());
    }
    let selected = parse_delete_selection(&selection, candidates.len())?;
    writeln!(
        session.out,
        "The following worktrees will be deleted; branches will be preserved:"
    )?;
    for index in &selected {
        let candidate = &candidates[*index];
        writeln!(session.out, "- path: {}", candidate.path.display())?;
        writeln!(
            session.out,
            "  branch: {}",
            candidate.branch.as_deref().unwrap_or("detached")
        )?;
        writeln!(
            session.out,
            "  dirty: {}",
            if candidate.dirty { "yes" } else { "no" }
        )?;
        if candidate.coordinated {
            writeln!(session.out, "  coordinated members:")?;
            for member in &candidate.members {
                writeln!(
                    session.out,
                    "    - {} dirty: {}",
                    member.path.display(),
                    if member.dirty { "yes" } else { "no" }
                )?;
            }
        }
    }
    let confirmation = dialoguer::Input::<String>::new()
        .with_prompt("Type Y to delete selected worktrees")
        .allow_empty(true)
        .interact_text()
        .map_err(|error| Error::message(format!("interactive confirmation failed: {error}")))?;
    if confirmation != "Y" {
        writeln!(session.out, "Cancelled.")?;
        return Ok(());
    }
    let mut failures = Vec::new();
    for index in selected {
        let candidate = &candidates[index];
        match delete_candidate(session, &repo, candidate) {
            Ok(()) => writeln!(session.out, "Deleted {}", candidate.path.display())?,
            Err(error) => {
                writeln!(
                    session.out,
                    "Failed {}: {}",
                    candidate.path.display(),
                    error
                )?;
                failures.push(format!("{}: {}", candidate.path.display(), error));
            }
        }
    }
    if failures.is_empty() {
        writeln!(session.out, "Deletion complete.")?;
        Ok(())
    } else {
        Err(Error::message(format!(
            "{} deletion(s) failed",
            failures.len()
        )))
    }
}

fn delete_candidate(
    session: &mut Session<'_>,
    repo: &RepoContext,
    candidate: &DeleteCandidate,
) -> AppResult<()> {
    let worktree = repo.worktree_by_path(&candidate.path).ok_or_else(|| {
        Error::message(format!(
            "target is not a linked worktree: {}",
            candidate.path.display()
        ))
    })?;
    if worktree.locked.is_some() {
        return Err(Error::message(format!(
            "worktree is locked: {}",
            candidate.path.display()
        )));
    }
    let mut state = auxiliary::read_state(&repo.main_root, &repo.git_common_dir)?;
    if let Some(entry) = auxiliary::worktree_entry(&state, &candidate.path).cloned() {
        auxiliary::validate_primary_worktree_branch(worktree, &entry.branch, &candidate.path)?;
        auxiliary::validate_refs(&session.git, &candidate.path, &entry)?;
        validate_auxiliary_worktrees_removable(&session.git, &entry)?;
        for auxiliary in entry.auxiliaries.values() {
            remove_git_worktree_force(session, &auxiliary.repository, &auxiliary.worktree)?;
        }
        remove_git_worktree_force(session, &repo.main_root, &candidate.path)?;
        auxiliary::remove_worktree_entry(&mut state, &candidate.path);
        auxiliary::write_state(&session.git, &repo.main_root, &state)?;
    } else {
        if auxiliary::read_auxiliary_marker(&session.git, &candidate.path)?.is_some() {
            return Err(Error::message(
                "delete is not supported for auxiliary-side worktrees",
            ));
        }
        remove_git_worktree_force(session, &repo.main_root, &candidate.path)?;
    }
    Ok(())
}

fn auxiliary_delete_members(git: &Git, entry: &WorktreeEntry) -> AppResult<Vec<DeleteMember>> {
    let mut members = Vec::new();
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
        members.push(DeleteMember {
            path: auxiliary.worktree.clone(),
            dirty: worktree_dirty(git, &auxiliary.worktree)?,
        });
    }
    Ok(members)
}

fn worktree_dirty(git: &Git, path: &Path) -> AppResult<bool> {
    let output = git
        .run(path, ["status", "--porcelain=v1", "--untracked-files=all"])
        .map_err(|error| {
            Error::message(format!(
                "failed to read dirty state for {}: {error}",
                path.display()
            ))
        })?;
    Ok(output.stdout.lines().any(|line| !line.is_empty()))
}

pub fn init_worktree(
    session: &mut Session<'_>,
    source_root: &Path,
    worktree_path: &Path,
    ignored_snapshot_root: Option<&Path>,
) -> AppResult<()> {
    copy_recursive_ignored_files_for_init(
        session,
        source_root,
        worktree_path,
        ignored_snapshot_root,
    )?;
    maybe_run_pnpm_install(session, worktree_path, "worktree initialized")
}

pub fn init_worktree_with_async_pnpm(
    session: &mut Session<'_>,
    source_root: &Path,
    worktree_path: &Path,
    ignored_snapshot_root: Option<&Path>,
) -> AppResult<()> {
    prepare_worktree_for_async_pnpm(session, source_root, worktree_path, ignored_snapshot_root)?;
    start_worktree_async_pnpm_install(session, worktree_path, "worktree initialized").map(|_| ())
}

fn worktree_init_without_pnpm(
    session: &mut Session<'_>,
    source_root: &Path,
    worktree_path: &Path,
) -> AppResult<()> {
    let copied_files = copied_ignored_files(session, source_root)?;
    let ignored_files_to_copy = snapshot_copy_pattern_files(session, source_root, &copied_files)?;
    print_copied_files(
        session,
        &copied_files,
        copy_snapshot_files(&ignored_files_to_copy, worktree_path)?,
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
    auxiliary::write_state(&session.git, &repo.main_root, state)?;

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
    ignored_snapshot_root: Option<&Path>,
) -> AppResult<()> {
    copy_recursive_ignored_files_for_init(
        session,
        source_root,
        worktree_path,
        ignored_snapshot_root,
    )
}

pub fn start_worktree_async_pnpm_install(
    session: &mut Session<'_>,
    worktree_path: &Path,
    partial_success_prefix: &str,
) -> AppResult<AsyncPnpmInstall> {
    start_async_pnpm_install(session, worktree_path, partial_success_prefix)
}

fn copy_recursive_ignored_files_for_init(
    session: &mut Session<'_>,
    source_root: &Path,
    worktree_path: &Path,
    ignored_snapshot_root: Option<&Path>,
) -> AppResult<()> {
    let copied_files = copied_ignored_files(session, source_root)?;
    let (ignored_files, snapshot_root) = match ignored_snapshot_root {
        Some(snapshot_root) => match snapshot_recursive_ignored_files_from_root(snapshot_root) {
            Ok(ignored_files) => (ignored_files, Some(snapshot_root)),
            Err(error) => {
                return match remove_recursive_ignored_file_snapshot_root(snapshot_root) {
                    Ok(()) => Err(error),
                    Err(cleanup_error) => {
                        Err(Error::message(format!("{error}; also {cleanup_error}")))
                    }
                };
            }
        },
        None => (
            { snapshot_copy_pattern_files(session, source_root, &copied_files)? },
            None,
        ),
    };
    let copy_result = copy_snapshot_files(&ignored_files, worktree_path)
        .map_err(|error| Error::message(format!("ignored file copy failed: {error}")))
        .and_then(|copied| print_copied_files(session, &copied_files, copied));
    let cleanup_result = snapshot_root.map_or(Ok(()), remove_recursive_ignored_file_snapshot_root);
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
    if auxiliary::read_auxiliary_marker(&session.git, &target)?.is_some() {
        return Err(Error::message(
            "remove is not supported for worktrees with auxiliary state",
        ));
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

    let copied_files = copied_ignored_files(session, &repo.main_root)?;
    let ignored_files = snapshot_copy_pattern_files(session, &repo.main_root, &copied_files)?;

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
    print_copied_files(
        session,
        &copied_files,
        copy_snapshot_files(&ignored_files, &path).map_err(|error| {
            Error::message(format!(
                "main worktree switched to {base} and linked worktree created, but ignored file copy failed: {error}"
            ))
        })?,
    )?;
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
    if auxiliary::read_auxiliary_marker(&Git, primary_worktree)?.is_some() {
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
        return current_branch_name(session, &repo.current_root);
    }
    prepare_base(session, repo, &opts.base)
}

fn current_branch_name(session: &mut Session<'_>, repo_root: &Path) -> AppResult<String> {
    let current = session
        .git
        .run(repo_root, ["branch", "--show-current"])?
        .stdout;
    let current = current.trim();
    if current.is_empty() {
        return Err(Error::message(
            "--from-current requires the current worktree to be on a named branch",
        ));
    }
    Ok(current.to_string())
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

fn copied_ignored_files(session: &Session<'_>, main_root: &Path) -> AppResult<CopiedIgnoredFiles> {
    let repo = resolve(&session.git, main_root)?;
    let config = auxiliary::load_effective_config(&repo.main_root, &repo.git_common_dir)?;
    let patterns = config.copy.unwrap_or_default();
    for pattern in &patterns {
        validate_copy_pattern(pattern)?;
    }
    let matcher = CopyPatternMatcher::new(&patterns)?;
    let pathspecs = copy_pattern_pathspecs(&patterns);
    Ok(CopiedIgnoredFiles {
        patterns,
        matcher,
        pathspecs,
    })
}

fn validate_copy_pattern(pattern: &str) -> AppResult<()> {
    if pattern.is_empty() {
        return Err(Error::message("copy entries must not be empty"));
    }
    if pattern.starts_with('!') {
        return Err(Error::message(format!(
            "copy entries do not support negation patterns: {pattern}"
        )));
    }
    if pattern.starts_with('/') || Path::new(pattern).is_absolute() {
        return Err(Error::message(format!(
            "copy entries must be relative patterns: {pattern}"
        )));
    }
    if Path::new(pattern).components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(Error::message(format!(
            "copy entries must not traverse outside the repository: {pattern}"
        )));
    }
    Ok(())
}

fn snapshot_copy_pattern_files(
    session: &Session<'_>,
    main_root: &Path,
    copied_files: &CopiedIgnoredFiles,
) -> AppResult<Vec<SnapshotFile>> {
    let mut ignored = Vec::new();
    for relative in copy_pattern_ignored_files(session, main_root, copied_files)? {
        if let Some(snapshot) = snapshot_file(main_root, relative)? {
            ignored.push(snapshot);
        }
    }
    Ok(dedupe_snapshot_files(ignored))
}

pub fn snapshot_send_out_worktree_init(
    session: &Session<'_>,
    main_root: &Path,
) -> AppResult<SendOutWorktreeInit> {
    let copied_files = copied_ignored_files(session, main_root)?;
    Ok(SendOutWorktreeInit {
        ignored_files: snapshot_copy_pattern_files(session, main_root, &copied_files)?,
    })
}

pub fn apply_send_out_worktree_init(
    session: &mut Session<'_>,
    worktree_path: &Path,
    init: &SendOutWorktreeInit,
) -> AppResult<()> {
    let copied_files = copied_ignored_files(session, worktree_path)?;
    print_copied_files(
        session,
        &copied_files,
        copy_snapshot_files(&init.ignored_files, worktree_path)
            .map_err(|error| Error::message(format!("ignored file copy failed: {error}")))?,
    )?;
    Ok(())
}

fn snapshot_recursive_ignored_files_from_root(root: &Path) -> AppResult<Vec<SnapshotFile>> {
    let mut ignored = Vec::new();
    collect_recursive_ignored_files_from_root(root, root, &mut ignored)?;
    ignored.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(ignored)
}

fn collect_recursive_ignored_files_from_root(
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
            collect_recursive_ignored_files_from_root(root, &path, ignored)?;
            continue;
        }

        if path.file_name().and_then(|name| name.to_str())
            == Some(RECURSIVE_IGNORED_FILE_SNAPSHOT_MARKER)
        {
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

fn copy_pattern_ignored_files(
    session: &Session<'_>,
    main_root: &Path,
    copied_files: &CopiedIgnoredFiles,
) -> AppResult<Vec<PathBuf>> {
    if copied_files.patterns.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = vec![
        "ls-files".to_string(),
        "--others".to_string(),
        "--ignored".to_string(),
        "--exclude-standard".to_string(),
        "--full-name".to_string(),
        "-z".to_string(),
        "--".to_string(),
    ];
    args.extend(copied_files.pathspecs.iter().cloned());
    let output = session.git.run_bytes(main_root, args)?;
    let mut ignored: Vec<_> = output
        .stdout
        .split(|byte| *byte == b'\0')
        .filter(|path| !path.is_empty())
        .map(path_buf_from_git_bytes)
        .filter(|path| copied_files.matcher.is_match(path))
        .collect();
    ignored.sort();
    ignored.dedup();
    Ok(ignored)
}

#[derive(Debug, Clone)]
struct CopyPatternMatcher {
    globset: GlobSet,
    basename_globset: GlobSet,
    directory_globset: GlobSet,
    descendant_globset: GlobSet,
}

impl CopyPatternMatcher {
    fn new(patterns: &[String]) -> AppResult<Self> {
        let mut glob_builder = GlobSetBuilder::new();
        let mut basename_builder = GlobSetBuilder::new();
        let mut directory_builder = GlobSetBuilder::new();
        let mut descendant_builder = GlobSetBuilder::new();
        for pattern in patterns {
            if pattern.ends_with('/') {
                for directory_pattern in copy_pattern_directory_patterns(pattern) {
                    directory_builder.add(Glob::new(&directory_pattern).map_err(|error| {
                        Error::message(format!("invalid copy pattern {pattern:?}: {error}"))
                    })?);
                }
                continue;
            }
            glob_builder.add(Glob::new(pattern).map_err(|error| {
                Error::message(format!("invalid copy pattern {pattern:?}: {error}"))
            })?);
            if let Some(root_pattern) = pattern.strip_prefix("**/") {
                glob_builder.add(Glob::new(root_pattern).map_err(|error| {
                    Error::message(format!("invalid copy pattern {pattern:?}: {error}"))
                })?);
            }
            if !pattern.contains('/') {
                basename_builder.add(Glob::new(pattern).map_err(|error| {
                    Error::message(format!("invalid copy pattern {pattern:?}: {error}"))
                })?);
            }
            for directory_pattern in copy_pattern_descendant_patterns(pattern) {
                descendant_builder.add(Glob::new(&directory_pattern).map_err(|error| {
                    Error::message(format!("invalid copy pattern {pattern:?}: {error}"))
                })?);
            }
        }
        Ok(Self {
            globset: glob_builder.build().map_err(|error| {
                Error::message(format!("failed to build copy pattern matcher: {error}"))
            })?,
            basename_globset: basename_builder.build().map_err(|error| {
                Error::message(format!("failed to build copy pattern matcher: {error}"))
            })?,
            directory_globset: directory_builder.build().map_err(|error| {
                Error::message(format!("failed to build copy pattern matcher: {error}"))
            })?,
            descendant_globset: descendant_builder.build().map_err(|error| {
                Error::message(format!("failed to build copy pattern matcher: {error}"))
            })?,
        })
    }

    fn is_match(&self, path: &Path) -> bool {
        self.globset.is_match(path)
            || path
                .file_name()
                .is_some_and(|name| self.basename_globset.is_match(Path::new(name)))
            || self.directory_globset.is_match(path)
            || self.descendant_globset.is_match(path)
    }
}

fn copy_pattern_pathspecs(patterns: &[String]) -> Vec<String> {
    let mut pathspecs = Vec::new();
    for pattern in patterns {
        if pattern.ends_with('/') {
            for directory_pattern in copy_pattern_directory_patterns(pattern) {
                pathspecs.push(git_glob_pathspec(&directory_pattern));
            }
            continue;
        }
        pathspecs.push(git_glob_pathspec(pattern));
        if let Some(root_pattern) = pattern.strip_prefix("**/") {
            pathspecs.push(git_glob_pathspec(root_pattern));
        }
        if !pattern.contains('/') {
            pathspecs.push(git_glob_pathspec(&format!("**/{pattern}")));
        }
        for directory_pattern in copy_pattern_descendant_patterns(pattern) {
            pathspecs.push(git_glob_pathspec(&directory_pattern));
        }
    }
    pathspecs.sort();
    pathspecs.dedup();
    pathspecs
}

fn git_glob_pathspec(pattern: &str) -> String {
    format!(":(glob){pattern}")
}

fn copy_pattern_directory_patterns(pattern: &str) -> Vec<String> {
    let trimmed = pattern.trim_end_matches('/');
    copy_pattern_descendant_patterns(trimmed)
}

fn copy_pattern_descendant_patterns(pattern: &str) -> Vec<String> {
    let mut patterns = vec![format!("{pattern}/**")];
    if !pattern.contains('/') {
        patterns.push(format!("**/{pattern}/**"));
    } else if let Some(root_pattern) = pattern.strip_prefix("**/") {
        patterns.push(format!("{root_pattern}/**"));
    }
    patterns
}

#[cfg(unix)]
fn path_buf_from_git_bytes(path: &[u8]) -> PathBuf {
    PathBuf::from(OsString::from_vec(path.to_vec()))
}

#[cfg(not(unix))]
fn path_buf_from_git_bytes(path: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(path).into_owned())
}

fn print_copied_files(
    session: &mut Session<'_>,
    _copied_files: &CopiedIgnoredFiles,
    copied: Vec<PathBuf>,
) -> AppResult<()> {
    if !copied.is_empty() {
        print_copied_file_count(session, copied.len())?;
    }
    Ok(())
}

fn print_copied_file_count(session: &mut Session<'_>, count: usize) -> AppResult<()> {
    if count > 0 {
        writeln!(session.out, "copied {count} ignored files")?;
    }
    Ok(())
}

fn start_async_init_worktree(
    session: &mut Session<'_>,
    source_root: &Path,
    worktree_path: &Path,
    ignored_snapshot_root: &Path,
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
        .arg(ignored_snapshot_root)
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

fn write_recursive_ignored_file_snapshot(
    ignored_files: &[SnapshotFile],
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
        RECURSIVE_IGNORED_FILE_SNAPSHOT_PREFIX,
        std::process::id(),
        worktree_name,
        nonce
    ));
    fs::create_dir_all(&snapshot_root).map_err(|error| {
        Error::message(format!(
            "worktree created, but failed to create ignored recursive file snapshot {}: {error}",
            snapshot_root.display()
        ))
    })?;
    write_recursive_ignored_file_snapshot_marker(&snapshot_root)?;
    cleanup_recursive_ignored_file_snapshot_on_error(
        copy_snapshot_files(ignored_files, &snapshot_root)
            .map_err(|error| {
                Error::message(format!(
                    "worktree created, but failed to snapshot ignored files in {}: {error}",
                    snapshot_root.display()
                ))
            })
            .map(|_| ()),
        &snapshot_root,
    )?;
    Ok(snapshot_root)
}

fn dedupe_snapshot_files(files: Vec<SnapshotFile>) -> Vec<SnapshotFile> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::with_capacity(files.len());
    for file in files {
        if seen.insert(file.relative.clone()) {
            deduped.push(file);
        }
    }
    deduped
}

fn write_recursive_ignored_file_snapshot_marker(snapshot_root: &Path) -> AppResult<()> {
    fs::write(
        snapshot_root.join(RECURSIVE_IGNORED_FILE_SNAPSHOT_MARKER),
        b"managed by wtk\n",
    )
    .map_err(|error| {
        Error::message(format!(
            "worktree created, but failed to mark ignored recursive file snapshot {}: {error}",
            snapshot_root.display()
        ))
    })
}

fn remove_recursive_ignored_file_snapshot_root(snapshot_root: &Path) -> AppResult<()> {
    validate_recursive_ignored_file_snapshot_root(snapshot_root)?;
    fs::remove_dir_all(snapshot_root).map_err(|error| {
        Error::message(format!(
            "failed to remove ignored recursive file snapshot {}: {error}",
            snapshot_root.display()
        ))
    })
}

fn validate_recursive_ignored_file_snapshot_root(snapshot_root: &Path) -> AppResult<()> {
    let file_name = snapshot_root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            Error::message(format!(
                "refusing to remove unmanaged ignored recursive file snapshot {}",
                snapshot_root.display()
            ))
        })?;
    let expected_parent = std::env::temp_dir();
    if snapshot_root.parent() != Some(expected_parent.as_path())
        || !file_name.starts_with(RECURSIVE_IGNORED_FILE_SNAPSHOT_PREFIX)
        || !snapshot_root
            .join(RECURSIVE_IGNORED_FILE_SNAPSHOT_MARKER)
            .is_file()
    {
        return Err(Error::message(format!(
            "refusing to remove unmanaged ignored recursive file snapshot {}",
            snapshot_root.display()
        )));
    }
    Ok(())
}

fn cleanup_recursive_ignored_file_snapshot_on_error(
    result: AppResult<()>,
    snapshot_root: &Path,
) -> AppResult<()> {
    match result {
        Ok(()) => Ok(()),
        Err(error) => match remove_recursive_ignored_file_snapshot_root(snapshot_root) {
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
        async_init_stdio, async_pnpm_install_stdio,
        cleanup_recursive_ignored_file_snapshot_on_error, finish, open_async_init_log,
        remove_recursive_ignored_file_snapshot_root, should_run_pnpm_install,
        write_recursive_ignored_file_snapshot_marker,
    };
    use crate::clipboard::ClipboardProvider;
    use crate::{AppResult, Error};
    use std::io;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

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
    fn cleanup_recursive_ignored_file_snapshot_on_error_removes_snapshot_root() {
        let snapshot_root = std::env::temp_dir().join(format!(
            "{}{}-{}",
            super::RECURSIVE_IGNORED_FILE_SNAPSHOT_PREFIX,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&snapshot_root).unwrap();
        write_recursive_ignored_file_snapshot_marker(&snapshot_root).unwrap();
        std::fs::write(snapshot_root.join(".env"), "SECRET=value\n").unwrap();

        let error = cleanup_recursive_ignored_file_snapshot_on_error(
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
    fn remove_recursive_ignored_file_snapshot_root_rejects_unmanaged_paths() {
        let snapshot_root = std::env::temp_dir().join(format!(
            "{}{}-{}",
            super::RECURSIVE_IGNORED_FILE_SNAPSHOT_PREFIX,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&snapshot_root).unwrap();
        std::fs::write(snapshot_root.join(".env"), "SECRET=value\n").unwrap();

        let error = remove_recursive_ignored_file_snapshot_root(&snapshot_root)
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

    #[test]
    fn copy_pattern_ignored_files_short_circuits_when_patterns_are_empty() {
        let mut out = io::sink();
        let mut clipboard = FailingClipboard;
        let session = super::Session::new(PathBuf::from("."), &mut out, &mut clipboard, false);

        let copied_files = super::CopiedIgnoredFiles {
            patterns: Vec::new(),
            matcher: super::CopyPatternMatcher::new(&[]).unwrap(),
            pathspecs: Vec::new(),
        };
        let ignored = super::copy_pattern_ignored_files(
            &session,
            Path::new("missing-directory"),
            &copied_files,
        )
        .expect("empty copy patterns should not invoke git");

        assert!(ignored.is_empty());
    }

    #[test]
    fn copy_pattern_matcher_treats_slashless_patterns_as_directory_basenames() {
        let matcher = super::CopyPatternMatcher::new(&["secrets".to_string()]).unwrap();

        assert!(matcher.is_match(Path::new("secrets")));
        assert!(matcher.is_match(Path::new("config/secrets")));
        assert!(matcher.is_match(Path::new("secrets/token")));
        assert!(matcher.is_match(Path::new("config/secrets/token")));
        assert!(!matcher.is_match(Path::new("config/secret/token")));
    }

    #[test]
    fn copy_pattern_matcher_preserves_descendants_for_slash_patterns_without_trailing_slash() {
        let matcher = super::CopyPatternMatcher::new(&["specs/change/active".to_string()]).unwrap();

        assert!(matcher.is_match(Path::new("specs/change/active")));
        assert!(matcher.is_match(Path::new("specs/change/active/plan.md")));
        assert!(!matcher.is_match(Path::new("specs/change/inactive/plan.md")));
    }

    #[test]
    fn copy_pattern_matcher_treats_trailing_slash_patterns_as_nested_directories() {
        let matcher = super::CopyPatternMatcher::new(&[".agents/".to_string()]).unwrap();

        assert!(matcher.is_match(Path::new(".agents/instructions.md")));
        assert!(matcher.is_match(Path::new("nested/.agents/instructions.md")));
        assert!(!matcher.is_match(Path::new("nested/agents/instructions.md")));
    }

    #[test]
    fn copy_pattern_pathspecs_include_descendants_for_slashless_and_directory_patterns() {
        let pathspecs = super::copy_pattern_pathspecs(&[
            "secrets".to_string(),
            ".agents/".to_string(),
            "specs/change/active".to_string(),
            "**/.env".to_string(),
        ]);

        assert!(pathspecs.contains(&":(glob)secrets".to_string()));
        assert!(pathspecs.contains(&":(glob)**/secrets".to_string()));
        assert!(pathspecs.contains(&":(glob)secrets/**".to_string()));
        assert!(pathspecs.contains(&":(glob)**/secrets/**".to_string()));
        assert!(pathspecs.contains(&":(glob).agents/**".to_string()));
        assert!(pathspecs.contains(&":(glob)**/.agents/**".to_string()));
        assert!(pathspecs.contains(&":(glob)specs/change/active".to_string()));
        assert!(pathspecs.contains(&":(glob)specs/change/active/**".to_string()));
        assert!(pathspecs.contains(&":(glob)**/.env".to_string()));
        assert!(pathspecs.contains(&":(glob).env".to_string()));
        assert!(pathspecs.contains(&":(glob)**/.env/**".to_string()));
        assert!(pathspecs.contains(&":(glob).env/**".to_string()));
    }

    #[test]
    fn validate_copy_pattern_rejects_negation() {
        let error = super::validate_copy_pattern("!secrets/public/**")
            .expect_err("negated copy patterns should fail fast");

        assert!(
            error
                .to_string()
                .contains("copy entries do not support negation patterns")
        );
    }
}
