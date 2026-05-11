package cli

import (
	"github.com/nettee/worktree-kit/internal/worktree"
	"github.com/spf13/cobra"
)

func newBringInCmd(svc worktree.Service) *cobra.Command {
	var opts worktree.Options
	cmd := &cobra.Command{
		Use:   "bring-in <branch>",
		Short: "Move a linked worktree branch back into the main worktree",
		Args:  requiredArg("branch"),
		RunE: func(cmd *cobra.Command, args []string) error {
			opts.Branch = args[0]
			return applyClipboard(svc, opts.NoClipboard).BringIn(cmd.Context(), opts)
		},
		ValidArgsFunction: worktreeCompletion(),
	}
	cmd.Flags().BoolVar(&opts.NoClipboard, "no-clipboard", false, "skip clipboard copy")
	return cmd
}
