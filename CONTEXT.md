# Worktree Kit

Worktree Kit manages Git worktree workflows for one repository and coordinated workspaces spanning multiple repositories.

## Language

**Repository Worktree**:
A real Git worktree belonging to one repository, either the main checkout or a linked worktree created by Git.
_Avoid_: Project folder, clone

**Repository Mode**:
The default `wtk` mode that operates on the current Git repository and its own Repository Worktrees.
_Avoid_: Sibling mode

**Primary Repository**:
The repository an agent opens directly as the center of a task. It owns the specs and may expose Auxiliary Repositories for coordinated code changes.
_Avoid_: Main repo, workspace repo

**Auxiliary Repository**:
A repository exposed from a Primary Repository when a task needs coordinated code changes or PRs outside the Primary Repository.
_Avoid_: Helper repo, linked repo

**Auxiliary Repository Ref**:
A local named reference from a Primary Repository to one Auxiliary Repository. Its name must match the Auxiliary Repository path's final segment.
_Avoid_: Repository alias, linked repository

**Auxiliary Group**:
A named local group of Auxiliary Repository Refs that can be selected when creating a Primary Repository worktree. A worktree may select zero or more Auxiliary Groups, and the resulting Auxiliary Repository set is fixed for that worktree.
_Avoid_: Profile, mode, workspace mode, repository mode

**Standalone Mode**:
The derived case where a Primary Repository worktree selects no Auxiliary Groups.
_Avoid_: Repository Mode, single-repository mode

**Coordinated Mode**:
The derived case where a Primary Repository worktree selects one or more Auxiliary Groups.
_Avoid_: Workspace Mode, multi-repository mode

**Workspace Mode**:
The explicit `wtk` mode that operates through a Workspace to coordinate multiple Linked Repositories.
_Avoid_: Parent-directory mode

**Workspace Manifest**:
A tracked `.wtk-workspace.toml` file at the Workspace repository root. Its presence marks the repository as a Workspace and records stable Linked Repository identity, not current worktree state.
_Avoid_: Workspace state file, runtime config

**Sibling Layout**:
The existing default layout where a repository's linked worktrees live next to the main worktree using names like `<repo>-wt-<branch-slug>`.
_Avoid_: Flat mode

**Workspace**:
A Git repository whose own branches and worktrees represent coordinated states across multiple Linked Repositories. Each Workspace Worktree reflects the corresponding branch and Repository Worktree selection for every Linked Repository it manages.
_Avoid_: Workspace Parent, parent-directory worktree, meta worktree

**Workspace Worktree**:
A real Git worktree belonging to a Workspace repository. It represents one coordinated workspace state and contains generated Workspace Refs for the Linked Repositories surfaced in that state.
_Avoid_: Workspace folder, control directory

**Workspace Ref**:
A generated reference inside a Workspace Worktree. For ref `A`, the ref path is `refs/A` and the absolute ref target is the Repository Worktree currently surfaced for Linked Repository `A`.
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

Dev: "When I move the Workspace to another feature branch, do the repos switch branches?"

Domain expert: "The Workspace moves to a Workspace Worktree for that branch, and its Workspace Refs surface the matching Repository Worktrees for each Linked Repository."
