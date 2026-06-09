# Steps

## Step 1: Repository Readable List

- Changed Repository Mode `wtk list` from YAML serialization to a compact table backed by a shared list row model.
- Added worktree basename display, branch metadata, relative HEAD commit time, state labels, short HEAD, row diagnostics, and sorting by HEAD committer timestamp.
- Verified with `e2e/test_repo_mode.py::test_repo_mode_status_and_list_readable`, `cargo test`, and full Python e2e/release tests.

## Step 2: JSON Machine Output

- Added `wtk list --json` parsing and JSON serialization with absolute paths, full HEADs, timestamps, labels, dirty state, diagnostics, and no ANSI styling.
- Updated CLI help so the default command is described as a compact table instead of YAML.
- Verified with `e2e/test_repo_mode.py::test_repo_mode_list_json`, `cargo test`, and full Python e2e/release tests.

## Step 3: Terminal Styling

- Added a small output styling helper for headers, current rows, warning rows, and error rows.
- Styling is enabled only for terminal stdout when `NO_COLOR` is unset/empty, and it is never applied to JSON output.
- Verified through unit coverage for list rendering helpers plus captured e2e output remaining plain text.

## Step 4: Workspace List Rows

- Routed `wtk list` through Repository/Workspace Mode dispatch.
- Added Workspace Mode list output with one row per Workspace Worktree using the same table, sorting, style, and JSON contracts as Repository Mode.
- Verified with `e2e/test_workspace_mode.py::test_workspace_mode_list_shows_workspace_rows_and_ref_health` and full Python e2e/release tests.

## Step 5: Workspace Ref Aggregates And Diagnostics

- Added Workspace Ref health summaries to Workspace Worktree rows, including `refs n/m ok|broken` table state and per-ref JSON details.
- Local Workspace Ref failures are surfaced as broken row/detail diagnostics instead of aborting the whole list.
- Verified by deleting one generated Workspace Ref in e2e and confirming default output plus JSON details show the broken ref while the command succeeds.

## Step 6: Documentation And Regression Coverage

- Updated README usage and Workspace Mode documentation for readable `wtk list`, `wtk list --json`, sorting, basename display, and Workspace Ref summaries.
- Replaced old Repository Mode YAML list assertions with readable/JSON assertions and added Workspace Mode list coverage.
- Verified with `cargo test` and `uv run --project e2e pytest e2e tests/test_release.py`.
