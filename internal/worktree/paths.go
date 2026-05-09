package worktree

import (
	"path/filepath"
	"regexp"
	"strings"
)

var slugBad = regexp.MustCompile(`[^A-Za-z0-9._-]+`)
var slugDash = regexp.MustCompile(`-+`)

func BranchSlug(branch string) string {
	s := strings.Trim(branch, " /\t\n")
	s = strings.ReplaceAll(s, "/", "-")
	s = slugBad.ReplaceAllString(s, "-")
	s = slugDash.ReplaceAllString(s, "-")
	s = strings.Trim(s, "-.")
	if s == "" {
		return "branch"
	}
	return s
}

func DefaultPath(mainRoot, branch string) string {
	repo := filepath.Base(filepath.Clean(mainRoot))
	return filepath.Join(filepath.Dir(filepath.Clean(mainRoot)), repo+"-wt-"+BranchSlug(branch))
}
