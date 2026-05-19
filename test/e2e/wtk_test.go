package e2e

import (
	"bytes"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestCreateRemoveSendOutBringInAndCompletion(t *testing.T) {
	bin := buildWTK(t)
	repo := initRepo(t, "main")
	runGit(t, repo, "branch", "feature/existing")
	out := runWTK(t, bin, repo, "checkout", "feature/existing", "--no-clipboard")
	if !strings.Contains(out, "git -C") || !strings.Contains(out, "created worktree") {
		t.Fatalf("unexpected checkout output:\n%s", out)
	}
	linked := filepath.Join(filepath.Dir(repo), filepath.Base(repo)+"-wt-feature-existing")
	if _, err := os.Stat(linked); err != nil {
		t.Fatal(err)
	}
	runWTK(t, bin, repo, "remove", linked, "--no-clipboard")
	if _, err := os.Stat(linked); !os.IsNotExist(err) {
		t.Fatalf("linked worktree still exists: %v", err)
	}

	runGit(t, repo, "switch", "-c", "feature/send")
	subdir := filepath.Join(repo, "sub")
	if err := os.Mkdir(subdir, 0o755); err != nil {
		t.Fatal(err)
	}
	out = runWTK(t, bin, subdir, "send-out", "--no-clipboard")
	if !strings.Contains(out, "sent feature/send out") {
		t.Fatalf("unexpected send-out output:\n%s", out)
	}
	linked = filepath.Join(filepath.Dir(repo), filepath.Base(repo)+"-wt-feature-send")
	if branch := strings.TrimSpace(runGit(t, repo, "branch", "--show-current")); branch != "main" {
		t.Fatalf("main worktree branch = %q", branch)
	}
	runWTK(t, bin, repo, "bring-in", "feature/send", "--no-clipboard")
	if branch := strings.TrimSpace(runGit(t, repo, "branch", "--show-current")); branch != "feature/send" {
		t.Fatalf("branch = %q", branch)
	}
	out = runWTK(t, bin, repo, "send-out", "--no-clipboard")
	if !strings.Contains(out, "sent feature/send out") {
		t.Fatalf("unexpected second send-out output:\n%s", out)
	}
	completed := completionLines(t, bin, repo, "__complete", "bring-in", "fea")
	if !containsLine(completed, "feature/send") {
		t.Fatalf("bring-in completion missing branch: %v", completed)
	}
	if containsLine(completed, linked) {
		t.Fatalf("bring-in completion unexpectedly included linked path: %v", completed)
	}

	for _, shell := range []string{"bash", "zsh", "fish", "powershell"} {
		out := runWTK(t, bin, repo, "completion", shell)
		if !strings.Contains(out, "wtk") {
			t.Fatalf("completion %s missing wtk", shell)
		}
	}
}

func TestCreateNewWithTrunkAndDirtyFailures(t *testing.T) {
	bin := buildWTK(t)
	repo := initRepo(t, "trunk")
	runWTK(t, bin, repo, "create", "feature/new", "--base", "trunk", "--no-clipboard")
	linked := filepath.Join(filepath.Dir(repo), filepath.Base(repo)+"-wt-feature-new")
	if _, err := os.Stat(linked); err != nil {
		t.Fatal(err)
	}
	runGit(t, repo, "worktree", "remove", linked)

	runGit(t, repo, "switch", "-c", "feature/dirty")
	if err := os.WriteFile(filepath.Join(repo, "dirty.txt"), []byte("dirty"), 0o644); err != nil {
		t.Fatal(err)
	}
	out, err := runWTKErr(bin, repo, "send-out", "--no-clipboard")
	if err == nil || !strings.Contains(out, "worktree is dirty") {
		t.Fatalf("expected dirty failure, out=%s err=%v", out, err)
	}
}

func TestCreateFromCurrentBranch(t *testing.T) {
	bin := buildWTK(t)
	repo := initRepo(t, "main")
	runGit(t, repo, "switch", "-c", "feature/base")
	if err := os.WriteFile(filepath.Join(repo, "base.txt"), []byte("base\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	runGit(t, repo, "add", ".")
	runGit(t, repo, "commit", "-m", "base")

	runWTK(t, bin, repo, "create", "feature/from-current", "--from-current", "--no-clipboard")
	linked := filepath.Join(filepath.Dir(repo), filepath.Base(repo)+"-wt-feature-from-current")
	if _, err := os.Stat(filepath.Join(linked, "base.txt")); err != nil {
		t.Fatalf("from-current worktree missing current-branch file: %v", err)
	}

	runWTK(t, bin, repo, "create", "feature/from-current-short", "-C", "--no-clipboard")
	linked = filepath.Join(filepath.Dir(repo), filepath.Base(repo)+"-wt-feature-from-current-short")
	if _, err := os.Stat(filepath.Join(linked, "base.txt")); err != nil {
		t.Fatalf("-C worktree missing current-branch file: %v", err)
	}

	out, err := runWTKErr(bin, repo, "create", "feature/conflict", "--base", "main", "--from-current", "--no-clipboard")
	if err == nil || !strings.Contains(out, "--base and --from-current cannot be used together") {
		t.Fatalf("expected base/from-current conflict, out=%s err=%v", out, err)
	}
}

func TestDirtyLinkedFailures(t *testing.T) {
	bin := buildWTK(t)
	repo := initRepo(t, "main")
	runGit(t, repo, "branch", "feature/dirty-linked")
	runWTK(t, bin, repo, "checkout", "feature/dirty-linked", "--no-clipboard")
	linked := filepath.Join(filepath.Dir(repo), filepath.Base(repo)+"-wt-feature-dirty-linked")
	if err := os.WriteFile(filepath.Join(linked, "dirty.txt"), []byte("dirty"), 0o644); err != nil {
		t.Fatal(err)
	}
	out, err := runWTKErr(bin, repo, "remove", linked, "--no-clipboard")
	if err == nil || !strings.Contains(out, "worktree is dirty") {
		t.Fatalf("expected dirty remove failure, out=%s err=%v", out, err)
	}
	out, err = runWTKErr(bin, repo, "bring-in", "feature/dirty-linked", "--no-clipboard")
	if err == nil || !strings.Contains(out, "worktree is dirty") {
		t.Fatalf("expected dirty bring-in failure, out=%s err=%v", out, err)
	}
}

func TestCreateNewDefaultFetchFastForwardsLocalMain(t *testing.T) {
	bin := buildWTK(t)
	base := t.TempDir()
	origin := filepath.Join(base, "origin.git")
	runGit(t, base, "init", "--bare", origin)
	seed := filepath.Join(base, "seed")
	runGit(t, base, "clone", origin, seed)
	runGit(t, seed, "switch", "-c", "main")
	runGit(t, seed, "config", "user.email", "test@example.com")
	runGit(t, seed, "config", "user.name", "Test")
	if err := os.WriteFile(filepath.Join(seed, "README.md"), []byte("one\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	runGit(t, seed, "add", ".")
	runGit(t, seed, "commit", "-m", "one")
	runGit(t, seed, "push", "-u", "origin", "main")
	repo := filepath.Join(base, "repo")
	runGit(t, base, "clone", origin, repo)
	runGit(t, repo, "switch", "main")
	runGit(t, repo, "config", "user.email", "test@example.com")
	runGit(t, repo, "config", "user.name", "Test")
	if err := os.WriteFile(filepath.Join(seed, "remote.txt"), []byte("two\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	runGit(t, seed, "add", ".")
	runGit(t, seed, "commit", "-m", "two")
	runGit(t, seed, "push")
	runWTK(t, bin, repo, "create", "feature/from-updated-main", "--no-clipboard")
	linked := filepath.Join(filepath.Dir(repo), filepath.Base(repo)+"-wt-feature-from-updated-main")
	if _, err := os.Stat(filepath.Join(linked, "remote.txt")); err != nil {
		t.Fatalf("feature branch missing fetched file: %v", err)
	}
}

func TestCreateNewDefaultRefusesNonFastForwardBase(t *testing.T) {
	bin := buildWTK(t)
	base := t.TempDir()
	origin := filepath.Join(base, "origin.git")
	runGit(t, base, "init", "--bare", origin)
	repo := filepath.Join(base, "repo")
	runGit(t, base, "clone", origin, repo)
	runGit(t, repo, "switch", "-c", "main")
	runGit(t, repo, "config", "user.email", "test@example.com")
	runGit(t, repo, "config", "user.name", "Test")
	if err := os.WriteFile(filepath.Join(repo, "README.md"), []byte("one\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	runGit(t, repo, "add", ".")
	runGit(t, repo, "commit", "-m", "one")
	runGit(t, repo, "push", "-u", "origin", "main")
	runGit(t, repo, "switch", "-c", "side")
	if err := os.WriteFile(filepath.Join(repo, "local.txt"), []byte("local\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	runGit(t, repo, "add", ".")
	runGit(t, repo, "commit", "-m", "local")
	runGit(t, repo, "branch", "-f", "main", "HEAD")
	seed := filepath.Join(base, "seed")
	runGit(t, base, "clone", origin, seed)
	runGit(t, seed, "switch", "main")
	runGit(t, seed, "config", "user.email", "test@example.com")
	runGit(t, seed, "config", "user.name", "Test")
	if err := os.WriteFile(filepath.Join(seed, "remote.txt"), []byte("remote\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	runGit(t, seed, "add", ".")
	runGit(t, seed, "commit", "-m", "remote")
	runGit(t, seed, "push")
	out, err := runWTKErr(bin, repo, "create", "feature/refuse", "--no-clipboard")
	if err == nil || !strings.Contains(out, "refusing to move it without a fast-forward") {
		t.Fatalf("expected non-ff failure, out=%s err=%v", out, err)
	}
}

func TestAmbiguousMainBranchFails(t *testing.T) {
	bin := buildWTK(t)
	repo := initRepo(t, "main")
	runGit(t, repo, "branch", "trunk")
	runGit(t, repo, "switch", "-c", "feature/ambiguous")
	out, err := runWTKErr(bin, repo, "send-out", "--no-clipboard")
	if err == nil || !strings.Contains(out, "cannot determine main branch") {
		t.Fatalf("expected ambiguous failure, out=%s err=%v", out, err)
	}
}

func TestArgumentAndFlagUsageErrors(t *testing.T) {
	bin := buildWTK(t)
	repo := initRepo(t, "main")

	tests := []struct {
		name   string
		args   []string
		reason string
		usage  string
	}{
		{name: "create missing branch", args: []string{"create"}, reason: "missing required argument: branch", usage: "wtk create <branch> [flags]"},
		{name: "create too many args", args: []string{"create", "feature/a", "feature/b"}, reason: "too many arguments: expected 1 branch", usage: "wtk create <branch> [flags]"},
		{name: "checkout missing branch", args: []string{"checkout"}, reason: "missing required argument: branch", usage: "wtk checkout <branch> [flags]"},
		{name: "checkout too many args", args: []string{"checkout", "feature/a", "feature/b"}, reason: "too many arguments: expected 1 branch", usage: "wtk checkout <branch> [flags]"},
		{name: "remove too many args", args: []string{"remove", "one", "two"}, reason: "too many arguments: expected at most 1 path", usage: "wtk remove [path] [flags]"},
		{name: "send-out unexpected arg", args: []string{"send-out", "extra"}, reason: "unexpected argument: extra", usage: "wtk send-out [flags]"},
		{name: "bring-in missing branch", args: []string{"bring-in"}, reason: "missing required argument: branch", usage: "wtk bring-in <branch> [flags]"},
		{name: "completion unsupported shell", args: []string{"completion", "tcsh"}, reason: "unsupported shell: tcsh", usage: "wtk completion <bash|zsh|fish|powershell> [flags]"},
		{name: "create unknown flag", args: []string{"create", "--wat"}, reason: "unknown flag: --wat", usage: "wtk create <branch> [flags]"},
		{name: "checkout unknown flag", args: []string{"checkout", "--wat"}, reason: "unknown flag: --wat", usage: "wtk checkout <branch> [flags]"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			assertUsageError(t, bin, repo, tt.args, tt.reason, tt.usage)
		})
	}
}

func buildWTK(t *testing.T) string {
	t.Helper()
	bin := filepath.Join(t.TempDir(), "wtk")
	cmd := exec.Command("go", "build", "-o", bin, "./cmd/wtk")
	cmd.Dir = repoRoot(t)
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("build failed: %v\n%s", err, out)
	}
	return bin
}

func initRepo(t *testing.T, branch string) string {
	t.Helper()
	dir := filepath.Join(t.TempDir(), "repo")
	if err := os.Mkdir(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	runGit(t, dir, "init", "-b", branch)
	runGit(t, dir, "config", "user.email", "test@example.com")
	runGit(t, dir, "config", "user.name", "Test")
	if err := os.WriteFile(filepath.Join(dir, "README.md"), []byte("test\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	runGit(t, dir, "add", ".")
	runGit(t, dir, "commit", "-m", "init")
	return dir
}

func runGit(t *testing.T, dir string, args ...string) string {
	t.Helper()
	cmd := exec.Command("git", args...)
	cmd.Dir = dir
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("git %s failed: %v\n%s", strings.Join(args, " "), err, out)
	}
	return string(out)
}

func runWTK(t *testing.T, bin, dir string, args ...string) string {
	t.Helper()
	out, err := runWTKErr(bin, dir, args...)
	if err != nil {
		t.Fatalf("wtk %s failed: %v\n%s", strings.Join(args, " "), err, out)
	}
	return out
}

func runWTKErr(bin, dir string, args ...string) (string, error) {
	cmd := exec.Command(bin, args...)
	cmd.Dir = dir
	out, err := cmd.CombinedOutput()
	return string(out), err
}

func assertUsageError(t *testing.T, bin, dir string, args []string, reason, usage string) {
	t.Helper()
	stdout, stderr, err := runWTKErrSplit(bin, dir, args...)
	if err == nil {
		t.Fatalf("wtk %s unexpectedly succeeded:\nstdout:\n%sstderr:\n%s", strings.Join(args, " "), stdout, stderr)
	}
	if stdout != "" {
		t.Fatalf("usage error wrote stdout, want empty stdout:\n%s", stdout)
	}
	if strings.Count(stderr, reason) != 1 {
		t.Fatalf("stderr should contain reason %q once:\n%s", reason, stderr)
	}
	if strings.Count(stderr, "Usage:") != 1 {
		t.Fatalf("stderr should contain one Usage section:\n%s", stderr)
	}
	if !strings.Contains(stderr, usage) {
		t.Fatalf("stderr missing command usage %q:\n%s", usage, stderr)
	}
	if !strings.Contains(stderr, "Flags:") {
		t.Fatalf("stderr missing Flags:\n%s", stderr)
	}
}

func completionLines(t *testing.T, bin, dir string, args ...string) []string {
	t.Helper()
	out := runWTK(t, bin, dir, args...)
	var lines []string
	for _, line := range strings.Split(strings.ReplaceAll(out, "\r\n", "\n"), "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, ":") || strings.HasPrefix(line, "Completion ended") {
			continue
		}
		lines = append(lines, line)
	}
	return lines
}

func containsLine(lines []string, want string) bool {
	for _, line := range lines {
		if line == want {
			return true
		}
	}
	return false
}

func runWTKErrSplit(bin, dir string, args ...string) (string, string, error) {
	cmd := exec.Command(bin, args...)
	cmd.Dir = dir
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	return stdout.String(), stderr.String(), err
}

func repoRoot(t *testing.T) string {
	t.Helper()
	cmd := exec.Command("git", "rev-parse", "--show-toplevel")
	out, err := cmd.Output()
	if err != nil {
		t.Fatal(err)
	}
	return strings.TrimSpace(string(out))
}
