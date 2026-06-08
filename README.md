# worktree-kit

`worktree-kit` provides the `wtk` CLI for common Git worktree workflows:

- `wtk new` creates a new branch in a linked worktree.
- `wtk checkout` checks out an existing branch or ref in a linked worktree.
- `wtk status` prints current repository/worktree status in YAML format.
- `wtk list` prints visible worktrees in YAML format.
- `wtk remove` removes a linked worktree.
- `wtk send-out` moves the current main-worktree branch to a linked worktree.
- `wtk bring-in` moves a linked worktree branch back into the main worktree.
- `wtk workspace init` and `wtk workspace add` configure Workspace Mode for coordinated multi-repository worktrees.

`wtk create` remains available as a compatibility alias for `wtk new`.

Repository Mode is the default single-repository behavior. Default linked worktree paths use the Sibling Layout: sibling directories named `<repo>-wt-<branch-slug>`.

Workspace Mode coordinates multiple Linked Repositories from a Workspace repository that participates in the same branch/worktree lifecycle as the repositories it manages. It stores stable membership in a tracked `.wtk-workspace.toml` manifest. Each Workspace Worktree owns generated `refs/<name>` entries that point to the currently surfaced Repository Worktree path for each Linked Repository. Repository paths and ref targets are absolute paths.

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
wtk remove ../repo-wt-feature-foo
wtk send-out
wtk bring-in feature/foo
wtk workspace init
wtk workspace add /absolute/path/to/repo
wtk upgrade
```

## Workspace Mode

Initialize a Workspace repository and add Linked Repositories:

```bash
wtk workspace init
wtk workspace add /absolute/path/to/A
wtk workspace add /absolute/path/to/B
```

The generated manifest shape is:

```toml
mode = "workspace"

[workspace.refs.A]
repository = "/absolute/path/to/A"
```

In Workspace Mode, `wtk status` emits aggregate Workspace status for the current Workspace Worktree and validates generated refs without repairing them. `wtk new` creates a coordinated Workspace Worktree plus matching Linked Repository Worktrees for the same branch. `wtk remove` removes the coordinated set. Workspace operations fail fast on malformed manifest state, relative paths, missing or incorrect refs, branch mismatches, dirty worktrees, branch collisions, or target path collisions.

`wtk workspace init` and `wtk workspace add` must be run from the Workspace main worktree. `wtk checkout`, `wtk send-out`, and `wtk bring-in` are repository-mode-only commands and are rejected in Workspace Mode.

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

Dirty worktrees, malformed Workspace manifests, missing generated refs, branch mismatches, ambiguous main branch detection, missing Git context, failed Git commands, ignored `.env` copy failures, and clipboard failures are reported directly. If Git succeeds and a later required step fails, `wtk` exits non-zero so the partial failure is visible.
