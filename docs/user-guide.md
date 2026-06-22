# User Guide

## Installation and upgrades

Install the latest release:

```bash
curl -fsSL https://raw.githubusercontent.com/nettee/worktree-kit/main/scripts/install.sh | sh
```

The installer downloads the matching GitHub release asset, verifies its checksum, installs `wtk` into `${WTK_INSTALL_DIR:-$HOME/.local/bin}`, and creates `~/.wtk/config.toml` with the default copy configuration when it does not already exist. It prints PATH and completion setup guidance after verifying `wtk --version`.

Upgrade an existing release install in place:

```bash
wtk upgrade
```

Local source install for development machines:

```bash
sh scripts/install-local.sh
```

The local installer also creates `~/.wtk/config.toml` when it does not already exist.

Check the installed version:

```bash
wtk --version
```

## Common Workflows

### Creating and checking out worktrees

Create a new branch in a linked worktree:

```bash
wtk new feature/login
```

Check out an existing branch or ref in a linked worktree:

```bash
wtk checkout feature/existing
```

By default, `wtk` uses the Sibling Layout: a repository's linked worktrees live next to the main worktree using names like `<repo>-wt-<branch-slug>`.

Commands that create linked worktrees copy configured ignored files from the main worktree into the new worktree at the same Git-root-relative paths. The default recursive file-name list is `.env` and `secrets.auto.tfvars`, and the default exact-path list is `specs/change/active`. Recursive matching is still exact-name based, so files such as `.env.local`, `.env.example`, `.envrc`, and `secrets.auto.tfvars.json` are not copied unless configured explicitly. When matching ignored files are copied, `wtk` prints one line per file, such as `copied ignored .env: <path>` or `copied ignored file: specs/change/active`.

Copy behavior can be configured in either repo-local `.wtk/config.toml` or global `~/.wtk/config.toml`. Repo-local config overrides global config for each configured list. The shape is:

```toml
[copy]
recursive = [".env", "secrets.auto.tfvars"]
exact = ["specs/change/active"]
```

For standalone `wtk new`, success is reported before asynchronous worktree initialization finishes copying ignored files into the new worktree. `wtk` prints info lines when that background initialization starts and where its temporary log file is written.

If a new worktree looks like a pnpm repo (`pnpm-lock.yaml` or `pnpm-workspace.yaml` at the root), `wtk new` starts `pnpm install` after the success message instead of waiting for it to finish first. Standalone `wtk new` does that from the asynchronous initializer, and coordinated `wtk new --ag ...` starts asynchronous installs for the primary and auxiliary worktrees after the coordinated worktree is created. Wait for the reported log file to finish before assuming ignored files or dependencies are ready.

### Listing and status

List visible worktrees:

```bash
wtk list
```

`wtk list` is optimized for scanning. The default output is a compact table with the worktree directory name, branch, relative HEAD commit time, state labels, and short HEAD. It does not print absolute paths by default. Rows are sorted by the current HEAD commit's committer time, newest first; dirty state is shown as a label but does not affect sorting.

Use JSON output for scripts:

```bash
wtk list --json
```

JSON includes absolute paths, full HEADs, timestamps, labels, diagnostics, and Auxiliary Ref details.

Print current repository and worktree status as YAML:

```bash
wtk status
```

`wtk status` validates generated refs for the current Primary Repository worktree when auxiliary state is recorded.

### Moving work out and back in

Move the current main-worktree branch to a linked worktree:

```bash
wtk send-out
```

`send-out` must run from the main worktree. It switches the main worktree back to the selected base branch and creates a linked worktree for the branch that was previously checked out.

Bring a linked worktree branch back into the main worktree:

```bash
wtk bring-in feature/login
```

`bring-in` must run from the main worktree. It finds the linked worktree for the named branch, removes that linked worktree, and switches the main worktree to the branch.

Both flows require the relevant worktrees to be clean. Worktrees with auxiliary state reject `send-out` and `bring-in` because those commands do not define an atomic multi-repository move.

### Auxiliary Groups and coordinated worktrees

The Primary Repository is the repository opened directly for a task. An Auxiliary Repository is another repository exposed from the Primary Repository when a task needs coordinated code changes or PRs outside the Primary Repository.

Create a local Auxiliary Group from the Primary Repository:

```bash
wtk auxiliary-group add full-stack /absolute/path/to/api /absolute/path/to/web
```

`wtk ag add` is a shorthand for `wtk auxiliary-group add`. Group creation resolves each repository path to a Git main worktree, derives the Auxiliary Repository Ref name from the repository directory name, creates or reuses `[auxiliaries.<name>]`, and writes `[auxiliary-groups.<group-name>]` in `.wtk/config.toml`.

Inspect configured groups:

```bash
wtk ag list
```

Remove a group definition:

```bash
wtk ag remove full-stack
```

Removing a group deletes that group's `[auxiliary-groups.<group-name>]` entry from `.wtk/config.toml`. Existing `[auxiliaries.*]` entries remain in the config and may be reused by other groups later.

The generated config shape is:

```toml
[auxiliaries.api]
repository = "/absolute/path/to/api"

[auxiliaries.web]
repository = "/absolute/path/to/web"

[auxiliary-groups.full-stack]
auxiliaries = ["api", "web"]
```

Create a coordinated worktree by selecting one or more groups:

```bash
wtk new feature/full-stack --ag full-stack
wtk new feature/full-stack --auxiliary-group full-stack
```

No selected Auxiliary Groups is the standalone case. With selected groups, `wtk new` creates the Primary Repository worktree plus matching Auxiliary Repository worktrees for the same branch. The Primary Repository worktree receives generated `refs/<auxiliary-name>` entries pointing to the Auxiliary Repository worktrees. `.wtk/worktrees.json` stores the expanded Auxiliary Repository state by absolute Primary Repository worktree path; changing the config later does not mutate existing worktrees.

Coordinated Primary Repository worktrees also receive a generated `WTK-AUXILIARY.md` file that lists the concrete Auxiliary Repository refs and targets. WTK keeps `/.wtk/`, `/refs/`, and `/WTK-AUXILIARY.md` ignored through `.git/info/exclude`.

`wtk list` shows ordinary and coordinated Primary Repository worktrees together and summarizes Auxiliary Ref health, such as `refs 2/2 ok` or `refs 1/2 broken`. `wtk remove` removes the coordinated set. `wtk remove --delete-branch` removes primary and auxiliary worktrees and branches after preflight checks.

WTK loads config with this precedence:

1. Legacy repo config at `$(git rev-parse --git-common-dir)/wtk/config.toml` when repo-local `.wtk/config.toml` is absent
2. Global config at `~/.wtk/config.toml`
3. Repo-local config at `.wtk/config.toml`

Repo-local config overrides global config when both set the same `copy` list. For backward compatibility, WTK still accepts the legacy `[groups.<group-name>]` key in existing config files, and reads the legacy `$(git rev-parse --git-common-dir)/wtk/worktrees.json` file when `.wtk/worktrees.json` is absent.

### Base branch selection

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

### Shell completion

```bash
wtk completion bash > /usr/local/etc/bash_completion.d/wtk
wtk completion zsh > "${fpath[1]}/_wtk"
wtk completion fish > ~/.config/fish/completions/wtk.fish
wtk completion powershell > wtk.ps1
```

### Failure behavior

Dirty worktrees, malformed Auxiliary Group config or worktree state, missing generated refs, branch mismatches, ambiguous main branch detection, missing Git context, failed Git commands, ignored local config copy failures, and clipboard failures are reported directly. If Git succeeds and a later required step fails, `wtk` exits non-zero so the partial failure is visible.

Every command prints the underlying `git` commands it runs. Successful commands copy the useful path or branch payload to the clipboard. Use `--no-clipboard` in CI or headless environments.

## Command reference

### `wtk new`

```text
wtk new <branch> [--path <path>] [--base <branch>] [--from-current] [--ag <group>] [--auxiliary-group <group>] [--no-clipboard]
```

Creates a new branch in a linked worktree.

`--ag` and `--auxiliary-group` may be repeated to select one or more Auxiliary Groups. `--path` is not supported with Auxiliary Groups. `--base` and `--from-current` cannot be combined.

### `wtk create`

```text
wtk create <branch> [--path <path>] [--base <branch>] [--from-current] [--ag <group>] [--auxiliary-group <group>] [--no-clipboard]
```

Compatibility alias for `wtk new`.

### `wtk checkout`

```text
wtk checkout <branch> [--path <path>] [--no-clipboard]
```

Checks out an existing branch or ref in a linked worktree.

### `wtk status`

```text
wtk status
```

Prints current repository and worktree status as YAML.

### `wtk list`

```text
wtk list [--json]
```

Lists visible worktrees in a compact table, or prints machine-readable JSON with `--json`.

### `wtk remove`

```text
wtk remove [path] [--delete-branch] [--no-clipboard]
```

Removes a linked worktree. With coordinated worktrees, removes the coordinated set. `--delete-branch` also deletes the relevant branch after preflight checks.

### `wtk send-out`

```text
wtk send-out [--path <path>] [--base <branch>] [--no-clipboard]
```

Moves the current main-worktree branch to a linked worktree. It refuses the selected base branch, dirty main worktrees, ambiguous main branch detection, and worktrees with auxiliary state.

### `wtk bring-in`

```text
wtk bring-in <branch> [--no-clipboard]
```

Moves a linked worktree branch back into the main worktree. It requires the main worktree and the linked worktree to be clean, and it rejects worktrees with auxiliary state.

### `wtk auxiliary-group`

```text
wtk auxiliary-group add <group-name> <repo-path>...
wtk auxiliary-group list
wtk auxiliary-group remove <group-name>
```

Manages Auxiliary Groups. `wtk ag` is an alias for `wtk auxiliary-group`.

Auxiliary repository paths must resolve to main worktrees. Duplicate repositories are rejected.

### `wtk upgrade`

```text
wtk upgrade
```

Downloads and installs the latest GitHub release over the current release-binary install.

### `wtk completion`

```text
wtk completion <bash|zsh|fish|powershell>
```

Generates a shell completion script.
