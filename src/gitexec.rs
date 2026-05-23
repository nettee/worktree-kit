use crate::{AppResult, Error};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: String,
    pub bare: bool,
    pub head: String,
}

#[derive(Debug, Clone)]
pub struct RepoContext {
    pub cwd: PathBuf,
    pub current_root: PathBuf,
    pub main_root: PathBuf,
    pub git_common_dir: PathBuf,
    pub current_is_main: bool,
    pub worktrees: Vec<Worktree>,
}

#[derive(Debug)]
pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub struct GitOutputBytes {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug)]
pub struct GitError {
    pub args: Vec<String>,
    pub exit_code: Option<i32>,
    pub stderr: String,
    pub stdout: String,
    pub source: std::io::Error,
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "git {} failed", self.args.join(" "))?;
        if let Some(code) = self.exit_code {
            write!(f, " with exit code {code}")?;
        }
        if !self.stderr.is_empty() {
            write!(f, ": {}", self.stderr)?;
        } else if !self.stdout.is_empty() {
            write!(f, ": {}", self.stdout)?;
        }
        Ok(())
    }
}

impl std::error::Error for GitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Git;

impl Git {
    pub fn run<I, S>(&self, dir: &Path, args: I) -> AppResult<GitOutput>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let rendered: Vec<String> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_string())
            .collect();
        let mut command = Command::new("git");
        command.current_dir(dir);
        command.args(rendered.iter().map(String::as_str));
        let output = command.output().map_err(|source| {
            Error::Git(GitError {
                args: rendered.clone(),
                exit_code: None,
                stderr: String::new(),
                stdout: String::new(),
                source,
            })
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string();
        let stderr = String::from_utf8_lossy(&output.stderr)
            .trim_end()
            .to_string();
        if output.status.success() {
            Ok(GitOutput { stdout, stderr })
        } else {
            Err(Error::Git(GitError {
                args: rendered,
                exit_code: output.status.code(),
                stderr,
                stdout,
                source: std::io::Error::other("git command failed"),
            }))
        }
    }

    pub fn run_bytes<I, S>(&self, dir: &Path, args: I) -> AppResult<GitOutputBytes>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let rendered: Vec<String> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_string())
            .collect();
        let mut command = Command::new("git");
        command.current_dir(dir);
        command.args(rendered.iter().map(String::as_str));
        let output = command.output().map_err(|source| {
            Error::Git(GitError {
                args: rendered.clone(),
                exit_code: None,
                stderr: String::new(),
                stdout: String::new(),
                source,
            })
        })?;

        if output.status.success() {
            Ok(GitOutputBytes {
                stdout: output.stdout,
                stderr: output.stderr,
            })
        } else {
            Err(Error::Git(GitError {
                args: rendered,
                exit_code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr)
                    .trim_end()
                    .to_string(),
                stdout: String::from_utf8_lossy(&output.stdout)
                    .trim_end()
                    .to_string(),
                source: std::io::Error::other("git command failed"),
            }))
        }
    }
}

pub fn resolve(git: &Git, cwd: &Path) -> AppResult<RepoContext> {
    let root = git
        .run(cwd, ["rev-parse", "--show-toplevel"])?
        .stdout
        .trim()
        .to_string();
    if root.is_empty() {
        return Err(Error::message("not inside a Git repository"));
    }

    let root_path = PathBuf::from(root);
    let common = git
        .run(
            &root_path,
            ["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?
        .stdout;
    let list = git
        .run(&root_path, ["worktree", "list", "--porcelain"])?
        .stdout;
    let worktrees = parse_worktree_list(&list);
    let main_root = worktrees
        .first()
        .map(|wt| wt.path.clone())
        .unwrap_or_else(|| root_path.clone());

    let current_root = absolute_path(&root_path);
    let main_root = absolute_path(&main_root);

    Ok(RepoContext {
        cwd: cwd.to_path_buf(),
        current_root: current_root.clone(),
        main_root: main_root.clone(),
        git_common_dir: PathBuf::from(common.trim()),
        current_is_main: same_path(&current_root, &main_root),
        worktrees,
    })
}

pub fn parse_worktree_list(input: &str) -> Vec<Worktree> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    trimmed
        .split("\n\n")
        .filter_map(|block| {
            let mut path = None;
            let mut branch = String::new();
            let mut bare = false;
            let mut head = String::new();

            for line in block.lines() {
                if let Some(value) = line.strip_prefix("worktree ") {
                    path = Some(PathBuf::from(value));
                } else if let Some(value) = line.strip_prefix("branch refs/heads/") {
                    branch = value.to_string();
                } else if let Some(value) = line.strip_prefix("HEAD ") {
                    head = value.to_string();
                } else if line == "bare" {
                    bare = true;
                }
            }

            path.map(|path| Worktree {
                path,
                branch,
                bare,
                head,
            })
        })
        .collect()
}

pub fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    let canonical = |path: &Path| -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| absolute_path(path))
    };
    canonical(a) == canonical(b)
}

impl RepoContext {
    pub fn worktree_by_path(&self, path: &Path) -> Option<&Worktree> {
        self.worktrees
            .iter()
            .find(|worktree| same_path(&worktree.path, path))
    }
}

pub fn is_git_exit(error: &Error, code: i32) -> bool {
    matches!(error, Error::Git(git_error) if git_error.exit_code == Some(code))
}

#[cfg(test)]
mod tests {
    use super::parse_worktree_list;
    use std::path::Path;

    #[test]
    fn parses_worktree_porcelain() {
        let input = "worktree /tmp/repo\nHEAD abc\nbranch refs/heads/main\n\nworktree /tmp/repo-wt-feature\nHEAD def\nbranch refs/heads/feature/foo\n";
        let worktrees = parse_worktree_list(input);
        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[1].path, Path::new("/tmp/repo-wt-feature"));
        assert_eq!(worktrees[1].branch, "feature/foo");
        assert_eq!(worktrees[1].head, "def");
    }
}
