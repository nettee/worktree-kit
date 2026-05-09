package gitexec

import (
	"context"
	"fmt"
	"path/filepath"
	"strings"
)

type Worktree struct {
	Path   string
	Branch string
	Bare   bool
	Head   string
}

type RepoContext struct {
	CWD           string
	CurrentRoot   string
	MainRoot      string
	GitCommonDir  string
	CurrentIsMain bool
	Worktrees     []Worktree
}

func Resolve(ctx context.Context, r Runner, cwd string) (RepoContext, error) {
	root, _, err := r.Run(ctx, cwd, "rev-parse", "--show-toplevel")
	if err != nil {
		return RepoContext{}, fmt.Errorf("not inside a Git repository: %w", err)
	}
	common, _, err := r.Run(ctx, root, "rev-parse", "--path-format=absolute", "--git-common-dir")
	if err != nil {
		return RepoContext{}, err
	}
	list, _, err := r.Run(ctx, root, "worktree", "list", "--porcelain")
	if err != nil {
		return RepoContext{}, err
	}
	worktrees := ParseWorktreeList(list)
	mainRoot := ""
	if len(worktrees) > 0 {
		mainRoot = worktrees[0].Path
	}
	if mainRoot == "" {
		mainRoot = root
	}
	rootAbs, _ := filepath.Abs(root)
	mainAbs, _ := filepath.Abs(mainRoot)
	return RepoContext{CWD: cwd, CurrentRoot: rootAbs, MainRoot: mainAbs, GitCommonDir: common, CurrentIsMain: samePath(rootAbs, mainAbs), Worktrees: worktrees}, nil
}

func ParseWorktreeList(s string) []Worktree {
	blocks := strings.Split(strings.TrimSpace(s), "\n\n")
	if len(blocks) == 1 && strings.TrimSpace(blocks[0]) == "" {
		return nil
	}
	out := make([]Worktree, 0, len(blocks))
	for _, b := range blocks {
		var wt Worktree
		for _, line := range strings.Split(b, "\n") {
			switch {
			case strings.HasPrefix(line, "worktree "):
				wt.Path = strings.TrimPrefix(line, "worktree ")
			case strings.HasPrefix(line, "branch "):
				wt.Branch = strings.TrimPrefix(strings.TrimPrefix(line, "branch "), "refs/heads/")
			case strings.HasPrefix(line, "HEAD "):
				wt.Head = strings.TrimPrefix(line, "HEAD ")
			case line == "bare":
				wt.Bare = true
			}
		}
		if wt.Path != "" {
			out = append(out, wt)
		}
	}
	return out
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

func (c RepoContext) WorktreeByPath(path string) (Worktree, bool) {
	abs, _ := filepath.Abs(path)
	for _, wt := range c.Worktrees {
		if samePath(wt.Path, abs) {
			return wt, true
		}
	}
	return Worktree{}, false
}
