# Design

## Research

### Existing System

- WTK parses config into `Config`, whose `copy` field is currently a `CopyConfig` table with separate optional `recursive` and `exact` lists. Source: `src/auxiliary.rs:12-28`.
- Effective config is loaded from legacy repo config, global `~/.wtk/config.toml`, and repo-local `.wtk/config.toml`; later files merge into earlier files. Source: `src/auxiliary.rs:688-695,764-780`.
- Current copy merge semantics override `recursive` and `exact` independently when each optional list is present. Source: `src/auxiliary.rs:113-120`.
- Current defaults are hardcoded as recursive file name `.env` and exact path `.agents`. Source: `src/worktree.rs:25-28`.
- Current recursive config only accepts terminal file names and rejects paths. Source: `src/worktree.rs:1586-1597`.
- Current exact config only accepts relative normal path components. Source: `src/worktree.rs:1599-1619`.
- Recursive matching currently uses `git ls-files --others --ignored --exclude-standard --full-name -z -- <name> :(glob)**/<name>` and filters by terminal file name. Source: `src/worktree.rs:1818-1855`.
- Exact matching currently uses `git ls-files --others --ignored --exclude-standard --full-name -z -- <relative>` and keeps the exact path or descendants. Source: `src/worktree.rs:1857-1885`.
- Copy output currently splits matched paths back into recursive and exact buckets for reporting. Source: `src/worktree.rs:1898-1951`.
- Project documentation currently describes the split default lists and their output labels. Source: `docs/user-guide.md:51-59`.
- The installer/default config template currently writes the split `[copy]` table. Source: `scripts/default-config.toml:1-6`.

### Design Inputs

- User requirement: merge the two copy settings into one `copy = [...]` setting.
- User requirement: use gitignore syntax to reduce learning cost.
- User requirement: format the list across multiple lines for easier user edits.
- User constraint: direct change only; no compatibility with the old config shape is required.

### Constraints & Dependencies

- The config model, serializer, default template, documentation, and e2e tests all reference the old split shape. Source: `src/auxiliary.rs:18-28`, `scripts/default-config.toml:1-6`, `docs/user-guide.md:51-59`, `e2e/test_env_copy.py:61-63,212-214,348-359,403-404`.
- Existing behavior deliberately copies only ignored files/symlinks, including ignored descendants under `.agents/` while leaving tracked descendants alone. Source: `docs/user-guide.md:51`, `src/worktree.rs:1857-1885`.

## Design Detail

### Design Decisions

- `copy = [...]` entries are Copy Patterns: gitignore-style patterns used to select ignored files or ignored directory descendants for copying. This removes the user-facing distinction between recursive file-name entries and exact path entries. Source: `src/auxiliary.rs:22-28`, `src/worktree.rs:1586-1619`.
- WTK must continue copying only files and symlinks that Git reports as ignored; tracked files remain outside Copy Pattern behavior even when they match a pattern. Source: `docs/user-guide.md:51`, `src/worktree.rs:1818-1885`.
- Copy Pattern matching should follow gitignore-style semantics rather than WTK-specific wildcard rules; invalid or unsafe copy intent such as empty entries, absolute paths, and parent-directory traversal must fail fast. Source: `src/worktree.rs:1586-1619`.
- The old `[copy] recursive/exact` config shape is intentionally removed without compatibility handling; old configs should fail to parse rather than silently migrate or continue. Source: `src/auxiliary.rs:22-28`, user decision.
- Runtime copy defaults are removed: missing `copy` means no ignored files are copied. The suggested `["**/.env", ".agents/"]` list belongs in the initialized/default config template, not in hidden code defaults. Source: `src/worktree.rs:25-28`, `scripts/default-config.toml:1-6`, user decision.
- Copy Patterns are read only from global `~/.wtk/config.toml`; repo-local config remains relevant for other WTK settings but does not provide Copy Patterns in this change. Source: `src/auxiliary.rs:688-784`, user decision.
- If repo-local `.wtk/config.toml` contains `copy`, WTK should fail fast with a clear error instead of ignoring it. Source: `src/auxiliary.rs:688-784`, user decision.
- Copy reporting should align with the unified Copy Pattern model and avoid printing one line per copied file when patterns or directories match many files. Source: `src/worktree.rs:1898-1951`, user decision.
- Copy reporting should use a compact total summary, such as `copied 12 ignored files`, and omit the line when no ignored files were copied. Source: `src/worktree.rs:1898-1951`, user decision.

### Derived Rules

- `**/.env` selects ignored `.env` files at any directory depth.
- `.agents/` selects ignored descendants under the Git-root-relative `.agents/` directory.
- Path patterns such as `apps/*/.env.local` are valid Copy Patterns.
- Copy Patterns are always interpreted relative to the source Repository Worktree's Git root.
- Documentation, default config, and tests should describe only `copy = [...]`.
- `copy = []` and missing `copy` both result in no Copy Patterns after config resolution.
- Error text for repo-local `copy` should explain that Copy Patterns are supported only in global `~/.wtk/config.toml`.
- Copy output should not expose old recursive/exact buckets or list every copied path.

### System Structure

- Change `Config.copy` from the current `CopyConfig` table to a top-level optional list of Copy Pattern strings.
- Keep auxiliary repositories and auxiliary groups in repo-local config loading, but resolve Copy Patterns from global config only.
- Add validation for Copy Pattern entries before any Git query runs.
- Replace `CopiedIgnoredFiles { recursive, exact }` with a unified structure that stores validated Copy Patterns.
- Use one snapshot path for all Copy Pattern matches, then dedupe by Git-root-relative path before copying.

### System Procedure

1. Load global `~/.wtk/config.toml` and extract `copy`, defaulting to `[]` when absent.
2. Load repo-local config for non-copy WTK settings; if it contains `copy`, stop with a clear error.
3. Validate each Copy Pattern as relative, non-empty, non-traversing gitignore-style copy intent.
4. Ask Git for ignored, untracked files and apply Copy Pattern matching relative to the source Repository Worktree root.
5. Snapshot matched files/symlinks, dedupe overlaps, copy them to the target Repository Worktree, and print one concise copied-count summary when count is greater than zero.

### Change Scope

Impact Areas:

- Config model: replace split copy table with one global list.
- Config loading: separate copy resolution from repo-local settings and fail fast on repo-local copy.
- Copy matching: replace recursive/exact matching with unified Copy Pattern matching.
- Output: replace per-file recursive/exact reporting with count summary reporting.
- Tests and docs: update fixtures, expected output, and user-facing examples to the new config shape.

Planned File Changes:

- `src/auxiliary.rs`: update config structs, parsing, serialization, and global/repo-local copy validation.
- `src/worktree.rs`: replace copied ignored file rule structures, validation, matching, reporting, and default handling.
- `scripts/default-config.toml`: write multiline `copy = [...]` template.
- `docs/user-guide.md` and `README.md`: describe global-only Copy Patterns and concise output.
- `e2e/test_env_copy.py`: update copy behavior coverage for new config shape and output.
- `e2e/test_repo_mode.py` and `e2e/test_auxiliary_group.py`: update old copy fixture syntax where those tests still need copy behavior.

### Edge Cases

- Missing global config and `copy = []` both mean no ignored copy work.
- Pattern overlaps copy each matched file once.
- Directory patterns such as `.agents/` copy ignored descendants, not tracked descendants.
- Tracked files matching `**/.env` are left alone because Git does not report them as ignored untracked files.
- Repo-local `copy` fails before worktree mutation begins.
- Invalid Copy Patterns fail before copy work begins.

### Verification Strategy

- Use black-box e2e coverage for the acceptance path because the feature is user-visible config plus real Git ignored-file behavior. Source: `e2e/README.md:1-14`.
- Run `uv run --project e2e pytest e2e/test_env_copy.py` as the EAG for copy behavior. Source: `e2e/README.md:9-14`.
- Keep focused unit tests only where validation or pattern normalization has edge cases that are awkward to express through e2e setup.
