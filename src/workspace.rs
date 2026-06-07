use crate::gitexec::{Git, RepoContext, Worktree, is_git_exit, resolve, same_path};
use crate::output;
use crate::paths::default_path;
use crate::worktree::{Options, Session};
use crate::{AppResult, Error};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs as unix_fs;
#[cfg(windows)]
use std::os::windows::fs as windows_fs;
use std::path::{Path, PathBuf};

const CONFIG_DIR: &str = ".wtk";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Repository,
    Workspace,
}

#[derive(Debug, Clone)]
pub struct WorkspaceContext {
    root: PathBuf,
    config_path: PathBuf,
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
    target: PathBuf,
    repo: RepoContext,
}

#[derive(Serialize)]
struct WorkspaceStatusOutput {
    mode: &'static str,
    workspace_root: PathBuf,
    config: PathBuf,
    refs: Vec<WorkspaceStatusRef>,
}

#[derive(Serialize)]
struct WorkspaceStatusRef {
    name: String,
    ref_path: PathBuf,
    repository: PathBuf,
    current_target: PathBuf,
    expected_path: Option<PathBuf>,
    branch: String,
    is_main: bool,
}

enum RollbackAction {
    RestoreRef { path: PathBuf, target: PathBuf },
    RemoveWorktree { repo: PathBuf, path: PathBuf },
    DeleteBranch { repo: PathBuf, branch: String },
    SwitchMain { repo: PathBuf, branch: String },
}

pub fn resolve_mode(git: &Git, cwd: &Path) -> AppResult<Mode> {
    Ok(match find_workspace_config(cwd)? {
        Some(path) => {
            let config = read_config(&path)?;
            match config.mode.as_str() {
                "workspace" => Mode::Workspace,
                "repository" => Mode::Repository,
                other => return Err(Error::message(format!("unsupported wtk mode: {other}"))),
            }
        }
        None => {
            let _ = git;
            Mode::Repository
        }
    })
}

pub fn init(session: &mut Session<'_>) -> AppResult<()> {
    let root = workspace_root(&session.git, &session.cwd)?;
    let config_dir = root.join(CONFIG_DIR);
    let config_path = config_dir.join(CONFIG_FILE);
    if config_path.exists() {
        return Err(Error::message(format!(
            "workspace config already exists: {}",
            config_path.display()
        )));
    }
    fs::create_dir_all(&config_dir)?;
    fs::create_dir_all(root.join("refs"))?;
    let config = WorkspaceConfig {
        mode: "workspace".to_string(),
        workspace: WorkspaceSection::default(),
    };
    write_config(&config_path, &config)?;
    writeln!(
        session.out,
        "initialized Workspace Mode at {}",
        root.display()
    )?;
    Ok(())
}

pub fn add(session: &mut Session<'_>, repository_path: &Path) -> AppResult<()> {
    let mut ctx = load_workspace(&session.git, &session.cwd)?;
    let refs = load_refs(&session.git, &ctx)?;
    for entry in &refs {
        if !same_path(&entry.target, &entry.repo.main_root) {
            return Err(Error::message(format!(
                "workspace add requires existing ref {} to point at its repository main worktree",
                entry.name
            )));
        }
    }

    let repository = strict_absolute(repository_path)?;
    let repo = resolve(&session.git, &repository)?;
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
    write_config(&ctx.config_path, &ctx.config)?;
    let ref_path = ctx.root.join("refs").join(&name);
    write_ref(&ref_path, &repo.main_root)?;
    writeln!(
        session.out,
        "added Workspace Ref {name} -> {}",
        repo.main_root.display()
    )?;
    Ok(())
}

pub fn status(session: &mut Session<'_>) -> AppResult<()> {
    let ctx = load_workspace(&session.git, &session.cwd)?;
    let refs = load_refs(&session.git, &ctx)?;
    let payload = WorkspaceStatusOutput {
        mode: "workspace",
        workspace_root: ctx.root.clone(),
        config: ctx.config_path.clone(),
        refs: refs
            .iter()
            .map(|entry| {
                let worktree = entry.repo.worktree_by_path(&entry.target);
                let branch = worktree.map(|wt| wt.branch.clone()).unwrap_or_default();
                let is_main = same_path(&entry.target, &entry.repo.main_root);
                WorkspaceStatusRef {
                    name: entry.name.clone(),
                    ref_path: entry.ref_path.clone(),
                    repository: entry.repository.clone(),
                    current_target: entry.target.clone(),
                    expected_path: if is_main || branch.is_empty() {
                        None
                    } else {
                        Some(default_path(&entry.repo.main_root, &branch))
                    },
                    branch,
                    is_main,
                }
            })
            .collect(),
    };
    serde_yaml::to_writer(&mut *session.out, &payload)
        .map_err(|error| Error::message(format!("failed to serialize status as YAML: {error}")))?;
    writeln!(session.out)?;
    Ok(())
}

pub fn new(session: &mut Session<'_>, opts: Options) -> AppResult<()> {
    if opts.branch.is_empty() {
        return Err(Error::message("branch is required"));
    }
    require_no_path(&opts)?;
    let ctx = load_workspace(&session.git, &session.cwd)?;
    let refs = load_refs(&session.git, &ctx)?;
    require_refs(&refs)?;
    let mut plan = Vec::new();
    for entry in refs {
        if branch_exists(&session.git, &entry.repo.main_root, &opts.branch)? {
            return Err(Error::message(format!(
                "branch already exists in {}: {}",
                entry.name, opts.branch
            )));
        }
        let path = default_path(&entry.repo.main_root, &opts.branch);
        ensure_creatable(&path)?;
        plan.push((entry, path));
    }

    let base = if opts.base.is_empty() {
        "HEAD"
    } else {
        &opts.base
    };
    let mut rollback = Vec::new();
    for (entry, path) in &plan {
        let args = vec![
            "worktree".to_string(),
            "add".to_string(),
            "-b".to_string(),
            opts.branch.clone(),
            path.display().to_string(),
            base.to_string(),
        ];
        output::git(session.out, &entry.repo.main_root, &args)?;
        if let Err(error) = session
            .git
            .run(&entry.repo.main_root, args.iter().map(String::as_str))
        {
            rollback_all(&session.git, session.out, rollback)?;
            return Err(error);
        }
        rollback.push(RollbackAction::DeleteBranch {
            repo: entry.repo.main_root.clone(),
            branch: opts.branch.clone(),
        });
        rollback.push(RollbackAction::RemoveWorktree {
            repo: entry.repo.main_root.clone(),
            path: path.clone(),
        });
    }
    for (entry, path) in &plan {
        rollback.push(RollbackAction::RestoreRef {
            path: entry.ref_path.clone(),
            target: entry.target.clone(),
        });
        write_ref(&entry.ref_path, path)?;
    }
    writeln!(
        session.out,
        "created workspace worktrees for {}",
        opts.branch
    )?;
    Ok(())
}

pub fn remove(session: &mut Session<'_>, opts: Options) -> AppResult<()> {
    let ctx = load_workspace(&session.git, &session.cwd)?;
    let refs = load_refs(&session.git, &ctx)?;
    require_refs(&refs)?;
    let branch_or_path = opts.path;
    let mut plan = Vec::new();
    for entry in refs {
        let worktree = workspace_remove_target(&entry, &branch_or_path)?;
        require_clean(&session.git, &worktree.path)?;
        plan.push((entry, worktree));
    }
    let mut rollback = Vec::new();
    for (entry, worktree) in &plan {
        rollback.push(RollbackAction::RestoreRef {
            path: entry.ref_path.clone(),
            target: entry.target.clone(),
        });
        write_ref(&entry.ref_path, &entry.repo.main_root)?;
        let args = vec![
            "worktree".to_string(),
            "remove".to_string(),
            worktree.path.display().to_string(),
        ];
        output::git(session.out, &entry.repo.main_root, &args)?;
        if let Err(error) = session
            .git
            .run(&entry.repo.main_root, args.iter().map(String::as_str))
        {
            rollback_all(&session.git, session.out, rollback)?;
            return Err(error);
        }
        rollback.pop();
        if opts.delete_branch && !worktree.branch.is_empty() {
            let branch_args = vec![
                "branch".to_string(),
                "-d".to_string(),
                worktree.branch.clone(),
            ];
            output::git(session.out, &entry.repo.main_root, &branch_args)?;
            if let Err(error) = session.git.run(
                &entry.repo.main_root,
                branch_args.iter().map(String::as_str),
            ) {
                return Err(error);
            }
        }
    }
    writeln!(session.out, "removed workspace worktrees")?;
    Ok(())
}

pub fn send_out(session: &mut Session<'_>, opts: Options) -> AppResult<()> {
    require_no_path(&opts)?;
    let ctx = load_workspace(&session.git, &session.cwd)?;
    let refs = load_refs(&session.git, &ctx)?;
    require_refs(&refs)?;
    let mut plan = Vec::new();
    for entry in refs {
        if !same_path(&entry.target, &entry.repo.main_root) {
            return Err(Error::message(format!(
                "send-out requires ref {} to point at its repository main worktree",
                entry.name
            )));
        }
        require_clean(&session.git, &entry.repo.main_root)?;
        let branch = current_branch(&session.git, &entry.repo.main_root)?;
        if branch.is_empty() {
            return Err(Error::message(format!(
                "send-out requires a named branch in {}",
                entry.name
            )));
        }
        let base = if opts.base.is_empty() {
            detect_main_branch(&session.git, &entry.repo)?
        } else {
            opts.base.clone()
        };
        if branch == base {
            return Err(Error::message(format!(
                "current branch {branch:?} is the base branch; no task branch to send out"
            )));
        }
        let path = default_path(&entry.repo.main_root, &branch);
        ensure_creatable(&path)?;
        plan.push((entry, branch, base, path));
    }
    let mut rollback = Vec::new();
    for (entry, branch, base, path) in &plan {
        let switch_args = vec!["switch".to_string(), base.clone()];
        output::git(session.out, &entry.repo.main_root, &switch_args)?;
        if let Err(error) = session.git.run(
            &entry.repo.main_root,
            switch_args.iter().map(String::as_str),
        ) {
            rollback_all(&session.git, session.out, rollback)?;
            return Err(error);
        }
        rollback.push(RollbackAction::SwitchMain {
            repo: entry.repo.main_root.clone(),
            branch: branch.clone(),
        });
        let add_args = vec![
            "worktree".to_string(),
            "add".to_string(),
            path.display().to_string(),
            branch.clone(),
        ];
        output::git(session.out, &entry.repo.main_root, &add_args)?;
        if let Err(error) = session
            .git
            .run(&entry.repo.main_root, add_args.iter().map(String::as_str))
        {
            rollback_all(&session.git, session.out, rollback)?;
            return Err(error);
        }
        rollback.push(RollbackAction::RemoveWorktree {
            repo: entry.repo.main_root.clone(),
            path: path.clone(),
        });
    }
    for (entry, _, _, path) in &plan {
        rollback.push(RollbackAction::RestoreRef {
            path: entry.ref_path.clone(),
            target: entry.target.clone(),
        });
        write_ref(&entry.ref_path, path)?;
    }
    writeln!(session.out, "sent workspace branches out")?;
    Ok(())
}

pub fn bring_in(session: &mut Session<'_>, opts: Options) -> AppResult<()> {
    if opts.branch.is_empty() {
        return Err(Error::message("branch is required"));
    }
    let ctx = load_workspace(&session.git, &session.cwd)?;
    let refs = load_refs(&session.git, &ctx)?;
    require_refs(&refs)?;
    let mut plan = Vec::new();
    for entry in refs {
        require_clean(&session.git, &entry.repo.main_root)?;
        let worktree = if same_path(&entry.target, &entry.repo.main_root) {
            entry
                .repo
                .worktrees
                .iter()
                .find(|wt| wt.branch == opts.branch && !same_path(&wt.path, &entry.repo.main_root))
                .cloned()
        } else {
            entry
                .repo
                .worktree_by_path(&entry.target)
                .filter(|wt| wt.branch == opts.branch)
                .cloned()
        }
        .ok_or_else(|| {
            Error::message(format!(
                "branch is not checked out in the surfaced linked worktree for {}: {}",
                entry.name, opts.branch
            ))
        })?;
        require_clean(&session.git, &worktree.path)?;
        plan.push((entry, worktree));
    }
    for (entry, worktree) in &plan {
        let remove_args = vec![
            "worktree".to_string(),
            "remove".to_string(),
            worktree.path.display().to_string(),
        ];
        output::git(session.out, &entry.repo.main_root, &remove_args)?;
        session.git.run(
            &entry.repo.main_root,
            remove_args.iter().map(String::as_str),
        )?;
        let switch_args = vec!["switch".to_string(), opts.branch.clone()];
        output::git(session.out, &entry.repo.main_root, &switch_args)?;
        session.git.run(
            &entry.repo.main_root,
            switch_args.iter().map(String::as_str),
        )?;
        write_ref(&entry.ref_path, &entry.repo.main_root)?;
    }
    writeln!(session.out, "brought workspace branches in")?;
    Ok(())
}

fn load_workspace(git: &Git, cwd: &Path) -> AppResult<WorkspaceContext> {
    let config_path = find_workspace_config(cwd)?.ok_or_else(|| {
        Error::message("Workspace Mode requires .wtk/config.toml in the workspace")
    })?;
    let config = read_config(&config_path)?;
    if config.mode != "workspace" {
        return Err(Error::message(format!(
            "expected mode = \"workspace\", found {:?}",
            config.mode
        )));
    }
    let root = config_path
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| Error::message(format!("invalid config path: {}", config_path.display())))?
        .to_path_buf();
    let _ = git;
    Ok(WorkspaceContext {
        root,
        config_path,
        config,
    })
}

fn load_refs(git: &Git, ctx: &WorkspaceContext) -> AppResult<Vec<WorkspaceRef>> {
    let mut entries = Vec::new();
    for (name, config) in &ctx.config.workspace.refs {
        let repository = require_absolute(&config.repository, "repository path")?;
        let basename = repository_basename(&repository)?;
        if basename != *name {
            return Err(Error::message(format!(
                "workspace ref {name} must match repository basename {basename}"
            )));
        }
        let ref_path = ctx.root.join("refs").join(name);
        let target = read_ref(&ref_path)?;
        let target = require_absolute(&target, "Workspace Ref target")?;
        let repo = resolve(git, &repository)?;
        if !same_path(&repo.main_root, &repository) {
            return Err(Error::message(format!(
                "configured repository does not resolve to its main worktree: {}",
                repository.display()
            )));
        }
        if repo.worktree_by_path(&target).is_none() {
            return Err(Error::message(format!(
                "Workspace Ref {} target is not a worktree in repository {}: {}",
                name,
                repository.display(),
                target.display()
            )));
        }
        entries.push(WorkspaceRef {
            name: name.clone(),
            ref_path,
            repository,
            target,
            repo,
        });
    }
    Ok(entries)
}

fn find_workspace_config(cwd: &Path) -> AppResult<Option<PathBuf>> {
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
    loop {
        let candidate = current.join(CONFIG_DIR).join(CONFIG_FILE);
        if candidate.exists() {
            return Ok(Some(candidate));
        }
        if !current.pop() {
            return Ok(None);
        }
    }
}

fn workspace_root(git: &Git, cwd: &Path) -> AppResult<PathBuf> {
    resolve(git, cwd).map(|repo| repo.current_root)
}

fn read_config(path: &Path) -> AppResult<WorkspaceConfig> {
    let text = fs::read_to_string(path).map_err(|error| {
        Error::message(format!(
            "failed to read workspace config {}: {}",
            path.display(),
            error
        ))
    })?;
    toml::from_str(&text).map_err(|error| {
        Error::message(format!(
            "failed to parse workspace config {}: {}",
            path.display(),
            error
        ))
    })
}

fn write_config(path: &Path, config: &WorkspaceConfig) -> AppResult<()> {
    let text = toml::to_string_pretty(config).map_err(|error| {
        Error::message(format!("failed to serialize workspace config: {error}"))
    })?;
    fs::write(path, text)?;
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

fn require_clean(git: &Git, dir: &Path) -> AppResult<()> {
    let status = git.run(
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

fn current_branch(git: &Git, repo: &Path) -> AppResult<String> {
    Ok(git
        .run(repo, ["branch", "--show-current"])?
        .stdout
        .trim()
        .to_string())
}

fn detect_main_branch(git: &Git, repo: &RepoContext) -> AppResult<String> {
    match git.run(
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
    let found = ["main", "master", "trunk", "develop"]
        .into_iter()
        .filter_map(
            |candidate| match branch_exists(git, &repo.main_root, candidate) {
                Ok(true) => Some(Ok(candidate.to_string())),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            },
        )
        .collect::<AppResult<Vec<_>>>()?;
    if found.len() == 1 {
        Ok(found[0].clone())
    } else {
        Err(Error::message(
            "cannot determine main branch; pass --base or run git config worktree-kit.mainBranch <branch>",
        ))
    }
}

fn workspace_remove_target(entry: &WorkspaceRef, branch_or_path: &str) -> AppResult<Worktree> {
    let candidate = if branch_or_path.is_empty() {
        entry.target.clone()
    } else {
        PathBuf::from(branch_or_path)
    };
    let worktree = entry
        .repo
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
        .ok_or_else(|| {
            Error::message(format!(
                "target is not a linked worktree for {}",
                entry.name
            ))
        })?;
    if same_path(&worktree.path, &entry.repo.main_root) {
        return Err(Error::message(format!(
            "target is not a linked worktree for {}: {}",
            entry.name,
            worktree.path.display()
        )));
    }
    Ok(worktree)
}

fn rollback_all(git: &Git, out: &mut dyn Write, actions: Vec<RollbackAction>) -> AppResult<()> {
    let mut failures = Vec::new();
    for action in actions.into_iter().rev() {
        let result = match action {
            RollbackAction::RestoreRef { path, target } => write_ref(&path, &target),
            RollbackAction::RemoveWorktree { repo, path } => {
                output::git(
                    out,
                    &repo,
                    &[
                        "worktree".into(),
                        "remove".into(),
                        path.display().to_string(),
                    ],
                )?;
                git.run(
                    &repo,
                    ["worktree", "remove", path.to_str().unwrap_or_default()],
                )
                .map(|_| ())
            }
            RollbackAction::DeleteBranch { repo, branch } => {
                output::git(out, &repo, &["branch".into(), "-D".into(), branch.clone()])?;
                git.run(&repo, ["branch", "-D", &branch]).map(|_| ())
            }
            RollbackAction::SwitchMain { repo, branch } => {
                output::git(out, &repo, &["switch".into(), branch.clone()])?;
                git.run(&repo, ["switch", &branch]).map(|_| ())
            }
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
