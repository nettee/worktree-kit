# Steps

## Step 1

- Added `src/auxiliary.rs` for `.wtk/config.toml` loading/saving, Auxiliary Repository Ref validation, group expansion, generated ref helpers, and `.wtk/worktrees.json` persistence.
- Added `wtk auxiliary-group add` and `wtk ag add` parser/runtime support, including duplicate repository and existing conflicting ref failures.
- Verified with `cargo test`, `uv run pytest e2e/test_auxiliary_group.py`, and the full `uv run pytest e2e`.

## Step 2

- Added repeatable `wtk new --ag <group>` and `--auxiliary-group <group>` support.
- Implemented coordinated creation for a Primary Repository worktree plus matching Auxiliary Repository worktrees, generated `refs/<auxiliary-name>`, and recorded expanded immutable state in `.wtk/worktrees.json`.
- Kept `wtk new` without Auxiliary Groups on the existing standalone path.
- Verified with `cargo test`, `uv run pytest e2e/test_auxiliary_group.py`, `uv run pytest e2e/test_pnpm.py`, and the full `uv run pytest e2e`.

## Step 3

- Made `wtk status` emit coordinated status when the current Primary worktree has auxiliary state.
- Made `wtk list` show ordinary and coordinated Primary worktrees together, with `auxiliary_refs` JSON/table summaries.
- Made `wtk remove` validate generated refs and remove recorded Auxiliary Repository worktrees with the Primary worktree.
- Made `wtk send-out` and `wtk bring-in` reject worktrees with auxiliary state.
- Verified with `cargo test`, `uv run pytest e2e/test_auxiliary_group.py`, and the full `uv run pytest e2e`.

## Step 4

- Removed legacy `wtk workspace <init|add|bootstrap>` parser/runtime dispatch, `.wtk-workspace.toml` mode dispatch, Workspace module code, generated Workspace `AGENTS.md` template, and Workspace e2e coverage.
- Updated CLI help, completion command set, list output naming, and CLI error tests to use Auxiliary Group language.
- Verified with `cargo check`, `cargo test`, and the full `uv run pytest e2e`.

## Step 5

- Updated `README.md`, `CONTEXT.md`, and `e2e/README.md` to describe Primary Repository, Auxiliary Repository, Auxiliary Group, generated refs, and `.wtk/worktrees.json`.
- Removed current user-facing Workspace Mode documentation while keeping only avoid-language references in the glossary.
- Verified with `rg` for old Workspace command/model references plus `cargo test` and the full `uv run pytest e2e`.
