package cli

import (
	"github.com/nettee/worktree-kit/internal/worktree"
	"github.com/spf13/cobra"
)

func newSendOutCmd(svc worktree.Service) *cobra.Command {
	var opts worktree.Options
	cmd := &cobra.Command{
		Use:   "send-out",
		Short: "Move the current main-worktree branch to a linked worktree",
		Args:  noArgs(),
		RunE: func(cmd *cobra.Command, args []string) error {
			return applyClipboard(svc, opts.NoClipboard).SendOut(cmd.Context(), opts)
		},
	}
	cmd.Flags().StringVar(&opts.Path, "path", "", "linked worktree path")
	cmd.Flags().StringVar(&opts.Base, "base", "", "base branch")
	cmd.Flags().BoolVar(&opts.NoClipboard, "no-clipboard", false, "skip clipboard copy")
	_ = cmd.RegisterFlagCompletionFunc("base", branchCompletion())
	return cmd
}
