# Worktree Kit

Worktree Kit manages Git worktree workflows for one repository and coordinated workspaces spanning multiple repositories.

## Language

**Repository Worktree**:
A real Git worktree belonging to one repository, either the main checkout or a linked worktree created by Git.
_Avoid_: Project folder, clone

**Repository Mode**:
The default `wtk` mode that operates on the current Git repository and its own Repository Worktrees.
_Avoid_: Sibling mode

**Workspace Mode**:
The explicit `wtk` mode that operates through a Workspace to coordinate multiple Linked Repositories.
_Avoid_: Parent-directory mode

**Sibling Layout**:
The existing default layout where a repository's linked worktrees live next to the main worktree using names like `<repo>-wt-<branch-slug>`.
_Avoid_: Flat mode

**Workspace**:
A lightweight Git repository that aggregates multiple Linked Repositories through symlinks. It stores workspace metadata in `.wtk/config.toml` and may contain Workspace Refs plus any other repository content the user chooses.
_Avoid_: Workspace Parent, parent-directory worktree, meta worktree

**Workspace Ref**:
A configured reference in a Workspace. For ref `A`, the ref path is `refs/A`, the absolute ref target is the currently surfaced worktree path, and `.wtk/config.toml` records the stable absolute repository path for repository `A`.
_Avoid_: Workspace link, repo alias, checkout shortcut

**Workspace Switch**:
Changing the Workspace's checked-out state so it surfaces a different coordinated set of Repository Worktrees.
_Avoid_: Multi-repo checkout

**Linked Repository**:
A repository that participates in a Workspace through one Workspace Ref.
_Avoid_: Child repo, subrepo

## Example Dialogue

Dev: "I'm working on a feature that touches API, web, and SDK."

Domain expert: "Create Repository Worktrees for each repo using the existing Sibling Layout, then record their repositories as Workspace Refs in a Workspace."

Dev: "When I switch the Workspace Parent, do the repos switch branches?"

Domain expert: "No. A Workspace Switch changes which derived Repository Worktrees the Workspace surfaces. The Repository Worktrees keep their own Git state."
