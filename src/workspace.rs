use crate::gitexec::{Git, RepoContext, Worktree, is_git_exit, resolve, same_path};
use crate::list::{self, ListOptions, ListOutput, WorkspaceRefDetail, WorkspaceRefSummary};
use crate::output;
use crate::paths::default_path;
use crate::worktree::{self, Options, Session};
use crate::{AppResult, Error};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs as unix_fs;
#[cfg(windows)]
use std::os::windows::fs as windows_fs;

const MANIFEST_FILE: &str = ".wtk-workspace.toml";
const GITIGNORE_FILE: &str = ".gitignore";
const AGENTS_FILE: &str = "AGENTS.md";
const WORKSPACE_GITIGNORE: &str = "refs/\n";
const WORKSPACE_AGENTS_TEMPLATE: &str = include_str!("templates/workspace/AGENTS.md");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Repository,
    Workspace,
}

#[derive(Debug, Clone)]
pub struct WorkspaceContext {
    repo: RepoContext,
    root: PathBuf,
    manifest_path: PathBuf,
    config: WorkspaceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkspaceConfig {
    mode: String,
    #[serde(default)]
    workspace: WorkspaceSection,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct WorkspaceSection {
    #[serde(default)]
    refs: BTreeMap<String, WorkspaceRefConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkspaceRefConfig {
    repository: PathBuf,
}

#[derive(Debug, Clone)]
struct WorkspaceRef {
    name: String,
    ref_path: PathBuf,
    repository: PathBuf,
    repo: RepoContext,
    expected_target: PathBuf,
    current_target: PathBuf,
}

#[derive(Debug, Clone)]
struct WorkspacePlan {
    repo_root: PathBuf,
    source_root: PathBuf,
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct WorkspaceStatusOutput {
    mode: &'static str,
    workspace_worktree: PathBuf,
    workspace_main_worktree: PathBuf,
    workspace_branch: String,
    current_is_main: bool,
    manifest: PathBuf,
    refs: Vec<WorkspaceStatusRef>,
}

#[derive(Debug, Clone, Serialize)]
struct WorkspaceStatusRef {
    name: String,
    ref_path: PathBuf,
    repository: PathBuf,
    current_target: PathBuf,
    expected_target: PathBuf,
    branch: String,
    is_main: bool,
}

#[derive(Debug, Clone)]
enum RollbackAction {
    RemoveWorktree {
        repo: PathBuf,
        path: PathBuf,
    },
    DeleteBranch {
        repo: PathBuf,
        branch: String,
    },
    CreateBranch {
        repo: PathBuf,
        branch: String,
        head: String,
    },
    RestoreWorktree {
        repo: PathBuf,
        path: PathBuf,
        branch: String,
        env_files: Vec<worktree::SnapshotFile>,
    },
    WriteRef {
        path: PathBuf,
        target: PathBuf,
    },
}

pub fn resolve_mode(git: &Git, cwd: &Path) -> AppResult<Mode> {
    Ok(match find_workspace_manifest(git, cwd)? {
        Some(path) => {
            let config = read_config(&path)?;
            match config.mode.as_str() {
                "workspace" => Mode::Workspace,
                other => {
                    return Err(Error::message(format!(
                        "invalid workspace manifest mode {:?} in {}",
                        other,
                        path.display()
                    )));
                }
            }
        }
        None => Mode::Repository,
    })
}

pub fn init(session: &mut Session<'_>) -> AppResult<()> {
    let repo = resolve(&session.git, &session.cwd)?;
    require_main_worktree(&repo, "workspace init")?;
    let manifest_path = repo.main_root.join(MANIFEST_FILE);
    if manifest_path.exists() {
        return Err(Error::message(format!(
            "workspace manifest already exists: {}",
            manifest_path.display()
        )));
    }
    initialize_workspace_files(&repo.main_root, WorkspaceSection::default())?;
    output::success(
        session.out,
        &format!("initialized Workspace Mode at {}", repo.main_root.display()),
    )?;
    Ok(())
}

pub fn add(session: &mut Session<'_>, repository_path: &Path) -> AppResult<()> {
    let mut ctx = load_workspace(&session.git, &session.cwd)?;
    require_main_worktree(&ctx.repo, "workspace add")?;
    let workspace_branch = current_branch(&session.git, &ctx.repo.current_root)?;
    if workspace_branch.is_empty() {
        return Err(Error::message(
            "Workspace Mode requires the Workspace main worktree to be on a named branch",
        ));
    }

    let repository = strict_absolute(repository_path)?;
    let repo = resolve(&session.git, &repository)?;
    let repository_branch = current_branch(&session.git, &repo.main_root)?;
    if repository_branch != workspace_branch {
        return Err(Error::message(format!(
            "workspace add requires the linked repository main worktree branch to match the Workspace branch: expected {workspace_branch}, found {repository_branch} in {}",
            repo.main_root.display()
        )));
    }
    let name = repository_basename(&repo.main_root)?;
    if ctx.config.workspace.refs.contains_key(&name) {
        return Err(Error::message(format!(
            "workspace ref already exists: {name}"
        )));
    }
    ctx.config.workspace.refs.insert(
        name.clone(),
        WorkspaceRefConfig {
            repository: repo.main_root.clone(),
        },
    );
    let desired_targets = ctx
        .config
        .workspace
        .refs
        .iter()
        .map(|(ref_name, ref_config)| {
            Ok((
                ref_name.clone(),
                resolve(&session.git, &ref_config.repository)?.main_root,
            ))
        })
        .collect::<AppResult<Vec<_>>>()?;
    let previous_manifest = fs::read_to_string(&ctx.manifest_path)?;
    let previous_refs = desired_targets
        .iter()
        .filter(|(ref_name, _)| ref_name != &name)
        .map(|(ref_name, _)| {
            let ref_path = ctx.root.join("refs").join(ref_name);
            let previous = if ref_path.exists() || fs::symlink_metadata(&ref_path).is_ok() {
                Some(read_ref(&ref_path)?)
            } else {
                None
            };
            Ok((ref_path, previous))
        })
        .collect::<AppResult<Vec<_>>>()?;

    write_config(&ctx.manifest_path, &ctx.config)?;
    for (ref_name, target) in &desired_targets {
        let ref_path = ctx.root.join("refs").join(ref_name);
        if let Err(error) = write_ref(&ref_path, target) {
            let _ = fs::write(&ctx.manifest_path, &previous_manifest);
            for (path, previous) in &previous_refs {
                let _ = restore_ref_state(path, previous.as_ref());
            }
            let _ = restore_ref_state(&ctx.root.join("refs").join(&name), None);
            return Err(error);
        }
    }
    output::success(
        session.out,
        &format!("added Workspace Ref {name} -> {}", repo.main_root.display()),
    )?;
    Ok(())
}

pub fn bootstrap(session: &mut Session<'_>, repository_paths: &[PathBuf]) -> AppResult<()> {
    if repository_paths.is_empty() {
        return Err(Error::message(
            "workspace bootstrap requires at least one repository path",
        ));
    }

    let mut entries = fs::read_dir(&session.cwd)?;
    if entries.next().transpose()?.is_some() {
        return Err(Error::message(
            "workspace bootstrap requires an empty directory before initialization",
        ));
    }

    let refs = bootstrap_refs(&session.git, repository_paths)?;

    session.git.run(&session.cwd, ["init", "-b", "main"])?;
    let repo = resolve(&session.git, &session.cwd)?;
    require_main_worktree(&repo, "workspace bootstrap")?;
    let workspace_branch = current_branch(&session.git, &repo.main_root)?;
    if workspace_branch != "main" {
        return Err(Error::message(format!(
            "workspace bootstrap requires the Workspace main worktree branch to be main, found {workspace_branch}"
        )));
    }

    initialize_workspace_files(&repo.main_root, WorkspaceSection { refs })?;
    write_workspace_bootstrap_files(&repo.main_root)?;
    let ctx = load_workspace_at_root(&session.git, &repo.main_root)?;
    write_workspace_refs(&session.git, &ctx)?;
    session.git.run(
        &repo.main_root,
        ["add", MANIFEST_FILE, GITIGNORE_FILE, AGENTS_FILE],
    )?;
    session
        .git
        .run(&repo.main_root, ["commit", "-m", "Initialize workspace"])?;

    output::success(
        session.out,
        &format!(
            "bootstrapped Workspace at {} with {} refs",
            repo.main_root.display(),
            repository_paths.len()
        ),
    )?;
    Ok(())
}

pub fn status(session: &mut Session<'_>) -> AppResult<()> {
    let ctx = load_workspace(&session.git, &session.cwd)?;
    let workspace_branch = current_branch(&session.git, &ctx.repo.current_root)?;
    if workspace_branch.is_empty() {
        return Err(Error::message(
            "Workspace Mode requires the current Workspace Worktree to be on a named branch",
        ));
    }
    let refs = load_workspace_refs(&session.git, &ctx, &workspace_branch, true)?;
    let payload = WorkspaceStatusOutput {
        mode: "workspace",
        workspace_worktree: ctx.repo.current_root.clone(),
        workspace_main_worktree: ctx.repo.main_root.clone(),
        workspace_branch: workspace_branch.clone(),
        current_is_main: ctx.repo.current_is_main,
        manifest: ctx.manifest_path.clone(),
        refs: refs
            .into_iter()
            .map(|entry| {
                Ok(WorkspaceStatusRef {
                    name: entry.name,
                    ref_path: entry.ref_path,
                    repository: entry.repository,
                    current_target: entry.current_target,
                    expected_target: entry.expected_target.clone(),
                    branch: branch_for_target(&session.git, &entry.repo, &entry.expected_target)?,
                    is_main: same_path(&entry.expected_target, &entry.repo.main_root),
                })
            })
            .collect::<AppResult<Vec<_>>>()?,
    };
    serde_yaml::to_writer(&mut *session.out, &payload)
        .map_err(|error| Error::message(format!("failed to serialize status as YAML: {error}")))?;
    writeln!(session.out)?;
    Ok(())
}

pub fn list(session: &mut Session<'_>, options: ListOptions) -> AppResult<()> {
    let ctx = load_workspace(&session.git, &session.cwd)?;
    let payload = workspace_list_output(&session.git, &ctx)?;
    list::render(
        session.out,
        &payload,
        options,
        output::Style::new(session.style_enabled && !options.json),
    )
}

fn workspace_list_output(git: &Git, ctx: &WorkspaceContext) -> AppResult<ListOutput> {
    let workspace_main_branch = current_branch(git, &ctx.repo.main_root)?;
    let mut rows = ctx
        .repo
        .worktrees
        .iter()
        .map(|worktree| {
            let mut row = list::repository_row_ignoring_workspace_refs(git, &ctx.repo, worktree);
            row.kind = "workspace_worktree";
            row.workspace_refs = Some(workspace_ref_summary(
                git,
                ctx,
                &worktree.path,
                &worktree.branch,
                &workspace_main_branch,
            ));
            row
        })
        .collect::<Vec<_>>();
    rows = list::sorted_rows(rows);
    Ok(ListOutput {
        mode: "workspace",
        worktrees: rows,
    })
}

fn workspace_ref_summary(
    git: &Git,
    ctx: &WorkspaceContext,
    workspace_worktree: &Path,
    workspace_branch: &str,
    workspace_main_branch: &str,
) -> WorkspaceRefSummary {
    let details = ctx
        .config
        .workspace
        .refs
        .iter()
        .map(|(name, config)| {
            workspace_ref_detail(
                git,
                name,
                config,
                workspace_worktree,
                workspace_branch,
                workspace_main_branch,
            )
        })
        .collect::<Vec<_>>();
    let ok = details.iter().filter(|detail| detail.ok).count();
    let total = details.len();
    WorkspaceRefSummary {
        total,
        ok,
        broken: total.saturating_sub(ok),
        details,
    }
}

fn workspace_ref_detail(
    git: &Git,
    name: &str,
    config: &WorkspaceRefConfig,
    workspace_worktree: &Path,
    workspace_branch: &str,
    workspace_main_branch: &str,
) -> WorkspaceRefDetail {
    let mut diagnostics = Vec::new();
    let (repo, expected_target) = match require_absolute(&config.repository, "repository path")
        .and_then(|repository| {
            let basename = repository_basename(&repository)?;
            if basename != *name {
                return Err(Error::message(format!(
                    "workspace ref {name} must match repository basename {basename}"
                )));
            }
            let repo = resolve(git, &repository)?;
            Ok((repository, repo))
        }) {
        Ok((repository, repo)) => {
            if !same_path(&repo.main_root, &repository) {
                diagnostics.push(format!(
                    "configured repository does not resolve to its main worktree: {}",
                    repository.display()
                ));
            }
            let expected_target =
                expected_target_for_branch(&repo, workspace_branch, workspace_main_branch);
            (Some(repo), expected_target)
        }
        Err(error) => {
            diagnostics.push(error.to_string());
            (None, config.repository.clone())
        }
    };
    let ref_path = workspace_worktree.join("refs").join(name);
    let current_target = match read_ref(&ref_path) {
        Ok(target) => Some(target),
        Err(error) => {
            diagnostics.push(format!("failed to read Workspace Ref {name}: {error}"));
            None
        }
    };

    if let Some(repo) = &repo {
        match repo.worktree_by_path(&expected_target) {
            None => diagnostics.push(format!(
                "expected Repository Worktree is missing: {}",
                expected_target.display()
            )),
            Some(expected_worktree) if expected_worktree.branch != workspace_branch => {
                diagnostics.push(format!(
                    "expected Repository Worktree branch mismatch: expected {workspace_branch}, found {}",
                    expected_worktree.branch
                ));
            }
            Some(_) => {}
        }
    }
    if let Some(current_target) = &current_target {
        if !same_path(current_target, &expected_target) {
            diagnostics.push(format!(
                "Workspace Ref points to {}, expected {}",
                current_target.display(),
                expected_target.display()
            ));
        }
    }

    WorkspaceRefDetail {
        name: name.to_string(),
        ok: diagnostics.is_empty(),
        expected_target,
        current_target,
        diagnostics,
    }
}

pub fn new(session: &mut Session<'_>, opts: Options) -> AppResult<()> {
    if opts.branch.is_empty() {
        return Err(Error::message("branch is required"));
    }
    require_no_path(&opts)?;

    let ctx = load_workspace(&session.git, &session.cwd)?;
    let workspace_branch = current_branch(&session.git, &ctx.repo.current_root)?;
    if workspace_branch.is_empty() {
        return Err(Error::message(
            "Workspace Mode requires the current Workspace Worktree to be on a named branch",
        ));
    }
    let refs = load_workspace_refs(&session.git, &ctx, &workspace_branch, true)?;
    require_refs(&refs)?;
    require_manifest_committed(&session.git, &ctx.root)?;

    let workspace_path = default_path(&ctx.repo.main_root, &opts.branch);
    if branch_exists(&session.git, &ctx.repo.main_root, &opts.branch)? {
        return Err(Error::message(format!(
            "workspace branch already exists: {}",
            opts.branch
        )));
    }
    ensure_creatable(&workspace_path)?;

    let mut linked_plans = Vec::new();
    for entry in &refs {
        if branch_exists(&session.git, &entry.repo.main_root, &opts.branch)? {
            return Err(Error::message(format!(
                "branch already exists in {}: {}",
                entry.name, opts.branch
            )));
        }
        let path = default_path(&entry.repo.main_root, &opts.branch);
        ensure_creatable(&path)?;
        linked_plans.push(WorkspacePlan {
            repo_root: entry.repo.main_root.clone(),
            source_root: entry.current_target.clone(),
            path,
        });
    }

    let base = if opts.base.is_empty() {
        workspace_branch.as_str()
    } else {
        opts.base.as_str()
    };
    let mut rollback = Vec::new();

    create_worktree(
        &session.git,
        session.out,
        &ctx.repo.main_root,
        &workspace_path,
        &opts.branch,
        base,
    )?;
    rollback.push(RollbackAction::DeleteBranch {
        repo: ctx.repo.main_root.clone(),
        branch: opts.branch.clone(),
    });
    rollback.push(RollbackAction::RemoveWorktree {
        repo: ctx.repo.main_root.clone(),
        path: workspace_path.clone(),
    });

    for plan in &linked_plans {
        if let Err(error) = create_worktree(
            &session.git,
            session.out,
            &plan.repo_root,
            &plan.path,
            &opts.branch,
            base,
        ) {
            rollback_all(&session.git, session.out, rollback)?;
            return Err(error);
        }
        rollback.push(RollbackAction::DeleteBranch {
            repo: plan.repo_root.clone(),
            branch: opts.branch.clone(),
        });
        rollback.push(RollbackAction::RemoveWorktree {
            repo: plan.repo_root.clone(),
            path: plan.path.clone(),
        });
    }

    for entry in &refs {
        let path = workspace_path.join("refs").join(&entry.name);
        let target = default_path(&entry.repo.main_root, &opts.branch);
        if let Err(error) = write_ref(&path, &target) {
            rollback_all(&session.git, session.out, rollback)?;
            return Err(error);
        }
    }

    for plan in &linked_plans {
        if let Err(error) = worktree::init_worktree(session, &plan.source_root, &plan.path, None) {
            rollback_all(&session.git, session.out, rollback)?;
            return Err(error);
        }
    }

    worktree::finish(
        session,
        opts.no_clipboard,
        workspace_path.display().to_string(),
        format!("created workspace worktree at {}", workspace_path.display()),
    )
}

pub fn remove(session: &mut Session<'_>, opts: Options) -> AppResult<()> {
    let ctx = load_workspace(&session.git, &session.cwd)?;
    let target = workspace_remove_target(&ctx.repo, &opts.path)?;
    require_clean(&session.git, &target.path, true)?;
    let branch = target.branch.clone();
    if branch.is_empty() {
        return Err(Error::message(
            "Workspace Mode remove requires a named branch in the target Workspace Worktree",
        ));
    }

    let target_ctx = load_workspace_at_root(&session.git, &target.path)?;
    let refs = load_workspace_refs(&session.git, &target_ctx, &branch, true)?;
    require_refs(&refs)?;

    let linked_targets = refs
        .iter()
        .map(|entry| {
            required_worktree_at_expected_target(&entry.name, &entry.repo, &entry.expected_target)
        })
        .collect::<AppResult<Vec<_>>>()?;
    let linked_env_snapshots = linked_targets
        .iter()
        .map(|linked| worktree::snapshot_dot_env_files_from_root(&linked.path))
        .collect::<AppResult<Vec<_>>>()?;
    let target_env_files = worktree::snapshot_dot_env_files_from_root(&target.path)?;

    let mut rollback = Vec::new();
    for linked in &linked_targets {
        require_clean(&session.git, &linked.path, false)?;
    }

    for ((entry, linked), env_files) in refs.iter().zip(&linked_targets).zip(&linked_env_snapshots)
    {
        if let Err(error) = remove_worktree(
            &session.git,
            session.out,
            &entry.repo.main_root,
            &linked.path,
        ) {
            rollback_all(&session.git, session.out, rollback)?;
            return Err(error);
        }
        rollback.push(RollbackAction::RestoreWorktree {
            repo: entry.repo.main_root.clone(),
            path: linked.path.clone(),
            branch: linked.branch.clone(),
            env_files: env_files.clone(),
        });
    }
    if let Err(error) =
        remove_worktree(&session.git, session.out, &ctx.repo.main_root, &target.path)
    {
        rollback_all(&session.git, session.out, rollback)?;
        return Err(error);
    }
    for entry in &refs {
        rollback.push(RollbackAction::WriteRef {
            path: target.path.join("refs").join(&entry.name),
            target: entry.expected_target.clone(),
        });
    }
    rollback.push(RollbackAction::RestoreWorktree {
        repo: ctx.repo.main_root.clone(),
        path: target.path.clone(),
        branch: target.branch.clone(),
        env_files: target_env_files,
    });

    if opts.delete_branch {
        for (entry, linked) in refs.iter().zip(&linked_targets) {
            if let Err(error) =
                delete_branch(&session.git, session.out, &entry.repo.main_root, &branch)
            {
                rollback_all(&session.git, session.out, rollback)?;
                return Err(error);
            }
            rollback.push(RollbackAction::CreateBranch {
                repo: entry.repo.main_root.clone(),
                branch: linked.branch.clone(),
                head: linked.head.clone(),
            });
        }
        if let Err(error) = delete_branch(&session.git, session.out, &ctx.repo.main_root, &branch) {
            rollback_all(&session.git, session.out, rollback)?;
            return Err(error);
        }
        rollback.push(RollbackAction::CreateBranch {
            repo: ctx.repo.main_root.clone(),
            branch: target.branch.clone(),
            head: target.head.clone(),
        });
    }

    worktree::finish(
        session,
        opts.no_clipboard,
        target.path.display().to_string(),
        format!("removed workspace worktree {}", target.path.display()),
    )
}

fn load_workspace(git: &Git, cwd: &Path) -> AppResult<WorkspaceContext> {
    let manifest_path = find_workspace_manifest(git, cwd)?.ok_or_else(|| {
        Error::message("Workspace Mode requires .wtk-workspace.toml in the workspace")
    })?;
    load_workspace_at_root(
        git,
        manifest_path.parent().ok_or_else(|| {
            Error::message(format!(
                "invalid manifest path: {}",
                manifest_path.display()
            ))
        })?,
    )
}

fn load_workspace_at_root(git: &Git, root: &Path) -> AppResult<WorkspaceContext> {
    let manifest_path = root.join(MANIFEST_FILE);
    let config = read_config(&manifest_path)?;
    if config.mode != "workspace" {
        return Err(Error::message(format!(
            "expected mode = \"workspace\", found {:?}",
            config.mode
        )));
    }
    let root = root.to_path_buf();
    let repo = resolve(git, &root)?;
    Ok(WorkspaceContext {
        repo: repo.clone(),
        root: repo.current_root,
        manifest_path,
        config,
    })
}

fn load_workspace_refs(
    git: &Git,
    ctx: &WorkspaceContext,
    workspace_branch: &str,
    validate_current_ref: bool,
) -> AppResult<Vec<WorkspaceRef>> {
    let workspace_main_branch = current_branch(git, &ctx.repo.main_root)?;
    if workspace_main_branch.is_empty() {
        return Err(Error::message(format!(
            "Workspace main worktree must be on a named branch: {}",
            ctx.repo.main_root.display()
        )));
    }
    let mut entries = Vec::new();
    for (name, config) in &ctx.config.workspace.refs {
        let repository = require_absolute(&config.repository, "repository path")?;
        let basename = repository_basename(&repository)?;
        if basename != *name {
            return Err(Error::message(format!(
                "workspace ref {name} must match repository basename {basename}"
            )));
        }
        let repo = resolve(git, &repository)?;
        if !same_path(&repo.main_root, &repository) {
            return Err(Error::message(format!(
                "configured repository does not resolve to its main worktree: {}",
                repository.display()
            )));
        }
        let expected_target =
            expected_target_for_branch(&repo, workspace_branch, &workspace_main_branch);
        let expected_worktree =
            required_worktree_at_expected_target(name, &repo, &expected_target)?;
        if expected_worktree.branch != workspace_branch {
            return Err(Error::message(format!(
                "Workspace Ref {name} target branch mismatch: expected {workspace_branch}, found {}",
                expected_worktree.branch
            )));
        }
        let ref_path = ctx.root.join("refs").join(name);
        let current_target = if validate_current_ref {
            let current_target = read_ref(&ref_path)?;
            let current_target = require_absolute(&current_target, "Workspace Ref target")?;
            if !same_path(&current_target, &expected_target) {
                return Err(Error::message(format!(
                    "Workspace Ref {name} points to {}, expected {}",
                    current_target.display(),
                    expected_target.display()
                )));
            }
            current_target
        } else {
            expected_target.clone()
        };
        entries.push(WorkspaceRef {
            name: name.clone(),
            ref_path,
            repository,
            repo,
            expected_target,
            current_target,
        });
    }
    Ok(entries)
}

fn find_workspace_manifest(git: &Git, cwd: &Path) -> AppResult<Option<PathBuf>> {
    let repo = match resolve(git, cwd) {
        Ok(repo) => repo,
        Err(_) => return Ok(None),
    };
    let start = if cwd.is_absolute() {
        cwd.to_path_buf()
    } else {
        std::env::current_dir()?.join(cwd)
    };
    let mut current = if start.is_dir() {
        start
    } else {
        start
            .parent()
            .ok_or_else(|| {
                Error::message(format!("invalid current directory: {}", start.display()))
            })?
            .to_path_buf()
    };
    let repo_root = repo.current_root;
    loop {
        let candidate = current.join(MANIFEST_FILE);
        if candidate.exists() {
            return Ok(Some(candidate));
        }
        if same_path(&current, &repo_root) {
            return Ok(None);
        }
        if !current.pop() {
            return Ok(None);
        }
    }
}

fn read_config(path: &Path) -> AppResult<WorkspaceConfig> {
    let text = fs::read_to_string(path).map_err(|error| {
        Error::message(format!(
            "failed to read workspace manifest {}: {}",
            path.display(),
            error
        ))
    })?;
    toml::from_str(&text).map_err(|error| {
        Error::message(format!(
            "failed to parse workspace manifest {}: {}",
            path.display(),
            error
        ))
    })
}

fn write_config(path: &Path, config: &WorkspaceConfig) -> AppResult<()> {
    let text = toml::to_string_pretty(config).map_err(|error| {
        Error::message(format!("failed to serialize workspace manifest: {error}"))
    })?;
    fs::write(path, text)?;
    Ok(())
}

fn initialize_workspace_files(root: &Path, workspace: WorkspaceSection) -> AppResult<()> {
    write_config(
        &root.join(MANIFEST_FILE),
        &WorkspaceConfig {
            mode: "workspace".to_string(),
            workspace,
        },
    )?;
    fs::create_dir_all(root.join("refs"))?;
    Ok(())
}

fn write_workspace_bootstrap_files(root: &Path) -> AppResult<()> {
    fs::write(root.join(GITIGNORE_FILE), WORKSPACE_GITIGNORE)?;
    fs::write(root.join(AGENTS_FILE), WORKSPACE_AGENTS_TEMPLATE)?;
    Ok(())
}

fn bootstrap_refs(
    git: &Git,
    repository_paths: &[PathBuf],
) -> AppResult<BTreeMap<String, WorkspaceRefConfig>> {
    let mut refs = BTreeMap::new();
    for repository_path in repository_paths {
        let repository = strict_absolute(repository_path)?;
        let repo = resolve(git, &repository)?;
        if !same_path(&repo.main_root, &repository) {
            return Err(Error::message(format!(
                "workspace bootstrap requires linked repository paths to resolve to main worktrees: {}",
                repository.display()
            )));
        }
        let branch = current_branch(git, &repo.main_root)?;
        if branch != "main" {
            return Err(Error::message(format!(
                "workspace bootstrap requires linked repository main worktrees to be on main: found {branch} in {}",
                repo.main_root.display()
            )));
        }
        let name = repository_basename(&repo.main_root)?;
        if refs
            .insert(
                name.clone(),
                WorkspaceRefConfig {
                    repository: repo.main_root.clone(),
                },
            )
            .is_some()
        {
            return Err(Error::message(format!(
                "workspace bootstrap received duplicate Workspace Ref name: {name}"
            )));
        }
    }
    Ok(refs)
}

fn write_workspace_refs(git: &Git, ctx: &WorkspaceContext) -> AppResult<()> {
    for (ref_name, ref_config) in &ctx.config.workspace.refs {
        let target = resolve(git, &ref_config.repository)?.main_root;
        write_ref(&ctx.root.join("refs").join(ref_name), &target)?;
    }
    Ok(())
}

fn read_ref(path: &Path) -> AppResult<PathBuf> {
    fs::read_link(path).map_err(|error| {
        Error::message(format!(
            "failed to read Workspace Ref {}: {}",
            path.display(),
            error
        ))
    })
}

fn write_ref(path: &Path, target: &Path) -> AppResult<()> {
    if !target.is_absolute() {
        return Err(Error::message(format!(
            "Workspace Ref target must be absolute: {}",
            target.display()
        )));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if path.exists() || fs::symlink_metadata(path).is_ok() {
        fs::remove_file(path)?;
    }
    #[cfg(unix)]
    unix_fs::symlink(target, path)?;
    #[cfg(windows)]
    windows_fs::symlink_dir(target, path)?;
    Ok(())
}

fn strict_absolute(path: &Path) -> AppResult<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    fs::canonicalize(&absolute).map_err(|error| {
        Error::message(format!(
            "failed to resolve absolute path {}: {}",
            absolute.display(),
            error
        ))
    })
}

fn require_absolute(path: &Path, label: &str) -> AppResult<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Err(Error::message(format!(
            "{label} must be absolute: {}",
            path.display()
        )))
    }
}

fn repository_basename(path: &Path) -> AppResult<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            Error::message(format!(
                "repository path has no basename: {}",
                path.display()
            ))
        })
}

fn require_main_worktree(repo: &RepoContext, action: &str) -> AppResult<()> {
    if repo.current_is_main {
        Ok(())
    } else {
        Err(Error::message(format!(
            "{action} must be run from the Workspace main worktree"
        )))
    }
}

fn require_no_path(opts: &Options) -> AppResult<()> {
    if opts.path.is_empty() {
        Ok(())
    } else {
        Err(Error::message(
            "--path is not supported in Workspace Mode; paths are derived from the workspace branch name",
        ))
    }
}

fn require_refs(refs: &[WorkspaceRef]) -> AppResult<()> {
    if refs.is_empty() {
        Err(Error::message(
            "Workspace Mode requires at least one Workspace Ref",
        ))
    } else {
        Ok(())
    }
}

fn branch_exists(git: &Git, repo: &Path, branch: &str) -> AppResult<bool> {
    match git.run(
        repo,
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

fn ensure_creatable(path: &Path) -> AppResult<()> {
    if path.exists() {
        return Err(Error::message(format!(
            "target path already exists: {}",
            path.display()
        )));
    }
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
    Ok(())
}

fn require_clean(git: &Git, dir: &Path, ignore_workspace_refs: bool) -> AppResult<()> {
    let status = git.run(
        dir,
        ["status", "--porcelain=v1", "--untracked-files=normal"],
    )?;
    let visible = status
        .stdout
        .lines()
        .filter(|line| {
            !ignore_workspace_refs
                || !matches!(
                    line.get(3..),
                    Some(path) if path == "refs/" || path.starts_with("refs/")
                )
        })
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

fn restore_ref_state(path: &Path, target: Option<&PathBuf>) -> AppResult<()> {
    match target {
        Some(target) => write_ref(path, target),
        None => {
            if path.exists() || fs::symlink_metadata(path).is_ok() {
                fs::remove_file(path)?;
            }
            Ok(())
        }
    }
}

fn require_manifest_committed(git: &Git, workspace_root: &Path) -> AppResult<()> {
    git.run(
        workspace_root,
        ["cat-file", "-e", &format!("HEAD:{MANIFEST_FILE}")],
    )
    .map(|_| ())
    .map_err(|error| {
        if is_git_exit(&error, 128) || is_git_exit(&error, 1) {
            Error::message(format!(
                "Workspace Mode new requires {} to be committed in HEAD",
                MANIFEST_FILE
            ))
        } else {
            error
        }
    })?;

    let status = git.run(
        workspace_root,
        ["status", "--porcelain=v1", "--", MANIFEST_FILE],
    )?;
    if status.stdout.trim().is_empty() {
        Ok(())
    } else {
        Err(Error::message(format!(
            "Workspace Mode new requires committed {} changes; found:\n{}",
            MANIFEST_FILE,
            status.stdout.trim_end()
        )))
    }
}

fn current_branch(git: &Git, repo: &Path) -> AppResult<String> {
    Ok(git
        .run(repo, ["branch", "--show-current"])?
        .stdout
        .trim()
        .to_string())
}

fn expected_target_for_branch(
    repo: &RepoContext,
    workspace_branch: &str,
    workspace_main_branch: &str,
) -> PathBuf {
    if workspace_branch == workspace_main_branch {
        repo.main_root.clone()
    } else {
        default_path(&repo.main_root, workspace_branch)
    }
}

fn branch_for_target(git: &Git, repo: &RepoContext, target: &Path) -> AppResult<String> {
    if same_path(target, &repo.main_root) {
        current_branch(git, &repo.main_root)
    } else {
        repo.worktree_by_path(target)
            .map(|worktree| worktree.branch.clone())
            .ok_or_else(|| {
                Error::message(format!(
                    "target is not a worktree in repository {}: {}",
                    repo.main_root.display(),
                    target.display()
                ))
            })
    }
}

fn required_worktree_at_expected_target(
    name: &str,
    repo: &RepoContext,
    expected_target: &Path,
) -> AppResult<Worktree> {
    repo.worktree_by_path(expected_target)
        .cloned()
        .ok_or_else(|| {
            Error::message(format!(
                "Workspace Ref {name} expected Repository Worktree is missing: {}",
                expected_target.display()
            ))
        })
}

fn workspace_remove_target(repo: &RepoContext, branch_or_path: &str) -> AppResult<Worktree> {
    let candidate = if branch_or_path.is_empty() {
        repo.current_root.clone()
    } else {
        PathBuf::from(branch_or_path)
    };
    let worktree = repo
        .worktrees
        .iter()
        .find(|worktree| {
            if branch_or_path.is_empty() {
                same_path(&worktree.path, &candidate)
            } else {
                worktree.branch == branch_or_path || same_path(&worktree.path, &candidate)
            }
        })
        .cloned()
        .ok_or_else(|| Error::message("target is not a Workspace Worktree"))?;
    if same_path(&worktree.path, &repo.main_root) {
        return Err(Error::message(format!(
            "target is not a linked Workspace Worktree: {}",
            worktree.path.display()
        )));
    }
    Ok(worktree)
}

fn create_worktree(
    git: &Git,
    out: &mut dyn std::io::Write,
    repo: &Path,
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
    output::git(out, repo, &args)?;
    git.run(repo, args.iter().map(String::as_str))?;
    Ok(())
}

fn remove_worktree(
    git: &Git,
    out: &mut dyn std::io::Write,
    repo: &Path,
    path: &Path,
) -> AppResult<()> {
    let args = vec![
        "worktree".to_string(),
        "remove".to_string(),
        "--force".to_string(),
        path.display().to_string(),
    ];
    output::git(out, repo, &args)?;
    git.run(repo, args.iter().map(String::as_str))?;
    Ok(())
}

fn delete_branch(
    git: &Git,
    out: &mut dyn std::io::Write,
    repo: &Path,
    branch: &str,
) -> AppResult<()> {
    let args = vec!["branch".to_string(), "-d".to_string(), branch.to_string()];
    output::git(out, repo, &args)?;
    git.run(repo, args.iter().map(String::as_str))?;
    Ok(())
}

fn rollback_all(
    git: &Git,
    out: &mut dyn std::io::Write,
    actions: Vec<RollbackAction>,
) -> AppResult<()> {
    let mut failures = Vec::new();
    for action in actions.into_iter().rev() {
        let result = match action {
            RollbackAction::RemoveWorktree { repo, path } => {
                let args = vec![
                    "worktree".to_string(),
                    "remove".to_string(),
                    "--force".to_string(),
                    path.display().to_string(),
                ];
                output::git(out, &repo, &args)?;
                git.run(&repo, args.iter().map(String::as_str)).map(|_| ())
            }
            RollbackAction::CreateBranch { repo, branch, head } => {
                let args = vec![
                    "branch".to_string(),
                    "-f".to_string(),
                    branch.clone(),
                    head.clone(),
                ];
                output::git(out, &repo, &args)?;
                git.run(&repo, args.iter().map(String::as_str)).map(|_| ())
            }
            RollbackAction::DeleteBranch { repo, branch } => {
                let args = vec!["branch".to_string(), "-D".to_string(), branch.clone()];
                output::git(out, &repo, &args)?;
                git.run(&repo, args.iter().map(String::as_str)).map(|_| ())
            }
            RollbackAction::RestoreWorktree {
                repo,
                path,
                branch,
                env_files,
            } => {
                let args = vec![
                    "worktree".to_string(),
                    "add".to_string(),
                    path.display().to_string(),
                    branch.clone(),
                ];
                output::git(out, &repo, &args)?;
                git.run(&repo, args.iter().map(String::as_str))?;
                worktree::restore_snapshot_files_to_root(&env_files, &path)
            }
            RollbackAction::WriteRef { path, target } => write_ref(&path, &target),
        };
        if let Err(error) = result {
            failures.push(error.to_string());
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::message(format!(
            "rollback failed: {}",
            failures.join("; ")
        )))
    }
}
