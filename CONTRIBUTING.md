# Contributing

`wtk` is a Rust CLI.

## Local development

Build locally:

```bash
cargo build --release --bin wtk
```

Run the Rust test suite:

```bash
cargo test
```

Run the full verification suite before changes that affect installer or end-to-end behavior:

```bash
cargo test
uv run --project e2e pytest e2e tests/test_release.py
sh scripts/test-install.sh
sh scripts/test-install-local.sh
```

Documentation changes should keep the README focused on project orientation, keep the User Guide as the complete user-facing reference, and preserve the canonical Primary/Auxiliary terminology from `CONTEXT.md`.
