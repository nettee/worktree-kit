# `wtk` end-to-end tests

This suite runs black-box end-to-end tests for `wtk` with real:

- Git repositories and linked worktrees
- Auxiliary Group configuration, coordinated worktrees, and generated refs
- ignored recursive file copying such as `.env` and `secrets.auto.tfvars`
- `pnpm install` flows

Run it from the repository root:

```bash
uv run --project e2e pytest
```

The harness builds the real release binary once with:

```bash
cargo build --release --bin wtk
```

Required local commands:

- `uv`
- `python`
- `cargo`
- `git`
- `node`
- `pnpm`
