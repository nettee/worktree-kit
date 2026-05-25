# Development

## Release flow

Releases are prepared through a release PR. Do not create or push release tags
manually while the PR is still open.

### 1. Prepare the release PR

Run the release script from a clean `main` branch. You can pass either an
explicit version or a semantic bump shortcut:

```bash
python3 scripts/release.py 0.1.0
python3 scripts/release.py major
python3 scripts/release.py minor
python3 scripts/release.py patch
```

The script will:

1. Verify required commands are available: `git`, `cargo`, and `gh`.
2. Verify the working tree is clean and the current branch is `main`.
3. Resolve the target version from the explicit argument or the current
   `Cargo.toml` version plus the requested semantic bump.
4. Fetch tags and verify the target version is greater than both:
   - the current `Cargo.toml` version
   - the latest existing release tag
5. Create a `release/v0.1.0` branch.
6. Update release files such as `Cargo.toml` and README pinned install examples.
7. Run `cargo test`.
8. Commit the version bump.
9. Push the release branch.
10. Open a GitHub PR with the `release` label.

For emergency use only, tests can be replaced by `cargo check`:

```bash
python3 scripts/release.py 0.1.0 --skip-tests
```

### 2. Review and merge the release PR

The PR must keep the `release` label. When the labeled PR is merged into
`main`, `.github/workflows/tag-release-pr.yml` runs automatically.

The workflow will:

1. Read the version from `Cargo.toml` on `main`.
2. Verify it is a valid `MAJOR.MINOR.PATCH` version.
3. Verify `v<version>` does not already exist.
4. Verify the version is greater than the latest existing release tag.
5. Create and push an annotated tag, for example `v0.1.0`.

### 3. GitHub Release assets

Pushing the release tag triggers `.github/workflows/release.yml`, which builds
and uploads:

```text
wtk_<version>_darwin_amd64.tar.gz
wtk_<version>_darwin_arm64.tar.gz
wtk_<version>_linux_amd64.tar.gz
wtk_<version>_linux_arm64.tar.gz
checksums.txt
```

### 4. Verify the release

After the release workflow finishes, verify the release exists and the installer
can install the new version:

```bash
gh release view v0.1.0
curl -fsSL https://raw.githubusercontent.com/nettee/worktree-kit/main/scripts/install.sh | WTK_VERSION=0.1.0 sh
wtk --version
```

## Release safety rules

- Release PRs are identified by the `release` label.
- The release script does not create tags.
- Tags are created only after the release PR is merged into `main`.
- Versions must increase; version rollback is rejected locally and in CI.
- Release tags use `v<version>`, for example `v0.1.0`.
- Asset versions do not include the leading `v`, for example
  `wtk_0.1.0_linux_amd64.tar.gz`.
