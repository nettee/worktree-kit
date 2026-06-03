# worktree-kit

`worktree-kit` provides the `wtk` CLI for common Git worktree workflows:

- `wtk new` creates a new branch in a linked worktree.
- `wtk checkout` checks out an existing branch or ref in a linked worktree.
- `wtk remove` removes a linked worktree.
- `wtk send-out` moves the current main-worktree branch to a linked worktree.
- `wtk bring-in` moves a linked worktree branch back into the main worktree.

`wtk create` remains available as a compatibility alias for `wtk new`.

Default linked worktree paths are sibling directories named `<repo>-wt-<branch-slug>`.

## Install

One-click install:

```bash
curl -fsSL https://raw.githubusercontent.com/nettee/worktree-kit/main/scripts/install.sh | sh
```

The installer downloads the matching GitHub release asset, verifies its checksum, and installs `wtk` into `${WTK_INSTALL_DIR:-$HOME/.local/bin}`. It prints PATH and completion setup guidance after verifying `wtk --version`.

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
wtk remove ../repo-wt-feature-foo
wtk send-out
wtk bring-in feature/foo
```

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
