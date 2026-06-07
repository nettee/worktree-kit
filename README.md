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

Workspace Mode coordinates multiple Linked Repositories from a lightweight Workspace repository. It stores mode and stable repository paths in `.wtk/config.toml`; Workspace Refs live at `refs/<name>` and point to the currently surfaced Repository Worktree path for each Linked Repository. Repository paths and ref targets are absolute paths.

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

The generated config shape is:

```toml
mode = "workspace"

[workspace.refs.A]
repository = "/absolute/path/to/A"
```

In Workspace Mode, `wtk status` emits aggregate Workspace status. `wtk new`, `wtk remove`, `wtk send-out`, and `wtk bring-in` fan out across every configured Workspace Ref. Workspace operations preflight linked repositories before mutation and fail fast on malformed config, relative paths, invalid refs, dirty worktrees, branch collisions, or target path collisions.

Every command prints the underlying `git` commands it runs. Successful commands copy the useful path or branch payload to the clipboard. Use `--no-clipboard` in CI or headless environments.

Commands that create linked worktrees also copy ignored files named exactly `.env` from the main worktree into the new worktree at the same Git-root-relative paths. Files such as `.env.local`, `.env.example`, and `.envrc` are not copied. When matching ignored `.env` files are copied, `wtk` prints one `copied ignored .env: <path>` line per file; when none are found, it prints nothing for this step.

`wtk send-out` also copies an ignored `specs/change/active` file into the linked worktree and prints `copied ignored file: specs/change/active` when that file is transferred.

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

Dirty worktrees, ambiguous main branch detection, missing Git context, failed Git commands, ignored `.env` copy failures, and clipboard failures are reported directly. If Git succeeds and a later required step fails, `wtk` exits non-zero so the partial failure is visible.
