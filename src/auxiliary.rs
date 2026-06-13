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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxiliaryMarker {
    pub primary_repository: PathBuf,
    pub primary_worktree: PathBuf,
    pub branch: String,
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
    git_common_dir: &Path,
    group_name: &str,
    repository_paths: &[PathBuf],
) -> AppResult<()> {
    validate_name(group_name, "auxiliary group name")?;
    if repository_paths.is_empty() {
        return Err(Error::message(
            "auxiliary-group add requires at least one repository path",
        ));
    }

    let config_path = read_config_path(primary_root, git_common_dir);
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
    write_config(&private_config_path(git_common_dir), &config)
}

pub fn expand_groups(
    git: &Git,
    primary_root: &Path,
    git_common_dir: &Path,
    group_names: &[String],
) -> AppResult<Vec<AuxiliarySelection>> {
    if group_names.is_empty() {
        return Ok(Vec::new());
    }
    let config_path = read_config_path(primary_root, git_common_dir);
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

pub fn read_state(primary_root: &Path, git_common_dir: &Path) -> AppResult<WorktreesState> {
    let path = state_path(primary_root, git_common_dir);
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

pub fn write_state(git_common_dir: &Path, state: &WorktreesState) -> AppResult<()> {
    let path = private_state_path(git_common_dir);
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

pub fn validate_primary_worktree_branch(
    worktree: &crate::gitexec::Worktree,
    expected_branch: &str,
    primary_worktree: &Path,
) -> AppResult<()> {
    if worktree.detached {
        return Err(Error::message(format!(
            "Primary worktree at {} is detached, expected branch {}",
            primary_worktree.display(),
            expected_branch
        )));
    }
    if worktree.branch != expected_branch {
        return Err(Error::message(format!(
            "Primary worktree at {} is on branch {}, expected {}",
            primary_worktree.display(),
            worktree.branch,
            expected_branch
        )));
    }
    Ok(())
}

pub fn ignored_ref_paths(entry: &WorktreeEntry) -> BTreeSet<String> {
    entry
        .auxiliaries
        .keys()
        .map(|name| format!("refs/{name}"))
        .collect()
}

pub fn status_line_ignored(line: &str, ignored: &BTreeSet<String>) -> bool {
    let Some(path) = line.get(3..) else {
        return false;
    };
    let Some(paths) = parse_status_paths(path) else {
        return false;
    };
    !paths.is_empty() && paths.iter().all(|path| ignored.contains(path))
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
    if fs::symlink_metadata(path).is_ok() {
        return Err(Error::message(format!(
            "Auxiliary Ref path already exists and will not be overwritten: {}",
            path.display()
        )));
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

pub fn install_ref_excludes(
    git: &Git,
    primary_worktree: &Path,
    entry: &WorktreeEntry,
) -> AppResult<()> {
    git.run(
        primary_worktree,
        ["config", "extensions.worktreeConfig", "true"],
    )?;
    let exclude_path = worktree_git_dir(git, primary_worktree)?
        .join("info")
        .join("exclude");
    if let Some(parent) = exclude_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut sections = Vec::new();
    if exclude_path.exists() {
        let existing = fs::read_to_string(&exclude_path).map_err(|error| {
            Error::message(format!(
                "failed to read git exclude file {}: {}",
                exclude_path.display(),
                error
            ))
        })?;
        let trimmed = existing.trim_end();
        if !trimmed.is_empty() {
            sections.push(trimmed.to_string());
        }
    }
    if let Some(inherited_path) = inherited_excludes_path(git, primary_worktree, &exclude_path)? {
        let inherited = fs::read_to_string(&inherited_path).map_err(|error| {
            Error::message(format!(
                "failed to read inherited git exclude file {}: {}",
                inherited_path.display(),
                error
            ))
        })?;
        let trimmed = inherited.trim_end();
        if !trimmed.is_empty() {
            sections.push(trimmed.to_string());
        }
    }
    let ignored = ignored_ref_paths(entry)
        .into_iter()
        .map(|path| escape_gitignore_pattern(&path))
        .collect::<Vec<_>>()
        .join("\n");
    if !ignored.is_empty() {
        sections.push(ignored);
    }
    let content = if sections.is_empty() {
        String::new()
    } else {
        format!("{}\n", sections.join("\n"))
    };
    fs::write(&exclude_path, content).map_err(|error| {
        Error::message(format!(
            "failed to write git exclude file {}: {}",
            exclude_path.display(),
            error
        ))
    })?;
    git.run(
        primary_worktree,
        [
            "config",
            "--worktree",
            "core.excludesFile",
            &exclude_path.display().to_string(),
        ],
    )?;

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

pub fn write_auxiliary_marker(
    git: &Git,
    auxiliary_worktree: &Path,
    marker: &AuxiliaryMarker,
) -> AppResult<()> {
    let path = auxiliary_marker_path(git, auxiliary_worktree)?;
    let text = serde_json::to_string_pretty(marker).map_err(|error| {
        Error::message(format!("failed to serialize auxiliary marker: {error}"))
    })?;
    fs::write(&path, format!("{text}\n")).map_err(|error| {
        Error::message(format!(
            "failed to write auxiliary marker {}: {}",
            path.display(),
            error
        ))
    })
}

pub fn read_auxiliary_marker(
    git: &Git,
    auxiliary_worktree: &Path,
) -> AppResult<Option<AuxiliaryMarker>> {
    let path = auxiliary_marker_path(git, auxiliary_worktree)?;
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| {
        Error::message(format!(
            "failed to read auxiliary marker {}: {}",
            path.display(),
            error
        ))
    })?;
    let marker = serde_json::from_str(&text).map_err(|error| {
        Error::message(format!(
            "failed to parse auxiliary marker {}: {}",
            path.display(),
            error
        ))
    })?;
    Ok(Some(marker))
}

pub fn state_path(primary_root: &Path, git_common_dir: &Path) -> PathBuf {
    let private = private_state_path(git_common_dir);
    if private.exists() {
        private
    } else {
        legacy_state_path(primary_root)
    }
}

fn read_config_path(primary_root: &Path, git_common_dir: &Path) -> PathBuf {
    let private = private_config_path(git_common_dir);
    if private.exists() {
        private
    } else {
        legacy_config_path(primary_root)
    }
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

fn private_state_path(git_common_dir: &Path) -> PathBuf {
    git_common_dir.join("wtk").join("worktrees.json")
}

fn legacy_state_path(primary_root: &Path) -> PathBuf {
    primary_root.join(".wtk").join("worktrees.json")
}

fn private_config_path(git_common_dir: &Path) -> PathBuf {
    git_common_dir.join("wtk").join("config.toml")
}

fn legacy_config_path(primary_root: &Path) -> PathBuf {
    primary_root.join(".wtk").join("config.toml")
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

fn auxiliary_marker_path(git: &Git, auxiliary_worktree: &Path) -> AppResult<PathBuf> {
    let git_dir = worktree_git_dir(git, auxiliary_worktree)?;
    Ok(git_dir.join("wtk-coordinated-primary.json"))
}

fn worktree_git_dir(git: &Git, worktree: &Path) -> AppResult<PathBuf> {
    let git_dir = git
        .run(
            worktree,
            ["rev-parse", "--path-format=absolute", "--git-dir"],
        )?
        .stdout;
    let git_dir = PathBuf::from(git_dir.trim());
    if !git_dir.is_absolute() {
        return Err(Error::message(format!(
            "Auxiliary worktree git dir must be absolute: {}",
            git_dir.display()
        )));
    }
    Ok(git_dir)
}

fn inherited_excludes_path(
    git: &Git,
    worktree: &Path,
    exclude_path: &Path,
) -> AppResult<Option<PathBuf>> {
    match git.run(worktree, ["config", "--path", "--get", "core.excludesFile"]) {
        Ok(output) => {
            let path = PathBuf::from(output.stdout.trim());
            if path.as_os_str().is_empty() || same_path(&path, exclude_path) {
                Ok(None)
            } else {
                Ok(Some(path))
            }
        }
        Err(Error::Git(error)) if error.exit_code == Some(1) => Ok(None),
        Err(error) => Err(error),
    }
}

fn parse_status_paths(path: &str) -> Option<Vec<String>> {
    let mut paths = Vec::new();
    let mut start = 0usize;
    let bytes = path.as_bytes();
    let mut index = 0usize;
    let mut in_quotes = false;

    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                in_quotes = !in_quotes;
                index += 1;
            }
            b'\\' if in_quotes => {
                index += 2;
            }
            b' ' if !in_quotes && bytes[index..].starts_with(b" -> ") => {
                paths.push(decode_status_path(path[start..index].trim())?);
                index += 4;
                start = index;
            }
            _ => {
                index += 1;
            }
        }
    }

    paths.push(decode_status_path(path[start..].trim())?);
    Some(paths)
}

fn decode_status_path(path: &str) -> Option<String> {
    if !(path.starts_with('"') && path.ends_with('"') && path.len() >= 2) {
        return Some(path.to_string());
    }

    let mut decoded = Vec::with_capacity(path.len() - 2);
    let mut bytes = path.as_bytes()[1..path.len() - 1].iter().copied();
    while let Some(byte) = bytes.next() {
        if byte != b'\\' {
            decoded.push(byte);
            continue;
        }
        let escaped = bytes.next()?;
        match escaped {
            b'"' | b'\\' => decoded.push(escaped),
            b'a' => decoded.push(0x07),
            b'b' => decoded.push(0x08),
            b'f' => decoded.push(0x0c),
            b'n' => decoded.push(b'\n'),
            b'r' => decoded.push(b'\r'),
            b't' => decoded.push(b'\t'),
            b'v' => decoded.push(0x0b),
            b'0'..=b'7' => {
                let mut value = escaped - b'0';
                for _ in 0..2 {
                    let Some(next) = bytes.clone().next() else {
                        break;
                    };
                    if !(b'0'..=b'7').contains(&next) {
                        break;
                    }
                    value = (value << 3) + (next - b'0');
                    bytes.next();
                }
                decoded.push(value);
            }
            _ => return None,
        }
    }

    String::from_utf8(decoded).ok()
}

fn escape_gitignore_pattern(path: &str) -> String {
    let mut escaped = String::with_capacity(path.len());
    for ch in path.chars() {
        match ch {
            '\\' | '*' | '?' | '[' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    while escaped.ends_with(' ') {
        let new_len = escaped.len() - 1;
        escaped.truncate(new_len);
        escaped.push_str("\\ ");
    }
    escaped
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
