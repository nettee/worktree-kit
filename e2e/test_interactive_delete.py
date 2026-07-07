from __future__ import annotations

import os

import pytest

from conftest import linked_worktree_path, run_git, run_pty_cmd


pytestmark = pytest.mark.skipif(os.name == "nt", reason="PTY helper requires Unix pseudo-terminals")


def run_delete_pty(wtk_bin, cwd, keys: bytes, timeout: float = 10.0, input_after_output: str | None = None):
    return run_pty_cmd(
        [str(wtk_bin), "delete"],
        cwd=cwd,
        input_bytes=keys,
        input_after_output=input_after_output,
        timeout=timeout,
    )


def test_interactive_delete_standalone_space_enter_exact_y(wtk_bin, run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    run_git(repo, "branch", "feature/delete")
    run_wtk("checkout", "feature/delete", "--no-clipboard", cwd=repo)
    linked = linked_worktree_path(repo, "feature/delete")

    result = run_delete_pty(wtk_bin, repo, b" \rY\r").assert_success()

    assert not linked.exists()
    assert "Deletion complete." in result.output
    assert "feature/delete" in run_git(repo, "branch", "--list", "feature/delete").stdout


@pytest.mark.parametrize("keys", [b"\r", b" \rn\r"])
def test_interactive_delete_cancels_empty_selection_or_non_y(wtk_bin, run_wtk, repo_factory, keys) -> None:
    repo = repo_factory.init_repo("repo")
    run_git(repo, "branch", "feature/cancel")
    run_wtk("checkout", "feature/cancel", "--no-clipboard", cwd=repo)
    linked = linked_worktree_path(repo, "feature/cancel")

    result = run_delete_pty(wtk_bin, repo, keys).assert_success()

    assert linked.exists()
    assert "cancel" in result.output.lower()


def test_interactive_delete_removes_dirty_worktree(wtk_bin, run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    run_git(repo, "branch", "feature/dirty")
    run_wtk("checkout", "feature/dirty", "--no-clipboard", cwd=repo)
    linked = linked_worktree_path(repo, "feature/dirty")
    (linked / "dirty.txt").write_text("dirty\n", encoding="utf-8")

    result = run_delete_pty(wtk_bin, repo, b" \rY\r").assert_success()

    assert not linked.exists()
    assert "dirty: yes" in result.output


def test_interactive_delete_can_select_second_and_multiple_rows(wtk_bin, run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    run_git(repo, "branch", "feature/a")
    run_git(repo, "branch", "feature/b")
    run_wtk("checkout", "feature/a", "--no-clipboard", cwd=repo)
    run_wtk("checkout", "feature/b", "--no-clipboard", cwd=repo)
    first = linked_worktree_path(repo, "feature/a")
    second = linked_worktree_path(repo, "feature/b")

    result = run_delete_pty(wtk_bin, repo, b" \x1b[B \rY\r").assert_success()

    assert not first.exists()
    assert not second.exists()
    assert "Deletion complete." in result.output


def test_interactive_delete_table_shows_dirty_updated_and_inline_non_deletable_reason(wtk_bin, run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    run_git(repo, "branch", "feature/overview")
    run_wtk("checkout", "feature/overview", "--no-clipboard", cwd=repo)
    linked = linked_worktree_path(repo, "feature/overview")
    (linked / "dirty.txt").write_text("dirty\n", encoding="utf-8")

    result = run_delete_pty(wtk_bin, repo, b"\r").assert_success()

    assert linked.exists()
    assert "Select worktrees to delete. Space toggles" in result.output
    assert "sel" in result.output
    assert "worktree" in result.output
    assert "branch" in result.output
    assert "updated" in result.output
    assert "state" in result.output
    assert "head" in result.output
    assert "Enter numbers/ranges to delete" not in result.output
    assert "Select worktrees to delete (Space to select, Enter to continue)" not in result.output
    assert "repo" in result.output
    assert "feature/overview" in result.output
    assert "dirty" in result.output
    assert "non-deletable: main worktree cannot be deleted" in result.output
    assert "Non-deletable worktrees:" not in result.output
    assert "Skipping non-deletable worktree" not in result.output


def test_interactive_delete_fails_without_tty(run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")

    result = run_wtk("delete", cwd=repo, check=False)

    result.assert_failure()
    assert "requires an interactive terminal" in result.output


def test_interactive_delete_esc_cancels(wtk_bin, run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    run_git(repo, "branch", "feature/invalid")
    run_wtk("checkout", "feature/invalid", "--no-clipboard", cwd=repo)
    linked = linked_worktree_path(repo, "feature/invalid")

    result = run_delete_pty(wtk_bin, repo, b"\x1b", timeout=5.0).assert_success()

    assert linked.exists()
    assert "No worktrees selected; cancelled." in result.output


def test_interactive_delete_ctrl_c_interrupts(wtk_bin, run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    run_git(repo, "branch", "feature/interrupt")
    run_wtk("checkout", "feature/interrupt", "--no-clipboard", cwd=repo)
    linked = linked_worktree_path(repo, "feature/interrupt")

    result = run_delete_pty(
        wtk_bin,
        repo,
        b"\x03",
        timeout=5.0,
        input_after_output="Select worktrees to delete. Space toggles",
    )

    result.assert_failure()
    assert linked.exists()
    assert "interactive selection interrupted" in result.output


def test_interactive_delete_coordinated_primary_cascades(wtk_bin, run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")
    run_wtk("ag", "add", "backend", str(api), cwd=primary)
    run_wtk("new", "feature/coord", "--base", "main", "--ag", "backend", "--no-clipboard", cwd=primary)
    primary_linked = linked_worktree_path(primary, "feature/coord")
    api_linked = linked_worktree_path(api, "feature/coord")

    result = run_delete_pty(wtk_bin, primary, b" \rY\r").assert_success()

    assert not primary_linked.exists()
    assert not api_linked.exists()
    assert "coordinated members" in result.output
    assert "feature/coord" in run_git(primary, "branch", "--list", "feature/coord").stdout
    assert "feature/coord" in run_git(api, "branch", "--list", "feature/coord").stdout


def test_interactive_delete_coordinated_dirty_auxiliary_is_shown_before_confirmation(wtk_bin, run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")
    run_wtk("ag", "add", "backend", str(api), cwd=primary)
    run_wtk("new", "feature/dirty-aux", "--base", "main", "--ag", "backend", "--no-clipboard", cwd=primary)
    primary_linked = linked_worktree_path(primary, "feature/dirty-aux")
    api_linked = linked_worktree_path(api, "feature/dirty-aux")
    (api_linked / "dirty.txt").write_text("dirty\n", encoding="utf-8")

    result = run_delete_pty(wtk_bin, primary, b" \rn\r").assert_success()

    assert primary_linked.exists()
    assert api_linked.exists()
    assert f"{api_linked} dirty: yes" in result.output


def test_interactive_delete_broken_coordinated_row_does_not_abort_healthy_delete(wtk_bin, run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")
    run_wtk("ag", "add", "backend", str(api), cwd=primary)
    run_wtk("new", "feature/broken", "--base", "main", "--ag", "backend", "--no-clipboard", cwd=primary)
    run_git(primary, "branch", "feature/healthy")
    run_wtk("checkout", "feature/healthy", "--no-clipboard", cwd=primary)
    primary_linked = linked_worktree_path(primary, "feature/broken")
    api_linked = linked_worktree_path(api, "feature/broken")
    healthy_linked = linked_worktree_path(primary, "feature/healthy")
    run_git(api, "worktree", "remove", "--force", str(api_linked))

    result = run_delete_pty(wtk_bin, primary, b" \rY\r", timeout=5.0).assert_success()

    assert primary.exists()
    assert primary_linked.exists()
    assert not healthy_linked.exists()
    assert "Deletion complete." in result.output
    assert "Non-deletable worktrees:" not in result.output
    assert "feature/broken" in result.output
    assert "non-deletable:" in result.output
    assert "Skipping non-deletable worktree" not in result.output
