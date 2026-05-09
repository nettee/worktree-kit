package worktree

import (
	"context"
	"errors"
	"io"
	"strings"
	"testing"

	"github.com/nettee/worktree-kit/internal/gitexec"
	"github.com/nettee/worktree-kit/internal/output"
)

type fakeGit struct{ responses map[string]string }

func (f fakeGit) Run(ctx context.Context, dir string, args ...string) (string, string, error) {
	key := strings.Join(args, " ")
	if v, ok := f.responses[key]; ok {
		return v, "", nil
	}
	return "", "", errors.New("missing response: " + key)
}

type failingClipboard struct{}

func (failingClipboard) WriteText(string) error { return errors.New("clipboard unavailable") }

func TestDetectMainBranchFromConfig(t *testing.T) {
	svc := Service{Git: fakeGit{responses: map[string]string{"config --get worktree-kit.mainBranch": "trunk"}}}
	got, err := svc.detectMainBranch(context.Background(), gitexec.RepoContext{MainRoot: "/tmp/repo"}, "")
	if err != nil {
		t.Fatal(err)
	}
	if got != "trunk" {
		t.Fatalf("got %q, want trunk", got)
	}
}

func TestFinishReportsClipboardPartialFailure(t *testing.T) {
	svc := Service{Clipboard: failingClipboard{}, Output: output.Renderer{Out: io.Discard}}
	err := svc.finish(context.Background(), false, "payload", "done")
	if err == nil || !strings.Contains(err.Error(), "operation succeeded, but clipboard copy failed") {
		t.Fatalf("expected partial failure, got %v", err)
	}
}
