---
id: 20260608-one-click-workspace-init
name: One Click Workspace Init
status: planned
created: '2026-06-08'
---

## Overview

### Problem Statement

- Workspace Mode setup is currently too manual for a common first-time initialization flow.
- The requested flow currently requires users to create and initialize a Git repository, run `wtk workspace init`, run `wtk workspace add` repeatedly, ignore generated `refs/`, and initialize `AGENTS.md`.

### Goals

- Provide a simple command that initializes a Workspace in one action.
- Reduce the number of manual setup steps needed before a user can work in Workspace Mode.

### Scope

- Covers Workspace Mode initialization ergonomics.
- Covers the setup steps named in GitHub issue #54: Git repository creation, Workspace initialization, adding multiple Linked Repositories, `.gitignore` setup for `refs/`, and `AGENTS.md` initialization.

### Success Criteria

- A user can complete the Workspace initialization flow with one clear `wtk` command instead of the current multi-step manual procedure.

## Research

### Existing System

- GitHub issue #54 requests a simpler one-command Workspace initialization flow because the current flow requires five manual steps: create a directory and run `git init`, run `wtk workspace init`, run `wtk workspace add` multiple times, add `refs/` to `.gitignore`, and initialize `AGENTS.md`. Source: `https://github.com/nettee/worktree-kit/issues/54`
- README documents `wtk workspace init` and `wtk workspace add` as the current Workspace Mode setup commands. Source: `README.md:12,84-90`
- README defines Workspace Mode as a Workspace repository with a tracked `.wtk-workspace.toml` manifest and generated `refs/<name>` entries whose targets are absolute paths to surfaced Repository Worktrees. Source: `README.md:18`
- README says `wtk workspace init` and `wtk workspace add` must be run from the Workspace main worktree. Source: `README.md:103`
- CLI parsing currently exposes only `wtk workspace init` and `wtk workspace add <repository-path>` under the `workspace` command. Source: `src/cli.rs:273-317,722-729`
- CLI dispatch routes `workspace init` to `workspace::init` and `workspace add` to `workspace::add`; these commands currently run with clipboard disabled. Source: `src/cli.rs:191-195`
- `workspace::init` resolves the current Git repository, requires the current worktree to be the Workspace main worktree, fails if `.wtk-workspace.toml` already exists, writes an empty Workspace manifest, creates `refs/`, and reports success. Source: `src/workspace.rs:124-147`
- `workspace::add` loads the Workspace manifest, requires the Workspace main worktree, requires the Workspace main worktree to be on a named branch, resolves the provided repository path as an absolute path, requires the linked repository main branch to match the Workspace branch, records the Linked Repository by basename, writes the manifest, writes refs, and rolls back manifest/ref changes if ref writing fails. Source: `src/workspace.rs:149-221`
- Workspace loading requires `.wtk-workspace.toml` and validates `mode = "workspace"` before resolving the Workspace repository context. Source: `src/workspace.rs:466-495`
- Workspace ref loading requires configured repository paths to be absolute, ref names to match repository basenames, repositories to resolve to main worktrees, and generated ref targets to match the expected Repository Worktree for the Workspace branch. Source: `src/workspace.rs:497-558`
- Existing E2E coverage initializes a Workspace by creating separate Git repositories, running `workspace init`, running `workspace add` for A and B, then committing `.wtk-workspace.toml`. Source: `tests/e2e.rs:167-185`
- Existing E2E coverage asserts `.wtk-workspace.toml` exists with `mode = "workspace"` and `refs/A` and `refs/B` point to the canonical repository paths after setup. Source: `tests/e2e.rs:187-202`
- Existing E2E coverage asserts Workspace membership changes from a linked Workspace Worktree fail with `workspace add must be run from the Workspace main worktree`. Source: `tests/e2e.rs:281-308`
- Existing E2E coverage asserts `wtk new` in Workspace Mode fails when `.wtk-workspace.toml` has uncommitted changes after adding a repository. Source: `tests/e2e.rs:424-459`
- The Workspace repository's current `.gitignore` pattern example in this repo ignores `.opencode/`, `.agents/`, `target/`, `__pycache__/`, and `specs/change/active`, but not generated `refs/`. Source: `.gitignore:1-6`
- Workspace Mode `new` currently requires `.wtk-workspace.toml` to exist in `HEAD` and have no uncommitted changes before continuing. Source: `src/workspace.rs:786-815`

### Design Inputs

- A previous Workspace Mode design left `.gitignore` handling for generated `refs/` as a follow-up decision, specifically asking whether `wtk workspace init` should automatically create or update `.gitignore` and how to fail when `refs/` is already tracked. Source: `specs/change/20260607-workspace-worktree-state-model/spec.md:116-120`
- The current Workspace Mode design treats Workspace membership changes as main-worktree-only, keeps runtime generated refs out of tracked manifest state, and records `refs/` as generated local state. Source: `specs/change/20260607-workspace-worktree-state-model/spec.md:57-64,89-94,124-129`
- The current glossary defines **Workspace**, **Workspace Manifest**, **Workspace Worktree**, **Workspace Ref**, and **Linked Repository** terms used by this feature. Source: `CONTEXT.md:15-45`
- Existing source and docs searches did not identify a current `AGENTS.md` initialization command, template, copy path, CLI subcommand, README convention, or test assertion. Source: repository search for `AGENTS|agents|init`, 2026-06-08; `src/workspace.rs:124-221`; `src/cli.rs:273-317,722-729`; `README.md:82-103`; `tests/e2e.rs:167-308`
- The existing file-initialization-like mechanism is specific to ignored `.env` files in worktree initialization; it is not a general template or workspace file generator. Source: `src/worktree.rs:220-249,1095-1136`; `tests/e2e.rs:1250-1358`

### Constraints & Dependencies

- Project rules require fail-fast behavior for missing config, bad inputs, failed subprocesses, violated invariants, and partial failures; the CLI README also says malformed Workspace manifests, missing refs, branch mismatches, dirty worktrees, failed Git commands, ignored `.env` copy failures, and clipboard failures are reported directly with non-zero exits for partial failure. Source: `AGENTS.md instructions supplied in conversation, 2026-06-08`; `README.md:136-138`
- Zest Dev requires Research facts to cite code, documentation, URL, or other compact fact sources. Source: `/Users/william/.agents/skills/zest-dev/research.md:8`

### Key References

- `https://github.com/nettee/worktree-kit/issues/54` - User-requested one-command Workspace initialization flow.
- `README.md:82-103` - Current documented Workspace Mode setup and constraints.
- `src/cli.rs:273-317,722-729` - Current `workspace` command surface and help text.
- `src/workspace.rs:124-221` - Current `workspace init` and `workspace add` behavior.
- `src/workspace.rs:786-815` - Current committed-manifest requirement for Workspace Mode `new`.
- `tests/e2e.rs:167-248,281-308,424-459` - Current Workspace setup and failure coverage.

## Design

### Design Summary

- Add a one-command Workspace bootstrap flow that runs inside an empty directory that will become the Workspace root.
- The bootstrap command owns the full initialization boundary described by issue #54: initialize Git, initialize Workspace Mode, add Linked Repositories, ignore generated `refs/`, initialize `AGENTS.md`, and create the initial commit.

### Design Decisions

- Decision: Treat one-click Workspace initialization as a full bootstrap command for a new Workspace, not as a thin wrapper around `workspace init` and repeated `workspace add` inside an already-created Git repository. Source: user design decision, 2026-06-08; issue #54 requires replacing the manual steps that include creating a directory and running `git init` in `https://github.com/nettee/worktree-kit/issues/54`; current `workspace::init` assumes an existing Git repository in `src/workspace.rs:124-147`.
- Decision: The bootstrap command should fail fast on unsafe or conflicting target state: missing required inputs, an unusable target directory, failed `git init`, invalid Linked Repository paths, duplicate Workspace Ref names, branch mismatches, `.gitignore` write failures, `AGENTS.md` conflicts, manifest/ref write failures, and any required subprocess failure. Source: user design decision, 2026-06-08; project fail-fast rule from `AGENTS.md instructions supplied in conversation, 2026-06-08`; README failure behavior in `README.md:136-138`.
- Decision: The bootstrap command must own `git init` and create the initial Workspace commit so users do not have to manually construct a Git state that Workspace Mode later depends on. Source: user design decision, 2026-06-08; Workspace Mode `new` requires `.wtk-workspace.toml` to be committed in `HEAD` and clean in `src/workspace.rs:786-815`; issue #54 lists manual `git init` and Workspace setup as current friction in `https://github.com/nettee/worktree-kit/issues/54`.
- Decision: Expose the command as `wtk workspace bootstrap <repo-path>...`, run from the directory that will become the Workspace root, without accepting a separate Workspace path argument. Source: user design decision, 2026-06-08; current Workspace command namespace is `wtk workspace <init|add>` in `src/cli.rs:273-317,722-729`.
- Decision: Initialize `AGENTS.md` from a minimal built-in Workspace template maintained as a source template file, not as a large hard-coded function string. Source: user design decision, 2026-06-08; repository search found no existing `AGENTS.md` template or generator, 2026-06-08.
- Decision: Require bootstrap to run from an empty directory. Do not support bootstrapping into an existing Git repository, a partially initialized Workspace, or a directory containing pre-existing files. Source: user design decision, 2026-06-08.
- Decision: Bootstrap creates a fixed initial file set: `.wtk-workspace.toml`, `.gitignore` containing `refs/`, and `AGENTS.md` from the source template, then commits those files. Source: user design decision, 2026-06-08; issue #54 lists Workspace initialization, ignoring `refs/`, and `AGENTS.md` initialization in `https://github.com/nettee/worktree-kit/issues/54`.
- Decision: Use plain `git init` and do not add special initial-branch alignment logic. The expected operating environment uses `main` as Git's default initial branch. Source: user design decision, 2026-06-08.
- Decision: Keep the existing `workspace add` branch-matching invariant for bootstrap; all Linked Repository main worktrees must be on `main` during bootstrap, and branch mismatches fail fast rather than being auto-corrected. Source: user design decision, 2026-06-08; current `workspace::add` compares Workspace branch and Linked Repository branch in `src/workspace.rs:152-166`.
- Decision: Reuse the existing Workspace initialization and membership logic for bootstrap after `git init`, extracting shared helpers where needed instead of duplicating manifest/ref behavior. Source: user design decision, 2026-06-08; current `workspace::init` and `workspace::add` behavior in `src/workspace.rs:124-221`.
- Decision: Do not add automatic rollback for failed bootstrap. The command should fail fast with clear diagnostics and leave partial state visible. Source: user design decision, 2026-06-08; project fail-fast rule from `AGENTS.md instructions supplied in conversation, 2026-06-08`.

### Derived Rules

- Because the command bootstraps the current Workspace directory, it runs inside the directory that will become the Workspace.
- Because bootstrap owns `git init`, it must validate that the current directory is empty before running `git init`.
- Because issue #54 asks to reduce the complete setup sequence, `.gitignore` and `AGENTS.md` initialization are part of the required success path rather than optional hidden best-effort steps.
- Because bootstrap owns `git init`, it should initialize the Workspace repository in one controlled path instead of accepting a manually pre-initialized Git repository with unknown state.
- Because bootstrap creates the initial commit, successful completion should leave the Workspace ready for normal Workspace Mode commands such as `wtk new <branch>`.
- Because commit creation is required for readiness, a failed `git commit` is a command failure, not a warning or partial success. Source: `src/workspace.rs:786-815`; `tests/e2e.rs:424-459`.
- Because the command does not accept a Workspace path, the current working directory is the only Workspace root candidate.
- Because the command receives Linked Repositories positionally, it requires at least one `<repo-path>`.
- Because Linked Repositories become Workspace Refs, duplicate repository basenames or duplicate derived ref names fail before mutation.
- Because existing `workspace add` already enforces Linked Repository branch and absolute path invariants, bootstrap should reuse the same validation semantics rather than inventing a weaker setup path. Source: `src/workspace.rs:149-221,497-558`.
- Because bootstrap reuses Workspace membership semantics, implementation should prefer shared helpers for manifest construction, repository validation, ref-name derivation, and ref symlink creation.
- Because bootstrap starts from an empty directory and does not roll back, failure recovery is to inspect the visible partial state and delete/recreate the Workspace directory before retrying.
- Because the `AGENTS.md` template is built in, the core bootstrap path needs no user-supplied template argument.
- Because the Workspace root must be empty, bootstrap creates `.gitignore` and `AGENTS.md` from scratch instead of merging with existing files.
- Because the template lives as a source file, future changes to Workspace guidance should be reviewable as ordinary template diffs.
- Because the initial file set is fixed, successful bootstrap should produce a predictable repository tree before Git metadata: `.wtk-workspace.toml`, `.gitignore`, `AGENTS.md`, and generated `refs/<name>` entries.

### Interfaces / APIs

- CLI: `wtk workspace bootstrap <repo-path>...`
- Arguments:
  - `<repo-path>...`: one or more Linked Repository paths.
- Behavior:
  - Must be run from the empty directory that will become the Workspace root.
  - Runs plain `git init`.
  - Creates `.wtk-workspace.toml`, `refs/<name>` entries, `.gitignore`, and `AGENTS.md`.
  - Creates an initial commit for the tracked initialization files.

### System Procedure

1. Validate the current directory is empty.
2. Validate at least one Linked Repository path was provided.
3. Preflight all Linked Repository inputs before mutation where possible: each path resolves to a Git repository main worktree, every repository is on `main`, and derived Workspace Ref names are unique.
4. Run `git init` in the current directory.
5. Reuse Workspace initialization logic to create `.wtk-workspace.toml` and `refs/`.
6. Reuse Workspace add semantics for each Linked Repository.
7. Write `.gitignore` with `refs/`.
8. Write `AGENTS.md` from the source template file.
9. Stage `.wtk-workspace.toml`, `.gitignore`, and `AGENTS.md`.
10. Create the initial commit.
11. Print concise success output.

### Change Scope

#### Impact Areas

- CLI command surface: add `workspace bootstrap`.
- Workspace initialization: add bootstrap orchestration and shared helpers for existing init/add behavior.
- Workspace guidance template: add a source template for generated `AGENTS.md`.
- Documentation and tests: document the one-command flow and verify fail-fast bootstrap behavior.

#### Planned File Changes

- `src/cli.rs` - parse and help-text support for `wtk workspace bootstrap <repo-path>...`.
- `src/workspace.rs` - add bootstrap orchestration and extract shared init/add helpers as needed.
- `src/templates/workspace/AGENTS.md` or equivalent - add the generated Workspace `AGENTS.md` template as a source file.
- `README.md` - document bootstrap as the recommended Workspace setup path and keep manual `init/add` as lower-level commands if still exposed.
- `tests/e2e.rs` - add bootstrap success and fail-fast coverage.

### Edge Cases

- Current directory is not empty.
- No Linked Repository paths are provided.
- A Linked Repository path does not resolve to a Git repository main worktree.
- Any Linked Repository main worktree is not on `main`.
- Two Linked Repositories derive the same Workspace Ref name.
- `git init`, file writes, staging, or commit fails.
- Bootstrap fails after mutation begins; partial state remains visible and the command exits non-zero.

### Verification Strategy

- E2E: bootstrap from an empty directory with two Linked Repositories on `main`; assert `.wtk-workspace.toml`, `.gitignore`, `AGENTS.md`, generated refs, and the initial commit exist.
- E2E: after successful bootstrap, run `wtk new <branch> --base main --no-clipboard` to verify the committed manifest is immediately usable.
- E2E: bootstrap rejects a non-empty directory before running `git init`.
- E2E: bootstrap rejects missing repo args, duplicate ref names, non-repository paths, and Linked Repositories not on `main`.
- Unit or focused integration coverage for any extracted shared helper where it reduces E2E setup burden.

## Plan

### Step 1: Bootstrap Command Skeleton

Type: AFK
Goal: Add the `wtk workspace bootstrap <repo-path>...` command surface and empty-directory preflight.
Scope: Update CLI parsing/help, add the Workspace bootstrap entrypoint, validate at least one repo path, and fail before mutation when the current directory is not empty.
Depends on: None

### Step 2: Workspace Bootstrap Core

Type: AFK
Goal: Build the full bootstrap flow using existing Workspace init/add semantics.
Scope: Run plain `git init`, reuse or extract helpers for manifest creation and Linked Repository membership, validate `main` branch/ref uniqueness, create generated refs, stage tracked files, and create the initial commit.
Depends on: Step 1

### Step 3: AGENTS Template And Gitignore

Type: AFK
Goal: Add the fixed generated guidance and ignore files required by bootstrap.
Scope: Add a source template for `AGENTS.md`, write it during bootstrap, create `.gitignore` with `refs/`, and include both in the initial commit.
Depends on: Step 2

### Step 4: Documentation And Verification

Type: AFK
Goal: Make the new setup path documented and regression-tested.
Scope: Update README Workspace setup docs and add E2E tests for successful bootstrap, immediate `wtk new`, and fail-fast cases.
Depends on: Step 3

## Notes

### Progress

- [x] Step 1: Bootstrap Command Skeleton
- [x] Step 2: Workspace Bootstrap Core
- [x] Step 3: AGENTS Template And Gitignore
- [ ] Step 4: Documentation And Verification

### Implementation

- Added `wtk workspace bootstrap <repo-path>...` parsing, help text, and dispatch.
- Added a Workspace bootstrap skeleton entrypoint that requires at least one Linked Repository path and rejects non-empty Workspace roots before mutation.
- Added the Workspace bootstrap core flow: preflight Linked Repository paths, require Linked Repository main worktrees on `main`, reject duplicate Workspace Ref names before mutation, run plain `git init`, write the Workspace manifest, create generated refs, stage `.wtk-workspace.toml`, and create the initial Workspace commit.
- Extracted shared Workspace file initialization for `workspace init` and `workspace bootstrap`.
- Added a source Workspace `AGENTS.md` template and bootstrap file generation for `AGENTS.md` plus `.gitignore` with `refs/`.
- Updated bootstrap staging so the initial commit includes `.wtk-workspace.toml`, `.gitignore`, and `AGENTS.md`.

### Verification

- `cargo test --lib cli::tests`
- `pytest e2e/test_cli_errors.py e2e/test_workspace_mode.py -k 'bootstrap or cli_usage'`
- `cargo test --lib workspace`
- `pytest e2e/test_workspace_mode.py -k bootstrap`
- Step 3 checks passed with generated file content and initial-commit assertions in bootstrap e2e coverage.
