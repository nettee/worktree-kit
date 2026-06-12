# Worktree Kit

Worktree Kit manages Git worktree workflows for a Primary Repository and optional coordinated Auxiliary Repositories.

## Language

**Repository Worktree**:
A real Git worktree belonging to one repository, either the main checkout or a linked worktree created by Git.
_Avoid_: Project folder, clone

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

**Sibling Layout**:
The existing default layout where a repository's linked worktrees live next to the main worktree using names like `<repo>-wt-<branch-slug>`.
_Avoid_: Flat mode

## Example Dialogue

Dev: "I'm working on a feature that touches API, web, and SDK."

Domain expert: "Create an Auxiliary Group in the Primary Repository that includes API, web, and SDK."

Dev: "When I create a worktree with that group, do the auxiliary repos follow?"

Domain expert: "Yes. `wtk new --ag <group>` creates matching Auxiliary Repository worktrees and generated refs in the Primary worktree."
