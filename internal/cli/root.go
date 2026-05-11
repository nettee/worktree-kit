package cli

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/spf13/cobra"

	clip "github.com/nettee/worktree-kit/internal/clipboard"
	"github.com/nettee/worktree-kit/internal/gitexec"
	"github.com/nettee/worktree-kit/internal/output"
	"github.com/nettee/worktree-kit/internal/worktree"
)

var Version = "0.0.1"

func Execute(ctx context.Context, args []string) error {
	return execute(ctx, args, os.Stdout, os.Stderr)
}

func execute(ctx context.Context, args []string, stdout, stderr io.Writer) error {
	cmd := newRoot(ctx, stdout, stderr)
	cmd.SetArgs(args)
	err := cmd.ExecuteContext(ctx)
	if err == nil {
		return nil
	}
	var usageErr *usageError
	if !errors.As(err, &usageErr) {
		return err
	}
	if writeErr := writeUsageError(stderr, usageErr); writeErr != nil {
		return fmt.Errorf("write usage error: %w", writeErr)
	}
	return err
}

func NewRoot(ctx context.Context, out io.Writer) *cobra.Command {
	return newRoot(ctx, out, out)
}

func newRoot(ctx context.Context, out, errOut io.Writer) *cobra.Command {
	svc := worktree.Service{Git: gitexec.Git{}, Clipboard: clip.System{}, Output: output.Renderer{Out: out}}
	root := &cobra.Command{
		Use:           "wtk",
		Short:         "Friendly Git worktree workflows",
		Version:       Version,
		SilenceUsage:  true,
		SilenceErrors: true,
	}
	root.SetOut(out)
	root.SetErr(errOut)
	root.SetFlagErrorFunc(func(cmd *cobra.Command, err error) error {
		return newUsageError(cmd, err)
	})
	root.AddCommand(newCreateCmd(svc), newCheckoutCmd(svc), newRemoveCmd(svc), newSendOutCmd(svc), newBringInCmd(svc), newCompletionCmd(root))
	_ = ctx
	return root
}

func IsUsageError(err error) bool {
	var usageErr *usageError
	return errors.As(err, &usageErr)
}

type usageError struct {
	cause error
	usage string
}

func (e *usageError) Error() string {
	return e.cause.Error()
}

func (e *usageError) Unwrap() error {
	return e.cause
}

func newUsageError(cmd *cobra.Command, err error) error {
	usage := cmd.UsageString()
	return &usageError{cause: err, usage: usage}
}

func writeUsageError(w io.Writer, usageErr *usageError) error {
	if _, err := fmt.Fprintln(w, usageErr.Error()); err != nil {
		return err
	}
	if _, err := fmt.Fprintln(w); err != nil {
		return err
	}
	if _, err := fmt.Fprint(w, usageErr.usage); err != nil {
		return err
	}
	return nil
}

func requiredArg(name string) cobra.PositionalArgs {
	return func(cmd *cobra.Command, args []string) error {
		if len(args) == 0 {
			return newUsageError(cmd, fmt.Errorf("missing required argument: %s", name))
		}
		if len(args) > 1 {
			return newUsageError(cmd, fmt.Errorf("too many arguments: expected 1 %s", name))
		}
		return nil
	}
}

func maximumOneArg(name string) cobra.PositionalArgs {
	return func(cmd *cobra.Command, args []string) error {
		if len(args) > 1 {
			return newUsageError(cmd, fmt.Errorf("too many arguments: expected at most 1 %s", name))
		}
		return nil
	}
}

func noArgs() cobra.PositionalArgs {
	return func(cmd *cobra.Command, args []string) error {
		if len(args) > 0 {
			return newUsageError(cmd, fmt.Errorf("unexpected argument: %s", args[0]))
		}
		return nil
	}
}

func oneOfArg(name string, values ...string) cobra.PositionalArgs {
	return func(cmd *cobra.Command, args []string) error {
		if err := requiredArg(name)(cmd, args); err != nil {
			return err
		}
		for _, value := range values {
			if args[0] == value {
				return nil
			}
		}
		return newUsageError(cmd, fmt.Errorf("unsupported %s: %s", name, args[0]))
	}
}

func applyClipboard(svc worktree.Service, disabled bool) worktree.Service {
	if disabled {
		svc.Clipboard = clip.Disabled{}
	}
	return svc
}

func branchCompletion() cobra.CompletionFunc {
	return func(cmd *cobra.Command, args []string, toComplete string) ([]string, cobra.ShellCompDirective) {
		out, err := gitLines(cmd.Context(), "branch", "--format=%(refname:short)")
		if err != nil {
			return nil, cobra.ShellCompDirectiveNoFileComp
		}
		return filterPrefix(out, toComplete), cobra.ShellCompDirectiveNoFileComp
	}
}

func worktreeCompletion() cobra.CompletionFunc {
	return func(cmd *cobra.Command, args []string, toComplete string) ([]string, cobra.ShellCompDirective) {
		out, err := gitLines(cmd.Context(), "worktree", "list", "--porcelain")
		if err != nil {
			return nil, cobra.ShellCompDirectiveNoFileComp
		}
		var paths []string
		for _, line := range out {
			if len(line) > 9 && line[:9] == "worktree " {
				paths = append(paths, line[9:])
			}
		}
		if len(paths) > 0 {
			paths = paths[1:]
		}
		return filterPrefix(paths, toComplete), cobra.ShellCompDirectiveNoFileComp
	}
}

func gitLines(ctx context.Context, args ...string) ([]string, error) {
	out, _, err := gitexec.Git{}.Run(ctx, ".", args...)
	if err != nil || out == "" {
		return nil, err
	}
	var lines []string
	for _, l := range splitLines(out) {
		if l != "" {
			lines = append(lines, l)
		}
	}
	return lines, nil
}

func filterPrefix(in []string, p string) []string {
	if p == "" {
		return in
	}
	var out []string
	for _, v := range in {
		if len(v) >= len(p) && v[:len(p)] == p {
			out = append(out, v)
		}
	}
	return out
}

func splitLines(s string) []string {
	var out []string
	start := 0
	for i, r := range s {
		if r == '\n' {
			out = append(out, s[start:i])
			start = i + 1
		}
	}
	out = append(out, s[start:])
	return out
}

func newCompletionCmd(root *cobra.Command) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "completion <bash|zsh|fish|powershell>",
		Short: "Generate shell completion script",
		Args:  oneOfArg("shell", "bash", "zsh", "fish", "powershell"),
		RunE: func(cmd *cobra.Command, args []string) error {
			switch args[0] {
			case "bash":
				return root.GenBashCompletion(cmd.OutOrStdout())
			case "zsh":
				return root.GenZshCompletion(cmd.OutOrStdout())
			case "fish":
				return root.GenFishCompletion(cmd.OutOrStdout(), true)
			case "powershell":
				return root.GenPowerShellCompletion(cmd.OutOrStdout())
			default:
				panic(fmt.Sprintf("validated shell reached default case: %s", strings.Join(args, ", ")))
			}
		},
	}
	return cmd
}
