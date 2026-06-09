# Design

## Research

### Existing System

- `wtk list` is parsed as a no-argument command with only `-h/--help`; it has no output-format, sort, or verbosity flags today. Source: `src/cli.rs:611-625`, `src/cli.rs:722`.
- CLI execution routes `status` through Repository/Workspace Mode dispatch, but routes `list` directly to `worktree::list`; there is no Workspace Mode-specific list implementation today. Source: `src/cli.rs:142-153`.
- Repository Mode `list` builds `ListOutput { worktrees }` from `RepoContext.worktrees` and serializes it with `serde_yaml`. Source: `src/worktree.rs:67-80`, `src/worktree.rs:197-216`.
- Each listed Repository Worktree currently includes `path`, `branch`, `bare`, `head`, `is_main`, and `is_current`. Source: `src/worktree.rs:72-80`, `src/worktree.rs:203-210`.
- Worktree discovery shells out to `git worktree list --porcelain`, parses blocks in Git output order, and does not add an application-level sort. Source: `src/gitexec.rs:154-174`, `src/gitexec.rs:193-226`.
- Parsed worktree data currently contains path, branch, bare, and HEAD only; it does not include commit time, filesystem time, dirty state, ahead/behind counts, or last checkout time. Source: `src/gitexec.rs:6-12`, `src/gitexec.rs:193-226`.
- Repository Mode default worktree paths use Sibling Layout names derived from `<repo>-wt-<branch-slug>`. Source: `README.md:16`, `src/paths.rs:35-45`.
- Workspace Mode status output reports the Workspace Worktree, Workspace branch, manifest, and per-ref Repository Worktree targets, but this shape is separate from `wtk list`. Source: `src/workspace.rs:71-91`, `src/workspace.rs:282-316`.
- Existing shared output helpers write plain text messages and a success checkmark; there is no current ANSI color/styling helper. Source: `src/output.rs:1-17`.

### Design Inputs

- README documents `wtk list` as printing visible worktrees in YAML format. Source: `README.md:5-9`.
- Root CLI help also describes `list` as "List visible worktrees as YAML". Source: `src/cli.rs:670-680`.
- Existing e2e coverage parses `wtk list` as YAML and asserts absolute `path` values plus `branch` and `is_current`. Source: `e2e/test_repo_mode.py:39-59`.
- The e2e helper has a generic `parse_yaml` helper used by current CLI assertions. Source: `e2e/conftest.py:106-107`.
- The current dependency set includes `serde_yaml` and `serde_json` but no table-rendering or time-formatting crate. Source: `Cargo.toml:6-10`.
- The project glossary distinguishes Repository Worktrees, Workspace Worktrees, Workspace Refs, Repository Mode, Workspace Mode, and Sibling Layout; list design should use those terms precisely. Source: `CONTEXT.md`.

### Constraints & Dependencies

- Zest Dev requires every code-, document-, or web-confirmed research finding to cite its fact source. Source: `/Users/william/.agents/skills/zest-dev/research.md`.
- The project values fail-fast behavior: missing or invalid required state should produce clear errors rather than fallback or fabricated output. Source: AGENTS.md instructions supplied in conversation, 2026-06-09.
- Changing default `wtk list` away from YAML will require updating README, CLI help, and e2e tests that currently assert the YAML contract. Source: `README.md:5-9`, `src/cli.rs:670-680`, `e2e/test_repo_mode.py:39-59`.
- The `NO_COLOR` convention says command-line software that adds ANSI color by default should suppress color when `NO_COLOR` is present and non-empty. Source: https://no-color.org/.
- Git worktree porcelain output includes attributes such as `worktree`, `HEAD`, `branch`, `bare`, `detached`, `locked`, and `prunable`; Git show pretty formats include `%ct` for committer date as a UNIX timestamp. Source: Git worktree docs at https://git-scm.com/docs/git-worktree.html; Git show docs at https://git-scm.com/docs/git-show/2.24.0.

### Key References

- `src/worktree.rs:67-80,197-216` - current Repository Mode list payload and YAML serialization.
- `src/gitexec.rs:154-174,193-226` - Git worktree discovery and parser.
- `src/cli.rs:142-153,611-625,670-680,722` - command routing, parser, and help text.
- `src/workspace.rs:71-91,282-316` - Workspace Mode status data shape that may inform future list behavior.
- `src/output.rs:1-17` - current output helper style.
- `e2e/test_repo_mode.py:39-59` - current list behavior test.
- `README.md:5-18` - documented list output and Worktree/Workspace model.

## Design

### Design Summary

- Make `wtk list` default to a compact human-readable view optimized for scanning many worktrees.
- Move machine-readable output behind `wtk list --json`.
- Treat YAML list output as an old implementation detail, not the compatibility format for this change.
- Use each worktree directory name as the default human-readable display name; branch names are important metadata but are not guaranteed to match manually chosen worktree paths.

### Design Decisions

- Decision: Change the default `wtk list` output from YAML to a compact human-readable view because the command's primary day-to-day job is scanning visible worktrees, and the current YAML output is documented and tested as fully expanded machine-shaped output. Source: user decision, 2026-06-09; current YAML implementation in `src/worktree.rs:197-216`; current docs/tests in `README.md:5-9`, `e2e/test_repo_mode.py:39-59`.
- Decision: Add `wtk list --json` as the explicit machine-readable output path. The crate already depends on `serde_json`, while current `parse_list` has no output-format flag and must be extended. Source: user decision, 2026-06-09; dependencies in `Cargo.toml:6-10`; parser shape in `src/cli.rs:611-625`.
- Decision: Design Repository Mode and Workspace Mode together. Repository Mode list rows represent Repository Worktrees; Workspace Mode list rows represent Workspace Worktrees, with Linked Repository / Workspace Ref state summarized as compact aggregate status instead of expanded into one row per ref. Source: user decision, 2026-06-09; Workspace terminology in `CONTEXT.md`; Workspace status currently has per-ref state in `src/workspace.rs:71-91`, `src/workspace.rs:282-316`.
- Decision: The human-readable list may use terminal presentation capabilities such as color and emphasis to improve scanability, rather than limiting the design to plain text. Source: user decision, 2026-06-09; current output helpers are plain text in `src/output.rs:1-17`.
- Decision: Terminal styling must enhance information hierarchy but must not be the only carrier of meaning. Current/main/broken/ref status must remain visible in text; JSON output must not include ANSI escapes; human-readable output must degrade to plain text when styling is disabled, including non-empty `NO_COLOR`. Source: user decision, 2026-06-09; `NO_COLOR` convention at https://no-color.org/.
- Decision: Define "recently updated" as the current HEAD commit's committer time and sort descending by that timestamp. Dirty state should be shown as a row marker but should not change sort order, so saving files does not reorder the list. Missing HEAD or invalid commit-time lookup should fail or surface an explicit row state rather than inventing a fallback timestamp. Source: user decision, 2026-06-09; current worktree data includes HEAD in `src/gitexec.rs:6-12`, `src/worktree.rs:72-80`; `%ct` timestamp support in Git show docs at https://git-scm.com/docs/git-show/2.24.0.
- Decision: The default human-readable row display name should be the worktree directory name, not the branch name, because explicit worktree paths can diverge from branches. Absolute paths should stay out of the default view; branch should appear as secondary metadata when it differs from or usefully clarifies the display name. Source: user decision, 2026-06-09; `wtk new`/`checkout` support explicit `--path` in CLI help at `src/cli.rs:695-715`; current list stores absolute `path` and `branch` separately in `src/worktree.rs:72-80`.
- Decision: Include a short table header in the default human-readable output so compact columns remain understandable without YAML keys. Source: user decision, 2026-06-09; current YAML output provides explicit field names through serialized keys in `src/worktree.rs:67-80`, `src/worktree.rs:214-216`.
- Decision: Treat `wtk list` as a diagnostic/discovery command that displays per-worktree and per-Workspace-Ref exceptions as row state instead of aborting the whole list. This is an explicit exception to the project's usual fail-fast preference; the command must not hide errors as success, and must show clear `error` / `broken` text plus JSON details. Source: user decision, 2026-06-09; project fail-fast preference from AGENTS.md instructions supplied in conversation, 2026-06-09.
- Decision: Keep true discovery failures as command failures: if WTK cannot establish any trustworthy list root, such as not being in a Git repository or `git worktree list --porcelain` itself failing, exit non-zero with a clear error. Source: user decision, 2026-06-09; current Git command failures propagate through `Git::run` in `src/gitexec.rs:69-109`.

### System Structure

- Extend CLI parsing so `wtk list` accepts `--json`.
- Introduce a list view model that is separate from the serialized JSON contract:
  - Shared row fields: marker, display name, absolute path, branch/detached/bare state, short/full HEAD, update time, state labels, diagnostics.
  - Repository Mode rows represent Repository Worktrees.
  - Workspace Mode rows represent Workspace Worktrees and include Workspace Ref aggregate state.
- Add a terminal styling helper for human-readable output that:
  - Can render bold/color for headers, current rows, and warning/error state.
  - Emits plain text when styling is disabled.
  - Never affects JSON output.
- Add per-row enrichment:
  - HEAD committer timestamp for sorting and display.
  - Dirty marker where available.
  - Workspace Ref aggregate counts for Workspace Mode.

### System Procedure

Repository Mode list:

1. Resolve Repository Mode context and discover Repository Worktrees.
2. For each Repository Worktree, derive the display name from the path basename.
3. Enrich each row with branch/head/update/dirty/state labels.
4. Preserve local row diagnostics instead of dropping rows when enrichment fails.
5. Sort rows by update timestamp descending, then current, then main, then display name.
6. Render the short human-readable table by default or JSON when `--json` is set.

Workspace Mode list:

1. Resolve Workspace Mode and discover Workspace Worktrees for the Workspace repository.
2. Treat each Workspace Worktree as one row.
3. For each row, derive display name from the Workspace Worktree path basename and branch/head/update/dirty state from that Workspace Worktree.
4. For each configured Workspace Ref, compute whether the expected Repository Worktree target for the row's branch is present and valid.
5. Render aggregate ref state such as `refs 3/3 ok` or `refs 2/3 broken`; include detailed ref diagnostics only in JSON.
6. Sort and render with the same rules as Repository Mode.

### Interfaces / APIs

- `wtk list`
  - Default human-readable table.
  - Short columns: marker, `worktree`, `branch`, `updated`, `state`, `head`.
  - No absolute paths in default output.
- `wtk list --json`
  - Machine-readable output.
  - Includes absolute paths, full HEADs, branch/detached/bare state, update timestamps, labels, diagnostics, and Workspace Ref details.
  - Does not include ANSI styling.
- `wtk list -h/--help`
  - Documents the default readable table and `--json`.

### Change Scope

#### Impact Areas

- CLI contract: default `wtk list` changes from YAML to compact human-readable output; machine-readable output moves to JSON.
- Repository Mode list rendering: add sorting, display-name derivation, row enrichment, diagnostics, and styling.
- Workspace Mode list behavior: add mode dispatch and Workspace Worktree rows with ref aggregate state.
- Output styling: add ANSI styling with plain-text fallback.
- Documentation and tests: update README, CLI help, unit tests, and e2e coverage for the new contract.

#### Planned File Changes

- `src/cli.rs` - parse `wtk list --json`, route `list` through Repository/Workspace Mode dispatch, and update help text/tests.
- `src/worktree.rs` - replace YAML-only list rendering with Repository Mode readable/JSON list behavior and shared row enrichment where appropriate.
- `src/workspace.rs` - add Workspace Mode list behavior that summarizes Workspace Worktrees and Workspace Ref aggregate state.
- `src/output.rs` - add terminal styling helpers and plain-text fallback support.
- `src/gitexec.rs` - extend parsed worktree metadata if needed and add helper calls for commit timestamps/dirty state.
- `README.md` - document readable `wtk list`, `wtk list --json`, sorting, display names, and Workspace Mode summary behavior.
- `e2e/test_repo_mode.py` - replace YAML list assertions with readable and JSON list assertions.
- `e2e/test_workspace_mode.py` - add Workspace Mode list assertions for rows and ref aggregate state.

### Edge Cases

- Worktree path basename is unavailable or not valid Unicode.
- Branch is detached, missing, or bare.
- HEAD commit timestamp lookup fails for one row.
- Worktree has dirty changes.
- Two rows have the same update timestamp.
- Directory basename duplicates another row's display name.
- Workspace Manifest has configured refs, but one Workspace Worktree has missing, wrong, or unreadable generated refs.
- A linked Repository Worktree target for a Workspace Ref is missing.
- `NO_COLOR` is non-empty.
- Output is captured by tests or scripts.

### Verification Strategy

- Unit-test `wtk list --json` parsing and rejection of unknown list flags.
- Unit-test display-name derivation, sorting tie-breakers, relative-time formatting, and styling disabled behavior.
- E2E-test Repository Mode default output:
  - Has a header.
  - Shows path basename, branch, relative update time, state labels, and short head.
  - Does not show absolute paths.
  - Sorts by HEAD commit time descending.
- E2E-test Repository Mode JSON output:
  - Parses as JSON.
  - Includes absolute path and full HEAD.
  - Contains no ANSI escapes.
- E2E-test Workspace Mode default output:
  - Lists Workspace Worktrees as rows.
  - Shows aggregate refs status.
  - Shows broken refs as row state without aborting the whole list.
- E2E-test foundational failures remain non-zero when no trustworthy list can be established.
