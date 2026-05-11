package cli

import (
	"github.com/nettee/worktree-kit/internal/worktree"
	"github.com/spf13/cobra"
)

func newCheckoutCmd(svc worktree.Service) *cobra.Command {
	var opts worktree.Options
	cmd := &cobra.Command{
		Use:   "checkout <branch>",
		Short: "Check out an existing branch in a linked worktree",
		Args:  requiredArg("branch"),
		RunE: func(cmd *cobra.Command, args []string) error {
			opts.Branch = args[0]
			return applyClipboard(svc, opts.NoClipboard).Checkout(cmd.Context(), opts)
		},
		ValidArgsFunction: branchCompletion(),
	}
	cmd.Flags().StringVar(&opts.Path, "path", "", "linked worktree path")
	cmd.Flags().BoolVar(&opts.NoClipboard, "no-clipboard", false, "skip clipboard copy")
	return cmd
}
