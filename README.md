# worktree-kit

`worktree-kit` provides the `wtk` CLI for common Git worktree workflows:

- `wtk new` creates a new branch in a linked worktree.
- `wtk checkout` checks out an existing branch or ref in a linked worktree.
- `wtk status` prints current repository/worktree status in YAML format.
- `wtk list` prints visible worktrees in a compact table.
- `wtk remove` removes a linked worktree.
- `wtk send-out` moves the current main-worktree branch to a linked worktree.
- `wtk bring-in` moves a linked worktree branch back into the main worktree.
- `wtk auxiliary-group add` creates a local group of Auxiliary Repositories for coordinated multi-repository worktrees.

`wtk create` remains available as a compatibility alias for `wtk new`.

The Primary Repository is the repository an agent opens directly for a task. By default, `wtk new` creates a standalone linked worktree for that repository using the Sibling Layout: sibling directories named `<repo>-wt-<branch-slug>`.

For coordinated changes, a Primary Repository stores local Auxiliary Groups in `.wtk/config.toml`. `wtk new --ag <group>` expands those groups, creates matching Auxiliary Repository worktrees, writes generated `refs/<auxiliary-name>` entries in the Primary worktree, and records fixed expanded state in `.wtk/worktrees.json`.

## Install

One-click install:

```bash
curl -fsSL https://raw.githubusercontent.com/nettee/worktree-kit/main/scripts/install.sh | sh
```

The installer downloads the matching GitHub release asset, verifies its checksum, and installs `wtk` into `${WTK_INSTALL_DIR:-$HOME/.local/bin}`. It prints PATH and completion setup guidance after verifying `wtk --version`.

Upgrade an existing release install in place:

```bash
wtk upgrade
```

Local source install for development machines:

```bash
sh scripts/install-local.sh
```

Check the installed version:

```bash
wtk --version
```

## Development

`wtk` is now a Rust CLI.

Build locally:

```bash
cargo build --release --bin wtk
```

Run the full test suite:

```bash
cargo test
uv run --project e2e pytest e2e tests/test_release.py
sh scripts/test-install.sh
sh scripts/test-install-local.sh
```

## Usage

```bash
wtk new feature/foo
wtk new feature/from-current --from-current
wtk checkout feature/existing
wtk status
wtk list
wtk list --json
wtk remove ../repo-wt-feature-foo
wtk send-out
wtk bring-in feature/foo
wtk auxiliary-group add full-stack /absolute/path/to/api /absolute/path/to/web
wtk new feature/full-stack --ag full-stack
wtk upgrade
```

`wtk list` is optimized for scanning. The default output is a compact table with the worktree directory name, branch, relative HEAD commit time, state labels, and short HEAD. It does not print absolute paths by default. Rows are sorted by the current HEAD commit's committer time, newest first; dirty state is shown as a label but does not affect sorting.

Use `wtk list --json` for machine-readable output. JSON includes absolute paths, full HEADs, timestamps, labels, diagnostics, and Auxiliary Ref details.

## Auxiliary Groups

Create a local Auxiliary Group from the Primary Repository:

```bash
wtk auxiliary-group add full-stack /absolute/path/to/api /absolute/path/to/web
```

`wtk ag add` is a shorthand for `wtk auxiliary-group add`. Group creation resolves each repository path to a Git main worktree, derives the Auxiliary Repository Ref name from the repository directory name, creates or reuses `[auxiliaries.<name>]`, and writes `[groups.<group-name>]` in `.wtk/config.toml`. For backward compatibility, WTK still reads the legacy `$(git rev-parse --git-common-dir)/wtk/config.toml` file when `.wtk/config.toml` is absent.

Inspect configured groups:

```bash
wtk ag list
```

Remove a group definition:

```bash
wtk ag remove full-stack
```

Removing a group also prunes any Auxiliary Repository Ref entries that are no longer referenced by any remaining group.
If no Auxiliary Groups remain, WTK removes the now-empty config file instead of leaving behind an empty stub, including when migrating from the legacy git-common-dir config path.

The generated config shape is:

```toml
[auxiliaries.api]
repository = "/absolute/path/to/api"

[auxiliaries.web]
repository = "/absolute/path/to/web"

[groups.full-stack]
auxiliaries = ["api", "web"]
```

Create a coordinated worktree by selecting one or more groups:

```bash
wtk new feature/full-stack --ag full-stack
wtk new feature/full-stack --auxiliary-group full-stack
```

No selected Auxiliary Groups is the standalone case. With selected groups, `wtk new` creates the Primary Repository worktree plus matching Auxiliary Repository worktrees for the same branch. The Primary worktree receives generated `refs/<auxiliary-name>` entries pointing to the Auxiliary Repository worktrees. `.wtk/worktrees.json` stores the expanded Auxiliary Repository state by absolute Primary worktree path; changing the config later does not mutate existing worktrees. For backward compatibility, WTK still reads the legacy `$(git rev-parse --git-common-dir)/wtk/worktrees.json` file when `.wtk/worktrees.json` is absent.

Coordinated Primary worktrees also receive a generated `WTK-AUXILIARY.md` file that lists the concrete Auxiliary Repository refs and targets for agents. WTK keeps both `/refs/` and `/WTK-AUXILIARY.md` ignored through `.git/info/exclude`.

`wtk status` validates generated refs for the current Primary worktree when auxiliary state is recorded. `wtk list` shows ordinary and coordinated Primary worktrees together and summarizes Auxiliary Ref health, such as `refs 2/2 ok` or `refs 1/2 broken`. `wtk remove` removes the coordinated set. `wtk send-out` and `wtk bring-in` reject worktrees with auxiliary state because those commands do not define an atomic multi-repository move.

Every command prints the underlying `git` commands it runs. Successful commands copy the useful path or branch payload to the clipboard. Use `--no-clipboard` in CI or headless environments.

Commands that create linked worktrees also copy ignored files named exactly `.env` from the main worktree into the new worktree at the same Git-root-relative paths. Files such as `.env.local`, `.env.example`, and `.envrc` are not copied. When matching ignored `.env` files are copied, `wtk` prints one `copied ignored .env: <path>` line per file; when none are found, it prints nothing for this step.

If the new worktree looks like a pnpm repo (`pnpm-lock.yaml` or `pnpm-workspace.yaml` at the root), `wtk` then runs `pnpm install` inside the new worktree before reporting success.

## Create base selection

`wtk new` selects the base for the new branch by this precedence:

1. `--base`
2. `--from-current` / `-C`, which uses the branch checked out in the current worktree
3. `git config worktree-kit.mainBranch`
4. `origin/HEAD`
5. one unambiguous local branch among `main`, `master`, `trunk`, `develop`

Set an explicit default when needed:

```bash
git config worktree-kit.mainBranch trunk
```

## Completion

```bash
wtk completion bash > /usr/local/etc/bash_completion.d/wtk
wtk completion zsh > "${fpath[1]}/_wtk"
wtk completion fish > ~/.config/fish/completions/wtk.fish
wtk completion powershell > wtk.ps1
```

## Failure behavior

Dirty worktrees, malformed Auxiliary Group config or worktree state, missing generated refs, branch mismatches, ambiguous main branch detection, missing Git context, failed Git commands, ignored `.env` copy failures, and clipboard failures are reported directly. If Git succeeds and a later required step fails, `wtk` exits non-zero so the partial failure is visible.
