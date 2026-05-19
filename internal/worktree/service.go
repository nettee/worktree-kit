package worktree

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/nettee/worktree-kit/internal/clipboard"
	"github.com/nettee/worktree-kit/internal/gitexec"
	"github.com/nettee/worktree-kit/internal/output"
)

type Service struct {
	Git       gitexec.Runner
	Clipboard clipboard.Clipboard
	Output    output.Renderer
}

type Options struct {
	Branch       string
	Path         string
	Base         string
	FromCurrent  bool
	DeleteBranch bool
	NoClipboard  bool
}

func (s Service) Create(ctx context.Context, opts Options) error {
	repo, err := s.repo(ctx)
	if err != nil {
		return err
	}
	branch := opts.Branch
	if branch == "" {
		return errors.New("branch is required")
	}
	path := opts.Path
	if path == "" {
		path = DefaultPath(repo.MainRoot, branch)
	}
	path, _ = filepath.Abs(path)
	if _, err := os.Stat(path); err == nil {
		return fmt.Errorf("target path already exists: %s", path)
	}
	args := []string{"worktree", "add"}
	base, err := s.prepareCreateBase(ctx, repo, opts)
	if err != nil {
		return err
	}
	args = append(args, "-b", branch, path, base)
	s.Output.Git(repo.MainRoot, args...)
	if _, _, err := s.Git.Run(ctx, repo.MainRoot, args...); err != nil {
		return err
	}
	return s.finish(ctx, opts.NoClipboard, path, "created worktree at %s", path)
}

func (s Service) Checkout(ctx context.Context, opts Options) error {
	repo, err := s.repo(ctx)
	if err != nil {
		return err
	}
	branch := opts.Branch
	if branch == "" {
		return errors.New("branch is required")
	}
	path := opts.Path
	if path == "" {
		path = DefaultPath(repo.MainRoot, branch)
	}
	path, _ = filepath.Abs(path)
	if _, err := os.Stat(path); err == nil {
		return fmt.Errorf("target path already exists: %s", path)
	}
	args := []string{"worktree", "add", path, branch}
	s.Output.Git(repo.MainRoot, args...)
	if _, _, err := s.Git.Run(ctx, repo.MainRoot, args...); err != nil {
		return err
	}
	return s.finish(ctx, opts.NoClipboard, path, "created worktree at %s", path)
}

func (s Service) Remove(ctx context.Context, opts Options) error {
	repo, err := s.repo(ctx)
	if err != nil {
		return err
	}
	target := opts.Path
	if target == "" {
		if repo.CurrentIsMain {
			return errors.New("path is required when removing from the main worktree")
		}
		target = repo.CurrentRoot
	}
	target, _ = filepath.Abs(target)
	wt, ok := repo.WorktreeByPath(target)
	if !ok || samePath(target, repo.MainRoot) {
		return fmt.Errorf("target is not a linked worktree: %s", target)
	}
	if err := s.requireClean(ctx, target); err != nil {
		return err
	}
	s.Output.Git(repo.MainRoot, "worktree", "remove", target)
	if _, _, err := s.Git.Run(ctx, repo.MainRoot, "worktree", "remove", target); err != nil {
		return err
	}
	if opts.DeleteBranch {
		if wt.Branch == "" {
			return errors.New("cannot delete branch for detached linked worktree")
		}
		s.Output.Git(repo.MainRoot, "branch", "-d", wt.Branch)
		if _, _, err := s.Git.Run(ctx, repo.MainRoot, "branch", "-d", wt.Branch); err != nil {
			return fmt.Errorf("worktree removed, but branch deletion failed; run git -C %s branch -d %s after resolving the issue: %w", repo.MainRoot, wt.Branch, err)
		}
	}
	payload := target
	if opts.DeleteBranch {
		payload += "\n" + wt.Branch
	}
	return s.finish(ctx, opts.NoClipboard, payload, "removed worktree %s", target)
}

func (s Service) SendOut(ctx context.Context, opts Options) error {
	repo, err := s.repo(ctx)
	if err != nil {
		return err
	}
	if !repo.CurrentIsMain {
		return errors.New("send-out must be run from the main worktree")
	}
	if err := s.requireClean(ctx, repo.MainRoot); err != nil {
		return err
	}
	branch, _, err := s.Git.Run(ctx, repo.MainRoot, "branch", "--show-current")
	if err != nil {
		return err
	}
	if branch == "" {
		return errors.New("send-out requires a named branch")
	}
	base := opts.Base
	if base == "" {
		base, err = s.detectMainBranch(ctx, repo, "")
		if err != nil {
			return err
		}
	}
	if branch == base {
		return fmt.Errorf("current branch %q is the base branch; no task branch to send out", branch)
	}
	path := opts.Path
	if path == "" {
		path = DefaultPath(repo.MainRoot, branch)
	}
	path, _ = filepath.Abs(path)
	if _, err := os.Stat(path); err == nil {
		return fmt.Errorf("target path already exists: %s", path)
	}
	if err := ensureCreatableParent(path); err != nil {
		return err
	}
	s.Output.Git(repo.MainRoot, "switch", base)
	if _, _, err := s.Git.Run(ctx, repo.MainRoot, "switch", base); err != nil {
		return err
	}
	s.Output.Git(repo.MainRoot, "worktree", "add", path, branch)
	if _, _, err := s.Git.Run(ctx, repo.MainRoot, "worktree", "add", path, branch); err != nil {
		return fmt.Errorf("main worktree switched to %s, but linked worktree creation failed; recover with git -C %s switch %s after resolving the issue: %w", base, repo.MainRoot, branch, err)
	}
	return s.finish(ctx, opts.NoClipboard, path, "sent %s out to %s", branch, path)
}

func (s Service) BringIn(ctx context.Context, opts Options) error {
	repo, err := s.repo(ctx)
	if err != nil {
		return err
	}
	if !repo.CurrentIsMain {
		return errors.New("bring-in must be run from the main worktree")
	}
	branch := opts.Branch
	if branch == "" {
		return errors.New("branch is required")
	}
	target := ""
	for _, wt := range repo.Worktrees {
		if wt.Branch == branch && !samePath(wt.Path, repo.MainRoot) {
			target = wt.Path
			break
		}
	}
	if target == "" {
		return fmt.Errorf("branch is not checked out in a linked worktree: %s", branch)
	}
	target, _ = filepath.Abs(target)
	if err := s.requireClean(ctx, repo.MainRoot); err != nil {
		return err
	}
	if err := s.requireClean(ctx, target); err != nil {
		return err
	}
	if _, _, err := s.Git.Run(ctx, repo.MainRoot, "rev-parse", "--verify", branch); err != nil {
		return fmt.Errorf("branch cannot be checked out in main worktree: %w", err)
	}
	s.Output.Git(repo.MainRoot, "worktree", "remove", target)
	if _, _, err := s.Git.Run(ctx, repo.MainRoot, "worktree", "remove", target); err != nil {
		return err
	}
	s.Output.Git(repo.MainRoot, "switch", branch)
	if _, _, err := s.Git.Run(ctx, repo.MainRoot, "switch", branch); err != nil {
		return fmt.Errorf("worktree removed; failed to switch to %s: %w", branch, err)
	}
	return s.finish(ctx, opts.NoClipboard, branch, "brought %s into main worktree", branch)
}

func (s Service) repo(ctx context.Context) (gitexec.RepoContext, error) {
	cwd, err := os.Getwd()
	if err != nil {
		return gitexec.RepoContext{}, err
	}
	return gitexec.Resolve(ctx, s.Git, cwd)
}

func (s Service) requireClean(ctx context.Context, dir string) error {
	out, _, err := s.Git.Run(ctx, dir, "status", "--porcelain=v1", "--untracked-files=normal")
	if err != nil {
		return err
	}
	if strings.TrimSpace(out) != "" {
		return fmt.Errorf("worktree is dirty at %s:\n%s", dir, out)
	}
	return nil
}

func (s Service) detectMainBranch(ctx context.Context, repo gitexec.RepoContext, explicit string) (string, error) {
	if explicit != "" {
		return explicit, nil
	}
	if v, _, err := s.Git.Run(ctx, repo.MainRoot, "config", "--get", "worktree-kit.mainBranch"); err == nil && strings.TrimSpace(v) != "" {
		return strings.TrimSpace(v), nil
	} else if err != nil && !isGitExit(err, 1) {
		return "", err
	}
	if v, _, err := s.Git.Run(ctx, repo.MainRoot, "symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"); err == nil && strings.HasPrefix(v, "origin/") {
		return strings.TrimPrefix(v, "origin/"), nil
	} else if err != nil && !isGitExit(err, 1) {
		return "", err
	}
	var found []string
	for _, c := range []string{"main", "master", "trunk", "develop"} {
		if _, _, err := s.Git.Run(ctx, repo.MainRoot, "show-ref", "--verify", "--quiet", "refs/heads/"+c); err == nil {
			found = append(found, c)
		} else if err != nil && !isGitExit(err, 1) {
			return "", err
		}
	}
	if len(found) == 1 {
		return found[0], nil
	}
	return "", fmt.Errorf("cannot determine main branch; pass --base or run git config worktree-kit.mainBranch <branch>")
}

func (s Service) prepareBase(ctx context.Context, repo gitexec.RepoContext, explicit string) (string, error) {
	if explicit != "" {
		return explicit, nil
	}
	detected, err := s.detectMainBranch(ctx, repo, "")
	if err != nil {
		return "", err
	}
	base := detected
	s.Output.Git(repo.MainRoot, "fetch", "origin", base)
	if _, _, err := s.Git.Run(ctx, repo.MainRoot, "fetch", "origin", base); err != nil {
		return "", err
	}
	current, _, err := s.Git.Run(ctx, repo.MainRoot, "branch", "--show-current")
	if err != nil {
		return "", err
	}
	if current == base {
		s.Output.Git(repo.MainRoot, "merge", "--ff-only", "origin/"+base)
		if _, _, err := s.Git.Run(ctx, repo.MainRoot, "merge", "--ff-only", "origin/"+base); err != nil {
			return "", err
		}
		return base, nil
	}
	if _, _, err := s.Git.Run(ctx, repo.MainRoot, "show-ref", "--verify", "--quiet", "refs/heads/"+base); err == nil {
		s.Output.Git(repo.MainRoot, "merge-base", "--is-ancestor", base, "origin/"+base)
		if _, _, err := s.Git.Run(ctx, repo.MainRoot, "merge-base", "--is-ancestor", base, "origin/"+base); err != nil {
			return "", fmt.Errorf("local %s is not an ancestor of origin/%s; refusing to move it without a fast-forward", base, base)
		}
	} else if err != nil && !isGitExit(err, 1) {
		return "", err
	}
	s.Output.Git(repo.MainRoot, "branch", "-f", base, "origin/"+base)
	if _, _, err := s.Git.Run(ctx, repo.MainRoot, "branch", "-f", base, "origin/"+base); err != nil {
		if strings.Contains(err.Error(), "checked out") || strings.Contains(err.Error(), "cannot force update") {
			s.Output.Warn("%s is checked out; using origin/%s as base", base, base)
			return "origin/" + base, nil
		}
		return "", err
	}
	return base, nil
}

func (s Service) prepareCreateBase(ctx context.Context, repo gitexec.RepoContext, opts Options) (string, error) {
	if opts.FromCurrent {
		if opts.Base != "" {
			return "", errors.New("--base and --from-current cannot be used together")
		}
		current, _, err := s.Git.Run(ctx, repo.CurrentRoot, "branch", "--show-current")
		if err != nil {
			return "", err
		}
		current = strings.TrimSpace(current)
		if current == "" {
			return "", errors.New("--from-current requires the current worktree to be on a named branch")
		}
		return current, nil
	}
	return s.prepareBase(ctx, repo, opts.Base)
}

func ensureCreatableParent(path string) error {
	parent := filepath.Dir(path)
	info, err := os.Stat(parent)
	if err != nil {
		return fmt.Errorf("target parent is unavailable: %s: %w", parent, err)
	}
	if !info.IsDir() {
		return fmt.Errorf("target parent is not a directory: %s", parent)
	}
	probe, err := os.CreateTemp(parent, ".wtk-write-check-*")
	if err != nil {
		return fmt.Errorf("target parent is not writable: %s: %w", parent, err)
	}
	name := probe.Name()
	if err := probe.Close(); err != nil {
		return err
	}
	if err := os.Remove(name); err != nil {
		return err
	}
	return nil
}

func isGitExit(err error, code int) bool {
	var gitErr *gitexec.Error
	return errors.As(err, &gitErr) && gitErr.ExitCode == code
}

func (s Service) finish(ctx context.Context, noClipboard bool, payload, format string, args ...any) error {
	s.Output.Success(format, args...)
	if noClipboard {
		return nil
	}
	if err := s.Clipboard.WriteText(payload); err != nil {
		return fmt.Errorf("operation succeeded, but clipboard copy failed: %w", err)
	}
	s.Output.Info("copied to clipboard: %s", payload)
	_ = ctx
	return nil
}

func samePath(a, b string) bool {
	aa, _ := filepath.Abs(a)
	bb, _ := filepath.Abs(b)
	if real, err := filepath.EvalSymlinks(aa); err == nil {
		aa = real
	}
	if real, err := filepath.EvalSymlinks(bb); err == nil {
		bb = real
	}
	return filepath.Clean(aa) == filepath.Clean(bb)
}
