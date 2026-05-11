package cli

import (
	"github.com/nettee/worktree-kit/internal/worktree"
	"github.com/spf13/cobra"
)

func newRemoveCmd(svc worktree.Service) *cobra.Command {
	var opts worktree.Options
	cmd := &cobra.Command{
		Use:   "remove [path]",
		Short: "Remove a linked worktree",
		Args:  maximumOneArg("path"),
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) > 0 {
				opts.Path = args[0]
			}
			return applyClipboard(svc, opts.NoClipboard).Remove(cmd.Context(), opts)
		},
		ValidArgsFunction: worktreeCompletion(),
	}
	cmd.Flags().BoolVar(&opts.DeleteBranch, "delete-branch", false, "delete branch after removing worktree")
	cmd.Flags().BoolVar(&opts.NoClipboard, "no-clipboard", false, "skip clipboard copy")
	return cmd
}
