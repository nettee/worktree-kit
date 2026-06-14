---
id: 20260614-wtk-auxiliary-agent-guidance
name: Wtk Auxiliary Agent Guidance
status: implemented
created: '2026-06-14'
---

## Overview

### Problem Statement

- The previous change replaced Workspace Mode with the Primary Repository / Auxiliary Repository model.
- That removal also removed the generated Workspace `AGENTS.md` guidance that told agents how to work in coordinated repositories.
- WTK needs a replacement guidance artifact named `WTK-AUXILIARY.md` that explains the auxiliary-repository workflow without reintroducing Workspace Mode language.
- The new guidance should be based on the previous `AGENTS.md` behavior where it still applies, but should reflect that agents open the Primary Repository and that specs remain there.

### Goals

- Define the role, location, and lifecycle of `WTK-AUXILIARY.md`.
- Explain what agents should do when a Primary worktree has Auxiliary Repositories.
- Generate the guidance into each coordinated Primary worktree while keeping it ignored by Git.
- Preserve fail-fast development guidance: missing or inconsistent auxiliary state should be visible, not papered over.
- Keep the spec discussion focused on large decision principles first, then derive detailed rules from those principles.

### Scope

- Add or update project behavior and tests needed for `WTK-AUXILIARY.md`.
- Do not bring back Workspace Mode, `.wtk-workspace.toml`, or generated Workspace `AGENTS.md`.

## Research

See [design.md](./design.md).

## Design

### Design Summary

Generate `WTK-AUXILIARY.md` as local ignored guidance in every coordinated Primary worktree created by `wtk new --ag` / `--auxiliary-group`. The file is not a tracked project document and is not source-of-truth state; it is a worktree-specific reminder for agents that names the concrete Auxiliary Repositories, their generated `refs/<name>` entrypoints, and their resolved worktree targets.

Creation treats the guidance file and common Git exclude rule as required generated state, so synchronous write failures fail the coordinated creation and use the existing rollback path. After creation, `wtk status` and `wtk remove` do not strongly validate the file because coordination state remains in `wtk/worktrees.json`, Auxiliary markers, and generated refs.

See [design.md](./design.md) for design detail.

### E2E Acceptance Gate (EAG)

Acceptance behavior: creating a coordinated Primary worktree with `wtk new --ag <group>` creates `WTK-AUXILIARY.md` in that Primary worktree, writes common Git exclude rules for `/refs/` and `/WTK-AUXILIARY.md`, and the file lists the concrete Auxiliary Repository refs and targets while leaving `git status --porcelain --untracked-files=all` clean.

Verification path: `uv run --project e2e pytest e2e/test_auxiliary_group.py`.

## Plan

### Step 1: Generate Auxiliary Guidance

Type: AFK
Goal: Create the local `WTK-AUXILIARY.md` guidance file for coordinated Primary worktrees.
Scope: Add a small generator near the auxiliary coordination code, render concrete auxiliary names, `refs/<name>` paths, worktree targets, Primary Repository spec ownership, generated-ref rules, and per-repository PR guidance, then call it during `wtk new --ag` after auxiliary refs/state are available.
Depends on: None

### Step 2: Ignore Generated Guidance

Type: AFK
Goal: Keep `WTK-AUXILIARY.md` out of Git consistently with generated Auxiliary Refs.
Scope: Extend generated exclude installation so the true common Git exclude file, `.git/info/exclude`, ignores both `/refs/` and `/WTK-AUXILIARY.md`, without using worktree-specific `core.excludesFile`, and fail coordinated creation if exclude installation fails.
Depends on: Step 1

### Step 3: E2E Coverage

Type: AFK
Goal: Prove the generated guidance exists, contains the useful concrete entries, and remains ignored.
Scope: Extend `e2e/test_auxiliary_group.py` to assert `WTK-AUXILIARY.md` exists in coordinated Primary worktrees, includes Primary/Auxiliary guidance and concrete refs/targets, is absent for standalone worktrees, and does not appear in `git status` or staged changes.
Depends on: Step 2

### Step 4: Documentation Follow-Up

Type: AFK
Goal: Keep project documentation aligned with implemented behavior.
Scope: If the implementation changes documented behavior, usage, commands, setup, or workflows, update the relevant project docs.
Depends on: Step 3

## Progress

- [x] Step 1: Generate Auxiliary Guidance
- [x] Step 2: Ignore Generated Guidance
- [x] Step 3: E2E Coverage
- [x] Step 4: Documentation Follow-Up

## Implementation

See [steps.md](./steps.md).
