from __future__ import annotations

import json

from conftest import linked_worktree_path, parse_yaml, run_git


def test_auxiliary_group_new_status_list_and_remove(run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")
    web = repo_factory.init_repo("web")

    run_wtk("auxiliary-group", "add", "full-stack", str(api), str(web), cwd=primary)

    config_text = (primary / ".wtk" / "config.toml").read_text(encoding="utf-8")
    assert "[auxiliaries.api]" in config_text
    assert "[auxiliaries.web]" in config_text
    assert "[groups.full-stack]" in config_text

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

    state = json.loads((primary / ".wtk" / "worktrees.json").read_text(encoding="utf-8"))
    entry = state["worktrees"][str(primary_linked.resolve())]
    assert entry["branch"] == "feature/aux"
    assert set(entry["auxiliaries"]) == {"api", "web"}

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


def test_auxiliary_group_add_rejects_duplicate_repository(run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")

    result = run_wtk("ag", "add", "dupe", str(api), str(api), cwd=primary, check=False)

    result.assert_failure()
    assert "duplicate auxiliary repository" in result.output
    assert not (primary / ".wtk" / "config.toml").exists()
