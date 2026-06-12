---
id: 20260612-workspace-spec-helper-repo-refs
name: Workspace Spec Helper Repo Refs
status: planned
created: '2026-06-12'
---

## Overview

### Problem Statement

- Issue 70 says the current model is not friendly enough for persisting specs.
- The intended direction is to replace the existing Workspace Mode / Repository Mode / Workspace Repository / Linked Repository mental model with a Primary Repository centered model.
- Agents open the Primary Repository directly; it is the task entrypoint, context center, and natural home for specs.
- Specs remain under the Primary Repository in `specs/change/...`; they must not move into a dedicated auxiliary or workspace repository.
- Worktrees are created with zero or more selected Auxiliary Groups, and that selection is fixed at creation time and cannot be modified afterward.
- An Auxiliary Repository Ref is the local named reference from a Primary Repository to one Auxiliary Repository; the ref name must match the Auxiliary Repository path's final segment.
- An Auxiliary Group is a named local group of Auxiliary Repository Refs; multiple groups can be combined with repeated `--ag <group-name>` or `--auxiliary-group <group-name>` arguments on `wtk new`.
- In this change, Auxiliary Repository Refs are created or reused only as part of Auxiliary Group creation; standalone ref management commands are out of scope.
- Users create groups with `wtk auxiliary-group add <group-name> <repository-path>...`; `wtk ag ...` is a supported shorthand.
- Selecting no Auxiliary Groups is the standalone case, while selecting one or more Auxiliary Groups is the coordinated case.
- The default is no Auxiliary Groups; repository-level default group configuration is out of scope for this change.
- WTK should store Auxiliary Group configuration as local state under `.wtk/config.toml` and should not enforce whether that configuration is tracked or ignored by Git.
- WTK should store fixed per-worktree expanded Auxiliary Repository state under `.wtk/worktrees.json`; it does not need to preserve the selected Auxiliary Group names.
- Primary Repository worktrees with Auxiliary Repositories should generate `refs/<auxiliary-name>` entries that point to the corresponding Auxiliary Repository worktrees, following the existing Workspace Ref structure.
- The Primary Repository main worktree should not receive auxiliary refs or auxiliary worktree state; only linked Primary worktrees created by `wtk new --ag` have recorded auxiliary state.
- Auxiliary Repositories participate in coordinated code changes and PRs; they are not spec storage repositories.
- Legacy Workspace command entrypoints and artifacts are removed in this change: `wtk workspace init`, `wtk workspace add`, `wtk workspace bootstrap`, `.wtk-workspace.toml`, Workspace Repository bootstrapping, and generated Workspace `AGENTS.md` guidance.
- Existing general worktree commands remain, but their implementation should no longer dispatch between Repository Mode and Workspace Mode.

### Goals

- Decide the high-level Primary/Auxiliary repository model before specifying command details.
- Choose replacement terminology for the current Workspace/Repository Mode language and identify which old terms become historical or migration-only.
- Use Auxiliary Group as the primary user-facing concept instead of making users choose between Standalone Mode and Coordinated Mode directly.
- Preserve the useful coordinated worktree mechanics from Workspace Mode while removing the equal-peer workspace repository model.
- Preserve observable failures: invalid refs, missing helper repository state, or ambiguous spec locations should fail clearly instead of silently falling back.

### Source

- GitHub issue: https://github.com/nettee/worktree-kit/issues/70

## Research

See [design.md](./design.md).

## Design

### Design Summary

Replace the Workspace Repository centered model with Primary Repository local coordination. A Primary Repository owns specs and is the agent entrypoint; optional Auxiliary Repositories are reached through local Auxiliary Repository Refs and selected through Auxiliary Groups.

Auxiliary Groups are local configuration in `.wtk/config.toml`. `wtk auxiliary-group add` / `wtk ag add` creates a group from repository paths, automatically creating or reusing Auxiliary Repository Refs whose names match repository basenames. `wtk new` accepts repeatable `--ag` / `--auxiliary-group` flags, expands the selected groups, deduplicates repositories, creates matching Auxiliary Repository worktrees, writes generated `refs/<auxiliary-name>` entries in the Primary worktree, and records the expanded per-worktree state in `.wtk/worktrees.json`.

No selected Auxiliary Groups is the default and represents the standalone case. Selected groups are creation-time input only; existing worktrees store expanded Auxiliary Repository state and do not change when `.wtk/config.toml` changes.

### E2E Acceptance Gate (EAG)

Acceptance behavior: a user can create an Auxiliary Group, create a Primary worktree with `wtk new --ag <group>`, see generated `refs/<auxiliary-name>` entries pointing at created Auxiliary Repository worktrees, and remove the Primary worktree with its recorded Auxiliary Repository worktrees.

Verification path: `pytest e2e/test_auxiliary_group.py`.

## Plan

### Step 1: Auxiliary Group Configuration

Type: AFK
Goal: Add the local configuration model and group creation command.
Scope: Implement `.wtk/config.toml` loading/saving, Auxiliary Repository Ref validation, `wtk auxiliary-group add`, and `wtk ag add`, with parser/help/tests and fail-fast validation for invalid repositories, duplicate names, and existing conflicting refs.
Depends on: None

### Step 2: New Worktree Auxiliary Expansion

Type: AFK
Goal: Create Primary worktrees with selected Auxiliary Groups.
Scope: Add repeatable `wtk new --ag` / `--auxiliary-group`, expand selected groups, deduplicate resolved repositories, create matching Auxiliary Repository worktrees, generate `refs/<auxiliary-name>`, and write `.wtk/worktrees.json`.
Depends on: Step 1

### Step 3: Auxiliary-Aware Status, List, And Remove

Type: AFK
Goal: Make existing lifecycle commands understand fixed auxiliary worktree state.
Scope: Read `.wtk/worktrees.json`, validate generated refs, surface auxiliary diagnostics in status/list, list both ordinary Primary worktrees and Primary worktrees with auxiliary state, reject `send-out`/`bring-in` for worktrees with auxiliary state, and remove recorded Auxiliary Repository worktrees with the Primary worktree.
Depends on: Step 2

### Step 4: Workspace Model Cleanup

Type: AFK
Goal: Remove legacy Workspace/Repository Mode entrypoints and align user-facing docs with the Primary Repository and Auxiliary Group model.
Scope: Remove `wtk workspace <init|add|bootstrap>`, `.wtk-workspace.toml` mode detection, generated Workspace templates, and Workspace Mode docs/tests; keep general worktree commands but remove their Repository Mode vs Workspace Mode dispatch split.
Depends on: Step 3

### Step 5: Documentation Follow-Up

Type: AFK
Goal: Keep project documentation aligned with the implemented behavior.
Scope: If the implementation changes documented behavior, usage, commands, setup, or workflows, update the relevant project docs.
Depends on: Step 4

## Progress

- [ ] Step 1: Auxiliary Group Configuration
- [ ] Step 2: New Worktree Auxiliary Expansion
- [ ] Step 3: Auxiliary-Aware Status, List, And Remove
- [ ] Step 4: Workspace Model Cleanup
- [ ] Step 5: Documentation Follow-Up

## Implementation

See [steps.md](./steps.md).

## Deferred Follow-Ups (DFU)

- Add Auxiliary Group browsing and management commands such as `wtk ag list`, `wtk ag remove`, or `wtk ag update`.
- Consider adding `wtk new -a <group-name>` as a shorter alias for `wtk new --ag <group-name>`.
