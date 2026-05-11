package cli

import (
	"github.com/nettee/worktree-kit/internal/worktree"
	"github.com/spf13/cobra"
)

func newBringInCmd(svc worktree.Service) *cobra.Command {
	var opts worktree.Options
	cmd := &cobra.Command{
		Use:   "bring-in <linked-worktree-path>",
		Short: "Move a linked worktree branch back into the main worktree",
		Args:  requiredArg("linked-worktree-path"),
		RunE: func(cmd *cobra.Command, args []string) error {
			opts.Path = args[0]
			return applyClipboard(svc, opts.NoClipboard).BringIn(cmd.Context(), opts)
		},
		ValidArgsFunction: worktreeCompletion(),
	}
	cmd.Flags().BoolVar(&opts.NoClipboard, "no-clipboard", false, "skip clipboard copy")
	return cmd
}
