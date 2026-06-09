---
id: 20260609-readable-wtk-list
name: Readable Wtk List
status: implemented
created: '2026-06-09'
---

## Overview

### Problem Statement

`wtk list` currently presents primarily YAML-shaped output. With many Repository Worktrees or Workspace Worktrees, the fully expanded output is hard to scan within one screen and repeats information that is not useful for day-to-day selection.

### Goals

- Show as much useful worktree information as possible in minimal vertical and horizontal space.
- Avoid showing absolute worktree paths in the default readable output; prefer branch names or worktree names.
- Sort list output by a recent-update signal so the most relevant worktrees appear first.

### Constraints

- Use the project's existing domain language for Repository Worktrees, Workspace Worktrees, and Workspace Refs.
- Confirm large decision principles first, then derive lower-level display rules from those principles where possible.
- Record code, documentation, or web-confirmed facts with sources as the spec develops.

## Research

See [design.md](./design.md).

## Design

See [design.md](./design.md).

## Plan

### Step 1: Repository Readable List

Type: AFK
Goal: Replace Repository Mode's default YAML list with a compact human-readable table.
Scope: Add the shared list row model, path-basename display names, short table header, short HEAD display, relative update text from HEAD committer time, current/main/dirty/error labels, sorting by update time, and plain-text rendering for Repository Worktrees.
Depends on: None

### Step 2: JSON Machine Output

Type: AFK
Goal: Add explicit machine-readable list output through `wtk list --json`.
Scope: Extend CLI parsing/help for `--json`; serialize Repository Mode rows as JSON with absolute path, full HEAD, branch/state, timestamps, labels, diagnostics, and no ANSI escapes. Remove YAML expectations from the list command contract.
Depends on: Step 1

### Step 3: Terminal Styling

Type: AFK
Goal: Improve default list scanability with terminal styling while preserving plain-text meaning.
Scope: Add output styling helpers for headers, current rows, and warning/error labels; keep text markers as the source of meaning; disable styling for JSON and when `NO_COLOR` is non-empty.
Depends on: Step 1, Step 2

### Step 4: Workspace List Rows

Type: AFK
Goal: Make Workspace Mode `wtk list` show Workspace Worktrees as compact rows.
Scope: Route `list` through Repository/Workspace Mode dispatch; discover Workspace Worktrees; render one row per Workspace Worktree using the same display, sorting, style, and JSON contracts as Repository Mode.
Depends on: Step 1, Step 2, Step 3

### Step 5: Workspace Ref Aggregates And Diagnostics

Type: AFK
Goal: Add Workspace Ref aggregate health to Workspace Mode list without expanding refs into default rows.
Scope: For each Workspace Worktree row, compute configured ref counts and broken/missing target diagnostics; show default summaries such as `refs 3/3 ok` or `refs 2/3 broken`; include per-ref details in JSON; keep local row/ref exceptions visible instead of aborting the whole list.
Depends on: Step 4

### Step 6: Documentation And Regression Coverage

Type: AFK
Goal: Lock in the new `wtk list` contract for users and future changes.
Scope: Update README and CLI help; replace Repository Mode YAML e2e assertions with readable and JSON assertions; add Workspace Mode e2e coverage for row-level listing and broken ref display; add unit coverage for parsing, sorting, display names, relative time, and styling fallback.
Depends on: Steps 1-5

## Progress

- [x] Step 1: Repository Readable List
- [x] Step 2: JSON Machine Output
- [x] Step 3: Terminal Styling
- [x] Step 4: Workspace List Rows
- [x] Step 5: Workspace Ref Aggregates And Diagnostics
- [x] Step 6: Documentation And Regression Coverage

## Implementation

See [steps.md](./steps.md).
