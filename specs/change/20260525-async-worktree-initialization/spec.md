---
id: 20260525-async-worktree-initialization
name: Async Worktree Initialization
status: implemented
created: '2026-05-25'
---

## Overview

### Problem Statement

- `wtk create <branch>` currently performs worktree initialization tasks synchronously before the main flow finishes.
- In the observed flow, ignored `.env` copying and `pnpm install` run before the success message and clipboard copy.
- This delays the user-visible completion point: `✓ created worktree ...` and `==> copied to clipboard: ...`.

### Goals

- Make `pnpm install` asynchronous so it does not block the main `wtk create` flow.
- Make ignored `.env` copying asynchronous for the same reason.
- Treat dependency install and `.env` copying as worktree initialization actions that can continue after the worktree path is ready.
- Let the main flow reach clipboard copy as quickly as possible after the worktree is created.

### Scope

- Applies to `wtk create` worktree creation behavior.
- Covers initialization actions shown in the provided command output:
  - copy ignored `.env` files into the new worktree
  - run `pnpm install` in the new worktree

### Success Criteria

- `wtk create auto-db-migration` reports the created worktree path and copies it to the clipboard without waiting for `.env` copy or `pnpm install` to complete.
- Initialization actions still run for the new worktree.
- The CLI output makes it clear which work completed in the main flow and which initialization work is continuing asynchronously.

## Research

### Existing System

- `wtk create` dispatches through `execute_worktree`, which constructs a `Session` and returns command failure when the callback returns an error. Source: `src/cli.rs:101-104,129-163`
- `worktree::create` currently snapshots ignored `.env` files, runs `git worktree add`, copies ignored `.env` files, runs `pnpm install`, then calls `finish` to print success and copy the path to the clipboard. Source: `src/worktree.rs:68-103`
- `worktree::checkout` uses the same synchronous ignored `.env` copy and `pnpm install` sequence before `finish`. Source: `src/worktree.rs:106-138`
- Ignored `.env` copy writes each snapshot into the new worktree, preserves file permissions, recreates symlinks, and returns relative paths for output. Source: `src/worktree.rs:618-670,777-783`
- Ignored `.env` discovery uses `git ls-files --others --ignored --exclude-standard --full-name -z -- .env :(glob)**/.env` and filters to paths whose file name is exactly `.env`. Source: `src/worktree.rs:741-764`
- `pnpm install` runs only when the worktree has `pnpm-lock.yaml` or `pnpm-workspace.yaml`. Source: `src/worktree.rs:787-830`
- The current `pnpm install` implementation is blocking because it calls `Command::new("pnpm").arg("install").current_dir(worktree_path).output()`. Source: `src/worktree.rs:796-804`
- `finish` prints the success message first, then writes the payload to the clipboard, then prints `copied to clipboard`. Source: `src/worktree.rs:832-847`

### Design Inputs

- Existing partial-success style wraps post-worktree failures with messages such as `worktree created, but ignored .env copy failed` and `worktree created, but pnpm install failed`. Source: `src/worktree.rs:89-97,804-824`
- Tests already cover ignored `.env` copying for root files, symlinks, permissions, nested paths, and ignored-only directories. Source: `tests/e2e.rs:97-260`
- Tests assert tracked `.env` files are not reported, similarly named files are not copied, ignored `.env` directories are ignored, no `.env` files are silent, and copy output uses git-root-relative paths one per line. Source: `tests/e2e.rs:414-565`
- The `pnpm install` E2E uses a fake `pnpm` on `PATH` and asserts the command receives `install`, runs in the linked worktree, and emits `running pnpm install`. Source: `tests/e2e.rs:286-326,991-1026`
- Unit tests already cover clipboard partial failure and pnpm-worktree detection. Source: `src/worktree.rs:851-922`

### Constraints & Dependencies

- Because `.env` copy and `pnpm install` currently return errors through the main `create` callback, making them asynchronous changes whether those failures can fail the `wtk create` process. Source: `src/cli.rs:156-163`, `src/worktree.rs:89-103,787-824`
- `checkout` currently shares the same initialization behavior as `create`, but the requested scope names `wtk create`; design needs to decide whether to keep `checkout` unchanged or refactor shared helpers carefully. Source: `src/worktree.rs:68-138`
- Existing tests use `Command::output()` to wait for the CLI process and then immediately inspect copied files or fake-pnpm logs, so asynchronous initialization will require test synchronization or adjusted output expectations. Source: `tests/e2e.rs:103-120,286-326,887-918`

### Key References

- `src/worktree.rs:68-103` - `create` ordering: env snapshot, worktree add, env copy, pnpm install, finish.
- `src/worktree.rs:618-783` - ignored `.env` copy and output helpers.
- `src/worktree.rs:787-830` - pnpm detection and synchronous install execution.
- `tests/e2e.rs:97-326,414-565` - create env-copy and pnpm-install behavior coverage.

## Design

### Design Summary

- Keep `git worktree add` in the blocking create path because the worktree path must exist before success/clipboard output.
- Move post-create initialization into a detached background `wtk init-worktree` subprocess started after `finish` succeeds.
- Add `wtk init-worktree` as an advanced command that performs worktree initialization from a staged `.env` snapshot and then runs `pnpm install`.
- Stage ignored `.env` snapshots before `git worktree add` to preserve current behavior: copy the exact ignored `.env` content, permissions, and symlink targets observed from the source checkout.
- Parent `wtk create` prints success and copies the path to clipboard before launching/continuing background initialization, so the user-visible main flow reaches clipboard quickly.

### Design Decisions

- Decision: Do not use a Rust background thread for initialization because the CLI process can exit immediately after success; use a detached child process so initialization can continue independently. Source: `src/worktree.rs:68-103`, `src/cli.rs:156-163`
- Decision: Keep `.env` snapshotting before `git worktree add`, then serialize the snapshot into a temporary staging directory for the child process. This preserves the existing snapshot semantics while allowing copy to occur after the main flow finishes. Source: `src/worktree.rs:76,89-96,618-729`
- Decision: Launch `wtk init-worktree` only after `finish` succeeds, so a clipboard failure remains visible as a partial failure and does not report a successful main flow followed by background work. Source: `src/worktree.rs:97-103,832-847`
- Decision: The background initializer should fail visibly through stderr/output and non-zero exit in the child, but parent `wtk create` should not convert child initialization failure into create failure because the requirement is non-blocking initialization. Source: `src/cli.rs:156-163`, `src/worktree.rs:89-103,787-824`
- Decision: Scope the behavioral change to `create` first; leave `checkout` synchronous unless explicitly changed later, because the request specifically targets `wtk create` and `checkout` currently shares similar but separate calls. Source: `src/worktree.rs:68-138`

### System Procedure

Flow:
  1. `wtk create` validates options and computes the target worktree path.
  2. Snapshot ignored `.env` files from the main root.
  3. Run `git worktree add` synchronously.
  4. Call `finish` to print `✓ created worktree ...` and copy the path to clipboard.
  5. Stage the `.env` snapshot into a temporary directory and spawn `wtk init-worktree <stage> <worktree-path>` without waiting.
  6. `init-worktree` copies staged ignored `.env` files, prints copy lines, runs `pnpm install` when applicable, and exits non-zero on initialization failure.

### Interfaces / APIs

- Advanced CLI command: `init-worktree <stage-dir> <worktree-path>`
  - Exposed as an advanced command for initializing a worktree after creation.
  - Reads staged metadata/files from `<stage-dir>`.
  - Deletes the stage directory after successful or failed initialization when possible.

### Change Scope

- Impact Areas:
  - CLI parsing: add advanced initializer command.
  - Worktree create flow: reorder success/clipboard before initialization and spawn detached initialization.
  - Initialization staging: serialize and replay ignored `.env` snapshots.
  - Tests: adjust expectations that currently assume copy/install complete before `wtk create` returns.
- Planned File Changes:
  - `src/cli.rs` - parse and dispatch `init-worktree` command.
  - `src/worktree.rs` - add staging/spawn/init helpers and update `create` ordering.
  - `tests/e2e.rs` - update create tests to wait for background initialization artifacts/logs and assert clipboard/success output precedes init output where practical.

### Edge Cases

- No ignored `.env` files: stage should still allow pnpm initialization to run when needed, with no copy output.
- Staged symlink snapshots: replay symlink targets as current copy logic does.
- Missing `pnpm`: child initializer exits non-zero and reports failure; parent create remains successful after clipboard.
- Parent executable path unavailable: fail fast after `finish` only if initialization cannot be started, with a clear error rather than pretending background init began.
- Clipboard failure: preserve existing partial-failure behavior and do not start background initialization when `finish` fails.

### Verification Strategy

- Env copy regression: existing ignored `.env` create tests should pass after adding a wait helper for background-created files. Source: `tests/e2e.rs:97-260,414-565`
- Pnpm regression: fake `pnpm` test should wait for the fake log and assert `ARGS:install` and linked-worktree `PWD`. Source: `tests/e2e.rs:286-326,991-1026`
- Main-flow ordering: add or update an E2E assertion that `created worktree` / `copied to clipboard` appear before initialization output when clipboard is enabled or use a fake slow pnpm to confirm `create` returns before install completes. Source: `src/worktree.rs:832-847`, `tests/e2e.rs:887-918`
- Unit coverage: keep `should_run_pnpm_install` coverage and add focused tests for snapshot staging/replay if the helpers are small enough to test without spawning a process. Source: `src/worktree.rs:851-922`

## Plan

- [x] Step 1: Add worktree initialization command
  - [x] Substep 1.1 Implement: Add CLI variant and parser path for `init-worktree`.
  - [x] Substep 1.2 Implement: Add worktree initializer entrypoint that runs env replay and pnpm install.
  - [x] Substep 1.3 Verify: Run targeted unit/build check for CLI compilation.
- [x] Step 2: Stage and spawn create initialization
  - [x] Substep 2.1 Implement: Move ignored `.env` snapshot/copy into `init-worktree` so it runs asynchronously from the source root.
  - [x] Substep 2.2 Implement: Update `create` to call `finish` before spawning detached initialization.
  - [x] Substep 2.3 Implement: Spawn the current executable with `init-worktree` args and null stdio without waiting.
  - [x] Substep 2.4 Verify: Run create-related tests and fix ordering/synchronization issues.
- [x] Step 3: Update async-aware test coverage
  - [x] Substep 3.1 Implement: Add polling helpers for background env copy and fake pnpm logs.
  - [x] Substep 3.2 Implement: Update existing create env-copy and pnpm tests for background completion.
  - [x] Substep 3.3 Implement: Add regression coverage that slow pnpm does not block create completion.
  - [x] Substep 3.4 Verify: Run full test suite.

## Notes

### Implementation

- `src/cli.rs` - added advanced `init-worktree <source-root> <worktree-path>` command parsing/dispatch.
- `src/worktree.rs` - changed `create` to finish success/clipboard first, then spawn detached `wtk init-worktree`; added `init_worktree` entrypoint for ignored `.env` copy and pnpm install.
- `tests/e2e.rs` - updated create env/pnpm tests for background completion and added a slow-pnpm regression test proving create does not wait for install.
- Design deviation: skipped temporary staging and instead made `init-worktree` read ignored `.env` files from `source-root`, making the `.env` copy itself fully asynchronous.

### Verification

- `cargo fmt --check` - passed.
- `cargo test` - passed: 10 unit tests, 25 e2e tests, 0 doctests.
