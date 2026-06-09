use crate::gitexec::{Git, RepoContext, Worktree, same_path};
use crate::output::Style;
use crate::{AppResult, Error};
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Default, Clone, Copy)]
pub struct ListOptions {
    pub json: bool,
}

#[derive(Debug, Serialize)]
pub struct ListOutput {
    pub mode: &'static str,
    pub worktrees: Vec<ListRow>,
}

#[derive(Debug, Serialize)]
pub struct ListRow {
    pub kind: &'static str,
    pub display_name: String,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub head: String,
    pub short_head: String,
    pub is_main: bool,
    pub is_current: bool,
    pub dirty: bool,
    pub updated_at: Option<i64>,
    pub updated: String,
    pub labels: Vec<String>,
    pub diagnostics: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_refs: Option<WorkspaceRefSummary>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceRefSummary {
    pub total: usize,
    pub ok: usize,
    pub broken: usize,
    pub details: Vec<WorkspaceRefDetail>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceRefDetail {
    pub name: String,
    pub ok: bool,
    pub expected_target: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_target: Option<PathBuf>,
    pub diagnostics: Vec<String>,
}

pub fn repository_output(git: &Git, repo: &RepoContext) -> ListOutput {
    ListOutput {
        mode: "repository",
        worktrees: sorted_rows(
            repo.worktrees
                .iter()
                .map(|worktree| repository_row(git, repo, worktree))
                .collect(),
        ),
    }
}

pub fn render(
    out: &mut dyn Write,
    output: &ListOutput,
    options: ListOptions,
    style: Style,
) -> AppResult<()> {
    if options.json {
        serde_json::to_writer_pretty(&mut *out, output).map_err(|error| {
            Error::message(format!("failed to serialize list as JSON: {error}"))
        })?;
        writeln!(out)?;
        return Ok(());
    }

    render_table(out, &output.worktrees, style)
}

pub fn sorted_rows(mut rows: Vec<ListRow>) -> Vec<ListRow> {
    rows.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.is_current.cmp(&left.is_current))
            .then_with(|| right.is_main.cmp(&left.is_main))
            .then_with(|| left.display_name.cmp(&right.display_name))
    });
    rows
}

pub fn repository_row(git: &Git, repo: &RepoContext, worktree: &Worktree) -> ListRow {
    let mut diagnostics = Vec::new();
    let updated_at = commit_timestamp(git, &worktree.path, &mut diagnostics);
    let dirty = dirty_state(git, &worktree.path, &mut diagnostics);
    let is_main = same_path(&worktree.path, &repo.main_root);
    let is_current = same_path(&worktree.path, &repo.current_root);
    let mut labels = labels_for_worktree(worktree, is_main, is_current, dirty);
    if !diagnostics.is_empty() {
        labels.push("error".to_string());
    }

    ListRow {
        kind: "repository_worktree",
        display_name: display_name(&worktree.path),
        path: worktree.path.clone(),
        branch: if worktree.branch.is_empty() {
            None
        } else {
            Some(worktree.branch.clone())
        },
        bare: worktree.bare,
        detached: worktree.detached,
        head: worktree.head.clone(),
        short_head: short_head(&worktree.head),
        is_main,
        is_current,
        dirty,
        updated_at,
        updated: relative_time(updated_at),
        labels,
        diagnostics,
        workspace_refs: None,
    }
}

pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn relative_time(timestamp: Option<i64>) -> String {
    let Some(timestamp) = timestamp else {
        return "unknown".to_string();
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(timestamp);
    let seconds = now.saturating_sub(timestamp);
    if seconds < 60 {
        "now".to_string()
    } else if seconds < 60 * 60 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 60 * 60 * 24 {
        format!("{}h ago", seconds / (60 * 60))
    } else {
        format!("{}d ago", seconds / (60 * 60 * 24))
    }
}

fn labels_for_worktree(
    worktree: &Worktree,
    is_main: bool,
    is_current: bool,
    dirty: bool,
) -> Vec<String> {
    let mut labels = Vec::new();
    if is_current {
        labels.push("current".to_string());
    }
    if is_main {
        labels.push("main".to_string());
    }
    if dirty {
        labels.push("dirty".to_string());
    }
    if worktree.bare {
        labels.push("bare".to_string());
    }
    if worktree.detached {
        labels.push("detached".to_string());
    }
    if worktree.locked.is_some() {
        labels.push("locked".to_string());
    }
    if worktree.prunable.is_some() {
        labels.push("prunable".to_string());
    }
    labels
}

fn commit_timestamp(git: &Git, path: &Path, diagnostics: &mut Vec<String>) -> Option<i64> {
    match git.run(path, ["show", "-s", "--format=%ct", "HEAD"]) {
        Ok(output) => match output.stdout.trim().parse::<i64>() {
            Ok(timestamp) => Some(timestamp),
            Err(error) => {
                diagnostics.push(format!("failed to parse HEAD timestamp: {error}"));
                None
            }
        },
        Err(error) => {
            diagnostics.push(format!("failed to read HEAD timestamp: {error}"));
            None
        }
    }
}

fn dirty_state(git: &Git, path: &Path, diagnostics: &mut Vec<String>) -> bool {
    match git.run(
        path,
        ["status", "--porcelain=v1", "--untracked-files=normal"],
    ) {
        Ok(output) => !output.stdout.trim().is_empty(),
        Err(error) => {
            diagnostics.push(format!("failed to read dirty state: {error}"));
            false
        }
    }
}

fn short_head(head: &str) -> String {
    head.chars().take(7).collect()
}

fn branch_text(row: &ListRow) -> String {
    if row.bare {
        "(bare)".to_string()
    } else if row.detached {
        "(detached)".to_string()
    } else {
        row.branch.clone().unwrap_or_else(|| "-".to_string())
    }
}

fn state_text(row: &ListRow) -> String {
    let mut labels = row.labels.clone();
    if let Some(summary) = &row.workspace_refs {
        let status = if summary.broken == 0 { "ok" } else { "broken" };
        labels.insert(0, format!("refs {}/{} {status}", summary.ok, summary.total));
    }
    if labels.is_empty() {
        "-".to_string()
    } else {
        labels.join(" ")
    }
}

fn render_table(out: &mut dyn Write, rows: &[ListRow], style: Style) -> AppResult<()> {
    let rendered_rows = rows
        .iter()
        .map(|row| {
            (
                if row.is_current { "*" } else { " " },
                row.display_name.clone(),
                branch_text(row),
                row.updated.clone(),
                state_text(row),
                row.short_head.clone(),
                row.diagnostics.is_empty(),
            )
        })
        .collect::<Vec<_>>();
    let worktree_width = rendered_rows
        .iter()
        .map(|(_, worktree, _, _, _, _, _)| worktree.len())
        .chain(std::iter::once("worktree".len()))
        .max()
        .unwrap_or("worktree".len());
    let branch_width = rendered_rows
        .iter()
        .map(|(_, _, branch, _, _, _, _)| branch.len())
        .chain(std::iter::once("branch".len()))
        .max()
        .unwrap_or("branch".len());
    let updated_width = rendered_rows
        .iter()
        .map(|(_, _, _, updated, _, _, _)| updated.len())
        .chain(std::iter::once("updated".len()))
        .max()
        .unwrap_or("updated".len());
    let state_width = rendered_rows
        .iter()
        .map(|(_, _, _, _, state, _, _)| state.len())
        .chain(std::iter::once("state".len()))
        .max()
        .unwrap_or("state".len());

    let header = format!(
        "  {:worktree_width$}  {:branch_width$}  {:updated_width$}  {:state_width$}  head",
        "worktree", "branch", "updated", "state"
    );
    writeln!(out, "{}", style.header(&header))?;
    for (marker, worktree, branch, updated, state, head, diagnostics_ok) in rendered_rows {
        let line = format!(
            "{} {:worktree_width$}  {:branch_width$}  {:updated_width$}  {:state_width$}  {}",
            marker, worktree, branch, updated, state, head
        );
        if marker == "*" {
            writeln!(out, "{}", style.current(&line))?;
        } else if !diagnostics_ok || state.contains("broken") || state.contains("error") {
            writeln!(out, "{}", style.error(&line))?;
        } else if state.contains("dirty") || state.contains("prunable") {
            writeln!(out, "{}", style.warning(&line))?;
        } else {
            writeln!(out, "{line}")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{display_name, relative_time, sorted_rows};
    use crate::list::ListRow;
    use std::path::Path;

    #[test]
    fn display_name_uses_path_basename() {
        assert_eq!(
            display_name(Path::new("/tmp/repo-wt-feature")),
            "repo-wt-feature"
        );
    }

    #[test]
    fn relative_time_formats_compact_values() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert_eq!(relative_time(Some(now - 90)), "1m ago");
        assert_eq!(relative_time(None), "unknown");
    }

    #[test]
    fn sorting_prefers_newer_then_current() {
        let mut older_current = row("b", Some(1), true, false);
        let mut newer = row("a", Some(2), false, false);
        let rows = sorted_rows(vec![older_current, newer]);
        assert_eq!(rows[0].display_name, "a");

        older_current = row("b", Some(1), true, false);
        newer = row("a", Some(1), false, false);
        let rows = sorted_rows(vec![newer, older_current]);
        assert_eq!(rows[0].display_name, "b");
    }

    fn row(name: &str, updated_at: Option<i64>, is_current: bool, is_main: bool) -> ListRow {
        ListRow {
            kind: "repository_worktree",
            display_name: name.to_string(),
            path: Path::new("/tmp").join(name),
            branch: Some(name.to_string()),
            bare: false,
            detached: false,
            head: "abcdef0".to_string(),
            short_head: "abcdef0".to_string(),
            is_main,
            is_current,
            dirty: false,
            updated_at,
            updated: "now".to_string(),
            labels: Vec::new(),
            diagnostics: Vec::new(),
            workspace_refs: None,
        }
    }
}
