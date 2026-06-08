## Workspace Guidance

- This repository is a Workspace for Worktree Kit.
- Treat `.wtk-workspace.toml` as tracked Workspace Manifest state.
- Treat `refs/` as generated local Workspace Ref state; do not commit files under `refs/`.
- Run Workspace membership changes from the Workspace main worktree.
- Linked Repositories are addressed through Workspace Refs by repository basename.
