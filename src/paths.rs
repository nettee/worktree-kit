use std::path::{Path, PathBuf};

pub fn branch_slug(branch: &str) -> String {
    let trimmed = branch.trim_matches(|c: char| c == ' ' || c == '/' || c == '\t' || c == '\n');
    let mut out = String::new();
    let mut previous_dash = false;

    for ch in trimmed.chars() {
        let mapped = match ch {
            '/' => '-',
            c if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' => c,
            _ => '-',
        };

        if mapped == '-' {
            if previous_dash {
                continue;
            }
            previous_dash = true;
            out.push(mapped);
        } else {
            previous_dash = false;
            out.push(mapped);
        }
    }

    let trimmed = out.trim_matches(|c: char| c == '-' || c == '.').to_string();
    if trimmed.is_empty() {
        "branch".to_string()
    } else {
        trimmed
    }
}

pub fn default_path(main_root: &Path, branch: &str) -> PathBuf {
    let repo = main_root
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    main_root
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{repo}-wt-{}", branch_slug(branch)))
}

#[cfg(test)]
mod tests {
    use super::{branch_slug, default_path};
    use std::path::Path;

    #[test]
    fn slug_rewrites_branch_name() {
        assert_eq!(branch_slug(" feature/foo "), "feature-foo");
        assert_eq!(branch_slug("a///b"), "a-b");
        assert_eq!(branch_slug("..."), "branch");
    }

    #[test]
    fn default_path_uses_repo_prefix() {
        let path = default_path(Path::new("/tmp/repo"), "feature/foo");
        assert_eq!(path, Path::new("/tmp/repo-wt-feature-foo"));
    }
}
