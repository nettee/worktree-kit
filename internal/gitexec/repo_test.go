package gitexec

import "testing"

func TestParseWorktreeList(t *testing.T) {
	in := "worktree /tmp/repo\nHEAD abc\nbranch refs/heads/main\n\nworktree /tmp/repo-wt-feature\nHEAD def\nbranch refs/heads/feature/foo\n"
	got := ParseWorktreeList(in)
	if len(got) != 2 {
		t.Fatalf("len = %d, want 2", len(got))
	}
	if got[1].Path != "/tmp/repo-wt-feature" || got[1].Branch != "feature/foo" || got[1].Head != "def" {
		t.Fatalf("unexpected linked worktree: %+v", got[1])
	}
}
