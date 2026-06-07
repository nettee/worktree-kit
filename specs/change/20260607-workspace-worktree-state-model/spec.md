---
id: 20260607-workspace-worktree-state-model
name: Workspace Worktree State Model
status: planned
created: '2026-06-07'
---

## Overview

### Problem Statement

- PR 51 added Workspace Mode, but `wtk new` changes only Workspace Refs and linked repositories; the Workspace repository itself remains on its original Git branch.
- This makes the current coordinated workspace state hard to observe from normal Git tools and shell prompts.
- The Workspace model needs refinement before further implementation: a Workspace is a Git repository, so it should follow the same branch/worktree conventions as the repositories it manages.

### Goals

- Treat the Workspace repository as a first-class participant in Workspace Mode branch and worktree operations.
- Use a tracked `.wtk-workspace.toml` file as the Workspace Mode marker and stable workspace configuration.
- Keep runtime state out of `.wtk-workspace.toml`.
- Preserve the existing Sibling Layout rule for Workspace worktrees and linked Repository Worktrees.
- Drive the design through explicit high-level principles first, then derive lower-level command and file rules from those principles.

## Research

### Existing System

- PR 51's active implementation detects Workspace Mode by searching upward for `.wtk/config.toml` and reading `mode = "workspace"`. Source: `src/workspace.rs:102-118`
- `workspace::new` creates branches/worktrees in each configured linked repository and rewrites Workspace Ref symlinks, but it does not create or switch a branch/worktree in the Workspace repository itself. Source: `src/workspace.rs:221-286`
- `workspace::status` reports `mode`, `workspace_root`, `config`, and per-ref targets; it does not report the Workspace repository's current Git branch. Source: `src/workspace.rs:182-218`
- The current README defines Workspace Mode as a lightweight Workspace repository whose `refs/<name>` entries point to currently surfaced Repository Worktree paths. Source: `README.md:18`
- The current README says Workspace Mode commands fan out `wtk new`, `wtk remove`, `wtk send-out`, and `wtk bring-in` across every configured Workspace Ref. Source: `README.md:101`
- The existing Sibling Layout rule derives linked worktree paths as sibling directories named `<repo>-wt-<branch-slug>`. Source: `README.md:14`, `src/paths.rs:35-44`

### Design Inputs

- Manual verification showed `wtk status` reporting all Workspace Refs on branch `test-1` while the shell prompt in `/Users/william/projects/workspaces/vela-alert` still showed the Workspace repository on `main`. Source: user manual output, 2026-06-07.
- The implemented spec said "Worktree paths are derived from the workspace worktree/branch name" and "Switching the workspace changes which coordinated repository worktrees are surfaced through the workspace." Source: `specs/change/20260607-parent-directory-linked-worktrees/spec.md:26-28`
- The implemented spec also chose `.wtk/config.toml` as the mode marker and stable repository path store; this is now under reconsideration. Source: `specs/change/20260607-parent-directory-linked-worktrees/spec.md:99-107`
- The current glossary defines **Workspace** as storing workspace metadata in `.wtk/config.toml`; this term must be revised if `.wtk-workspace.toml` becomes the marker/config file. Source: `CONTEXT.md:24-26`

### Constraints & Dependencies

- Zest Dev requires facts confirmed through code, docs, or other sources to cite their source in the spec. Source: `/Users/william/.agents/skills/zest-dev/research.md:8`
- The project values fail-fast behavior for missing config, invalid refs, dirty worktrees, and failed Git commands. Source: `AGENTS.md instructions supplied in conversation, 2026-06-07`; `README.md:118-120`

## Design

### Design Summary

- Redesign Workspace Mode so the Workspace repository participates in the same branch/worktree lifecycle as the Linked Repositories it coordinates.
- A Workspace Worktree represents one coordinated state across the Workspace and every Linked Repository.
- Workspace Refs are generated per Workspace Worktree and point to the Repository Worktrees for that same coordinated branch/state.

### Design Decisions

- Decision: Treat the Workspace repository as a first-class participant in Workspace Mode operations; its own branch and worktree should reflect the coordinated branch and worktree state of every Linked Repository it manages. Source: user design decision, 2026-06-07; current implementation gap in `src/workspace.rs:221-286`; observed manual mismatch from user output, 2026-06-07.
- Decision: Replace `.wtk/config.toml` mode detection with a tracked root `.wtk-workspace.toml` Workspace Manifest; the file location changes, but the TOML schema stays aligned with the current `.wtk/config.toml` shape for now, including `mode = "workspace"` and `[workspace.refs.<name>]` entries. Source: user design decision, 2026-06-07; current `.wtk/config.toml` behavior in `src/workspace.rs:102-118`; prior schema in `specs/change/20260607-parent-directory-linked-worktrees/spec.md:99-107`.
- Decision: Store Linked Repository paths as absolute paths in `.wtk-workspace.toml` because the Workspace is a personal local coordination repository and path stability matters more than cross-machine portability. Source: user design decision, 2026-06-07; current implementation already requires absolute configured repository paths in `src/workspace.rs:548-570`.
- Decision: Treat `refs/` as generated local state that is not tracked by Git; each Workspace Worktree owns its own generated refs pointing to the linked Repository Worktrees for that Workspace branch/state. Source: user design decision, 2026-06-07; current implementation writes symlink refs with `write_ref` in `src/workspace.rs:654-671`.
- Decision: Require the Workspace Worktree branch name and every surfaced Linked Repository branch name to match exactly, and derive all corresponding worktree paths with the same Sibling Layout rule. Source: user design decision, 2026-06-07; existing Sibling Layout implementation in `src/paths.rs:35-44`.
- Decision: Allow Workspace membership changes, including `wtk workspace init` and `wtk workspace add`, only from the Workspace main worktree; task Workspace Worktrees consume the manifest but do not change membership. Source: user design decision, 2026-06-07; current Git context resolution can distinguish main vs linked worktree through `RepoContext.current_is_main` in `src/gitexec.rs:14-22`.
- Decision: Keep `wtk status` read-only in Workspace Mode; it validates the current Workspace Worktree invariant and fails fast on missing or incorrect generated refs instead of repairing them. Source: user design decision, 2026-06-07; project fail-fast behavior in `README.md:118-120`.
- Decision: Do not implement an explicit `wtk workspace sync` or repair command in this change; record generated-ref repair as follow-up work instead. Source: user design decision, 2026-06-07.
- Decision: Keep `wtk new` interaction consistent with Repository Mode: the command creates a new worktree, leaves the caller in the original directory, and reports/copies the new Workspace Worktree path as the useful success payload. Source: user design decision, 2026-06-07; Repository Mode `create` finishes with the new worktree path in `src/worktree.rs:120-137`.
- Decision: Remove Workspace Mode support for `send-out` and `bring-in` because coordinating those branch movements across Workspace and Linked Repositories creates too many edge cases; those commands should remain Repository Mode features only. Source: user design decision, 2026-06-07; current Workspace Mode implements both commands in `src/workspace.rs:345-520`.
- Decision: Continue rejecting `checkout` in Workspace Mode for this change; switching to existing coordinated branches/worktrees is a separate design problem from creating and removing coordinated worktrees. Source: user design decision, 2026-06-07; current CLI already rejects Workspace Mode checkout in `src/cli.rs:129-135`.
- Decision: Keep Workspace Mode `remove` invocation behavior consistent with Repository Mode for this change; do not add a new restriction that it must be run from the Workspace main worktree. Source: user design decision, 2026-06-07.
- Decision: Keep Workspace Mode `remove` command interaction consistent with Repository Mode; the difference is that the operation removes the coordinated Workspace Worktree plus matching linked Repository Worktrees instead of one Repository Worktree. Source: user design decision, 2026-06-07; Repository Mode remove command shape in `src/cli.rs:698-704`.
- Decision: Do not support migration or compatibility for PR 51's `.wtk/config.toml` Workspace Mode state; this change may replace the old Workspace Mode behavior directly. Source: user design decision, 2026-06-07.

### System Procedure

Workspace state invariant:

1. Load `.wtk-workspace.toml` from the Workspace repository root.
2. Resolve the current Workspace Worktree and its Git branch.
3. For every configured Linked Repository, derive the expected Repository Worktree path with the same branch and Sibling Layout rule.
4. Generate or validate `refs/<name>` in the current Workspace Worktree so it points to that expected Repository Worktree.
5. Fail fast if any linked branch, worktree, path, or ref violates the invariant.

Workspace status:

1. Load the manifest and current Workspace Worktree branch.
2. Compute expected `refs/<name>` targets for that branch.
3. Validate existing generated refs without mutating them.
4. Exit non-zero with clear diagnostics for missing refs, wrong targets, missing Repository Worktrees, or branch mismatches.

Workspace membership updates:

1. Resolve the Workspace repository and require the current Workspace Worktree to be the main worktree.
2. Update tracked `.wtk-workspace.toml`.
3. Generate or update main Workspace Worktree refs to point at each Linked Repository's main worktree.
4. Do not mutate existing task Workspace Worktrees during membership updates.

Workspace new:

1. Resolve the current Workspace repository and load `.wtk-workspace.toml`.
2. Preflight that the requested branch and all derived Sibling Layout worktree paths are available in the Workspace repository and every Linked Repository.
3. Create the Workspace Worktree for the requested branch.
4. Create the matching Repository Worktree in every Linked Repository using the same branch name and Sibling Layout rule.
5. Generate `refs/<name>` inside the new Workspace Worktree so each ref points to the matching Repository Worktree.
6. Run existing worktree initialization behavior for created Repository Worktrees where applicable.
7. Report/copy the new Workspace Worktree path as the success payload.
8. Roll back Workspace and Linked Repository branches, worktrees, and generated refs created by this operation if a required step fails after mutation begins.

Workspace remove:

1. Accept the same user-facing command shape as Repository Mode `remove`.
2. Resolve the target coordinated branch/worktree from the provided branch-or-path using Repository Mode-compatible behavior.
3. Preflight the Workspace Worktree and every matching linked Repository Worktree for clean state.
4. Remove the Workspace Worktree and all matching linked Repository Worktrees.
5. If `--delete-branch` is set, delete the same branch from the Workspace repository and every Linked Repository after worktree removal succeeds.
6. Roll back only resources changed by the current operation when a failure occurs after mutation begins.

### Follow-Up Issues

- Add an explicit generated-ref repair command, tentatively `wtk workspace sync`, after the new Workspace Worktree state model is implemented and manually validated.
- Decide whether Workspace Mode `remove` should eventually be restricted to the Workspace main worktree, or whether deleting from a task Workspace Worktree should remain allowed.
- Decide whether `wtk workspace init` should automatically create or update `.gitignore` so generated `refs/` are ignored, and how to fail when `refs/` is already tracked.

### Change Scope

#### Impact Areas

- Workspace Mode state model: Workspace repository branch/worktree state becomes part of coordinated `new`, `remove`, and `status` operations.
- Workspace configuration: replace `.wtk/config.toml` mode marker with tracked `.wtk-workspace.toml`.
- Generated refs: keep `refs/` untracked and regenerate/validate it per Workspace Worktree.
- Documentation: README must describe new Workspace Mode usage, tracked/untracked conventions, and the worktree relationship between Workspace and Linked Repositories.

#### Planned File Changes

- `CONTEXT.md` - keep canonical Workspace terminology aligned with the new model.
- `README.md` - document Workspace Mode setup, usage, Git tracking conventions, and Workspace/Linked Repository worktree linkage.
- `src/workspace.rs` - update mode detection, manifest loading, Workspace repo worktree participation, generated refs, status, rollback behavior, and remove Workspace Mode `send-out`/`bring-in`.
- `src/cli.rs` - adjust help text and command routing so Workspace Mode rejects `send-out`/`bring-in`.
- `tests/e2e.rs` - replace PR 51 expectations with Workspace repo worktree participation, manifest tracking, refs ignore behavior, linked worktree fan-out checks, and `send-out`/`bring-in` rejection in Workspace Mode.

### Edge Cases

- `.wtk-workspace.toml` exists but has malformed TOML, missing `mode = "workspace"`, relative repository paths, duplicate refs, or ref/repository basename mismatch.
- The Workspace Worktree branch does not match a generated ref target's linked Repository Worktree branch.
- A generated ref is missing, points to a non-worktree path, or points to the correct repository but wrong branch.
- A target Workspace or linked Repository Worktree path already exists before `new`.
- Any Workspace or linked Repository Worktree is dirty before `remove`.
- `checkout`, `send-out`, or `bring-in` is invoked from Workspace Mode.

### Verification Strategy

- E2E tests should cover `workspace init`, `workspace add`, `status`, `new`, and `remove` under the revised state model.
- E2E tests should assert that `.wtk-workspace.toml` is created with the current schema and `refs/` remains generated local state.
- E2E tests should assert that `wtk new <branch>` creates the Workspace Worktree plus all linked Repository Worktrees with matching branch names and Sibling Layout paths.
- E2E tests should assert that `wtk status` fails fast on missing or incorrect generated refs.
- E2E tests should assert that Workspace Mode rejects `checkout`, `send-out`, and `bring-in`.
- Existing Repository Mode tests should continue to pass unchanged.

## Plan

### Step 1: Manifest And Mode Foundation

Type: AFK
Goal: Replace PR 51's `.wtk/config.toml` Workspace Mode marker with tracked root `.wtk-workspace.toml`.
Scope: Update manifest read/write, upward discovery, `workspace init`, `workspace add`, and mode resolution. Keep the current TOML schema shape and absolute repository path validation.
Depends on: None

### Step 2: Workspace New

Type: AFK
Goal: Create a coordinated Workspace Worktree and matching linked Repository Worktrees with one command.
Scope: Update Workspace Mode `new` to preflight the Workspace repository and every Linked Repository, create matching branch/worktree sets with Sibling Layout, generate refs in the new Workspace Worktree, preserve existing initialization behavior for linked Repository Worktrees, report the new Workspace Worktree path, and roll back operation-created resources on failure.
Depends on: Step 1

### Step 3: Workspace Status

Type: AFK
Goal: Make `wtk status` reflect and validate the current Workspace Worktree state.
Scope: Report the Workspace repository branch/worktree and per-ref linked Repository Worktree state from `.wtk-workspace.toml`; validate generated refs read-only and fail fast on missing refs, wrong targets, missing worktrees, or branch mismatches.
Depends on: Step 1, Step 2

### Step 4: Workspace Remove

Type: AFK
Goal: Remove a coordinated Workspace Worktree set using Repository Mode-compatible command semantics.
Scope: Update Workspace Mode `remove` to resolve the target branch/worktree, preflight clean state across Workspace and linked Repository Worktrees, remove all matching worktrees, optionally delete matching branches with `--delete-branch`, and roll back current-operation mutations on failure.
Depends on: Step 1, Step 2

### Step 5: Unsupported Commands And Documentation

Type: AFK
Goal: Remove unsupported Workspace Mode workflows and document the revised usage model.
Scope: Reject `checkout`, `send-out`, and `bring-in` in Workspace Mode; update CLI help and README with setup, usage, tracked/untracked conventions, and Workspace/Linked Repository worktree linkage.
Depends on: Step 1

### Step 6: Regression Coverage

Type: AFK
Goal: Protect the revised Workspace Mode state model and Repository Mode compatibility.
Scope: Replace old Workspace Mode E2E expectations with manifest, generated refs, Workspace Worktree participation, `new`, `status`, `remove`, unsupported command rejection, rollback, and README-relevant behavior checks. Keep existing Repository Mode tests passing.
Depends on: Steps 1-5

## Notes

### Progress

- [ ] Step 1: Manifest And Mode Foundation
- [ ] Step 2: Workspace New
- [ ] Step 3: Workspace Status
- [ ] Step 4: Workspace Remove
- [ ] Step 5: Unsupported Commands And Documentation
- [ ] Step 6: Regression Coverage
