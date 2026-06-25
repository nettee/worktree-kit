# worktree-kit

`wtk` is a CLI that smooths out everyday Git worktree workflows for agentic coding.

Agentic coding makes isolated worktrees more useful: one task can live in one branch, one experiment can stay out of the main checkout, and related repository changes can be kept side by side. Raw `git worktree` gives you the foundation, but the day-to-day workflow still leaves repetitive setup, branch movement, and multi-repository coordination to you.

`wtk` adds a thin workflow layer for those common cases.

## Quick start

Install the latest release:

```bash
curl -fsSL https://raw.githubusercontent.com/nettee/worktree-kit/main/scripts/install.sh | sh
```

Check the installed version:

```bash
wtk --version
```

Create a linked worktree for a new branch:

```bash
wtk new feature/login
```

List visible worktrees:

```bash
wtk list
```

See the [User Guide](docs/user-guide.md) for upgrades, alternate installs, full workflows, and the command reference.

## Why wtk

`git worktree` is a good primitive for parallel coding work, but the useful workflow around it is usually bigger than one Git command:

- New worktrees often need local config, secrets, and project setup before they are ready.
- Work often starts in the main worktree before you decide it should move to a linked worktree.
- Multi-repository tasks need matching branches and stable paths across repositories.

`wtk` keeps those workflows close to Git while removing the repeated manual steps.

## Common workflows

### Create worktrees that are ready to use

Use `wtk new` for a new branch, or `wtk checkout` for an existing branch or ref:

```bash
wtk new feature/login
wtk checkout feature/existing
```

By default, `wtk` creates sibling worktree directories named like `<repo>-wt-<branch-slug>`. It can copy ignored local config files selected by global `~/.wtk/config.toml` Copy Patterns (for example `**/.env` and `.agents/` from the default template), and it runs `pnpm install` for pnpm repositories. Repo-local `.wtk/config.toml` cannot configure `copy`.

### Move work out after you already started

Sometimes work begins in the main worktree and only later needs to move into its own linked worktree. `wtk send-out` moves the current main-worktree branch out:

```bash
wtk send-out
```

When the branch should come back, use `wtk bring-in`:

```bash
wtk bring-in feature/login
```

### Coordinate related repositories

For tasks that span repositories, define an Auxiliary Group from the Primary Repository, the repository you open directly for the task:

```bash
wtk auxiliary-group add full-stack /absolute/path/to/api /absolute/path/to/web
```

Then create a coordinated worktree by selecting that group:

```bash
wtk new feature/full-stack --ag full-stack
```

`wtk` creates matching worktrees for the selected Auxiliary Repositories and exposes them from the Primary Repository worktree through stable generated refs.

## User Guide

The [User Guide](docs/user-guide.md) has the full workflow guide and command reference, including:

- install, upgrade, and local source install
- creating, checking out, listing, and removing worktrees
- `send-out` and `bring-in`
- Auxiliary Groups and coordinated worktrees
- base branch selection
- shell completion
- failure behavior and generated files

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for local development, verification, and release flow.
