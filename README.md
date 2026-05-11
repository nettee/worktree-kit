# worktree-kit

`worktree-kit` provides the `wtk` CLI for common Git worktree workflows:

- `wtk create` creates a new branch in a linked worktree.
- `wtk checkout` checks out an existing branch or ref in a linked worktree.
- `wtk remove` removes a linked worktree.
- `wtk send-out` moves the current main-worktree branch to a linked worktree.
- `wtk bring-in` moves a linked worktree branch back into the main worktree.

Default linked worktree paths are sibling directories named `<repo>-wt-<branch-slug>`.

## Install

One-click install:

```bash
curl -fsSL https://raw.githubusercontent.com/nettee/worktree-kit/main/scripts/install.sh | sh
```

The installer downloads the matching GitHub release asset, verifies its checksum, and installs `wtk` into `${WTK_INSTALL_DIR:-$HOME/.local/bin}`. It prints PATH and completion setup guidance after verifying `wtk --version`.

Install a specific release:

```bash
curl -fsSL https://raw.githubusercontent.com/nettee/worktree-kit/main/scripts/install.sh | WTK_VERSION=0.0.1 sh
```

Go install is also supported for development machines with Go 1.25 or newer:

```bash
go install github.com/nettee/worktree-kit/cmd/wtk@latest
```

Check the installed version:

```bash
wtk --version
```

## Usage

```bash
wtk create feature/foo
wtk checkout feature/existing
wtk remove ../repo-wt-feature-foo
wtk send-out
wtk bring-in ../repo-wt-feature-foo
```

Every command prints the underlying `git` commands it runs. Successful commands copy the useful path or branch payload to the clipboard. Use `--no-clipboard` in CI or headless environments.

## Main branch detection

`wtk` detects the base branch by this precedence:

1. `--base`
2. `git config worktree-kit.mainBranch`
3. `origin/HEAD`
4. one unambiguous local branch among `main`, `master`, `trunk`, `develop`

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

Dirty worktrees, ambiguous main branch detection, missing Git context, failed Git commands, and clipboard failures are reported directly. If Git succeeds and clipboard copy fails, `wtk` prints the Git success and exits non-zero so the partial failure is visible.
