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
    assert run_git(primary_linked, "status", "--porcelain", "--untracked-files=normal").stdout == ""
    run_git(primary_linked, "add", ".")
    assert run_git(primary_linked, "diff", "--cached", "--name-only").stdout == ""
    (primary / "refs").mkdir()
    (primary / "refs" / "api").write_text("real ref\n", encoding="utf-8")
    assert run_git(primary, "status", "--porcelain", "--untracked-files=all").stdout == "?? refs/api\n"

    state = json.loads((wtk_dir / "worktrees.json").read_text(encoding="utf-8"))
    entry = state["worktrees"][str(primary_linked.resolve())]
    assert entry["branch"] == "feature/aux"
    assert set(entry["auxiliaries"]) == {"api", "web"}
    assert run_git(primary, "status", "--porcelain", "--untracked-files=all").stdout == "?? refs/api\n"

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
    assert run_git(primary, "status", "--porcelain", "--untracked-files=all").stdout == "?? refs/api\n"
    assert "feature/aux" not in run_git(primary, "branch", "--list", "feature/aux").stdout
    assert "feature/aux" not in run_git(api, "branch", "--list", "feature/aux").stdout
    assert "feature/aux" not in run_git(web, "branch", "--list", "feature/aux").stdout


def test_auxiliary_group_new_escapes_gitignore_metacharacters(run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    wildcard_aux = repo_factory.init_repo("a*")

    run_wtk("auxiliary-group", "add", "backend", str(wildcard_aux), cwd=primary)
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
    (primary_linked / "refs" / "actual").write_text("visible\n", encoding="utf-8")

    assert run_git(primary_linked, "status", "--porcelain", "--untracked-files=all").stdout == "?? refs/actual\n"

    listing = json.loads(run_wtk("list", "--json", cwd=primary).stdout)
    row = next(row for row in listing["worktrees"] if row["path"] == str(primary_linked.resolve()))
    assert row["dirty"] is True
    assert "dirty" in row["labels"]

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
    assert "refs/actual" in removed.output


def test_auxiliary_group_new_preserves_global_excludes(run_wtk, repo_factory, tmp_path) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")
    home = tmp_path / "home"
    home.mkdir()
    global_excludes = tmp_path / "global-excludes"
    global_excludes.write_text(".DS_Store\n", encoding="utf-8")
    env = {"HOME": str(home)}

    run_git(primary, "config", "--global", "core.excludesFile", str(global_excludes), env=env)
    run_wtk("auxiliary-group", "add", "backend", str(api), cwd=primary, env=env)
    run_wtk(
        "new",
        "feature/aux",
        "--base",
        "main",
        "--ag",
        "backend",
        "--no-clipboard",
        cwd=primary,
        env=env,
    )
    primary_linked = linked_worktree_path(primary, "feature/aux")
    (primary_linked / ".DS_Store").write_text("ignored\n", encoding="utf-8")

    assert run_git(primary_linked, "status", "--porcelain", "--untracked-files=normal", env=env).stdout == ""
    run_git(primary_linked, "add", ".", env=env)
    assert run_git(primary_linked, "diff", "--cached", "--name-only", env=env).stdout == ""


def test_auxiliary_group_new_skips_missing_global_excludes(
    run_wtk, repo_factory, tmp_path
) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")
    home = tmp_path / "home"
    home.mkdir()
    missing_global_excludes = tmp_path / "missing-global-excludes"
    env = {"HOME": str(home)}

    run_git(
        primary,
        "config",
        "--global",
        "core.excludesFile",
        str(missing_global_excludes),
        env=env,
    )
    run_wtk("auxiliary-group", "add", "backend", str(api), cwd=primary, env=env)
    run_wtk(
        "new",
        "feature/aux",
        "--base",
        "main",
        "--ag",
        "backend",
        "--no-clipboard",
        cwd=primary,
        env=env,
    )

    primary_linked = linked_worktree_path(primary, "feature/aux")
    assert not missing_global_excludes.exists()
    assert (primary_linked / "refs" / "api").is_symlink()


def test_auxiliary_group_new_preserves_implicit_global_excludes(
    run_wtk, repo_factory, tmp_path
) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")
    home = tmp_path / "home"
    global_excludes = home / ".config" / "git" / "ignore"
    global_excludes.parent.mkdir(parents=True)
    global_excludes.write_text(".DS_Store\n", encoding="utf-8")
    env = {"HOME": str(home)}

    run_wtk("auxiliary-group", "add", "backend", str(api), cwd=primary, env=env)
    run_wtk(
        "new",
        "feature/aux",
        "--base",
        "main",
        "--ag",
        "backend",
        "--no-clipboard",
        cwd=primary,
        env=env,
    )
    primary_linked = linked_worktree_path(primary, "feature/aux")
    (primary_linked / ".DS_Store").write_text("ignored\n", encoding="utf-8")

    assert run_git(primary_linked, "status", "--porcelain", "--untracked-files=normal", env=env).stdout == ""
    run_git(primary_linked, "add", ".", env=env)
    assert run_git(primary_linked, "diff", "--cached", "--name-only", env=env).stdout == ""


def test_auxiliary_group_new_prepares_each_auxiliary_base(run_wtk, tmp_path) -> None:
    primary_origin = tmp_path / "primary-origin.git"
    api_origin = tmp_path / "api-origin.git"
    run_git(tmp_path, "init", "--bare", str(primary_origin))
    run_git(tmp_path, "init", "--bare", str(api_origin))

    primary_seed = tmp_path / "primary-seed"
    api_seed = tmp_path / "api-seed"
    run_git(tmp_path, "clone", str(primary_origin), str(primary_seed))
    run_git(tmp_path, "clone", str(api_origin), str(api_seed))
    for repo in (primary_seed, api_seed):
        run_git(repo, "switch", "-c", "main")
        run_git(repo, "config", "user.email", "test@example.com")
        run_git(repo, "config", "user.name", "Test User")
        (repo / "README.md").write_text("seed\n", encoding="utf-8")
        run_git(repo, "add", ".")
        run_git(repo, "commit", "-m", "init")
        run_git(repo, "push", "-u", "origin", "main")

    primary = tmp_path / "primary"
    api = tmp_path / "api"
    run_git(tmp_path, "clone", str(primary_origin), str(primary))
    run_git(tmp_path, "clone", str(api_origin), str(api))
    for repo in (primary, api):
        run_git(repo, "switch", "main")
        run_git(repo, "config", "user.email", "test@example.com")
        run_git(repo, "config", "user.name", "Test User")

    run_wtk("auxiliary-group", "add", "backend", str(api), cwd=primary)

    (api_seed / "api.txt").write_text("fresh\n", encoding="utf-8")
    run_git(api_seed, "add", "api.txt")
    run_git(api_seed, "commit", "-m", "advance api main")
    run_git(api_seed, "push")
    fresh_main = run_git(api_seed, "rev-parse", "HEAD").stdout.strip()

    assert run_git(api, "rev-parse", "main").stdout.strip() != fresh_main

    run_wtk("new", "feature/aux", "--ag", "backend", "--no-clipboard", cwd=primary)

    api_linked = linked_worktree_path(api, "feature/aux")
    assert api_linked.exists()
    assert (api_linked / "api.txt").read_text(encoding="utf-8") == "fresh\n"
    assert run_git(api_linked, "rev-parse", "HEAD").stdout.strip() == fresh_main


def test_auxiliary_group_new_from_current_uses_primary_branch(run_wtk, repo_factory) -> None:
    primary = repo_factory.init_repo("primary")
    api = repo_factory.init_repo("api")

    run_git(primary, "checkout", "-b", "release/1.0")
    (primary / "primary-release.txt").write_text("primary release\n", encoding="utf-8")
    run_git(primary, "add", "primary-release.txt")
    run_git(primary, "commit", "-m", "primary release")

    run_git(api, "checkout", "-b", "release/1.0")
    (api / "api-release.txt").write_text("api release\n", encoding="utf-8")
    run_git(api, "add", "api-release.txt")
    run_git(api, "commit", "-m", "api release")
    run_git(api, "checkout", "main")

    run_wtk("auxiliary-group", "add", "backend", str(api), cwd=primary)
    run_wtk("new", "feature/aux", "--ag", "backend", "--from-current", "--no-clipboard", cwd=primary)

    primary_linked = linked_worktree_path(primary, "feature/aux")
    api_linked = linked_worktree_path(api, "feature/aux")

    assert (primary_linked / "primary-release.txt").read_text(encoding="utf-8") == "primary release\n"
    assert (api_linked / "api-release.txt").read_text(encoding="utf-8") == "api release\n"
    assert run_git(api_linked, "branch", "--show-current").stdout.strip() == "feature/aux"


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


def test_auxiliary_group_remove_rejects_auxiliary_side_removal(run_wtk, repo_factory) -> None:
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

    removed = run_wtk("remove", "--no-clipboard", cwd=api_linked, check=False)

    removed.assert_failure()
    assert "remove is not supported for worktrees with auxiliary state" in removed.output
    assert api_linked.exists()
    state = json.loads((git_common_dir(primary) / "wtk" / "worktrees.json").read_text(encoding="utf-8"))
    assert str(primary_linked.resolve()) in state["worktrees"]


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


def test_auxiliary_group_bring_in_rejects_auxiliary_side_coordinated_branch(run_wtk, repo_factory) -> None:
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
    api_linked = linked_worktree_path(api, "feature/aux")

    brought_in = run_wtk("bring-in", "feature/aux", "--no-clipboard", cwd=api, check=False)

    brought_in.assert_failure()
    assert "bring-in is not supported for worktrees with auxiliary state" in brought_in.output
    assert api_linked.exists()
    assert run_git(api, "branch", "--show-current").stdout.strip() == "main"


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


def test_auxiliary_group_remove_preflights_branch_delete_failure(
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
    assert "cannot remove coordinated worktree with --delete-branch" in removed.output
    assert "primary branch deletion would fail" in removed.output
    assert primary_linked.exists()
    assert api_linked.exists()

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
