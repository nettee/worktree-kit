---
id: 20260707-interactive-worktree-delete
name: Interactive Worktree Delete
status: planned
created: '2026-07-07'
---

## Overview

Users can already inspect Repository Worktrees with `wtk list`, but deleting more than one Repository Worktree is not yet an efficient guided CLI workflow.

Goals:
- Add an interactive CLI deletion flow that displays a worktree list similar to `wtk list`.
- Let users select multiple Repository Worktrees with Space and confirm deletion with Enter.
- Support batch deletion from the terminal without requiring users to manually copy paths into repeated commands.

Constraints:
- Follow the Zest Dev discussion flow before implementation.
- Record code, documentation, or web-confirmed facts with fact sources in the spec.
- Discuss and confirm large decision principles first; derive lower-level rules from those principles instead of asking about each detail individually.

## Research

See [design.md](./design.md).

## Design

### Design Summary

Add `wtk delete` as a human-only interactive deletion command. With no arguments it opens a multi-select list that mirrors the existing flat `wtk list` scanning model, lets users select linked Repository Worktrees with Space, shows a full deletion summary, and executes only after the user types exactly `Y`.

`wtk list` keeps its current default output. `wtk delete` reuses the same row data where possible, but treats coordinated Primary Repository worktrees as deletion roots: selecting one deletes the Primary Repository Worktree plus its recorded Auxiliary Repository Worktrees. Dirty selected worktrees are allowed and removed with force semantics after the explicit `Y` confirmation. The command does not delete branches.

See [design.md](./design.md) for design detail.

### E2E Acceptance Gate (EAG)

Acceptance behavior: in a TTY-driven e2e scenario, `wtk delete` displays selectable worktrees, accepts Space/Enter selection, requires exact `Y` confirmation, removes selected standalone and coordinated worktree sets including dirty members, leaves branches intact, and reports partial failures with a non-zero exit.

Verification path: add and run e2e coverage for interactive `wtk delete` using a pseudo-terminal/test harness, plus the existing repository/coordinated test suite.

## Plan

### Step 1 (AFK): Add `wtk delete` command shell

Goal: Introduce the new command without changing existing `wtk list` or `wtk remove` behavior.
Scope: Add CLI parsing/help/dispatch for `wtk delete`, require TTY for no-arg interactive mode, handle help/non-TTY/unsupported args, and add focused parser/error tests.
Depends on: None

### Step 2 (AFK): Build delete candidates from list-style worktree state

Goal: Produce the flat selectable model that powers the interactive prompt and confirmation summary.
Scope: Reuse existing repository/list/coordinated-state facts to build candidates with display fields, truncation, dirty/current/main/protected markers, coordinated member expansion, and full summary data.
Depends on: Step 1

### Step 3 (AFK): Implement interactive selection and strict confirmation

Goal: Deliver the human CLI flow before destructive execution.
Scope: Add `dialoguer` multi-select, render flat list rows similar to `wtk list`, support Space/Enter selection, cancellation/empty-selection exits, and exact uppercase `Y` line confirmation after a full deletion summary.
Depends on: Step 2

### Step 4 (AFK): Implement batch deletion execution

Goal: Remove selected standalone and coordinated worktrees according to confirmed semantics.
Scope: Add force worktree removal for dirty selected worktrees, coordinated cascading deletion from selected Primary Repository Worktrees, branch preservation, structural preflight failures, state updates, per-item success/failure summaries, and non-zero exit on any failure.
Depends on: Step 3

### Step 5 (AFK): Add e2e coverage for interactive delete

Goal: Prove the command works through realistic terminal behavior.
Scope: Add pseudo-terminal/test-harness coverage for standalone deletion, cancellation, non-`Y` cancellation, exact `Y` deletion, dirty deletion, branch preservation, non-TTY failure, coordinated cascade deletion, and at least one coordinated failure path.
Depends on: Step 4

### Step 6 (AFK): EAG Validation

Goal: Validate the completed change against the Spec's EAG before wrap-up work.
Scope: Run the automated e2e verification path defined in the Design section: interactive `wtk delete` pseudo-terminal coverage plus existing repository/coordinated tests.
Depends on: Step 5

### Step 7 (AFK): Documentation Sync

Goal: Keep project documentation aligned with the implemented behavior.
Scope: If the implementation changes documented behavior, usage, commands, setup, or workflows, update the relevant project docs.
Depends on: Step 6

## Progress

- [ ] Step 1 (AFK): Add `wtk delete` command shell
- [ ] Step 2 (AFK): Build delete candidates from list-style worktree state
- [ ] Step 3 (AFK): Implement interactive selection and strict confirmation
- [ ] Step 4 (AFK): Implement batch deletion execution
- [ ] Step 5 (AFK): Add e2e coverage for interactive delete
- [ ] Step 6 (AFK): EAG Validation
- [ ] Step 7 (AFK): Documentation Sync

## Implementation

See [steps.md](./steps.md).

## Deferred Follow-Ups (DFU)

None.
