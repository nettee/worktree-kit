from __future__ import annotations

import json
import os
import time

import pytest

from conftest import linked_worktree_path, parse_yaml, run_git


def test_repo_mode_create_remove_send_out_bring_in_and_completion(run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    run_git(repo, "branch", "feature/existing")

    out = run_wtk("checkout", "feature/existing", "--no-clipboard", cwd=repo).output
    linked = linked_worktree_path(repo, "feature/existing")
    assert linked.exists()
    assert "created worktree" in out

    run_wtk("remove", str(linked), "--no-clipboard", cwd=repo)
    assert not linked.exists()

    run_git(repo, "switch", "-c", "feature/send")
    (repo / "sub").mkdir()
    out = run_wtk("send-out", "--no-clipboard", cwd=repo / "sub").output
    linked = linked_worktree_path(repo, "feature/send")
    assert linked.exists()
    assert "sent feature/send out" in out
    assert run_git(repo, "branch", "--show-current").stdout.strip() == "main"

    run_wtk("bring-in", "feature/send", "--no-clipboard", cwd=repo)
    assert run_git(repo, "branch", "--show-current").stdout.strip() == "feature/send"

    completion = run_wtk("__complete", cwd=repo).stdout
    assert "new" in completion
    assert "status" in completion

    for shell in ["bash", "zsh", "fish", "powershell"]:
        assert "wtk" in run_wtk("completion", shell, cwd=repo).stdout


def test_send_out_copies_configured_exact_files(run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(
        repo,
        {
            ".gitignore": "specs/change/active\n",
            ".wtk/config.toml": """
[copy]
recursive = []
exact = ["specs/change/active"]
""".lstrip(),
        },
        "configure exact copy",
    )
    (repo / "specs" / "change").mkdir(parents=True)
    (repo / "specs" / "change" / "active").write_text("ACTIVE\n", encoding="utf-8")
    run_git(repo, "switch", "-c", "feature/send-spec")

    out = run_wtk("send-out", "--no-clipboard", cwd=repo).output
    linked = linked_worktree_path(repo, "feature/send-spec")

    assert linked.joinpath("specs/change/active").read_text(encoding="utf-8") == "ACTIVE\n"
    assert "copied ignored file: specs/change/active" in out


@pytest.mark.skipif(os.name == "nt", reason="requires unix symlink semantics")
def test_send_out_dedupes_recursive_and_exact_symlink_copy(
    run_wtk, repo_factory, tmp_path
) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(
        repo,
        {
            ".gitignore": "apps/web/.env\n",
            ".wtk/config.toml": """
[copy]
exact = ["apps/web/.env"]
""".lstrip(),
            "apps/web/keep.txt": "web\n",
        },
        "configure duplicate send-out copy",
    )

    shared_env = tmp_path / "shared.env"
    shared_env.write_text("WEB=value\n", encoding="utf-8")
    os.symlink(shared_env, repo / "apps" / "web" / ".env")
    run_git(repo, "switch", "-c", "feature/deduped-send-out")

    out = run_wtk("send-out", "--no-clipboard", cwd=repo).output
    linked = linked_worktree_path(repo, "feature/deduped-send-out")

    assert linked.joinpath("apps/web/.env").is_symlink()
    assert linked.joinpath("apps/web/.env").resolve() == shared_env.resolve()
    assert out.count("apps/web/.env") == 1


@pytest.mark.skipif(os.name == "nt", reason="requires unix symlink semantics")
def test_checkout_dedupes_recursive_and_exact_symlink_copy(
    run_wtk, repo_factory, tmp_path
) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(
        repo,
        {
            ".gitignore": "apps/web/.env\n",
            ".wtk/config.toml": """
[copy]
exact = ["apps/web/.env"]
""".lstrip(),
            "apps/web/keep.txt": "web\n",
        },
        "configure duplicate checkout copy",
    )

    shared_env = tmp_path / "shared.env"
    shared_env.write_text("WEB=value\n", encoding="utf-8")
    os.symlink(shared_env, repo / "apps" / "web" / ".env")

    run_git(repo, "branch", "feature/deduped-checkout")
    out = run_wtk("checkout", "feature/deduped-checkout", "--no-clipboard", cwd=repo).output
    linked = linked_worktree_path(repo, "feature/deduped-checkout")

    assert linked.joinpath("apps/web/.env").is_symlink()
    assert linked.joinpath("apps/web/.env").resolve() == shared_env.resolve()
    assert out.count("apps/web/.env") == 1


def test_repo_mode_status_and_list_readable(run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    run_git(repo, "branch", "feature/status")
    run_wtk("checkout", "feature/status", "--no-clipboard", cwd=repo)
    linked = linked_worktree_path(repo, "feature/status")
    (linked / "dirty.txt").write_text("dirty\n", encoding="utf-8")

    status = parse_yaml(run_wtk("status", cwd=linked).stdout)
    assert status["current_is_main"] is False
    assert status["main_root"] == str(repo.resolve())
    assert status["current_root"] == str(linked.resolve())

    listing = run_wtk("list", cwd=linked).stdout
    lines = [line for line in listing.splitlines() if line.strip()]
    assert lines[0].split() == ["worktree", "branch", "updated", "state", "head"]
    assert "worktrees:" not in listing
    assert str(repo.resolve()) not in listing
    assert str(linked.resolve()) not in listing
    assert any(line.startswith("  repo ") and " main " in line for line in lines[1:])
    assert any(
        line.startswith("* repo-wt-feature-status ")
        and " feature/status " in line
        and " current" in line
        and " dirty" in line
        for line in lines[1:]
    )


def test_repo_mode_list_json(run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    run_git(repo, "branch", "feature/json")
    run_wtk("checkout", "feature/json", "--no-clipboard", cwd=repo)
    linked = linked_worktree_path(repo, "feature/json")

    output = run_wtk("list", "--json", cwd=linked).stdout
    assert "\x1b[" not in output
    listing = json.loads(output)
    assert listing["mode"] == "repository"
    worktrees = listing["worktrees"]
    assert len(worktrees) == 2
    assert any(
        entry["path"] == str(repo.resolve())
        and entry["display_name"] == "repo"
        and entry["is_main"] is True
        and len(entry["head"]) == 40
        for entry in worktrees
    )
    assert any(
        entry["path"] == str(linked.resolve())
        and entry["display_name"] == "repo-wt-feature-json"
        and entry["branch"] == "feature/json"
        and entry["is_current"] is True
        for entry in worktrees
    )


def test_repo_mode_list_sorts_by_head_commit_time(run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    run_git(repo, "branch", "feature/newer")
    run_wtk("checkout", "feature/newer", "--no-clipboard", cwd=repo)
    linked = linked_worktree_path(repo, "feature/newer")
    (linked / "newer.txt").write_text("newer\n", encoding="utf-8")
    run_git(linked, "add", ".")
    run_git(
        linked,
        "commit",
        "-m",
        "newer",
        env={
            "GIT_AUTHOR_DATE": "2030-01-01T00:00:00Z",
            "GIT_COMMITTER_DATE": "2030-01-01T00:00:00Z",
        },
    )

    listing = run_wtk("list", cwd=repo).stdout
    rows = [line for line in listing.splitlines()[1:] if line.strip()]
    assert rows[0].startswith("  repo-wt-feature-newer ")
    assert rows[1].startswith("* repo ")


def test_repo_mode_new_with_explicit_base_from_current_and_dirty_failures(run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo", branch="trunk")
    run_wtk("new", "feature/new", "--base", "trunk", "--no-clipboard", cwd=repo)
    linked = linked_worktree_path(repo, "feature/new")
    assert linked.exists()
    assert not linked.joinpath("WTK-AUXILIARY.md").exists()
    run_git(repo, "worktree", "remove", str(linked))

    run_git(repo, "switch", "-c", "feature/base")
    (repo / "base.txt").write_text("base\n", encoding="utf-8")
    run_git(repo, "add", ".")
    run_git(repo, "commit", "-m", "base")

    run_wtk("new", "feature/from-current", "--from-current", "--no-clipboard", cwd=repo)
    assert linked_worktree_path(repo, "feature/from-current").joinpath("base.txt").exists()
    run_wtk("new", "feature/from-current-short", "-C", "--no-clipboard", cwd=repo)
    assert linked_worktree_path(repo, "feature/from-current-short").joinpath("base.txt").exists()

    conflict = run_wtk(
        "new",
        "feature/conflict",
        "--base",
        "trunk",
        "--from-current",
        "--no-clipboard",
        cwd=repo,
        check=False,
    )
    conflict.assert_failure()
    assert "--base and --from-current cannot be used together" in conflict.output

    (repo / "dirty.txt").write_text("dirty\n", encoding="utf-8")
    dirty = run_wtk("send-out", "--no-clipboard", cwd=repo, check=False)
    dirty.assert_failure()
    assert "worktree is dirty" in dirty.output


def test_repo_mode_dirty_linked_and_main_branch_resolution_failures(run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    run_git(repo, "branch", "feature/dirty-linked")
    run_wtk("checkout", "feature/dirty-linked", "--no-clipboard", cwd=repo)
    linked = linked_worktree_path(repo, "feature/dirty-linked")
    (linked / "dirty.txt").write_text("dirty\n", encoding="utf-8")

    removed = run_wtk("remove", str(linked), "--no-clipboard", cwd=repo, check=False)
    removed.assert_failure()
    assert "worktree is dirty" in removed.output

    brought = run_wtk("bring-in", "feature/dirty-linked", "--no-clipboard", cwd=repo, check=False)
    brought.assert_failure()
    assert "worktree is dirty" in brought.output

    ambiguous = repo_factory.init_repo("ambiguous")
    run_git(ambiguous, "branch", "trunk")
    run_git(ambiguous, "switch", "-c", "feature/ambiguous")
    result = run_wtk("send-out", "--no-clipboard", cwd=ambiguous, check=False)
    result.assert_failure()
    assert "cannot determine main branch" in result.output


def test_repo_mode_default_base_fast_forward_and_non_fast_forward_refusal(run_wtk, repo_factory, tmp_path) -> None:
    origin = tmp_path / "origin.git"
    run_git(tmp_path, "init", "--bare", str(origin))

    seed = tmp_path / "seed"
    run_git(tmp_path, "clone", str(origin), str(seed))
    run_git(seed, "switch", "-c", "main")
    run_git(seed, "config", "user.email", "test@example.com")
    run_git(seed, "config", "user.name", "Test User")
    (seed / "README.md").write_text("one\n", encoding="utf-8")
    run_git(seed, "add", ".")
    run_git(seed, "commit", "-m", "one")
    run_git(seed, "push", "-u", "origin", "main")

    repo = tmp_path / "repo"
    run_git(tmp_path, "clone", str(origin), str(repo))
    run_git(repo, "switch", "main")
    run_git(repo, "config", "user.email", "test@example.com")
    run_git(repo, "config", "user.name", "Test User")

    (seed / "remote.txt").write_text("two\n", encoding="utf-8")
    run_git(seed, "add", ".")
    run_git(seed, "commit", "-m", "two")
    run_git(seed, "push")

    run_wtk("new", "feature/from-updated-main", "--no-clipboard", cwd=repo)
    assert linked_worktree_path(repo, "feature/from-updated-main").joinpath("remote.txt").exists()

    diverged = tmp_path / "diverged"
    run_git(tmp_path, "clone", str(origin), str(diverged))
    run_git(diverged, "switch", "main")
    run_git(diverged, "config", "user.email", "test@example.com")
    run_git(diverged, "config", "user.name", "Test User")
    run_git(diverged, "switch", "-c", "side")
    (diverged / "local.txt").write_text("local\n", encoding="utf-8")
    run_git(diverged, "add", ".")
    run_git(diverged, "commit", "-m", "local")
    run_git(diverged, "branch", "-f", "main", "HEAD")

    result = run_wtk("new", "feature/refuse", "--no-clipboard", cwd=diverged, check=False)
    result.assert_failure()
    assert "refusing to move it without a fast-forward" in result.output
