from __future__ import annotations

import os
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path

import pytest
import yaml


ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))


def require_command(name: str) -> str:
    resolved = shutil.which(name)
    if resolved is None:
        pytest.skip(f"required command is unavailable: {name}")
    return resolved


@dataclass
class CmdResult:
    args: list[str]
    cwd: Path
    returncode: int
    stdout: str
    stderr: str

    @property
    def output(self) -> str:
        return f"{self.stdout}{self.stderr}"

    def assert_success(self) -> "CmdResult":
        assert self.returncode == 0, self.describe()
        return self

    def assert_failure(self) -> "CmdResult":
        assert self.returncode != 0, f"command unexpectedly succeeded\n{self.describe()}"
        return self

    def describe(self) -> str:
        rendered = " ".join(self.args)
        return (
            f"command failed: {rendered}\n"
            f"cwd: {self.cwd}\n"
            f"exit: {self.returncode}\n"
            f"stdout:\n{self.stdout}\n"
            f"stderr:\n{self.stderr}"
        )


def run_cmd(
    args: list[str],
    *,
    cwd: Path,
    env: dict[str, str] | None = None,
    check: bool = True,
) -> CmdResult:
    merged_env = os.environ.copy()
    if env:
        merged_env.update(env)
    completed = subprocess.run(
        args,
        cwd=cwd,
        env=merged_env,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    result = CmdResult(
        args=args,
        cwd=cwd,
        returncode=completed.returncode,
        stdout=completed.stdout,
        stderr=completed.stderr,
    )
    if check:
        result.assert_success()
    return result


def run_git(cwd: Path, *args: str, check: bool = True, env: dict[str, str] | None = None) -> CmdResult:
    return run_cmd(["git", *args], cwd=cwd, env=env, check=check)


def wait_until(reason: str, predicate, timeout: float = 5.0, interval: float = 0.05) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(interval)
    raise AssertionError(f"timed out waiting for {reason}")


def linked_worktree_path(repo: Path, branch: str) -> Path:
    return repo.parent / f"{repo.name}-wt-{branch.replace('/', '-').replace(chr(92), '-')}"


def parse_yaml(output: str):
    return yaml.safe_load(output)


@pytest.fixture(scope="session")
def required_commands() -> dict[str, str]:
    return {
        name: require_command(name)
        for name in ["uv", "python", "cargo", "git", "node", "pnpm"]
    }


@pytest.fixture(scope="session")
def wtk_bin(required_commands: dict[str, str]) -> Path:
    run_cmd(["cargo", "build", "--release", "--bin", "wtk"], cwd=ROOT)
    bin_name = "wtk.exe" if os.name == "nt" else "wtk"
    path = ROOT / "target" / "release" / bin_name
    assert path.exists(), f"missing built binary: {path}"
    return path


@pytest.fixture
def run_wtk(wtk_bin: Path):
    def _run(*args: str, cwd: Path, check: bool = True, env: dict[str, str] | None = None) -> CmdResult:
        return run_cmd([str(wtk_bin), *args], cwd=cwd, check=check, env=env)

    return _run


class RepoFactory:
    def __init__(self, root: Path):
        self.root = root

    def init_repo(self, name: str, branch: str = "main") -> Path:
        repo = self.root / name
        repo.mkdir()
        run_git(repo, "init", "-b", branch)
        run_git(repo, "config", "user.email", "test@example.com")
        run_git(repo, "config", "user.name", "Test User")
        (repo / "README.md").write_text("test\n", encoding="utf-8")
        run_git(repo, "add", ".")
        run_git(repo, "commit", "-m", "init")
        return repo

    def commit_files(self, repo: Path, files: dict[str, str], message: str) -> None:
        for relative, contents in files.items():
            path = repo / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(contents, encoding="utf-8")
        run_git(repo, "add", ".")
        run_git(repo, "commit", "-m", message)

    def add_real_pnpm_project(
        self,
        repo: Path,
        *,
        package_name: str | None = None,
        delay_seconds: float = 0.0,
        fail_postinstall: bool = False,
        marker_name: str = ".pnpm-postinstall.txt",
    ) -> None:
        package_name = package_name or repo.name
        if fail_postinstall:
            script_lines = [
                'import { writeFile } from "node:fs/promises";',
                f'await writeFile("{marker_name}", "failing\\n", "utf8");',
                'throw new Error("postinstall failure");',
            ]
        elif delay_seconds > 0:
            script_lines = [
                'import { writeFile } from "node:fs/promises";',
                'import { setTimeout as sleep } from "node:timers/promises";',
                f"await sleep({int(delay_seconds * 1000)});",
                f'await writeFile("{marker_name}", "done\\n", "utf8");',
            ]
        else:
            script_lines = [
                'import { writeFile } from "node:fs/promises";',
                f'await writeFile("{marker_name}", "done\\n", "utf8");',
            ]

        package_json = "\n".join(
            [
                "{",
                f'  "name": "{package_name}",',
                '  "version": "1.0.0",',
                '  "private": true,',
                '  "scripts": {',
                '    "postinstall": "node postinstall.mjs"',
                "  }",
                "}",
            ]
        )
        self.commit_files(
            repo,
            {
                "package.json": package_json + "\n",
                "postinstall.mjs": "\n".join(script_lines) + "\n",
            },
            "add pnpm fixture",
        )
        run_cmd(
            ["pnpm", "install", "--ignore-scripts", "--lockfile-only"],
            cwd=repo,
        )
        run_git(repo, "add", "pnpm-lock.yaml")
        run_git(repo, "commit", "-m", "add pnpm lockfile")


@pytest.fixture
def repo_factory(tmp_path: Path) -> RepoFactory:
    return RepoFactory(tmp_path)
