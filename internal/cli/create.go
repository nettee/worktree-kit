package cli

import (
	"github.com/nettee/worktree-kit/internal/worktree"
	"github.com/spf13/cobra"
)

func newCreateCmd(svc worktree.Service) *cobra.Command {
	var opts worktree.Options
	cmd := &cobra.Command{
		Use:   "create <branch>",
		Short: "Create a linked worktree",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			opts.Branch = args[0]
			return applyClipboard(svc, opts.NoClipboard).Create(cmd.Context(), opts)
		},
		ValidArgsFunction: branchCompletion(),
	}
	cmd.Flags().StringVar(&opts.Path, "path", "", "linked worktree path")
	cmd.Flags().StringVar(&opts.Base, "base", "", "base branch for --new")
	cmd.Flags().BoolVar(&opts.New, "new", false, "create a new branch")
	cmd.Flags().BoolVar(&opts.NoClipboard, "no-clipboard", false, "skip clipboard copy")
	_ = cmd.RegisterFlagCompletionFunc("base", branchCompletion())
	return cmd
}
