# Steps

## Step 1

- changed: Replaced split `CopyConfig` table with top-level optional `copy = [...]`, removed runtime defaults, rejected repo-local copy in effective and repo-local config loading, and updated default config template.
- verified: `cargo check`; `uv run --project e2e pytest e2e/test_auxiliary_group.py` covers repo-local copy rejection in Auxiliary Group paths; e2e EAG after all steps.
- deviations-followups: None.

## Step 2

- changed: Added unified Copy Pattern validation and globset-based matching over Git-reported ignored untracked files/symlinks, including overlap dedupe, pre-mutation glob compilation, and globbed directory pattern descendant handling.
- verified: `cargo test`; `uv run --project e2e pytest e2e/test_env_copy.py` covers invalid pattern fail-fast, globbed directories, and directory/file distinction.
- deviations-followups: None.

## Step 3

- changed: Replaced recursive/exact per-file copy output with a concise copied-file count summary and no line for zero copied files.
- verified: `uv run --project e2e pytest e2e/test_env_copy.py e2e/test_repo_mode.py e2e/test_auxiliary_group.py e2e/test_pnpm.py`.
- deviations-followups: None.

## Step 4

- changed: Updated e2e fixtures to global multiline copy lists and added coverage for no runtime defaults, repo-local rejection, concise output, ignored-only directory behavior, and overlap dedupe.
- verified: `uv run --project e2e pytest e2e/test_env_copy.py`; related e2e suite passed.
- deviations-followups: None.

## Step 5

- changed: Ran and fixed the EAG until passing.
- verified: `uv run --project e2e pytest e2e/test_env_copy.py` passed.
- deviations-followups: None.

## Step 6

- changed: Updated README, user guide, e2e README, CLI help, and installer/default-config fixtures for global-only Copy Patterns.
- verified: `scripts/test-install-local.sh && scripts/test-install.sh` passed.
- deviations-followups: None.
