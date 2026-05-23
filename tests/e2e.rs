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
fn create_checkout_and_send_out_run_pnpm_install_for_pnpm_projects() {
    let bin = build_wtk();
    let repo = init_repo("main");
    std::fs::write(repo.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n").unwrap();
    run_git(&repo, ["add", "."]);
    run_git(&repo, ["commit", "-m", "add pnpm lockfile"]);
    run_git(&repo, ["branch", "feature/existing"]);

    let fake_bin = temp_dir().join("fake-bin");
    std::fs::create_dir_all(&fake_bin).unwrap();
    let log_path = temp_dir().join("pnpm.log");
    install_fake_pnpm(&fake_bin, &log_path);

    run_wtk_with_env(
        &bin,
        &repo,
        [("PATH", path_with_prefix(&fake_bin))],
        ["create", "feature/new", "--base", "main", "--no-clipboard"],
    );
    run_wtk_with_env(
        &bin,
        &repo,
        [("PATH", path_with_prefix(&fake_bin))],
        ["checkout", "feature/existing", "--no-clipboard"],
    );

    run_git(&repo, ["switch", "-c", "feature/send"]);
    run_wtk_with_env(
        &bin,
        &repo,
        [("PATH", path_with_prefix(&fake_bin))],
        ["send-out", "--no-clipboard"],
    );

    let create_path = repo.parent().unwrap().join(format!(
        "{}-wt-feature-new",
        repo.file_name().unwrap().to_string_lossy()
    ));
    let checkout_path = repo.parent().unwrap().join(format!(
        "{}-wt-feature-existing",
        repo.file_name().unwrap().to_string_lossy()
    ));
    let send_out_path = repo.parent().unwrap().join(format!(
        "{}-wt-feature-send",
        repo.file_name().unwrap().to_string_lossy()
    ));

    let log = std::fs::read_to_string(&log_path).unwrap();
    let lines: Vec<_> = log.lines().collect();
    assert_eq!(lines.len(), 3, "unexpected pnpm log: {log}");
    assert_logged_path(&lines, &create_path);
    assert_logged_path(&lines, &checkout_path);
    assert_logged_path(&lines, &send_out_path);
}

#[test]
fn non_pnpm_projects_do_not_run_pnpm_install() {
    let bin = build_wtk();
    let repo = init_repo("main");
    let fake_bin = temp_dir().join("fake-bin");
    std::fs::create_dir_all(&fake_bin).unwrap();
    let log_path = temp_dir().join("pnpm.log");
    install_fake_pnpm(&fake_bin, &log_path);

    run_wtk_with_env(
        &bin,
        &repo,
        [("PATH", path_with_prefix(&fake_bin))],
        [
            "create",
            "feature/no-pnpm",
            "--base",
            "main",
            "--no-clipboard",
        ],
    );

    assert!(
        !log_path.exists()
            || std::fs::read_to_string(&log_path)
                .unwrap()
                .trim()
                .is_empty()
    );
}

#[test]
fn pnpm_project_fails_fast_when_pnpm_is_missing() {
    let bin = build_wtk();
    let repo = init_repo("main");
    std::fs::write(repo.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n").unwrap();
    run_git(&repo, ["add", "."]);
    run_git(&repo, ["commit", "-m", "add pnpm lockfile"]);

    let empty_bin = temp_dir().join("empty-bin");
    std::fs::create_dir_all(&empty_bin).unwrap();
    let (out, status) = run_wtk_err_with_env(
        &bin,
        &repo,
        [("PATH", path_with_git_only(&empty_bin))],
        [
            "create",
            "feature/missing-pnpm",
            "--base",
            "main",
            "--no-clipboard",
        ],
    );

    assert!(!status.success());
    assert!(out.contains("pnpm project detected and pnpm was not found on PATH"));
    assert!(!out.contains("✓ created worktree"));
    let linked = repo.parent().unwrap().join(format!(
        "{}-wt-feature-missing-pnpm",
        repo.file_name().unwrap().to_string_lossy()
    ));
    assert!(linked.exists());
}

#[test]
fn pnpm_project_surfaces_install_failures() {
    let bin = build_wtk();
    let repo = init_repo("main");
    std::fs::write(repo.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n").unwrap();
    run_git(&repo, ["add", "."]);
    run_git(&repo, ["commit", "-m", "add pnpm lockfile"]);

    let fake_bin = temp_dir().join("fake-bin");
    std::fs::create_dir_all(&fake_bin).unwrap();
    let log_path = temp_dir().join("pnpm.log");
    install_fake_pnpm(&fake_bin, &log_path);
    let (out, status) = run_wtk_err_with_env(
        &bin,
        &repo,
        [
            ("PATH", path_with_prefix(&fake_bin)),
            ("WTK_TEST_PNPM_FAIL", "1".to_string()),
        ],
        [
            "create",
            "feature/pnpm-fail",
            "--base",
            "main",
            "--no-clipboard",
        ],
    );

    assert!(!status.success());
    assert!(out.contains("pnpm install failed"));
    assert!(out.contains("fake pnpm failure"));
    assert!(!out.contains("✓ created worktree"));
}

#[test]
fn send_out_reports_state_when_pnpm_install_fails() {
    let bin = build_wtk();
    let repo = init_repo("main");
    std::fs::write(repo.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n").unwrap();
    run_git(&repo, ["add", "."]);
    run_git(&repo, ["commit", "-m", "add pnpm lockfile"]);
    run_git(&repo, ["switch", "-c", "feature/send-fail"]);

    let fake_bin = temp_dir().join("fake-bin");
    std::fs::create_dir_all(&fake_bin).unwrap();
    let log_path = temp_dir().join("pnpm.log");
    install_fake_pnpm(&fake_bin, &log_path);
    let (out, status) = run_wtk_err_with_env(
        &bin,
        &repo,
        [
            ("PATH", path_with_prefix(&fake_bin)),
            ("WTK_TEST_PNPM_FAIL", "1".to_string()),
        ],
        ["send-out", "--no-clipboard"],
    );

    assert!(!status.success());
    assert!(out.contains("main worktree switched to main, and worktree created at"));
    assert!(out.contains("pnpm install failed"));
    assert!(!out.contains("✓ sent feature/send-fail out"));
    assert_eq!(run_git(&repo, ["branch", "--show-current"]).trim(), "main");
    let linked = repo.parent().unwrap().join(format!(
        "{}-wt-feature-send-fail",
        repo.file_name().unwrap().to_string_lossy()
    ));
    assert!(linked.exists());
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

fn run_wtk_with_env<const N: usize, const M: usize, K: AsRef<str>>(
    bin: &Path,
    dir: &Path,
    envs: [(K, String); M],
    args: [&str; N],
) -> String {
    let (output, status) = run_wtk_err_with_env(bin, dir, envs, args);
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

fn run_wtk_err_with_env<const N: usize, const M: usize, K: AsRef<str>>(
    bin: &Path,
    dir: &Path,
    envs: [(K, String); M],
    args: [&str; N],
) -> (String, std::process::ExitStatus) {
    let mut command = Command::new(bin);
    command.args(args).current_dir(dir);
    for (key, value) in envs {
        command.env(key.as_ref(), value);
    }
    let output = command.output().unwrap();
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

fn install_fake_pnpm(dir: &Path, log_path: &Path) {
    let script_path = dir.join("pnpm");
    std::fs::write(
        &script_path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$PWD\" >> \"{}\"\nif [ \"${{WTK_TEST_PNPM_FAIL:-}}\" = \"1\" ]; then\n  echo 'fake pnpm failure' >&2\n  exit 7\nfi\n",
            log_path.display()
        ),
    )
    .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&script_path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script_path, permissions).unwrap();
    }
}

fn path_with_prefix(prefix: &Path) -> String {
    let mut entries = vec![prefix.to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        entries.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(entries)
        .unwrap()
        .into_string()
        .unwrap()
}

fn path_with_git_only(prefix: &Path) -> String {
    std::env::join_paths([prefix.to_path_buf(), git_bin_dir()])
        .unwrap()
        .into_string()
        .unwrap()
}

fn git_bin_dir() -> PathBuf {
    let output = Command::new("which").arg("git").output().unwrap();
    assert!(
        output.status.success(),
        "which git failed: {:?}",
        output.status
    );
    PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
        .parent()
        .unwrap()
        .to_path_buf()
}

fn assert_logged_path(lines: &[&str], expected: &Path) {
    let expected = std::fs::canonicalize(expected).unwrap();
    assert!(
        lines.iter().any(|line| {
            std::fs::canonicalize(line)
                .map(|path| path == expected)
                .unwrap_or(false)
        }),
        "missing logged path {} in {:?}",
        expected.display(),
        lines
    );
}
