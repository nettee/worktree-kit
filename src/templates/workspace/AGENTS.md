## Workspace Guidance

This repository is a Workspace for Worktree Kit. It is a shared entrypoint for coordinated Linked Repositories; it does not contain the feature code itself.

Linked Repositories are surfaced through generated Workspace Refs under `refs/`, named by repository basename. For example:

- `refs/<repository-name>`

When modifying code, go through the matching Workspace Ref and edit the corresponding Linked Repository. When opening PRs, create one PR in each modified Linked Repository.

Operational rules:

- Treat `.wtk-workspace.toml` as tracked Workspace Manifest state.
- Treat `refs/` as generated local Workspace Ref state; do not commit files under `refs/`.
- Run Workspace membership changes from the Workspace main worktree.
