---
id: 20260625-copy-gitignore-patterns
name: Copy Gitignore Patterns
status: planned
created: '2026-06-25'
---

## Overview

WTK should replace the current split ignored-file copy configuration with one user-facing copy list that uses gitignore-style patterns.

### Problem Statement

- The current `[copy].recursive` and `[copy].exact` settings expose implementation categories to users.
- Users should be able to express copy intent as a list of ignored path patterns without learning WTK-specific matching categories.

### Goals

- Use a single `copy = [...]` configuration list.
- Use gitignore-style pattern syntax so the learning cost matches existing Git ignore knowledge.
- Prefer multiline TOML lists so adding, removing, and reviewing entries is straightforward.
- Configure Copy Patterns only through global `~/.wtk/config.toml`.

### Constraints

- Do not preserve compatibility with the old `[copy] recursive/exact` shape.
- Keep copy behavior limited to files and symlinks that Git reports as ignored.
- Do not support repo-local Copy Pattern config in this change.

### Success Criteria

- The initialized global config template can be written as:

  ```toml
  copy = [
    "**/.env",
    ".agents/",
  ]
  ```

- Existing worktree creation flows copy ignored `.env` files and ignored `.agents/` descendants when that template config is present.
- When no `copy` config is present, WTK copies no ignored files by default.
- Copy output is concise and does not print one line for every copied file when directory patterns match many files.

## Research

See [design.md](./design.md).

## Design

### Design Summary

Replace the current split copy table with a single ordered `copy = [...]` list of Copy Patterns. Patterns use gitignore-style syntax for user familiarity, but WTK still copies only files and symlinks that Git reports as ignored.

See [design.md](./design.md) for design detail.

### E2E Acceptance Gate (EAG)

Acceptance behavior: With global `~/.wtk/config.toml` containing multiline `copy = ["**/.env", ".agents/"]`, worktree creation copies only Git-ignored matching files/symlinks, leaves tracked matches alone, rejects repo-local `copy`, and reports copied files with concise summary output.

Verification path: `uv run --project e2e pytest e2e/test_env_copy.py`

## Plan

### Step 1 (AFK): Replace Copy Config Model

Goal: Represent Copy Patterns as one global `copy = [...]` list with no runtime defaults.
Scope: Update config deserialization/serialization, global config resolution for copy, repo-local copy rejection, and default config template generation.
Depends on: None

### Step 2 (AFK): Implement Gitignore-Style Copy Matching

Goal: Copy ignored files and symlinks selected by Copy Patterns.
Scope: Replace recursive/exact matching with unified pattern matching, preserve ignored-only behavior, dedupe overlapping patterns, reject unsafe entries, and keep target paths Git-root-relative.
Depends on: Step 1

### Step 3 (AFK): Simplify Copy Reporting

Goal: Align output with the unified Copy Pattern model.
Scope: Replace per-file recursive/exact reporting with concise copied-file count summaries and no copy line when zero files are copied.
Depends on: Step 2

### Step 4 (AFK): Update End-to-End Coverage

Goal: Prove the new config shape and behavior through black-box tests.
Scope: Update existing copy e2e tests for global-only multiline `copy = [...]`, ignored-only directory behavior, no implicit runtime defaults, repo-local rejection, and concise output.
Depends on: Step 3

### Step 5 (AFK): EAG Validation

Goal: Validate the completed change against the Spec's EAG before wrap-up work.
Scope: Run `uv run --project e2e pytest e2e/test_env_copy.py` and address failures.
Depends on: Step 4

### Step 6 (AFK): Documentation Sync

Goal: Keep project documentation aligned with the implemented behavior.
Scope: If the implementation changes documented behavior, usage, commands, setup, or workflows, update the relevant project docs.
Depends on: Step 5

## Progress

- [ ] Step 1 (AFK): Replace Copy Config Model
- [ ] Step 2 (AFK): Implement Gitignore-Style Copy Matching
- [ ] Step 3 (AFK): Simplify Copy Reporting
- [ ] Step 4 (AFK): Update End-to-End Coverage
- [ ] Step 5 (AFK): EAG Validation
- [ ] Step 6 (AFK): Documentation Sync

## Implementation

See [steps.md](./steps.md).

## Deferred Follow-Ups (DFU)

None.
