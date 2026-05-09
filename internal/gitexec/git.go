package gitexec

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"os/exec"
	"strings"
)

type Error struct {
	Args     []string
	ExitCode int
	Stderr   string
	Stdout   string
	Err      error
}

func (e *Error) Error() string {
	return fmt.Sprintf("git %s failed: %v%s", strings.Join(e.Args, " "), e.Err, detail(e.Stderr, e.Stdout))
}

func (e *Error) Unwrap() error { return e.Err }

type Runner interface {
	Run(ctx context.Context, dir string, args ...string) (string, string, error)
}

type Git struct{}

func (Git) Run(ctx context.Context, dir string, args ...string) (string, string, error) {
	cmd := exec.CommandContext(ctx, "git", args...)
	cmd.Dir = dir
	var out, errb bytes.Buffer
	cmd.Stdout = &out
	cmd.Stderr = &errb
	err := cmd.Run()
	stdout := strings.TrimRight(out.String(), "\n")
	stderr := strings.TrimRight(errb.String(), "\n")
	if err != nil {
		exitCode := -1
		var exitErr *exec.ExitError
		if errors.As(err, &exitErr) {
			exitCode = exitErr.ExitCode()
		}
		return stdout, stderr, &Error{Args: args, ExitCode: exitCode, Stderr: stderr, Stdout: stdout, Err: err}
	}
	return stdout, stderr, nil
}

func detail(stderr, stdout string) string {
	if stderr != "" {
		return ": " + stderr
	}
	if stdout != "" {
		return ": " + stdout
	}
	return ""
}
