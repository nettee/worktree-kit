from __future__ import annotations

import time

from conftest import linked_worktree_path, wait_until
from conftest import run_git


def test_new_runs_real_pnpm_install(run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.add_real_pnpm_project(repo, marker_name=".pnpm-installed.txt")

    out = run_wtk("new", "feature/pnpm-install", "--base", "main", "--no-clipboard", cwd=repo).output
    linked = linked_worktree_path(repo, "feature/pnpm-install")
    assert linked.exists()
    wait_until("real pnpm install marker", lambda: linked.joinpath(".pnpm-installed.txt").exists(), timeout=10.0)
    assert "initializing worktree asynchronously" in out


def test_new_returns_before_slow_real_pnpm_install_finishes(run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.add_real_pnpm_project(repo, delay_seconds=2.0, marker_name=".pnpm-slow.txt")

    started = time.monotonic()
    out = run_wtk("new", "feature/slow-pnpm-install", "--base", "main", "--no-clipboard", cwd=repo).output
    elapsed = time.monotonic() - started

    assert elapsed < 1.5, f"create waited for slow pnpm install: {elapsed:.2f}s"
    assert "created worktree" in out
    linked = linked_worktree_path(repo, "feature/slow-pnpm-install")
    wait_until("slow real pnpm marker", lambda: linked.joinpath(".pnpm-slow.txt").exists(), timeout=15.0)


def test_send_out_returns_before_slow_real_pnpm_install_finishes(run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.add_real_pnpm_project(repo, delay_seconds=2.0, marker_name=".pnpm-send-out.txt")
    run_git(repo, "switch", "-c", "feature/send-slow-pnpm")

    started = time.monotonic()
    out = run_wtk("send-out", "--no-clipboard", cwd=repo).output
    elapsed = time.monotonic() - started

    assert elapsed < 1.5, f"send-out waited for slow pnpm install: {elapsed:.2f}s"
    assert "running pnpm install asynchronously" in out
    linked = linked_worktree_path(repo, "feature/send-slow-pnpm")
    wait_until("slow send-out pnpm marker", lambda: linked.joinpath(".pnpm-send-out.txt").exists(), timeout=15.0)


def test_workspace_new_copies_env_and_runs_real_pnpm_install(run_wtk, workspace_factory, repo_factory) -> None:
    workspace, members = workspace_factory.create()
    run_wtk("workspace", "init", cwd=workspace)
    run_wtk("workspace", "add", str(members["A"]), cwd=workspace)
    run_wtk("workspace", "add", str(members["B"]), cwd=workspace)
    repo_factory.commit_workspace_manifest(workspace)

    for name, repo in members.items():
        repo_factory.commit_files(repo, {".gitignore": ".env\n"}, "ignore env")
        (repo / ".env").write_text(f"{name}=value\n", encoding="utf-8")
        repo_factory.add_real_pnpm_project(repo, marker_name=f".pnpm-{name.lower()}.txt")

    out = run_wtk("new", "feature/ws-init", "--base", "main", "--no-clipboard", cwd=workspace).output
    workspace_linked = linked_worktree_path(workspace, "feature/ws-init")
    linked_a = linked_worktree_path(members["A"], "feature/ws-init")
    linked_b = linked_worktree_path(members["B"], "feature/ws-init")

    assert workspace_linked.exists()
    wait_until("workspace pnpm markers", lambda: linked_a.joinpath(".pnpm-a.txt").exists() and linked_b.joinpath(".pnpm-b.txt").exists(), timeout=15.0)
    assert linked_a.joinpath(".env").read_text(encoding="utf-8") == "A=value\n"
    assert linked_b.joinpath(".env").read_text(encoding="utf-8") == "B=value\n"
    assert "copied ignored .env: .env" in out
    assert "running pnpm install asynchronously" in out


def test_workspace_new_returns_before_slow_real_pnpm_install_finishes(
    run_wtk, workspace_factory, repo_factory
) -> None:
    workspace, members = workspace_factory.create()
    run_wtk("workspace", "init", cwd=workspace)
    run_wtk("workspace", "add", str(members["A"]), cwd=workspace)
    run_wtk("workspace", "add", str(members["B"]), cwd=workspace)
    repo_factory.commit_workspace_manifest(workspace)

    for name, repo in members.items():
        repo_factory.commit_files(repo, {".gitignore": ".env\n"}, "ignore env")
        (repo / ".env").write_text(f"{name}=value\n", encoding="utf-8")
        repo_factory.add_real_pnpm_project(
            repo, delay_seconds=2.0, marker_name=f".pnpm-slow-{name.lower()}.txt"
        )

    started = time.monotonic()
    out = run_wtk(
        "new", "feature/ws-slow-pnpm", "--base", "main", "--no-clipboard", cwd=workspace
    ).output
    elapsed = time.monotonic() - started

    assert elapsed < 1.5, f"workspace create waited for slow pnpm install: {elapsed:.2f}s"
    assert "created workspace worktree" in out
    assert "running pnpm install asynchronously" in out
    linked_a = linked_worktree_path(members["A"], "feature/ws-slow-pnpm")
    linked_b = linked_worktree_path(members["B"], "feature/ws-slow-pnpm")
    assert linked_a.joinpath(".env").read_text(encoding="utf-8") == "A=value\n"
    assert linked_b.joinpath(".env").read_text(encoding="utf-8") == "B=value\n"
    wait_until(
        "slow workspace pnpm markers",
        lambda: linked_a.joinpath(".pnpm-slow-a.txt").exists()
        and linked_b.joinpath(".pnpm-slow-b.txt").exists(),
        timeout=15.0,
    )
