use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

static BIN_PATH: OnceLock<PathBuf> = OnceLock::new();

#[test]
fn create_remove_send_out_bring_in_and_completion() {
    let bin = build_wtk();
    let repo = init_repo("main");
    run_git(&repo, ["branch", "feature/existing"]);

    let out = run_wtk(
        &bin,
        &repo,
        ["checkout", "feature/existing", "--no-clipboard"],
    );
    assert!(out.contains("git -C"));
    assert!(out.contains("created worktree"));

    let mut linked = repo.parent().unwrap().join(format!(
        "{}-wt-feature-existing",
        repo.file_name().unwrap().to_string_lossy()
    ));
    assert!(linked.exists());

    run_wtk(
        &bin,
        &repo,
        ["remove", linked.to_str().unwrap(), "--no-clipboard"],
    );
    assert!(!linked.exists());

    run_git(&repo, ["switch", "-c", "feature/send"]);
    let subdir = repo.join("sub");
    std::fs::create_dir(&subdir).unwrap();
    let out = run_wtk(&bin, &subdir, ["send-out", "--no-clipboard"]);
    assert!(out.contains("sent feature/send out"));

    linked = repo.parent().unwrap().join(format!(
        "{}-wt-feature-send",
        repo.file_name().unwrap().to_string_lossy()
    ));
    assert_eq!(run_git(&repo, ["branch", "--show-current"]).trim(), "main");

    run_wtk(&bin, &repo, ["bring-in", "feature/send", "--no-clipboard"]);
    assert_eq!(
        run_git(&repo, ["branch", "--show-current"]).trim(),
        "feature/send"
    );

    let out = run_wtk(&bin, &repo, ["send-out", "--no-clipboard"]);
    assert!(out.contains("sent feature/send out"));

    let completed = completion_lines(&bin, &repo, ["__complete", "bring-in", "fea"]);
    assert!(completed.iter().any(|line| line == "feature/send"));
    assert!(
        !completed
            .iter()
            .any(|line| line == linked.to_str().unwrap())
    );

    for shell in ["bash", "zsh", "fish", "powershell"] {
        let out = run_wtk(&bin, &repo, ["completion", shell]);
        assert!(out.contains("wtk"));
    }
}

#[test]
fn create_new_with_trunk_and_dirty_failures() {
    let bin = build_wtk();
    let repo = init_repo("trunk");
    run_wtk(
        &bin,
        &repo,
        ["create", "feature/new", "--base", "trunk", "--no-clipboard"],
    );
    let linked = repo.parent().unwrap().join(format!(
        "{}-wt-feature-new",
        repo.file_name().unwrap().to_string_lossy()
    ));
    assert!(linked.exists());
    run_git(&repo, ["worktree", "remove", linked.to_str().unwrap()]);

    run_git(&repo, ["switch", "-c", "feature/dirty"]);
    std::fs::write(repo.join("dirty.txt"), "dirty").unwrap();
    let (out, status) = run_wtk_err(&bin, &repo, ["send-out", "--no-clipboard"]);
    assert!(!status.success());
    assert!(out.contains("worktree is dirty"));
}

#[test]
fn create_copies_ignored_root_env() {
    let bin = build_wtk();
    let repo = init_repo("main");
    commit_files(&repo, &[(".gitignore", ".env\n")], "add gitignore");
    std::fs::write(repo.join(".env"), "ROOT=value\n").unwrap();

    let out = run_wtk(
        &bin,
        &repo,
        [
            "create",
            "feature/root-env",
            "--base",
            "main",
            "--no-clipboard",
        ],
    );
    let linked = linked_worktree_path(&repo, "feature/root-env");

    assert_eq!(
        std::fs::read_to_string(linked.join(".env")).unwrap(),
        "ROOT=value\n"
    );
    assert!(out.contains("copied ignored .env: .env"));
}

#[test]
fn create_recursively_copies_ignored_child_env_files() {
    let bin = build_wtk();
    let repo = init_repo("main");
    commit_files(
        &repo,
        &[
            (".gitignore", "apps/web/.env\nservices/api/.env\n"),
            ("apps/web/keep.txt", "web\n"),
            ("services/api/keep.txt", "api\n"),
        ],
        "add tracked dirs",
    );
    std::fs::write(repo.join("apps/web/.env"), "WEB=value\n").unwrap();
    std::fs::write(repo.join("services/api/.env"), "API=value\n").unwrap();

    let out = run_wtk(
        &bin,
        &repo,
        [
            "create",
            "feature/child-envs",
            "--base",
            "main",
            "--no-clipboard",
        ],
    );
    let linked = linked_worktree_path(&repo, "feature/child-envs");

    assert_eq!(
        std::fs::read_to_string(linked.join("apps/web/.env")).unwrap(),
        "WEB=value\n"
    );
    assert_eq!(
        std::fs::read_to_string(linked.join("services/api/.env")).unwrap(),
        "API=value\n"
    );
    assert!(out.contains("copied ignored .env: apps/web/.env"));
    assert!(out.contains("copied ignored .env: services/api/.env"));
}

#[test]
fn checkout_copies_ignored_env_files() {
    let bin = build_wtk();
    let repo = init_repo("main");
    run_git(&repo, ["branch", "feature/existing-env"]);
    commit_files(&repo, &[(".gitignore", ".env\n")], "add gitignore");
    std::fs::write(repo.join(".env"), "CHECKOUT=value\n").unwrap();

    let out = run_wtk(
        &bin,
        &repo,
        ["checkout", "feature/existing-env", "--no-clipboard"],
    );
    let linked = linked_worktree_path(&repo, "feature/existing-env");

    assert_eq!(
        std::fs::read_to_string(linked.join(".env")).unwrap(),
        "CHECKOUT=value\n"
    );
    assert!(out.contains("copied ignored .env: .env"));
}

#[test]
fn send_out_copies_ignored_env_files() {
    let bin = build_wtk();
    let repo = init_repo("main");
    commit_files(&repo, &[(".gitignore", ".env\n")], "add gitignore");
    std::fs::write(repo.join(".env"), "SENDOUT=value\n").unwrap();
    run_git(&repo, ["switch", "-c", "feature/send-env"]);

    let out = run_wtk(&bin, &repo, ["send-out", "--no-clipboard"]);
    let linked = linked_worktree_path(&repo, "feature/send-env");

    assert_eq!(
        std::fs::read_to_string(linked.join(".env")).unwrap(),
        "SENDOUT=value\n"
    );
    assert!(out.contains("copied ignored .env: .env"));
}

#[test]
fn tracked_env_files_are_not_reported_by_copy_mechanism() {
    let bin = build_wtk();
    let repo = init_repo("main");
    commit_files(&repo, &[(".env", "TRACKED=value\n")], "add tracked env");

    let out = run_wtk(
        &bin,
        &repo,
        [
            "create",
            "feature/tracked-env",
            "--base",
            "main",
            "--no-clipboard",
        ],
    );
    let linked = linked_worktree_path(&repo, "feature/tracked-env");

    assert_eq!(
        std::fs::read_to_string(linked.join(".env")).unwrap(),
        "TRACKED=value\n"
    );
    assert!(!out.contains("copied ignored .env:"));
}

#[test]
fn similarly_named_env_files_are_not_copied() {
    let bin = build_wtk();
    let repo = init_repo("main");
    commit_files(
        &repo,
        &[
            (
                ".gitignore",
                ".env.local\n.env.example\n.envrc\nconfig/.env.local\n",
            ),
            ("config/keep.txt", "keep\n"),
        ],
        "add ignore patterns",
    );
    std::fs::write(repo.join(".env.local"), "LOCAL=value\n").unwrap();
    std::fs::write(repo.join(".env.example"), "EXAMPLE=value\n").unwrap();
    std::fs::write(repo.join(".envrc"), "DIRENV=value\n").unwrap();
    std::fs::write(repo.join("config/.env.local"), "CHILD=value\n").unwrap();

    let out = run_wtk(
        &bin,
        &repo,
        [
            "create",
            "feature/named-envs",
            "--base",
            "main",
            "--no-clipboard",
        ],
    );
    let linked = linked_worktree_path(&repo, "feature/named-envs");

    assert!(!linked.join(".env.local").exists());
    assert!(!linked.join(".env.example").exists());
    assert!(!linked.join(".envrc").exists());
    assert!(!linked.join("config/.env.local").exists());
    assert!(!out.contains("copied ignored .env:"));
}

#[test]
fn no_ignored_env_files_is_silent_success() {
    let bin = build_wtk();
    let repo = init_repo("main");

    let out = run_wtk(
        &bin,
        &repo,
        [
            "create",
            "feature/no-env",
            "--base",
            "main",
            "--no-clipboard",
        ],
    );

    assert!(out.contains("created worktree"));
    assert!(!out.contains("copied ignored .env:"));
}

#[test]
fn copy_output_uses_git_root_relative_paths_one_per_line() {
    let bin = build_wtk();
    let repo = init_repo("main");
    commit_files(
        &repo,
        &[
            (".gitignore", ".env\napps/web/.env\n"),
            ("apps/web/keep.txt", "web\n"),
        ],
        "add env paths",
    );
    std::fs::write(repo.join(".env"), "ROOT=value\n").unwrap();
    std::fs::write(repo.join("apps/web/.env"), "WEB=value\n").unwrap();

    let out = run_wtk(
        &bin,
        &repo.join("apps"),
        [
            "create",
            "feature/output-env",
            "--base",
            "main",
            "--no-clipboard",
        ],
    );

    let normalized = out.replace("\r\n", "\n");
    let copied_lines: Vec<_> = normalized
        .lines()
        .filter(|line| line.starts_with("copied ignored .env: "))
        .collect();
    assert_eq!(
        copied_lines,
        vec![
            "copied ignored .env: .env",
            "copied ignored .env: apps/web/.env",
        ]
    );
}

#[test]
fn create_from_current_branch() {
    let bin = build_wtk();
    let repo = init_repo("main");
    run_git(&repo, ["switch", "-c", "feature/base"]);
    std::fs::write(repo.join("base.txt"), "base\n").unwrap();
    run_git(&repo, ["add", "."]);
    run_git(&repo, ["commit", "-m", "base"]);

    run_wtk(
        &bin,
        &repo,
        [
            "create",
            "feature/from-current",
            "--from-current",
            "--no-clipboard",
        ],
    );
    let mut linked = repo.parent().unwrap().join(format!(
        "{}-wt-feature-from-current",
        repo.file_name().unwrap().to_string_lossy()
    ));
    assert!(linked.join("base.txt").exists());

    run_wtk(
        &bin,
        &repo,
        [
            "create",
            "feature/from-current-short",
            "-C",
            "--no-clipboard",
        ],
    );
    linked = repo.parent().unwrap().join(format!(
        "{}-wt-feature-from-current-short",
        repo.file_name().unwrap().to_string_lossy()
    ));
    assert!(linked.join("base.txt").exists());

    let (out, status) = run_wtk_err(
        &bin,
        &repo,
        [
            "create",
            "feature/conflict",
            "--base",
            "main",
            "--from-current",
            "--no-clipboard",
        ],
    );
    assert!(!status.success());
    assert!(out.contains("--base and --from-current cannot be used together"));
}

#[test]
fn dirty_linked_failures() {
    let bin = build_wtk();
    let repo = init_repo("main");
    run_git(&repo, ["branch", "feature/dirty-linked"]);
    run_wtk(
        &bin,
        &repo,
        ["checkout", "feature/dirty-linked", "--no-clipboard"],
    );
    let linked = repo.parent().unwrap().join(format!(
        "{}-wt-feature-dirty-linked",
        repo.file_name().unwrap().to_string_lossy()
    ));
    std::fs::write(linked.join("dirty.txt"), "dirty").unwrap();

    let (out, status) = run_wtk_err(
        &bin,
        &repo,
        ["remove", linked.to_str().unwrap(), "--no-clipboard"],
    );
    assert!(!status.success());
    assert!(out.contains("worktree is dirty"));

    let (out, status) = run_wtk_err(
        &bin,
        &repo,
        ["bring-in", "feature/dirty-linked", "--no-clipboard"],
    );
    assert!(!status.success());
    assert!(out.contains("worktree is dirty"));
}

#[test]
fn create_new_default_fetch_fast_forwards_local_main() {
    let bin = build_wtk();
    let base = temp_dir();
    let origin = base.join("origin.git");
    run_git(&base, ["init", "--bare", origin.to_str().unwrap()]);

    let seed = base.join("seed");
    run_git(
        &base,
        ["clone", origin.to_str().unwrap(), seed.to_str().unwrap()],
    );
    run_git(&seed, ["switch", "-c", "main"]);
    run_git(&seed, ["config", "user.email", "test@example.com"]);
    run_git(&seed, ["config", "user.name", "Test"]);
    std::fs::write(seed.join("README.md"), "one\n").unwrap();
    run_git(&seed, ["add", "."]);
    run_git(&seed, ["commit", "-m", "one"]);
    run_git(&seed, ["push", "-u", "origin", "main"]);

    let repo = base.join("repo");
    run_git(
        &base,
        ["clone", origin.to_str().unwrap(), repo.to_str().unwrap()],
    );
    run_git(&repo, ["switch", "main"]);
    run_git(&repo, ["config", "user.email", "test@example.com"]);
    run_git(&repo, ["config", "user.name", "Test"]);

    std::fs::write(seed.join("remote.txt"), "two\n").unwrap();
    run_git(&seed, ["add", "."]);
    run_git(&seed, ["commit", "-m", "two"]);
    run_git(&seed, ["push"]);

    run_wtk(
        &bin,
        &repo,
        ["create", "feature/from-updated-main", "--no-clipboard"],
    );
    let linked = repo.parent().unwrap().join(format!(
        "{}-wt-feature-from-updated-main",
        repo.file_name().unwrap().to_string_lossy()
    ));
    assert!(linked.join("remote.txt").exists());
}

#[test]
fn create_new_default_refuses_non_fast_forward_base() {
    let bin = build_wtk();
    let base = temp_dir();
    let origin = base.join("origin.git");
    run_git(&base, ["init", "--bare", origin.to_str().unwrap()]);

    let repo = base.join("repo");
    run_git(
        &base,
        ["clone", origin.to_str().unwrap(), repo.to_str().unwrap()],
    );
    run_git(&repo, ["switch", "-c", "main"]);
    run_git(&repo, ["config", "user.email", "test@example.com"]);
    run_git(&repo, ["config", "user.name", "Test"]);
    std::fs::write(repo.join("README.md"), "one\n").unwrap();
    run_git(&repo, ["add", "."]);
    run_git(&repo, ["commit", "-m", "one"]);
    run_git(&repo, ["push", "-u", "origin", "main"]);

    run_git(&repo, ["switch", "-c", "side"]);
    std::fs::write(repo.join("local.txt"), "local\n").unwrap();
    run_git(&repo, ["add", "."]);
    run_git(&repo, ["commit", "-m", "local"]);
    run_git(&repo, ["branch", "-f", "main", "HEAD"]);

    let seed = base.join("seed");
    run_git(
        &base,
        ["clone", origin.to_str().unwrap(), seed.to_str().unwrap()],
    );
    run_git(&seed, ["switch", "main"]);
    run_git(&seed, ["config", "user.email", "test@example.com"]);
    run_git(&seed, ["config", "user.name", "Test"]);
    std::fs::write(seed.join("remote.txt"), "remote\n").unwrap();
    run_git(&seed, ["add", "."]);
    run_git(&seed, ["commit", "-m", "remote"]);
    run_git(&seed, ["push"]);

    let (out, status) = run_wtk_err(&bin, &repo, ["create", "feature/refuse", "--no-clipboard"]);
    assert!(!status.success());
    assert!(out.contains("refusing to move it without a fast-forward"));
}

#[test]
fn ambiguous_main_branch_fails() {
    let bin = build_wtk();
    let repo = init_repo("main");
    run_git(&repo, ["branch", "trunk"]);
    run_git(&repo, ["switch", "-c", "feature/ambiguous"]);
    let (out, status) = run_wtk_err(&bin, &repo, ["send-out", "--no-clipboard"]);
    assert!(!status.success());
    assert!(out.contains("cannot determine main branch"));
}

#[test]
fn argument_and_flag_usage_errors() {
    let bin = build_wtk();
    let repo = init_repo("main");
    let cases = [
        (
            vec!["create"],
            "missing required argument: branch",
            "wtk create <branch> [flags]",
        ),
        (
            vec!["create", "feature/a", "feature/b"],
            "too many arguments: expected 1 branch",
            "wtk create <branch> [flags]",
        ),
        (
            vec!["checkout"],
            "missing required argument: branch",
            "wtk checkout <branch> [flags]",
        ),
        (
            vec!["checkout", "feature/a", "feature/b"],
            "too many arguments: expected 1 branch",
            "wtk checkout <branch> [flags]",
        ),
        (
            vec!["remove", "one", "two"],
            "too many arguments: expected at most 1 path",
            "wtk remove [path] [flags]",
        ),
        (
            vec!["send-out", "extra"],
            "unexpected argument: extra",
            "wtk send-out [flags]",
        ),
        (
            vec!["bring-in"],
            "missing required argument: branch",
            "wtk bring-in <branch> [flags]",
        ),
        (
            vec!["completion", "tcsh"],
            "unsupported shell: tcsh",
            "wtk completion <bash|zsh|fish|powershell> [flags]",
        ),
        (
            vec!["create", "--wat"],
            "unknown flag: --wat",
            "wtk create <branch> [flags]",
        ),
        (
            vec!["checkout", "--wat"],
            "unknown flag: --wat",
            "wtk checkout <branch> [flags]",
        ),
    ];

    for (args, reason, usage) in cases {
        assert_usage_error(&bin, &repo, &args, reason, usage);
    }
}

fn build_wtk() -> PathBuf {
    BIN_PATH
        .get_or_init(|| {
            let status = Command::new("cargo")
                .args(["build", "--release", "--bin", "wtk"])
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .status()
                .expect("cargo build should start");
            assert!(status.success(), "cargo build failed with {status}");

            let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            path.push("target");
            path.push("release");
            path.push(if cfg!(windows) { "wtk.exe" } else { "wtk" });
            path
        })
        .clone()
}

fn init_repo(branch: &str) -> PathBuf {
    let dir = temp_dir().join("repo");
    std::fs::create_dir(&dir).unwrap();
    run_git(&dir, ["init", "-b", branch]);
    run_git(&dir, ["config", "user.email", "test@example.com"]);
    run_git(&dir, ["config", "user.name", "Test"]);
    std::fs::write(dir.join("README.md"), "test\n").unwrap();
    run_git(&dir, ["add", "."]);
    run_git(&dir, ["commit", "-m", "init"]);
    dir
}

fn linked_worktree_path(repo: &Path, branch: &str) -> PathBuf {
    repo.parent().unwrap().join(format!(
        "{}-wt-{}",
        repo.file_name().unwrap().to_string_lossy(),
        branch.replace(['/', '\\'], "-")
    ))
}

fn commit_files(repo: &Path, files: &[(&str, &str)], message: &str) {
    for (path, contents) in files {
        let full_path = repo.join(path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(full_path, contents).unwrap();
    }
    run_git(&repo, ["add", "."]);
    run_git(&repo, ["commit", "-m", message]);
}

fn run_git<const N: usize>(dir: &Path, args: [&str; N]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}\n{}",
        args,
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn run_wtk<const N: usize>(bin: &Path, dir: &Path, args: [&str; N]) -> String {
    let (output, status) = run_wtk_err(bin, dir, args);
    assert!(
        status.success(),
        "wtk {:?} failed: {status}\n{output}",
        args
    );
    output
}

fn run_wtk_err<const N: usize>(
    bin: &Path,
    dir: &Path,
    args: [&str; N],
) -> (String, std::process::ExitStatus) {
    let output = Command::new(bin)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (combined, output.status)
}

fn run_wtk_err_split<const N: usize>(
    bin: &Path,
    dir: &Path,
    args: [&str; N],
) -> (String, String, std::process::ExitStatus) {
    let output = Command::new(bin)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status,
    )
}

fn assert_usage_error(bin: &Path, dir: &Path, args: &[&str], reason: &str, usage: &str) {
    let (stdout, stderr, status) = match args.len() {
        1 => run_wtk_err_split(bin, dir, [args[0]]),
        2 => run_wtk_err_split(bin, dir, [args[0], args[1]]),
        3 => run_wtk_err_split(bin, dir, [args[0], args[1], args[2]]),
        _ => run_wtk_err_split(bin, dir, [args[0], args[1], args[2], args[3]]),
    };
    assert!(!status.success(), "wtk {:?} unexpectedly succeeded", args);
    assert!(stdout.is_empty(), "usage error wrote stdout: {stdout}");
    assert!(stderr.contains(reason), "stderr missing {reason}: {stderr}");
    assert!(stderr.contains("Usage:"), "stderr missing Usage: {stderr}");
    assert!(
        stderr.contains(usage),
        "stderr missing usage {usage}: {stderr}"
    );
    assert!(stderr.contains("Flags:"), "stderr missing Flags: {stderr}");
}

fn completion_lines<const N: usize>(bin: &Path, dir: &Path, args: [&str; N]) -> Vec<String> {
    run_wtk(bin, dir, args)
        .replace("\r\n", "\n")
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty() && !line.starts_with(':') && !line.starts_with("Completion ended")
        })
        .map(ToOwned::to_owned)
        .collect()
}

fn temp_dir() -> PathBuf {
    static NEXT_ID: OnceLock<std::sync::atomic::AtomicU64> = OnceLock::new();
    let next_id = NEXT_ID.get_or_init(|| std::sync::atomic::AtomicU64::new(0));
    let unique = next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("wtk-rs-test-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    path
}
