# Steps

## Step 1: Generate Auxiliary Guidance

- Added `WTK-AUXILIARY.md` rendering in `src/auxiliary.rs`.
- Coordinated `wtk new --ag` now writes the guidance file inside the existing synchronous rollback section after the coordinated `WorktreeEntry` is available.
- The guidance lists concrete Auxiliary Repository names, `refs/<name>` entrypoints, resolved target worktrees, Primary Repository spec ownership, generated-ref rules, and per-repository PR guidance.
- Verified by `cargo test`, `uv run --project e2e pytest e2e/test_auxiliary_group.py`, and the full `uv run --project e2e pytest e2e`.

## Step 2: Ignore Generated Guidance

- Updated generated auxiliary excludes to write `/refs/` and `/WTK-AUXILIARY.md` to the true common Git exclude file at `.git/info/exclude`.
- Removed the previous worktree-specific `core.excludesFile` installation path to avoid p10k issues.
- Preserved existing `.git/info/exclude` content while adding the generated WTK patterns idempotently.
- Verified by `cargo test`, `uv run --project e2e pytest e2e/test_auxiliary_group.py`, and the full `uv run --project e2e pytest e2e`.

## Step 3: E2E Coverage

- Extended `e2e/test_auxiliary_group.py` to assert the generated guidance contents, common exclude entries, clean Git status, and ignored `/refs/` behavior.
- Added standalone absence coverage in `e2e/test_repo_mode.py`.
- Updated older refs-dirty expectations to match the new common-exclude decision that `refs/` is generated local state.
- Verified by `uv run --project e2e pytest e2e/test_auxiliary_group.py`, targeted repo-mode E2E, and the full `uv run --project e2e pytest e2e`.

## Step 4: Documentation Follow-Up

- Updated `README.md` to document generated `WTK-AUXILIARY.md` and common exclude behavior for coordinated Primary worktrees.
- No glossary or ADR update was needed because the existing Primary/Auxiliary terms already cover the feature.
- Verified by `cargo test` and the full `uv run --project e2e pytest e2e`.
