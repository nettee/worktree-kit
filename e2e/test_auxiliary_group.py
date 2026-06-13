from __future__ import annotations

import json

from conftest import git_common_dir, linked_worktree_path, parse_yaml, run_git


def test_auxiliary_group_new_status_list_and_remove(run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")
    web = repo_factory.init_repo("web")
    wtk_dir = git_common_dir(primary) / "wtk"

    run_wtk("auxiliary-group", "add", "full-stack", str(api), str(web), cwd=primary)

    config_text = (wtk_dir / "config.toml").read_text(encoding="utf-8")
    assert "[auxiliaries.api]" in config_text
    assert "[auxiliaries.web]" in config_text
    assert "[groups.full-stack]" in config_text
    assert run_git(primary, "status", "--porcelain", "--untracked-files=normal").stdout == ""

    out = run_wtk(
        "new",
        "feature/aux",
        "--base",
        "main",
        "--ag",
        "full-stack",
        "--no-clipboard",
        cwd=primary,
    ).output
    primary_linked = linked_worktree_path(primary, "feature/aux")
    api_linked = linked_worktree_path(api, "feature/aux")
    web_linked = linked_worktree_path(web, "feature/aux")

    assert str(primary_linked) in out
    assert primary_linked.exists()
    assert api_linked.exists()
    assert web_linked.exists()
    assert (primary_linked / "refs" / "api").resolve() == api_linked.resolve()
    assert (primary_linked / "refs" / "web").resolve() == web_linked.resolve()

    state = json.loads((wtk_dir / "worktrees.json").read_text(encoding="utf-8"))
    entry = state["worktrees"][str(primary_linked.resolve())]
    assert entry["branch"] == "feature/aux"
    assert set(entry["auxiliaries"]) == {"api", "web"}
    assert run_git(primary, "status", "--porcelain", "--untracked-files=normal").stdout == ""

    status = parse_yaml(run_wtk("status", cwd=primary_linked).stdout)
    assert status["mode"] == "coordinated"
    assert status["primary_worktree"] == str(primary_linked.resolve())
    assert set(status["auxiliaries"]) == {"api", "web"}

    listing = json.loads(run_wtk("list", "--json", cwd=primary).stdout)
    row = next(row for row in listing["worktrees"] if row["path"] == str(primary_linked.resolve()))
    assert row["dirty"] is False
    assert "dirty" not in row["labels"]
    assert row["auxiliary_refs"]["total"] == 2
    assert row["auxiliary_refs"]["broken"] == 0

    rejected = run_wtk("send-out", "--no-clipboard", cwd=primary_linked, check=False)
    rejected.assert_failure()
    assert "auxiliary state" in rejected.output

    run_wtk("remove", str(primary_linked), "--delete-branch", "--no-clipboard", cwd=primary)
    assert not primary_linked.exists()
    assert not api_linked.exists()
    assert not web_linked.exists()
    assert "feature/aux" not in run_git(primary, "branch", "--list", "feature/aux").stdout
    assert "feature/aux" not in run_git(api, "branch", "--list", "feature/aux").stdout
    assert "feature/aux" not in run_git(web, "branch", "--list", "feature/aux").stdout


def test_auxiliary_group_remove_preserves_real_refs_changes(run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")

    run_wtk("auxiliary-group", "add", "backend", str(api), cwd=primary)
    run_wtk(
        "new",
        "feature/aux",
        "--base",
        "main",
        "--ag",
        "backend",
        "--no-clipboard",
        cwd=primary,
    )
    primary_linked = linked_worktree_path(primary, "feature/aux")
    (primary_linked / "refs" / "notes.txt").write_text("keep me\n", encoding="utf-8")

    removed = run_wtk(
        "remove",
        str(primary_linked),
        "--delete-branch",
        "--no-clipboard",
        cwd=primary,
        check=False,
    )

    removed.assert_failure()
    assert "worktree is dirty" in removed.output
    assert "refs/notes.txt" in removed.output
    assert primary_linked.exists()


def test_auxiliary_group_list_preserves_real_refs_changes(run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")

    run_wtk("auxiliary-group", "add", "backend", str(api), cwd=primary)
    run_wtk(
        "new",
        "feature/aux",
        "--base",
        "main",
        "--ag",
        "backend",
        "--no-clipboard",
        cwd=primary,
    )
    primary_linked = linked_worktree_path(primary, "feature/aux")
    (primary_linked / "refs" / "notes.txt").write_text("keep me\n", encoding="utf-8")

    listing = json.loads(run_wtk("list", "--json", cwd=primary).stdout)
    row = next(row for row in listing["worktrees"] if row["path"] == str(primary_linked.resolve()))

    assert row["dirty"] is True
    assert "dirty" in row["labels"]


def test_auxiliary_group_reports_auxiliary_branch_drift(run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")

    run_wtk("auxiliary-group", "add", "backend", str(api), cwd=primary)
    run_wtk(
        "new",
        "feature/aux",
        "--base",
        "main",
        "--ag",
        "backend",
        "--no-clipboard",
        cwd=primary,
    )
    primary_linked = linked_worktree_path(primary, "feature/aux")
    api_linked = linked_worktree_path(api, "feature/aux")
    run_git(api_linked, "checkout", "-B", "other-branch")

    status = run_wtk("status", cwd=primary_linked, check=False)
    status.assert_failure()
    assert "expected feature/aux" in status.output
    assert "other-branch" in status.output

    listing = json.loads(run_wtk("list", "--json", cwd=primary).stdout)
    row = next(row for row in listing["worktrees"] if row["path"] == str(primary_linked.resolve()))
    assert row["auxiliary_refs"]["broken"] == 1
    detail = row["auxiliary_refs"]["details"][0]
    assert detail["ok"] is False
    assert any("other-branch" in diagnostic for diagnostic in detail["diagnostics"])


def test_auxiliary_group_status_rejects_primary_branch_drift(run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")

    run_wtk("auxiliary-group", "add", "backend", str(api), cwd=primary)
    run_wtk(
        "new",
        "feature/aux",
        "--base",
        "main",
        "--ag",
        "backend",
        "--no-clipboard",
        cwd=primary,
    )
    primary_linked = linked_worktree_path(primary, "feature/aux")
    run_git(primary_linked, "checkout", "-B", "other-branch")

    status = run_wtk("status", cwd=primary_linked, check=False)

    status.assert_failure()
    assert "Primary worktree" in status.output
    assert "other-branch" in status.output


def test_auxiliary_group_list_reports_primary_branch_drift(run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")

    run_wtk("auxiliary-group", "add", "backend", str(api), cwd=primary)
    run_wtk(
        "new",
        "feature/aux",
        "--base",
        "main",
        "--ag",
        "backend",
        "--no-clipboard",
        cwd=primary,
    )
    primary_linked = linked_worktree_path(primary, "feature/aux")
    run_git(primary_linked, "checkout", "-B", "other-branch")

    listing = json.loads(run_wtk("list", "--json", cwd=primary).stdout)
    row = next(row for row in listing["worktrees"] if row["path"] == str(primary_linked.resolve()))

    assert "error" in row["labels"]
    assert any("Primary worktree" in diagnostic for diagnostic in row["diagnostics"])
    assert any("other-branch" in diagnostic for diagnostic in row["diagnostics"])


def test_auxiliary_group_remove_rejects_primary_branch_drift(run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")

    run_wtk("auxiliary-group", "add", "backend", str(api), cwd=primary)
    run_wtk(
        "new",
        "feature/aux",
        "--base",
        "main",
        "--ag",
        "backend",
        "--no-clipboard",
        cwd=primary,
    )
    primary_linked = linked_worktree_path(primary, "feature/aux")
    run_git(primary_linked, "checkout", "-B", "other-branch")

    removed = run_wtk(
        "remove",
        str(primary_linked),
        "--delete-branch",
        "--no-clipboard",
        cwd=primary,
        check=False,
    )

    removed.assert_failure()
    assert "Primary worktree" in removed.output
    assert "other-branch" in removed.output
    assert "expected feature/aux" in removed.output
    assert primary_linked.exists()


def test_auxiliary_group_remove_preflights_locked_auxiliaries(run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")
    web = repo_factory.init_repo("web")

    run_wtk("auxiliary-group", "add", "full-stack", str(api), str(web), cwd=primary)
    run_wtk(
        "new",
        "feature/aux",
        "--base",
        "main",
        "--ag",
        "full-stack",
        "--no-clipboard",
        cwd=primary,
    )
    primary_linked = linked_worktree_path(primary, "feature/aux")
    api_linked = linked_worktree_path(api, "feature/aux")
    web_linked = linked_worktree_path(web, "feature/aux")
    run_git(web, "worktree", "lock", str(web_linked), "--reason", "held")

    removed = run_wtk(
        "remove",
        str(primary_linked),
        "--delete-branch",
        "--no-clipboard",
        cwd=primary,
        check=False,
    )

    removed.assert_failure()
    assert "Auxiliary worktree web" in removed.output
    assert "locked" in removed.output
    assert primary_linked.exists()
    assert api_linked.exists()
    assert web_linked.exists()

    state = json.loads(
        ((git_common_dir(primary) / "wtk") / "worktrees.json").read_text(encoding="utf-8")
    )
    assert str(primary_linked.resolve()) in state["worktrees"]


def test_auxiliary_group_remove_keeps_state_when_branch_delete_fails(
    run_wtk, repo_factory
) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")

    run_wtk("auxiliary-group", "add", "backend", str(api), cwd=primary)
    run_wtk(
        "new",
        "feature/aux",
        "--base",
        "main",
        "--ag",
        "backend",
        "--no-clipboard",
        cwd=primary,
    )
    primary_linked = linked_worktree_path(primary, "feature/aux")
    api_linked = linked_worktree_path(api, "feature/aux")
    repo_factory.commit_files(primary_linked, {"feature.txt": "branch-only\n"}, "branch work")

    removed = run_wtk(
        "remove",
        str(primary_linked),
        "--delete-branch",
        "--no-clipboard",
        cwd=primary,
        check=False,
    )

    removed.assert_failure()
    assert "branch deletion failed" in removed.output
    assert "coordinated state remains" in removed.output
    assert not primary_linked.exists()
    assert not api_linked.exists()

    state = json.loads(
        ((git_common_dir(primary) / "wtk") / "worktrees.json").read_text(encoding="utf-8")
    )
    assert str(primary_linked.resolve()) in state["worktrees"]


def test_auxiliary_group_remove_preflights_locked_primary(run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")
    web = repo_factory.init_repo("web")

    run_wtk("auxiliary-group", "add", "full-stack", str(api), str(web), cwd=primary)
    run_wtk(
        "new",
        "feature/aux",
        "--base",
        "main",
        "--ag",
        "full-stack",
        "--no-clipboard",
        cwd=primary,
    )
    primary_linked = linked_worktree_path(primary, "feature/aux")
    api_linked = linked_worktree_path(api, "feature/aux")
    web_linked = linked_worktree_path(web, "feature/aux")
    run_git(primary, "worktree", "lock", str(primary_linked), "--reason", "held")

    removed = run_wtk(
        "remove",
        str(primary_linked),
        "--delete-branch",
        "--no-clipboard",
        cwd=primary,
        check=False,
    )

    removed.assert_failure()
    assert "Primary worktree" in removed.output
    assert "locked" in removed.output
    assert primary_linked.exists()
    assert api_linked.exists()
    assert web_linked.exists()

    state = json.loads(
        ((git_common_dir(primary) / "wtk") / "worktrees.json").read_text(encoding="utf-8")
    )
    assert str(primary_linked.resolve()) in state["worktrees"]


def test_auxiliary_group_new_rejects_existing_ref_path(run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")
    repo_factory.commit_files(primary, {"refs/api": "tracked\n"}, "add tracked ref file")

    run_wtk("auxiliary-group", "add", "backend", str(api), cwd=primary)
    result = run_wtk(
        "new",
        "feature/aux",
        "--base",
        "main",
        "--ag",
        "backend",
        "--no-clipboard",
        cwd=primary,
        check=False,
    )

    result.assert_failure()
    assert "will not be overwritten" in result.output
    assert "refs/api" in result.output
    assert (primary / "refs" / "api").read_text(encoding="utf-8") == "tracked\n"
    assert not linked_worktree_path(primary, "feature/aux").exists()


def test_auxiliary_group_ignores_quoted_generated_ref_paths(run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("my api")

    run_wtk("auxiliary-group", "add", "backend", str(api), cwd=primary)
    run_wtk(
        "new",
        "feature/aux",
        "--base",
        "main",
        "--ag",
        "backend",
        "--no-clipboard",
        cwd=primary,
    )
    primary_linked = linked_worktree_path(primary, "feature/aux")

    listing = json.loads(run_wtk("list", "--json", cwd=primary).stdout)
    row = next(row for row in listing["worktrees"] if row["path"] == str(primary_linked.resolve()))

    assert row["dirty"] is False
    assert "dirty" not in row["labels"]

    run_wtk("remove", str(primary_linked), "--delete-branch", "--no-clipboard", cwd=primary)
    assert not primary_linked.exists()


def test_auxiliary_group_add_rejects_duplicate_repository(run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")

    result = run_wtk("ag", "add", "dupe", str(api), str(api), cwd=primary, check=False)

    result.assert_failure()
    assert "duplicate auxiliary repository" in result.output
    assert not ((git_common_dir(primary) / "wtk") / "config.toml").exists()
