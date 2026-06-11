from __future__ import annotations

import json

from conftest import linked_worktree_path, parse_yaml, run_git


def test_workspace_mode_init_add_status_new_and_remove(run_wtk, workspace_factory, repo_factory) -> None:
    workspace, members = workspace_factory.create()

    run_wtk("workspace", "init", cwd=workspace)
    run_wtk("workspace", "add", str(members["A"]), cwd=workspace)
    run_wtk("workspace", "add", str(members["B"]), cwd=workspace)
    repo_factory.commit_workspace_manifest(workspace)

    manifest_text = (workspace / ".wtk-workspace.toml").read_text(encoding="utf-8")
    assert 'mode = "workspace"' in manifest_text
    assert (workspace / "refs" / "A").resolve() == members["A"].resolve()
    assert (workspace / "refs" / "B").resolve() == members["B"].resolve()

    status = parse_yaml(run_wtk("status", cwd=workspace).stdout)
    assert status["mode"] == "workspace"
    assert status["workspace_branch"] == "main"
    assert status["current_is_main"] is True
    assert len(status["refs"]) == 2

    out = run_wtk("new", "feature/ws", "--base", "main", "--no-clipboard", cwd=workspace).output
    workspace_linked = linked_worktree_path(workspace, "feature/ws")
    linked_a = linked_worktree_path(members["A"], "feature/ws")
    linked_b = linked_worktree_path(members["B"], "feature/ws")
    assert str(workspace_linked) in out
    assert workspace_linked.exists()
    assert linked_a.exists()
    assert linked_b.exists()

    linked_status = parse_yaml(run_wtk("status", cwd=workspace_linked).stdout)
    assert linked_status["workspace_branch"] == "feature/ws"
    assert linked_status["current_is_main"] is False
    assert (workspace_linked / "refs" / "A").resolve() == linked_a.resolve()
    assert (workspace_linked / "refs" / "B").resolve() == linked_b.resolve()

    run_wtk("remove", "feature/ws", "--delete-branch", "--no-clipboard", cwd=workspace)
    assert not workspace_linked.exists()
    assert not linked_a.exists()
    assert not linked_b.exists()
    assert "feature/ws" not in run_git(workspace, "branch", "--list", "feature/ws").stdout
    assert "feature/ws" not in run_git(members["A"], "branch", "--list", "feature/ws").stdout
    assert "feature/ws" not in run_git(members["B"], "branch", "--list", "feature/ws").stdout


def test_workspace_mode_status_and_membership_failures(run_wtk, workspace_factory, repo_factory) -> None:
    workspace, members = workspace_factory.create(member_names=("A", "B"))

    run_wtk("workspace", "init", cwd=workspace)
    run_wtk("workspace", "add", str(members["A"]), cwd=workspace)
    repo_factory.commit_workspace_manifest(workspace)
    run_wtk("new", "feature/ws", "--base", "main", "--no-clipboard", cwd=workspace)

    workspace_linked = linked_worktree_path(workspace, "feature/ws")
    (workspace_linked / "refs" / "A").unlink()

    missing = run_wtk("status", cwd=workspace_linked, check=False)
    missing.assert_failure()
    assert "failed to read Workspace Ref" in missing.output

    not_main = run_wtk("workspace", "add", str(members["B"]), cwd=workspace_linked, check=False)
    not_main.assert_failure()
    assert "workspace add must be run from the Workspace main worktree" in not_main.output


def test_workspace_mode_list_shows_workspace_rows_and_ref_health(run_wtk, workspace_factory, repo_factory) -> None:
    workspace, members = workspace_factory.create(member_names=("A", "B"))

    run_wtk("workspace", "init", cwd=workspace)
    run_wtk("workspace", "add", str(members["A"]), cwd=workspace)
    run_wtk("workspace", "add", str(members["B"]), cwd=workspace)
    repo_factory.commit_workspace_manifest(workspace)
    run_wtk("new", "feature/list", "--base", "main", "--no-clipboard", cwd=workspace)
    workspace_linked = linked_worktree_path(workspace, "feature/list")

    listing = run_wtk("list", cwd=workspace).stdout
    assert "worktree" in listing.splitlines()[0]
    assert "workspace" in listing
    assert "workspace-wt-feature-list" in listing
    assert "refs 2/2 ok" in listing
    assert str(workspace.resolve()) not in listing
    assert str(workspace_linked.resolve()) not in listing

    (workspace_linked / "refs" / "A").unlink()
    broken = run_wtk("list", cwd=workspace).stdout
    assert "workspace-wt-feature-list" in broken
    assert "refs 1/2 broken" in broken

    machine = json.loads(run_wtk("list", "--json", cwd=workspace).stdout)
    assert machine["mode"] == "workspace"
    assert all(not row["dirty"] for row in machine["worktrees"])
    linked_row = next(row for row in machine["worktrees"] if row["display_name"] == "workspace-wt-feature-list")
    assert linked_row["workspace_refs"]["total"] == 2
    assert linked_row["workspace_refs"]["broken"] == 1
    assert any(not detail["ok"] and detail["name"] == "A" for detail in linked_row["workspace_refs"]["details"])


def test_workspace_mode_list_marks_branch_mismatched_ref_targets_broken(
    run_wtk, workspace_factory, repo_factory
) -> None:
    workspace, members = workspace_factory.create(member_names=("A",))

    run_wtk("workspace", "init", cwd=workspace)
    run_wtk("workspace", "add", str(members["A"]), cwd=workspace)
    repo_factory.commit_workspace_manifest(workspace)
    run_wtk("new", "feature/list", "--base", "main", "--no-clipboard", cwd=workspace)

    linked_a = linked_worktree_path(members["A"], "feature/list")
    run_git(linked_a, "checkout", "-b", "other")

    machine = json.loads(run_wtk("list", "--json", cwd=workspace).stdout)
    linked_row = next(row for row in machine["worktrees"] if row["display_name"] == "workspace-wt-feature-list")
    detail = next(detail for detail in linked_row["workspace_refs"]["details"] if detail["name"] == "A")

    assert linked_row["workspace_refs"]["broken"] == 1
    assert detail["ok"] is False
    assert any("branch mismatch" in diagnostic for diagnostic in detail["diagnostics"])


def test_workspace_mode_list_marks_invalid_manifest_repository_paths_broken(
    run_wtk, workspace_factory, repo_factory
) -> None:
    workspace, members = workspace_factory.create(member_names=("A", "B"))

    run_wtk("workspace", "init", cwd=workspace)
    run_wtk("workspace", "add", str(members["A"]), cwd=workspace)
    run_wtk("workspace", "add", str(members["B"]), cwd=workspace)
    repo_factory.commit_workspace_manifest(workspace)
    run_wtk("new", "feature/list", "--base", "main", "--no-clipboard", cwd=workspace)

    manifest_path = workspace / ".wtk-workspace.toml"
    manifest_text = manifest_path.read_text(encoding="utf-8")
    manifest_text = manifest_text.replace(
        f'repository = "{members["A"]}"',
        'repository = "../A"',
    )
    manifest_text = manifest_text.replace(
        f'repository = "{members["B"]}"',
        f'repository = "{linked_worktree_path(members["B"], "feature/list")}"',
    )
    manifest_path.write_text(manifest_text, encoding="utf-8")

    machine = json.loads(run_wtk("list", "--json", cwd=workspace).stdout)
    linked_row = next(row for row in machine["worktrees"] if row["display_name"] == "workspace-wt-feature-list")
    detail_a = next(detail for detail in linked_row["workspace_refs"]["details"] if detail["name"] == "A")
    detail_b = next(detail for detail in linked_row["workspace_refs"]["details"] if detail["name"] == "B")

    assert linked_row["workspace_refs"]["broken"] == 2
    assert detail_a["ok"] is False
    assert any("repository path must be absolute" in diagnostic for diagnostic in detail_a["diagnostics"])
    assert detail_b["ok"] is False
    assert any(
        "configured repository does not resolve to its main worktree" in diagnostic
        or "must match repository basename" in diagnostic
        for diagnostic in detail_b["diagnostics"]
    )


def test_workspace_mode_list_marks_mismatched_ref_names_broken(
    run_wtk, workspace_factory, repo_factory
) -> None:
    workspace, members = workspace_factory.create(member_names=("A",))

    run_wtk("workspace", "init", cwd=workspace)
    run_wtk("workspace", "add", str(members["A"]), cwd=workspace)
    repo_factory.commit_workspace_manifest(workspace)
    run_wtk("new", "feature/list", "--base", "main", "--no-clipboard", cwd=workspace)

    manifest_path = workspace / ".wtk-workspace.toml"
    manifest_text = manifest_path.read_text(encoding="utf-8")
    manifest_text = manifest_text.replace('[workspace.refs.A]', '[workspace.refs.renamed]')
    manifest_path.write_text(manifest_text, encoding="utf-8")

    machine = json.loads(run_wtk("list", "--json", cwd=workspace).stdout)
    linked_row = next(row for row in machine["worktrees"] if row["display_name"] == "workspace-wt-feature-list")
    detail = next(
        detail for detail in linked_row["workspace_refs"]["details"] if detail["name"] == "renamed"
    )

    assert linked_row["workspace_refs"]["broken"] == 1
    assert detail["ok"] is False
    assert any(
        "workspace ref renamed must match repository basename A" in diagnostic
        for diagnostic in detail["diagnostics"]
    )


def test_workspace_mode_rejects_repo_only_commands(run_wtk, workspace_factory, repo_factory) -> None:
    workspace, members = workspace_factory.create(member_names=("A",))

    run_wtk("workspace", "init", cwd=workspace)
    run_wtk("workspace", "add", str(members["A"]), cwd=workspace)
    repo_factory.commit_workspace_manifest(workspace)

    for args in [
        ("checkout", "main", "--no-clipboard"),
        ("send-out", "--no-clipboard"),
        ("bring-in", "main", "--no-clipboard"),
    ]:
        result = run_wtk(*args, cwd=workspace, check=False)
        result.assert_failure()
        assert "not supported in Workspace Mode" in result.output


def test_workspace_bootstrap_rejects_non_empty_directory(run_wtk, tmp_path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    (workspace / "README.md").write_text("not empty\n", encoding="utf-8")

    result = run_wtk("workspace", "bootstrap", "../A", cwd=workspace, check=False)

    result.assert_failure()
    assert "workspace bootstrap requires an empty directory" in result.output


def test_workspace_bootstrap_creates_manifest_refs_and_initial_commit(run_wtk, tmp_path, repo_factory) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    member_a = repo_factory.init_repo("A")
    member_b = repo_factory.init_repo("B")
    git_config_global = tmp_path / "gitconfig"
    git_config_global.write_text("[init]\n\tdefaultBranch = master\n", encoding="utf-8")

    run_wtk(
        "workspace",
        "bootstrap",
        str(member_a),
        str(member_b),
        cwd=workspace,
        env={
            "GIT_AUTHOR_NAME": "Test User",
            "GIT_AUTHOR_EMAIL": "test@example.com",
            "GIT_COMMITTER_NAME": "Test User",
            "GIT_COMMITTER_EMAIL": "test@example.com",
            "GIT_CONFIG_GLOBAL": str(git_config_global),
        },
    )

    manifest_text = (workspace / ".wtk-workspace.toml").read_text(encoding="utf-8")
    gitignore_text = (workspace / ".gitignore").read_text(encoding="utf-8")
    agents_text = (workspace / "AGENTS.md").read_text(encoding="utf-8")
    assert run_git(workspace, "branch", "--show-current").stdout.strip() == "main"
    assert 'mode = "workspace"' in manifest_text
    assert gitignore_text == "refs/\n"
    assert "Workspace Guidance" in agents_text
    assert "Workspace Manifest" in agents_text
    assert (workspace / "refs" / "A").resolve() == member_a.resolve()
    assert (workspace / "refs" / "B").resolve() == member_b.resolve()
    assert run_git(workspace, "cat-file", "-e", "HEAD:.wtk-workspace.toml").returncode == 0
    assert run_git(workspace, "cat-file", "-e", "HEAD:.gitignore").returncode == 0
    assert run_git(workspace, "cat-file", "-e", "HEAD:AGENTS.md").returncode == 0
    head_files = set(run_git(workspace, "ls-tree", "--name-only", "HEAD").stdout.splitlines())
    assert {".wtk-workspace.toml", ".gitignore", "AGENTS.md"} <= head_files

    out = run_wtk("new", "feature/ws", "--base", "main", "--no-clipboard", cwd=workspace).output
    workspace_linked = linked_worktree_path(workspace, "feature/ws")
    linked_a = linked_worktree_path(member_a, "feature/ws")
    linked_b = linked_worktree_path(member_b, "feature/ws")
    assert str(workspace_linked) in out
    assert workspace_linked.exists()
    assert linked_a.exists()
    assert linked_b.exists()
    assert (workspace_linked / "refs" / "A").resolve() == linked_a.resolve()
    assert (workspace_linked / "refs" / "B").resolve() == linked_b.resolve()


def test_workspace_bootstrap_rejects_duplicate_ref_names_before_git_init(run_wtk, tmp_path, repo_factory) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    member = repo_factory.init_repo("A")

    result = run_wtk("workspace", "bootstrap", str(member), str(member), cwd=workspace, check=False)

    result.assert_failure()
    assert "duplicate Workspace Ref name: A" in result.output
    assert not (workspace / ".git").exists()


def test_workspace_bootstrap_rejects_non_repository_before_git_init(run_wtk, tmp_path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    not_repo = tmp_path / "not-repo"
    not_repo.mkdir()

    result = run_wtk("workspace", "bootstrap", str(not_repo), cwd=workspace, check=False)

    result.assert_failure()
    assert "git rev-parse --show-toplevel" in result.output
    assert not (workspace / ".git").exists()


def test_workspace_bootstrap_rejects_non_main_linked_repository_before_git_init(run_wtk, tmp_path, repo_factory) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    member = repo_factory.init_repo("A", branch="develop")

    result = run_wtk("workspace", "bootstrap", str(member), cwd=workspace, check=False)

    result.assert_failure()
    assert "linked repository main worktrees to be on main" in result.output
    assert not (workspace / ".git").exists()


def test_workspace_mode_new_requires_clean_manifest_history(run_wtk, workspace_factory, repo_factory) -> None:
    workspace, members = workspace_factory.create()

    run_wtk("workspace", "init", cwd=workspace)
    run_wtk("workspace", "add", str(members["A"]), cwd=workspace)
    repo_factory.commit_workspace_manifest(workspace)
    run_wtk("workspace", "add", str(members["B"]), cwd=workspace)

    dirty = run_wtk("new", "feature/ws-dirty", "--base", "main", "--no-clipboard", cwd=workspace, check=False)
    dirty.assert_failure()
    assert "requires committed .wtk-workspace.toml changes" in dirty.output

    run_git(workspace, "add", ".wtk-workspace.toml")
    staged = run_wtk("new", "feature/ws-staged", "--base", "main", "--no-clipboard", cwd=workspace, check=False)
    staged.assert_failure()
    assert "requires committed .wtk-workspace.toml changes" in staged.output


def test_workspace_mode_new_does_not_roll_back_when_async_pnpm_install_fails(
    run_wtk, workspace_factory, repo_factory
) -> None:
    workspace, members = workspace_factory.create()

    run_wtk("workspace", "init", cwd=workspace)
    run_wtk("workspace", "add", str(members["A"]), cwd=workspace)
    run_wtk("workspace", "add", str(members["B"]), cwd=workspace)
    repo_factory.commit_workspace_manifest(workspace)

    repo_factory.commit_files(members["A"], {".gitignore": ".env\n"}, "ignore env")
    repo_factory.commit_files(members["B"], {".gitignore": ".env\n"}, "ignore env")
    (members["A"] / ".env").write_text("A=value\n", encoding="utf-8")
    (members["B"] / ".env").write_text("B=value\n", encoding="utf-8")
    repo_factory.add_real_pnpm_project(members["A"], marker_name=".pnpm-ok.txt")
    repo_factory.add_real_pnpm_project(members["B"], fail_postinstall=True, marker_name=".pnpm-fail.txt")

    result = run_wtk("new", "feature/ws-fail", "--base", "main", "--no-clipboard", cwd=workspace)
    assert "running pnpm install asynchronously" in result.output
    assert "async pnpm install output will be written to" in result.output

    workspace_linked = linked_worktree_path(workspace, "feature/ws-fail")
    linked_a = linked_worktree_path(members["A"], "feature/ws-fail")
    linked_b = linked_worktree_path(members["B"], "feature/ws-fail")
    assert workspace_linked.exists()
    assert linked_a.exists()
    assert linked_b.exists()
    assert (workspace_linked / "refs" / "A").resolve() == linked_a.resolve()
    assert (workspace_linked / "refs" / "B").resolve() == linked_b.resolve()
    assert "feature/ws-fail" in run_git(
        workspace, "branch", "--list", "feature/ws-fail"
    ).stdout
    assert "feature/ws-fail" in run_git(
        members["A"], "branch", "--list", "feature/ws-fail"
    ).stdout
    assert "feature/ws-fail" in run_git(
        members["B"], "branch", "--list", "feature/ws-fail"
    ).stdout


def test_workspace_mode_new_rolls_back_before_starting_async_pnpm_for_any_linked_repo(
    run_wtk, workspace_factory, repo_factory
) -> None:
    workspace, members = workspace_factory.create()

    run_wtk("workspace", "init", cwd=workspace)
    run_wtk("workspace", "add", str(members["A"]), cwd=workspace)
    run_wtk("workspace", "add", str(members["B"]), cwd=workspace)
    repo_factory.commit_workspace_manifest(workspace)

    run_git(workspace, "branch", "broken-base")
    run_git(members["A"], "branch", "broken-base")

    run_git(members["B"], "checkout", "-b", "broken-base")
    repo_factory.commit_files(members["B"], {"config": "tracked\n"}, "add tracked config file")
    run_git(members["B"], "checkout", "main")

    repo_factory.commit_files(members["A"], {".gitignore": ".env\n"}, "ignore env")
    (members["A"] / ".env").write_text("A=value\n", encoding="utf-8")
    repo_factory.add_real_pnpm_project(members["A"], delay_seconds=2.0, marker_name=".pnpm-a.txt")

    repo_factory.commit_files(members["B"], {".gitignore": "config/.env\n"}, "ignore nested env")
    (members["B"] / "config").mkdir()
    (members["B"] / "config" / ".env").write_text("B=value\n", encoding="utf-8")

    result = run_wtk(
        "new",
        "feature/ws-copy-fail",
        "--base",
        "broken-base",
        "--no-clipboard",
        cwd=workspace,
        check=False,
    )
    result.assert_failure()
    assert "ignored .env copy failed" in result.output
    assert "running pnpm install asynchronously" not in result.output

    workspace_linked = linked_worktree_path(workspace, "feature/ws-copy-fail")
    linked_a = linked_worktree_path(members["A"], "feature/ws-copy-fail")
    linked_b = linked_worktree_path(members["B"], "feature/ws-copy-fail")
    assert not workspace_linked.exists()
    assert not linked_a.exists()
    assert not linked_b.exists()
    assert "feature/ws-copy-fail" not in run_git(
        workspace, "branch", "--list", "feature/ws-copy-fail"
    ).stdout
    assert "feature/ws-copy-fail" not in run_git(
        members["A"], "branch", "--list", "feature/ws-copy-fail"
    ).stdout
    assert "feature/ws-copy-fail" not in run_git(
        members["B"], "branch", "--list", "feature/ws-copy-fail"
    ).stdout
