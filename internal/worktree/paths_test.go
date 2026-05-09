package worktree

import "testing"

func TestBranchSlug(t *testing.T) {
	cases := map[string]string{
		"feature/foo":       "feature-foo",
		" bug fix / login ": "bug-fix-login",
		"///":               "branch",
	}
	for in, want := range cases {
		if got := BranchSlug(in); got != want {
			t.Fatalf("BranchSlug(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestDefaultPath(t *testing.T) {
	got := DefaultPath("/tmp/repo", "feature/foo")
	want := "/tmp/repo-wt-feature-foo"
	if got != want {
		t.Fatalf("DefaultPath() = %q, want %q", got, want)
	}
}
