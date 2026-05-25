#!/usr/bin/env python3
"""Prepare a release PR for worktree-kit.

This script intentionally does not create or push release tags. A merged PR with
the `release` label is tagged by `.github/workflows/tag-release-pr.yml`.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


RELEASE_LABEL = "release"
VERSION_PATTERN = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
CARGO_VERSION_PATTERN = re.compile(r'(?m)^version = "([^"]+)"$')
README_PINNED_VERSION_PATTERN = re.compile(r"WTK_VERSION=\d+\.\d+\.\d+")
SEMANTIC_BUMP_CHOICES = ("major", "minor", "patch")


@dataclass(frozen=True, order=True)
class Version:
    major: int
    minor: int
    patch: int

    @classmethod
    def parse(cls, value: str) -> "Version":
        match = VERSION_PATTERN.fullmatch(value)
        if not match:
            fail(f"version must be MAJOR.MINOR.PATCH, got: {value}")
        return cls(*(int(part) for part in match.groups()))

    def __str__(self) -> str:
        return f"{self.major}.{self.minor}.{self.patch}"

    def bump(self, bump: str) -> "Version":
        if bump == "major":
            return Version(self.major + 1, 0, 0)
        if bump == "minor":
            return Version(self.major, self.minor + 1, 0)
        if bump == "patch":
            return Version(self.major, self.minor, self.patch + 1)
        fail(f"unsupported semantic version bump: {bump}")


def fail(message: str) -> None:
    print(f"release: {message}", file=sys.stderr)
    raise SystemExit(1)


def run(args: list[str], *, capture: bool = False) -> str:
    try:
        result = subprocess.run(
            args,
            check=True,
            cwd=repo_root(),
            text=True,
            stdout=subprocess.PIPE if capture else None,
            stderr=subprocess.PIPE if capture else None,
        )
    except FileNotFoundError:
        fail(f"missing required command: {args[0]}")
    except subprocess.CalledProcessError as error:
        if capture and error.stderr:
            print(error.stderr, file=sys.stderr, end="")
        fail(f"command failed: {' '.join(args)}")
    return result.stdout.strip() if capture else ""


def repo_root() -> Path:
    return Path(__file__).resolve().parent.parent


def require_command(command: str) -> None:
    if shutil.which(command) is None:
        fail(f"missing required command: {command}")


def ensure_clean_worktree() -> None:
    status = run(["git", "status", "--porcelain"], capture=True)
    if status:
        fail("working tree must be clean before preparing a release")


def current_branch() -> str:
    return run(["git", "branch", "--show-current"], capture=True)


def ensure_base_branch(base: str) -> None:
    branch = current_branch()
    if branch != base:
        fail(f"run this from {base}; current branch is {branch or '<detached>'}")


def read_cargo_version() -> Version:
    cargo_toml = repo_root() / "Cargo.toml"
    text = cargo_toml.read_text()
    match = CARGO_VERSION_PATTERN.search(text)
    if not match:
        fail("Cargo.toml is missing a top-level version field")
    return Version.parse(match.group(1))


def latest_release_tag_version() -> Version | None:
    tags = run(["git", "tag", "--list", "v[0-9]*"], capture=True).splitlines()
    versions: list[Version] = []
    for tag in tags:
        value = tag.removeprefix("v")
        if VERSION_PATTERN.fullmatch(value):
            versions.append(Version.parse(value))
    return max(versions) if versions else None


def ensure_version_increases(target: Version) -> None:
    current = read_cargo_version()
    if target <= current:
        fail(f"target version {target} must be greater than Cargo.toml version {current}")

    latest_tag = latest_release_tag_version()
    if latest_tag is not None and target <= latest_tag:
        fail(f"target version {target} must be greater than latest release tag v{latest_tag}")


def resolve_target_version(value: str) -> Version:
    if value in SEMANTIC_BUMP_CHOICES:
        return read_cargo_version().bump(value)
    return Version.parse(value)


def ensure_tag_absent(target: Version) -> None:
    tag = f"v{target}"
    existing = run(["git", "tag", "--list", tag], capture=True)
    if existing:
        fail(f"tag already exists: {tag}")


def ensure_branch_absent(branch: str, remote: str) -> None:
    local = run(["git", "branch", "--list", branch], capture=True)
    if local:
        fail(f"local branch already exists: {branch}")
    remote_ref = run(["git", "ls-remote", "--heads", remote, branch], capture=True)
    if remote_ref:
        fail(f"remote branch already exists: {remote}/{branch}")


def update_version_files(target: Version) -> None:
    cargo_toml = repo_root() / "Cargo.toml"
    cargo_text = cargo_toml.read_text()
    cargo_text, cargo_count = CARGO_VERSION_PATTERN.subn(f'version = "{target}"', cargo_text, count=1)
    if cargo_count != 1:
        fail("failed to update Cargo.toml version")
    cargo_toml.write_text(cargo_text)

    readme = repo_root() / "README.md"
    readme_text = readme.read_text()
    readme_text, readme_count = README_PINNED_VERSION_PATTERN.subn(f"WTK_VERSION={target}", readme_text, count=1)
    if readme_count != 1:
        fail("failed to update README pinned install example")
    readme.write_text(readme_text)


def ensure_release_label() -> None:
    labels_json = run(["gh", "label", "list", "--limit", "200", "--json", "name"], capture=True)
    labels = json.loads(labels_json)
    if any(label.get("name") == RELEASE_LABEL for label in labels):
        return
    run([
        "gh",
        "label",
        "create",
        RELEASE_LABEL,
        "--description",
        "Release PR that should be tagged after merge",
        "--color",
        "0E8A16",
    ])


def ensure_changes_exist() -> None:
    status = run(["git", "status", "--porcelain"], capture=True)
    if not status:
        fail("version update produced no file changes")


def prepare_release(version: str, *, base: str, remote: str, skip_tests: bool) -> None:
    for command in ["git", "cargo", "gh"]:
        require_command(command)

    ensure_clean_worktree()
    ensure_base_branch(base)
    run(["git", "fetch", remote, base, "--tags"])
    run(["git", "pull", "--ff-only", remote, base])
    target = resolve_target_version(version)
    ensure_version_increases(target)
    ensure_tag_absent(target)

    release_branch = f"release/v{target}"
    ensure_branch_absent(release_branch, remote)

    run(["git", "checkout", "-b", release_branch])
    update_version_files(target)

    if not skip_tests:
        run(["cargo", "test"])
    else:
        run(["cargo", "check"])

    ensure_changes_exist()
    run(["git", "add", "Cargo.toml", "Cargo.lock", "README.md"])
    run(["git", "commit", "-m", f"Release v{target}"])
    run(["git", "push", "-u", remote, release_branch])
    ensure_release_label()

    body = (
        f"Release v{target}.\n\n"
        "After this PR is merged into main, the release label triggers the tag workflow "
        f"to create annotated tag v{target}. The tag push then triggers the release asset workflow."
    )
    run([
        "gh",
        "pr",
        "create",
        "--base",
        base,
        "--head",
        release_branch,
        "--title",
        f"Release v{target}",
        "--body",
        body,
        "--label",
        RELEASE_LABEL,
    ])


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Prepare and open a release PR.")
    parser.add_argument(
        "version",
        help="Target release version (for example 0.1.0) or semantic bump: major, minor, patch",
    )
    parser.add_argument("--base", default="main", help="Base branch for the release PR. Default: main")
    parser.add_argument("--remote", default="origin", help="Git remote to push to. Default: origin")
    parser.add_argument("--skip-tests", action="store_true", help="Run cargo check instead of cargo test")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    prepare_release(args.version, base=args.base, remote=args.remote, skip_tests=args.skip_tests)


if __name__ == "__main__":
    main()
