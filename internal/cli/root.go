package cli

import (
	"context"
	"fmt"
	"io"
	"os"

	"github.com/spf13/cobra"

	clip "github.com/nettee/worktree-kit/internal/clipboard"
	"github.com/nettee/worktree-kit/internal/gitexec"
	"github.com/nettee/worktree-kit/internal/output"
	"github.com/nettee/worktree-kit/internal/worktree"
)

var Version = "0.0.1"

func Execute(ctx context.Context, args []string) error {
	cmd := NewRoot(ctx, os.Stdout)
	cmd.SetArgs(args)
	return cmd.ExecuteContext(ctx)
}

func NewRoot(ctx context.Context, out io.Writer) *cobra.Command {
	svc := worktree.Service{Git: gitexec.Git{}, Clipboard: clip.System{}, Output: output.Renderer{Out: out}}
	root := &cobra.Command{
		Use:           "wtk",
		Short:         "Friendly Git worktree workflows",
		Version:       Version,
		SilenceUsage:  true,
		SilenceErrors: true,
	}
	root.SetOut(out)
	root.AddCommand(newCreateCmd(svc), newRemoveCmd(svc), newSendOutCmd(svc), newBringInCmd(svc), newCompletionCmd(root))
	_ = ctx
	return root
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
		Args:  cobra.ExactArgs(1),
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
				return fmt.Errorf("unsupported shell: %s", args[0])
			}
		},
	}
	return cmd
}
