use crate::gitexec::{Git, RepoContext, absolute_path, resolve, same_path};
use crate::{AppResult, Error};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub auxiliaries: BTreeMap<String, AuxiliaryRefConfig>,
    #[serde(default)]
    pub groups: BTreeMap<String, AuxiliaryGroupConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxiliaryRefConfig {
    pub repository: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxiliaryGroupConfig {
    pub auxiliaries: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AuxiliarySelection {
    pub name: String,
    pub repository: PathBuf,
    pub repo: RepoContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreesState {
    pub version: u32,
    #[serde(default)]
    pub worktrees: BTreeMap<PathBuf, WorktreeEntry>,
}

impl Default for WorktreesState {
    fn default() -> Self {
        WorktreesState {
            version: 1,
            worktrees: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeEntry {
    pub branch: String,
    pub auxiliaries: BTreeMap<String, AuxiliaryWorktreeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxiliaryWorktreeState {
    pub repository: PathBuf,
    pub worktree: PathBuf,
}

#[derive(Debug, Clone)]
pub struct AuxiliaryRefStatus {
    pub name: String,
    pub expected_target: PathBuf,
    pub current_target: PathBuf,
}

pub fn add_group(
    git: &Git,
    primary_root: &Path,
    group_name: &str,
    repository_paths: &[PathBuf],
) -> AppResult<()> {
    validate_name(group_name, "auxiliary group name")?;
    if repository_paths.is_empty() {
        return Err(Error::message(
            "auxiliary-group add requires at least one repository path",
        ));
    }

    let config_path = config_path(primary_root);
    let mut config = read_config_or_default(&config_path)?;
    if config.groups.contains_key(group_name) {
        return Err(Error::message(format!(
            "auxiliary group already exists: {group_name}"
        )));
    }

    let mut seen_repositories = Vec::<PathBuf>::new();
    let mut seen_names = BTreeMap::<String, PathBuf>::new();
    let mut group_refs = Vec::new();
    for input in repository_paths {
        let repo = resolve(git, &absolute_path(input))?;
        if !same_path(&repo.current_root, &repo.main_root) {
            return Err(Error::message(format!(
                "auxiliary repository path must resolve to a main worktree: {}",
                input.display()
            )));
        }
        if seen_repositories
            .iter()
            .any(|repository| same_path(repository, &repo.main_root))
        {
            return Err(Error::message(format!(
                "duplicate auxiliary repository: {}",
                repo.main_root.display()
            )));
        }
        let name = repository_basename(&repo.main_root)?;
        validate_name(&name, "auxiliary repository ref name")?;
        if let Some(existing) = seen_names.get(&name) {
            if !same_path(existing, &repo.main_root) {
                return Err(Error::message(format!(
                    "auxiliary repository ref name {name} is ambiguous between {} and {}",
                    existing.display(),
                    repo.main_root.display()
                )));
            }
        }
        if let Some(existing) = config.auxiliaries.get(&name) {
            let existing_repo = resolve(git, &existing.repository)?;
            if !same_path(&existing_repo.main_root, &repo.main_root) {
                return Err(Error::message(format!(
                    "auxiliary ref {name} already points to {}, not {}",
                    existing_repo.main_root.display(),
                    repo.main_root.display()
                )));
            }
        } else {
            config.auxiliaries.insert(
                name.clone(),
                AuxiliaryRefConfig {
                    repository: repo.main_root.clone(),
                },
            );
        }
        seen_repositories.push(repo.main_root.clone());
        seen_names.insert(name.clone(), repo.main_root);
        group_refs.push(name);
    }

    config.groups.insert(
        group_name.to_string(),
        AuxiliaryGroupConfig {
            auxiliaries: group_refs,
        },
    );
    write_config(&config_path, &config)
}

pub fn expand_groups(
    git: &Git,
    primary_root: &Path,
    group_names: &[String],
) -> AppResult<Vec<AuxiliarySelection>> {
    if group_names.is_empty() {
        return Ok(Vec::new());
    }
    let config_path = config_path(primary_root);
    let config = read_config(&config_path)?;
    let mut selected_group_names = BTreeSet::new();
    let mut by_repository = Vec::<AuxiliarySelection>::new();
    let mut by_name = BTreeMap::<String, PathBuf>::new();

    for group_name in group_names {
        if !selected_group_names.insert(group_name.clone()) {
            continue;
        }
        let group = config
            .groups
            .get(group_name)
            .ok_or_else(|| Error::message(format!("unknown auxiliary group: {group_name}")))?;
        if group.auxiliaries.is_empty() {
            return Err(Error::message(format!(
                "auxiliary group has no auxiliaries: {group_name}"
            )));
        }
        for auxiliary_name in &group.auxiliaries {
            let auxiliary = config.auxiliaries.get(auxiliary_name).ok_or_else(|| {
                Error::message(format!(
                    "auxiliary group {group_name} references missing auxiliary ref: {auxiliary_name}"
                ))
            })?;
            validate_name(auxiliary_name, "auxiliary repository ref name")?;
            let configured = require_absolute(&auxiliary.repository, "auxiliary repository path")?;
            let basename = repository_basename(&configured)?;
            if basename != *auxiliary_name {
                return Err(Error::message(format!(
                    "auxiliary ref {auxiliary_name} must match repository basename {basename}"
                )));
            }
            let repo = resolve(git, &configured)?;
            if !same_path(&repo.main_root, &configured) {
                return Err(Error::message(format!(
                    "configured auxiliary repository does not resolve to its main worktree: {}",
                    configured.display()
                )));
            }
            if by_repository
                .iter()
                .any(|selected| same_path(&selected.repository, &repo.main_root))
            {
                continue;
            }
            if let Some(existing) = by_name.get(auxiliary_name) {
                if !same_path(existing, &repo.main_root) {
                    return Err(Error::message(format!(
                        "auxiliary ref name {auxiliary_name} resolves to multiple repositories"
                    )));
                }
            }
            by_name.insert(auxiliary_name.clone(), repo.main_root.clone());
            by_repository.push(AuxiliarySelection {
                name: auxiliary_name.clone(),
                repository: repo.main_root.clone(),
                repo,
            });
        }
    }
    Ok(by_repository)
}

pub fn read_state(primary_root: &Path) -> AppResult<WorktreesState> {
    let path = state_path(primary_root);
    if !path.exists() {
        return Ok(WorktreesState::default());
    }
    let text = fs::read_to_string(&path).map_err(|error| {
        Error::message(format!(
            "failed to read worktree state {}: {}",
            path.display(),
            error
        ))
    })?;
    let state: WorktreesState = serde_json::from_str(&text).map_err(|error| {
        Error::message(format!(
            "failed to parse worktree state {}: {}",
            path.display(),
            error
        ))
    })?;
    if state.version != 1 {
        return Err(Error::message(format!(
            "unsupported worktree state version {} in {}",
            state.version,
            path.display()
        )));
    }
    Ok(state)
}

pub fn write_state(primary_root: &Path, state: &WorktreesState) -> AppResult<()> {
    let path = state_path(primary_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(state)
        .map_err(|error| Error::message(format!("failed to serialize worktree state: {error}")))?;
    fs::write(&path, format!("{text}\n")).map_err(|error| {
        Error::message(format!(
            "failed to write worktree state {}: {}",
            path.display(),
            error
        ))
    })
}

pub fn worktree_entry<'a>(
    state: &'a WorktreesState,
    primary_worktree: &Path,
) -> Option<&'a WorktreeEntry> {
    let key = absolute_path(primary_worktree);
    state
        .worktrees
        .iter()
        .find(|(path, _)| same_path(path, &key))
        .map(|(_, entry)| entry)
}

pub fn remove_worktree_entry(
    state: &mut WorktreesState,
    primary_worktree: &Path,
) -> Option<WorktreeEntry> {
    let key = absolute_path(primary_worktree);
    let found = state
        .worktrees
        .keys()
        .find(|path| same_path(path, &key))
        .cloned()?;
    state.worktrees.remove(&found)
}

pub fn validate_refs(
    git: &Git,
    primary_worktree: &Path,
    entry: &WorktreeEntry,
) -> AppResult<Vec<AuxiliaryRefStatus>> {
    let mut refs = Vec::new();
    for (name, auxiliary) in &entry.auxiliaries {
        let ref_path = primary_worktree.join("refs").join(name);
        let current = read_ref(&ref_path)?;
        let current = require_absolute(&current, "Auxiliary Ref target")?;
        if !same_path(&current, &auxiliary.worktree) {
            return Err(Error::message(format!(
                "Auxiliary Ref {name} points to {}, expected {}",
                current.display(),
                auxiliary.worktree.display()
            )));
        }
        validate_worktree_branch(git, name, &entry.branch, auxiliary)?;
        refs.push(AuxiliaryRefStatus {
            name: name.clone(),
            expected_target: auxiliary.worktree.clone(),
            current_target: current,
        });
    }
    Ok(refs)
}

pub fn validate_worktree_branch(
    git: &Git,
    name: &str,
    expected_branch: &str,
    auxiliary: &AuxiliaryWorktreeState,
) -> AppResult<()> {
    let repo = resolve(git, &auxiliary.worktree)?;
    if !same_path(&repo.current_root, &auxiliary.worktree) {
        return Err(Error::message(format!(
            "Auxiliary worktree {name} resolved to {}, expected {}",
            repo.current_root.display(),
            auxiliary.worktree.display()
        )));
    }
    let worktree = repo.worktree_by_path(&auxiliary.worktree).ok_or_else(|| {
        Error::message(format!(
            "Auxiliary worktree {name} is missing from git worktree list at {}",
            auxiliary.repository.display()
        ))
    })?;
    if worktree.detached {
        return Err(Error::message(format!(
            "Auxiliary worktree {name} at {} is detached, expected branch {}",
            auxiliary.worktree.display(),
            expected_branch
        )));
    }
    if worktree.branch != expected_branch {
        return Err(Error::message(format!(
            "Auxiliary worktree {name} at {} is on branch {}, expected {}",
            auxiliary.worktree.display(),
            worktree.branch,
            expected_branch
        )));
    }
    Ok(())
}

pub fn write_ref(path: &Path, target: &Path) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if path.exists() || fs::symlink_metadata(path).is_ok() {
        fs::remove_file(path)?;
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, path)?;
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, path)?;
    }
    Ok(())
}

pub fn read_ref(path: &Path) -> AppResult<PathBuf> {
    fs::read_link(path).map_err(|error| {
        Error::message(format!(
            "failed to read Auxiliary Ref {}: {}",
            path.display(),
            error
        ))
    })
}

pub fn state_path(primary_root: &Path) -> PathBuf {
    primary_root.join(".wtk").join("worktrees.json")
}

fn config_path(primary_root: &Path) -> PathBuf {
    primary_root.join(".wtk").join("config.toml")
}

fn read_config_or_default(path: &Path) -> AppResult<Config> {
    if path.exists() {
        read_config(path)
    } else {
        Ok(Config::default())
    }
}

fn read_config(path: &Path) -> AppResult<Config> {
    let text = fs::read_to_string(path).map_err(|error| {
        Error::message(format!(
            "failed to read WTK config {}: {}",
            path.display(),
            error
        ))
    })?;
    toml::from_str(&text).map_err(|error| {
        Error::message(format!(
            "failed to parse WTK config {}: {}",
            path.display(),
            error
        ))
    })
}

fn write_config(path: &Path, config: &Config) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(config)
        .map_err(|error| Error::message(format!("failed to serialize WTK config: {error}")))?;
    fs::write(path, text).map_err(|error| {
        Error::message(format!(
            "failed to write WTK config {}: {}",
            path.display(),
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
                "repository path has no final segment: {}",
                path.display()
            ))
        })
}

fn validate_name(name: &str, label: &str) -> AppResult<()> {
    if name.is_empty() || name == "." || name == ".." {
        return Err(Error::message(format!("{label} is invalid: {name:?}")));
    }
    let path = Path::new(name);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(Error::message(format!("{label} is invalid: {name:?}")));
    }
    Ok(())
}
