package cli

import (
	"bytes"
	"context"
	"strings"
	"testing"
)

func TestRootVersion(t *testing.T) {
	var out bytes.Buffer
	cmd := NewRoot(context.Background(), &out)
	cmd.SetArgs([]string{"--version"})

	if err := cmd.Execute(); err != nil {
		t.Fatalf("--version failed: %v", err)
	}

	got := out.String()
	if !strings.Contains(got, "0.0.1") {
		t.Fatalf("version output %q does not contain 0.0.1", got)
	}
}
