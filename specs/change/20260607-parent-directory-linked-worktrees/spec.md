---
id: 20260607-parent-directory-linked-worktrees
name: Parent Directory Linked Worktrees
status: researched
created: '2026-06-07'
---

## Overview

### Problem Statement

- Issue 45 asks `wtk` to support a parent-directory worktree mode.
- The selected worktree mode should be reflected in `wtk status`.
- `wtk` should also support linked worktrees across multiple repositories.

### Scope

- Define the product semantics for workspace mode and multi-repository linked worktrees.
- Update the relevant `wtk` commands and status output once the mode semantics are decided.
- Preserve current fail-fast behavior: missing config, ambiguous discovery, or invalid repository state should produce clear failures rather than hidden fallbacks.

### User Intent

- Keep existing repositories and Git worktrees managed with the current sibling layout.
- Add a Workspace Mode where a lightweight Git repository aggregates several repositories through symlinks.
- Store refs in the workspace; each ref points to the currently surfaced worktree path for a related repository such as A, B, or C.
- Worktree paths are derived from the workspace worktree/branch name using the existing sibling layout rule.
- Switching the workspace changes which coordinated repository worktrees are surfaced through the workspace.
- The goal is to avoid mixed-source edits caused by creating or switching a worktree in one repository while related repositories still point at unrelated branches or worktrees.
- Workspace metadata should live in `.wtk/config.toml` and include the current mode plus workspace refs.
- `wtk` should not restrict arbitrary repository contents in the Workspace Parent beyond the configuration and ref invariants it needs to operate.
- Workspace Mode configuration should record the stable repository path for each ref.
- For a workspace ref, the ref name, fixed ref path, and configured repository directory name must remain consistent. For example, ref name `A`, ref path `refs/A`, and repository directory `/path/to/A` match; mismatches are errors.
- Ref targets are worktree paths, not stable repository paths.
- Repository paths in `.wtk/config.toml` and Workspace Ref targets must be absolute paths.
- The workspace worktree/branch name must be creatable in every linked repository before the workspace switch/create operation can continue.
- If any repository cannot create the required branch/worktree because of a collision or another required-step failure, `wtk` must fail and roll back every repository already changed by that operation.
- Workspace names, branch names, and derived worktree paths must all follow the existing sibling layout rules used by Repository Mode.
- Workspace create/switch operations must run a full preflight before making changes to avoid dirty partial state.
- Workspace setup uses `wtk workspace init` to initialize Workspace Mode config and `wtk workspace add <repository-path>` to add refs from repository paths, converting input paths to absolute paths.
- Existing worktree operations keep their command names. In Workspace Mode, `new`, `remove`, `send-out`, and `bring-in` dispatch the corresponding operation into each configured ref repository instead of only the current repository.
- Name the current default single-repository behavior Repository Mode.
- Name the new coordinated multi-repository behavior Workspace Mode.

## Research

### Existing System

- The public command surface describes `wtk new`, `checkout`, `status`, `remove`, `send-out`, `bring-in`, and the `create` compatibility alias. Source: `README.md:3-12`
- Current documentation defines default linked worktree paths as sibling directories named `<repo>-wt-<branch-slug>`. Source: `README.md:14`
- `paths::default_path` implements the current sibling layout by taking `main_root.parent()` and joining `<repo>-wt-<branch-slug>`. Source: `src/paths.rs:35-44`
- `worktree::create` and `worktree::checkout` both call `create_target_path`, which uses `default_path` when `--path` is empty. Source: `src/worktree.rs:95-101,137-143,471-484`
- `RepoContext` represents one resolved Git repository/worktree set: `cwd`, `current_root`, `main_root`, `git_common_dir`, `current_is_main`, and `worktrees`. Source: `src/gitexec.rs:14-22`
- Repository context resolution uses Git as the source of truth: `rev-parse --show-toplevel`, `rev-parse --git-common-dir`, and `git worktree list --porcelain`. Source: `src/gitexec.rs:154-190`
- `wtk status` currently serializes a single-repository YAML payload with `cwd`, `current_root`, `main_root`, `git_common_dir`, `current_is_main`, and `worktrees`; it has no worktree mode field. Source: `src/worktree.rs:58-76,172-197`
- Existing E2E coverage asserts the current `status` YAML schema for a linked worktree in one repository. Source: `tests/e2e.rs:108-144`
- Test helpers also encode the current sibling default path rule. Source: `tests/e2e.rs:1223-1229`
- The async initialization spec added and implemented `init-worktree`; it is an internal/advanced worktree initialization command, not a parent-directory or multi-repo mode. Source: `specs/change/20260525-async-worktree-initialization/spec.md:70-116`

### Design Inputs

- Issue 45 explicitly requires parent-directory worktree mode, mode visibility in `wtk status`, and linked worktrees across multiple repositories, but does not define the mode shape, configuration source, discovery range, or YAML schema. Source: `https://github.com/nettee/worktree-kit/issues/45`
- Historical rewrite design intentionally kept Git worktree orchestration semantics and path rules compatible with the existing implementation, so changing the default layout is a compatibility decision rather than an implementation detail. Source: `specs/change/20260522-rust-rewrite-wtk/spec.md:76-77`
- The README states commands print underlying Git commands, successful commands copy useful payloads to the clipboard, and required-step failures are reported directly. Source: `README.md:75-81,108-110`

### Constraints & Dependencies

- Parent-directory mode needs an explicit model for path derivation before `create`/`checkout` can change safely, because current implicit paths are computed centrally in `paths::default_path`. Source: `src/paths.rs:35-44`, `src/worktree.rs:471-484`
- Multi-repository linked worktrees require either an aggregate discovery model or a scoped single-repository model, because `RepoContext` currently resolves only the Git repository containing the current working directory. Source: `src/gitexec.rs:14-22,154-190`
- `status` schema changes require E2E updates because the YAML fields are directly asserted. Source: `tests/e2e.rs:108-144`

### Key References

- `https://github.com/nettee/worktree-kit/issues/45` - requested feature.
- `README.md:3-14,75-81,108-110` - current public command, path, and failure behavior.
- `src/paths.rs:35-44` - current default path layout.
- `src/gitexec.rs:14-22,154-190` - current single-repository context resolution.
- `src/worktree.rs:58-76,172-197,471-484` - status payload and target path selection.
- `tests/e2e.rs:108-144,1223-1229` - current status and path behavior tests.

## Design

### Design Summary

- Add a Workspace layer above existing repository worktrees.
- Keep each repository's real Git worktrees in the current sibling layout.
- Represent a multi-repository feature workspace as a lightweight Git repository whose refs point to current worktree paths.
- Treat `.wtk/config.toml` as the source of truth for linked repository identity and the Workspace repository's current ref state as the source of truth for currently surfaced worktrees.
- Store mode and stable repository paths in `.wtk/config.toml`.
- Keep Repository Mode as the default mode; use Workspace Mode only when `.wtk/config.toml` explicitly selects it.
- In Workspace Mode, configuration records repository refs rather than explicit repository paths.

### Design Decisions

- Decision: Do not replace sibling layout for real Git worktrees; Workspace Mode composes existing per-repository worktrees through refs and derived worktree paths. Source: `README.md:14`, `src/paths.rs:35-44`
- Decision: Workspace refs point to current worktree paths because duplicating the same visible name as both a repo ref and worktree entry would be confusing; stable repository paths live in `.wtk/config.toml`. Source: user design decision, 2026-06-07.
- Decision: Workspace create/switch operations must preflight every linked repository and roll back all changed repositories if any branch/worktree cannot be created. This preserves all-or-nothing coordination instead of leaving repositories mixed across workspace states. Source: `README.md:108-110`, user design decision, 2026-06-07.
- Decision: Use `.wtk/config.toml` for mode and workspace ref metadata instead of constraining the rest of the Workspace repository contents. Source: user design decision, 2026-06-07.
- Decision: Use `Repository Mode` for the existing default single-repository behavior and `Workspace Mode` for the new coordinated multi-repository behavior; do not name the old mode after sibling layout because sibling layout is a path strategy, not the behavioral boundary. Source: `README.md:3-14`, user design decision, 2026-06-07.
- Decision: Workspace config should record stable repository paths, not per-worktree paths; for ref `A`, the Workspace Ref path is fixed as `refs/A`, the configured repository path must point to repository `A`, and the repository directory name must also be `A`. Mismatches fail fast. Source: user design decision, 2026-06-07.
- Decision: Require absolute paths for both configured repository paths and Workspace Ref targets to avoid ambiguous relative symlink interpretation and make validation deterministic. Source: user design decision, 2026-06-07.
- Decision: Workspace Mode must derive every repository branch and worktree path from the workspace name using the same sibling layout rules as Repository Mode. Source: `README.md:14`, `src/paths.rs:35-44`, user design decision, 2026-06-07.
- Decision: Run full preflight before any Workspace create/switch mutation, covering config mode, ref/repository naming consistency, absolute paths, Git repository validity, target branch/worktree availability, and clean-state requirements. Source: `README.md:108-110`, user design decision, 2026-06-07.
- Decision: Add `wtk workspace init` for creating Workspace Mode config and `wtk workspace add <repository-path>` for adding configured refs from repository paths normalized to absolute paths. Source: user design decision, 2026-06-07.
- Decision: Do not introduce a separate `workspace switch` verb for core worktree movement; existing worktree commands should inspect the current mode and run Repository Mode or Workspace Mode behavior. Source: `README.md:3-12`, user design decision, 2026-06-07.

### System Procedure

Workspace create/switch preflight:

1. Read `.wtk/config.toml` and require `mode = "workspace"`.
2. For every configured ref, require ref name, `refs/<name>`, and repository basename to match.
3. Require configured repository paths and existing Workspace Ref targets to be absolute paths.
4. Resolve every configured repository as a Git repository.
5. Derive the target branch and sibling-layout worktree path from the workspace name for every repository.
6. Verify every repository can create or switch to the target branch/worktree without collisions or dirty-state violations.
7. Mutate only after all repositories pass preflight.
8. If execution fails after mutation begins, roll back created worktrees, created branches, and changed Workspace Refs.

Workspace command routing:

1. Resolve the current mode from `.wtk/config.toml` when present.
2. In Repository Mode, keep existing single-repository command behavior.
3. In Workspace Mode, load configured refs and run the requested worktree operation against every ref repository.
4. Update `refs/<name>` only after the corresponding repository operation succeeds, and roll back all changed refs if any repository operation fails.

## Plan

<!-- Optional implementation step breakdown, created during Plan and updated during Implement. -->

## Notes

<!-- Optional sections — add what's relevant. -->

### Implementation

<!-- Files created/modified, decisions made during coding, deviations from design -->

### Verification

<!-- How the feature was verified: tests written, manual testing steps, results -->
