# Steps

## Step 1

Added `wtk delete` CLI parsing, help, dispatch, completion list entry, unsupported argument failures, and pre-prompt stdin/stdout TTY enforcement.

Verification: added parser unit coverage; `cargo check` passed before formatting.

## Step 2

Built delete candidates from the same repository row/list data path used by `wtk list`, excluding main/current, locked/error, auxiliary-side, and structurally invalid coordinated rows.

Verification: `cargo check` passed before formatting.

## Step 3

Added `dialoguer` MultiSelect selection flow and exact literal `Y` line confirmation; empty selection and non-`Y` confirmation cancel successfully.

Verification: `cargo check` passed before formatting.

## Step 4

Implemented confirmed batch deletion with forced worktree removal for dirty selections, coordinated primary cascade deletion, branch preservation, per-item success/failure output, and non-zero result on partial failure.

Verification: `cargo check` passed before formatting.

## Step 5

Added Unix PTY-driven e2e coverage for interactive `wtk delete`, including standalone Space/Enter selection plus exact `Y`, cancellation by empty selection and non-`Y`, dirty worktree deletion, branch preservation, non-TTY failure, coordinated primary cascade deletion, and a coordinated structural failure/protection path.

Verification: `uv run pytest e2e/test_interactive_delete.py` passed (7 tests).

Follow-up fix: coordinated delete candidates now determine and display dirty state for each member, exclude broken coordinated rows with diagnostics instead of aborting the whole selector, use cancelable prompt interaction, and keep selector order aligned with `wtk list`. Added regression coverage for dirty auxiliary summary and broken coordinated row isolation.

Follow-up verification: `cargo fmt && cargo check && cargo test && uv run pytest e2e/test_interactive_delete.py` passed (8 interactive e2e tests).

## Step 6

Ran the EAG verification path for interactive delete and existing repository/coordinated coverage.

Verification: `uv run pytest e2e/test_interactive_delete.py` passed (7 tests); `uv run pytest e2e/test_repo_mode.py e2e/test_auxiliary_group.py` passed (46 tests).

Follow-up verification: `uv run pytest e2e/test_repo_mode.py e2e/test_auxiliary_group.py` passed (46 tests).

## Step 7

Updated README/User Guide command documentation for interactive delete behavior.

Verification: documentation-only sync; no docs build configured.
