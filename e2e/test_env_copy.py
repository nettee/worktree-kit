from __future__ import annotations

import os
import stat

import pytest

from conftest import linked_worktree_path, wait_until
from conftest import run_git


def write_global_copy_config(home, patterns):
    (home / ".wtk").mkdir(parents=True)
    body = "copy = [\n" + "".join(f'  "{pattern}",\n' for pattern in patterns) + "]\n"
    (home / ".wtk" / "config.toml").write_text(body, encoding="utf-8")


def test_root_and_nested_ignored_env_files_copy(run_wtk, repo_factory, tmp_path) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(
        repo,
        {
            ".gitignore": ".env\napps/web/.env\nservices/api/.env\n",
            "apps/web/keep.txt": "web\n",
            "services/api/keep.txt": "api\n",
        },
        "add env paths",
    )
    (repo / ".env").write_text("ROOT=value\n", encoding="utf-8")
    (repo / "apps" / "web" / ".env").write_text("WEB=value\n", encoding="utf-8")
    (repo / "services" / "api" / ".env").write_text("API=value\n", encoding="utf-8")
    home = tmp_path / "home"
    write_global_copy_config(home, ["**/.env", ".agents/"])

    out = run_wtk("new", "feature/envs", "--base", "main", "--no-clipboard", cwd=repo, env={"HOME": str(home)}).output
    linked = linked_worktree_path(repo, "feature/envs")
    wait_until("copied env files", lambda: linked.joinpath(".env").exists() and linked.joinpath("apps/web/.env").exists())

    assert linked.joinpath(".env").read_text(encoding="utf-8") == "ROOT=value\n"
    assert linked.joinpath("apps/web/.env").read_text(encoding="utf-8") == "WEB=value\n"
    assert linked.joinpath("services/api/.env").read_text(encoding="utf-8") == "API=value\n"
    assert "initializing worktree asynchronously" in out
    assert "copied 3 ignored files" in out
    assert "copied ignored" not in out


def test_root_and_nested_ignored_secrets_auto_tfvars_copy(
    run_wtk, repo_factory, tmp_path
) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(
        repo,
        {
            ".gitignore": "secrets.auto.tfvars\napps/web/secrets.auto.tfvars\nservices/api/secrets.auto.tfvars\n",
            "apps/web/keep.txt": "web\n",
            "services/api/keep.txt": "api\n",
        },
        "add secrets tfvars paths",
    )
    (repo / "secrets.auto.tfvars").write_text('root_secret = "ROOT"\n', encoding="utf-8")
    (repo / "apps" / "web" / "secrets.auto.tfvars").write_text(
        'web_secret = "WEB"\n', encoding="utf-8"
    )
    (repo / "services" / "api" / "secrets.auto.tfvars").write_text(
        'api_secret = "API"\n', encoding="utf-8"
    )
    home = tmp_path / "home"
    (home / ".wtk").mkdir(parents=True)
    (home / ".wtk" / "config.toml").write_text(
        """
copy = [
  "**/secrets.auto.tfvars",
]
""".lstrip(),
        encoding="utf-8",
    )

    out = run_wtk(
        "new",
        "feature/tfvars",
        "--base",
        "main",
        "--no-clipboard",
        cwd=repo,
        env={"HOME": str(home)},
    ).output
    linked = linked_worktree_path(repo, "feature/tfvars")
    wait_until(
        "copied tfvars files",
        lambda: linked.joinpath("secrets.auto.tfvars").exists()
        and linked.joinpath("apps/web/secrets.auto.tfvars").exists()
        and linked.joinpath("services/api/secrets.auto.tfvars").exists(),
    )

    assert linked.joinpath("secrets.auto.tfvars").read_text(encoding="utf-8") == 'root_secret = "ROOT"\n'
    assert (
        linked.joinpath("apps/web/secrets.auto.tfvars").read_text(encoding="utf-8")
        == 'web_secret = "WEB"\n'
    )
    assert (
        linked.joinpath("services/api/secrets.auto.tfvars").read_text(encoding="utf-8")
        == 'api_secret = "API"\n'
    )
    assert "initializing worktree asynchronously" in out


def test_ignored_only_dirs_non_ascii_and_tracked_env_behavior(run_wtk, repo_factory, tmp_path) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(
        repo,
        {
            ".gitignore": "secrets/\ncafé/\n",
            ".env": "TRACKED=value\n",
        },
        "prepare env cases",
    )
    (repo / "secrets").mkdir()
    (repo / "secrets" / ".env").write_text("SECRET=value\n", encoding="utf-8")
    (repo / "café").mkdir()
    (repo / "café" / ".env").write_text("UNICODE=value\n", encoding="utf-8")
    home = tmp_path / "home"
    write_global_copy_config(home, ["**/.env"])

    out = run_wtk("new", "feature/mixed", "--base", "main", "--no-clipboard", cwd=repo, env={"HOME": str(home)}).output
    linked = linked_worktree_path(repo, "feature/mixed")
    wait_until("unicode env copy", lambda: linked.joinpath("café/.env").exists())

    assert linked.joinpath("secrets/.env").read_text(encoding="utf-8") == "SECRET=value\n"
    assert linked.joinpath("café/.env").read_text(encoding="utf-8") == "UNICODE=value\n"
    assert linked.joinpath(".env").read_text(encoding="utf-8") == "TRACKED=value\n"
    assert "copied ignored .env: .env" not in out


@pytest.mark.skipif(os.name == "nt", reason="requires unix symlink and mode semantics")
def test_symlink_and_permissions_are_preserved(run_wtk, repo_factory, tmp_path) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(repo, {".gitignore": ".env\n"}, "ignore env")

    shared_env = tmp_path / "shared.env"
    shared_env.write_text("ROOT=value\n", encoding="utf-8")
    os.symlink(shared_env, repo / ".env")
    home = tmp_path / "home"
    write_global_copy_config(home, ["**/.env"])

    run_wtk("new", "feature/root-env-symlink", "--base", "main", "--no-clipboard", cwd=repo, env={"HOME": str(home)})
    linked = linked_worktree_path(repo, "feature/root-env-symlink")
    wait_until("symlink copy", lambda: linked.joinpath(".env").exists())
    assert linked.joinpath(".env").is_symlink()
    assert linked.joinpath(".env").resolve() == shared_env.resolve()

    run_git(repo, "worktree", "remove", str(linked))
    os.unlink(repo / ".env")
    (repo / ".env").write_text("ROOT=value\n", encoding="utf-8")
    os.chmod(repo / ".env", 0o600)

    run_wtk("new", "feature/root-env-mode", "--base", "main", "--no-clipboard", cwd=repo, env={"HOME": str(home)})
    linked = linked_worktree_path(repo, "feature/root-env-mode")
    wait_until(
        "env permissions",
        lambda: linked.joinpath(".env").exists() and stat.S_IMODE(linked.joinpath(".env").stat().st_mode) == 0o600,
    )


def test_similarly_named_files_are_not_copied(run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(
        repo,
        {
            ".gitignore": ".env.local\n.env.example\n.envrc\nconfig/.env.local\n",
            "config/keep.txt": "keep\n",
        },
        "add ignore patterns",
    )
    (repo / ".env.local").write_text("LOCAL=value\n", encoding="utf-8")
    (repo / ".env.example").write_text("EXAMPLE=value\n", encoding="utf-8")
    (repo / ".envrc").write_text("DIRENV=value\n", encoding="utf-8")
    (repo / "config" / ".env.local").write_text("CHILD=value\n", encoding="utf-8")

    run_wtk("new", "feature/named-envs", "--base", "main", "--no-clipboard", cwd=repo)
    linked = linked_worktree_path(repo, "feature/named-envs")
    assert not linked.joinpath(".env.local").exists()
    assert not linked.joinpath(".env.example").exists()
    assert not linked.joinpath(".envrc").exists()
    assert not linked.joinpath("config/.env.local").exists()


def test_similarly_named_tfvars_files_are_not_copied(run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(
        repo,
        {
            ".gitignore": "secrets.auto.tfvars.json\nconfig/secrets.auto.tfvars.tpl\n",
            "config/keep.txt": "keep\n",
        },
        "add tfvars ignore patterns",
    )
    (repo / "secrets.auto.tfvars.json").write_text('{"secret":"value"}\n', encoding="utf-8")
    (repo / "config" / "secrets.auto.tfvars.tpl").write_text('secret = "value"\n', encoding="utf-8")

    run_wtk("new", "feature/named-tfvars", "--base", "main", "--no-clipboard", cwd=repo)
    linked = linked_worktree_path(repo, "feature/named-tfvars")
    assert not linked.joinpath("secrets.auto.tfvars.json").exists()
    assert not linked.joinpath("config/secrets.auto.tfvars.tpl").exists()


def test_global_copy_config_controls_copy_patterns(
    run_wtk, repo_factory, tmp_path
) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(
        repo,
        {
            ".gitignore": ".env\n.env.local\nnotes/secret.txt\n",
            "notes/keep.txt": "keep\n",
        },
        "add copy config fixtures",
    )
    (repo / ".env").write_text("DEFAULT=value\n", encoding="utf-8")
    (repo / ".env.local").write_text("LOCAL=value\n", encoding="utf-8")
    (repo / "notes" / "secret.txt").write_text("SECRET=value\n", encoding="utf-8")

    home = tmp_path / "home"
    (home / ".wtk").mkdir(parents=True)
    (home / ".wtk" / "config.toml").write_text(
        """
copy = [
  "**/.env.local",
  "notes/secret.txt",
]
""".lstrip(),
        encoding="utf-8",
    )

    run_wtk(
        "new",
        "feature/global-config",
        "--base",
        "main",
        "--no-clipboard",
        cwd=repo,
        env={"HOME": str(home)},
    )
    linked = linked_worktree_path(repo, "feature/global-config")
    wait_until(
        "global-config copied files",
        lambda: linked.joinpath(".env.local").exists()
        and linked.joinpath("notes/secret.txt").exists(),
    )

    assert not linked.joinpath(".env").exists()
    assert linked.joinpath(".env.local").read_text(encoding="utf-8") == "LOCAL=value\n"
    assert (
        linked.joinpath("notes/secret.txt").read_text(encoding="utf-8")
        == "SECRET=value\n"
    )


def test_invalid_copy_pattern_fails_before_worktree_creation(
    run_wtk, repo_factory, tmp_path
) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(repo, {".gitignore": ".env\n"}, "ignore env")
    (repo / ".env").write_text("ROOT=value\n", encoding="utf-8")
    home = tmp_path / "home"
    write_global_copy_config(home, ["["])

    result = run_wtk(
        "new",
        "feature/bad-copy-pattern",
        "--base",
        "main",
        "--no-clipboard",
        cwd=repo,
        env={"HOME": str(home)},
        check=False,
    )
    linked = linked_worktree_path(repo, "feature/bad-copy-pattern")

    result.assert_failure()
    assert "invalid copy pattern" in result.output
    assert not linked.exists()


def test_no_runtime_defaults_skip_ignored_copy(
    run_wtk, repo_factory, tmp_path
) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(repo, {".gitignore": ".env\n"}, "ignore env only")

    (repo / ".env").write_text("ROOT=value\n", encoding="utf-8")
    home = tmp_path / "home"

    out = run_wtk("new", "feature/missing-agents", "--base", "main", "--no-clipboard", cwd=repo, env={"HOME": str(home)}).output
    linked = linked_worktree_path(repo, "feature/missing-agents")

    assert linked.exists()
    assert not linked.joinpath(".env").exists()
    assert "copied " not in out


def test_globbed_directory_copy_pattern_matches_ignored_descendants(
    run_wtk, repo_factory, tmp_path
) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(
        repo,
        {
            ".gitignore": "apps/web/.cache/\napps/api/.cache/\n",
            "apps/web/keep.txt": "web\n",
            "apps/api/keep.txt": "api\n",
        },
        "ignore globbed cache dirs",
    )
    (repo / "apps" / "web" / ".cache").mkdir()
    (repo / "apps" / "web" / ".cache" / "token.txt").write_text("WEB\n", encoding="utf-8")
    (repo / "apps" / "api" / ".cache").mkdir()
    (repo / "apps" / "api" / ".cache" / "token.txt").write_text("API\n", encoding="utf-8")
    home = tmp_path / "home"
    write_global_copy_config(home, ["apps/*/.cache/"])

    out = run_wtk("new", "feature/globbed-dir", "--base", "main", "--no-clipboard", cwd=repo, env={"HOME": str(home)}).output
    linked = linked_worktree_path(repo, "feature/globbed-dir")
    wait_until(
        "globbed directory copied",
        lambda: linked.joinpath("apps/web/.cache/token.txt").exists()
        and linked.joinpath("apps/api/.cache/token.txt").exists(),
    )

    assert linked.joinpath("apps/web/.cache/token.txt").read_text(encoding="utf-8") == "WEB\n"
    assert linked.joinpath("apps/api/.cache/token.txt").read_text(encoding="utf-8") == "API\n"
    assert "copied 2 ignored files" in out


def test_directory_copy_pattern_does_not_match_file_with_same_name(
    run_wtk, repo_factory, tmp_path
) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(repo, {".gitignore": ".agents\n"}, "ignore agents file")
    (repo / ".agents").write_text("not a dir\n", encoding="utf-8")
    home = tmp_path / "home"
    write_global_copy_config(home, [".agents/"])

    out = run_wtk("new", "feature/agents-file", "--base", "main", "--no-clipboard", cwd=repo, env={"HOME": str(home)}).output
    linked = linked_worktree_path(repo, "feature/agents-file")

    assert linked.exists()
    assert not linked.joinpath(".agents").exists()
    assert "copied " not in out


@pytest.mark.skipif(os.name == "nt", reason="requires unix symlink semantics")
def test_copy_pattern_agents_directory_copies_ignored_files_and_symlinks(
    run_wtk, repo_factory, tmp_path
) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(
        repo,
        {
            ".gitignore": ".agents/\n",
        },
        "ignore agents directory",
    )
    (repo / ".agents").mkdir()
    (repo / ".agents" / "instructions.md").write_text("LOCAL=1\n", encoding="utf-8")
    shared = tmp_path / "shared-agents.txt"
    shared.write_text("SHARED=1\n", encoding="utf-8")
    os.symlink(shared, repo / ".agents" / "shared.txt")
    home = tmp_path / "home"
    write_global_copy_config(home, [".agents/"])

    out = run_wtk("new", "feature/agents-dir", "--base", "main", "--no-clipboard", cwd=repo, env={"HOME": str(home)}).output
    linked = linked_worktree_path(repo, "feature/agents-dir")
    wait_until(
        "agents directory copied",
        lambda: linked.joinpath(".agents/instructions.md").exists()
        and linked.joinpath(".agents/shared.txt").exists(),
    )

    assert linked.joinpath(".agents/instructions.md").read_text(encoding="utf-8") == "LOCAL=1\n"
    assert linked.joinpath(".agents/shared.txt").is_symlink()
    assert linked.joinpath(".agents/shared.txt").resolve() == shared.resolve()
    assert "initializing worktree asynchronously" in out


def test_copy_pattern_agents_directory_only_copies_ignored_descendants(
    run_wtk, repo_factory, tmp_path
) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(
        repo,
        {
            ".gitignore": ".agents/local.md\n.agents/nested/\n",
            ".agents/tracked.md": "TRACKED=1\n",
            ".agents/committed.txt": "COMMITTED=1\n",
        },
        "add partially tracked agents directory",
    )
    (repo / ".agents" / "tracked.md").write_text("MODIFIED=1\n", encoding="utf-8")
    (repo / ".agents" / "local.md").write_text("LOCAL=1\n", encoding="utf-8")
    (repo / ".agents" / "nested").mkdir()
    (repo / ".agents" / "nested" / "private.txt").write_text("PRIVATE=1\n", encoding="utf-8")
    home = tmp_path / "home"
    write_global_copy_config(home, [".agents/"])

    out = run_wtk("new", "feature/agents-partial", "--base", "main", "--no-clipboard", cwd=repo, env={"HOME": str(home)}).output
    linked = linked_worktree_path(repo, "feature/agents-partial")
    wait_until(
        "partial agents directory copied",
        lambda: linked.joinpath(".agents/local.md").exists()
        and linked.joinpath(".agents/nested/private.txt").exists(),
    )

    assert linked.joinpath(".agents/local.md").read_text(encoding="utf-8") == "LOCAL=1\n"
    assert (
        linked.joinpath(".agents/nested/private.txt").read_text(encoding="utf-8")
        == "PRIVATE=1\n"
    )
    assert linked.joinpath(".agents/tracked.md").read_text(encoding="utf-8") == "TRACKED=1\n"
    assert (
        linked.joinpath(".agents/committed.txt").read_text(encoding="utf-8")
        == "COMMITTED=1\n"
    )
    assert "initializing worktree asynchronously" in out


def test_slashless_copy_pattern_matches_ignored_directory_descendants(
    run_wtk, repo_factory, tmp_path
) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(
        repo,
        {
            ".gitignore": "secrets/\n",
        },
        "ignore secrets directory",
    )
    (repo / "secrets").mkdir()
    (repo / "secrets" / "token").write_text("SECRET=1\n", encoding="utf-8")
    home = tmp_path / "home"
    write_global_copy_config(home, ["secrets"])

    out = run_wtk(
        "new",
        "feature/secrets-dir",
        "--base",
        "main",
        "--no-clipboard",
        cwd=repo,
        env={"HOME": str(home)},
    ).output
    linked = linked_worktree_path(repo, "feature/secrets-dir")
    wait_until(
        "slashless secrets directory copied",
        lambda: linked.joinpath("secrets/token").exists(),
    )

    assert linked.joinpath("secrets/token").read_text(encoding="utf-8") == "SECRET=1\n"
    assert "copied 1 ignored files" in out


def test_copy_pattern_preserves_ignored_descendants_for_slash_path_directories(
    run_wtk, repo_factory, tmp_path
) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(
        repo,
        {
            ".gitignore": "specs/change/active\n",
        },
        "ignore active spec directory",
    )
    (repo / "specs" / "change" / "active").mkdir(parents=True)
    (repo / "specs" / "change" / "active" / "plan.md").write_text(
        "ACTIVE\n", encoding="utf-8"
    )
    home = tmp_path / "home"
    write_global_copy_config(home, ["specs/change/active"])

    out = run_wtk(
        "new",
        "feature/active-spec",
        "--base",
        "main",
        "--no-clipboard",
        cwd=repo,
        env={"HOME": str(home)},
    ).output
    linked = linked_worktree_path(repo, "feature/active-spec")
    wait_until(
        "slash path directory descendants copied",
        lambda: linked.joinpath("specs/change/active/plan.md").exists(),
    )

    assert (
        linked.joinpath("specs/change/active/plan.md").read_text(encoding="utf-8")
        == "ACTIVE\n"
    )
    assert "copied 1 ignored files" in out


def test_repo_local_copy_config_is_rejected(
    run_wtk, repo_factory, tmp_path
) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(
        repo,
        {
            ".gitignore": ".env\n.env.local\nspecs/change/active\n",
        },
        "add override fixtures",
    )
    (repo / ".env").write_text("DEFAULT=value\n", encoding="utf-8")
    (repo / ".env.local").write_text("GLOBAL=value\n", encoding="utf-8")
    (repo / "specs" / "change").mkdir(parents=True)
    (repo / "specs" / "change" / "active").write_text("ACTIVE\n", encoding="utf-8")

    home = tmp_path / "home"
    (home / ".wtk").mkdir(parents=True)
    (home / ".wtk" / "config.toml").write_text(
        """
copy = [
  "**/.env.local",
  "specs/change/active",
]
""".lstrip(),
        encoding="utf-8",
    )
    (repo / ".wtk").mkdir()
    (repo / ".wtk" / "config.toml").write_text(
        """
copy = ["**/.env"]
""".lstrip(),
        encoding="utf-8",
    )

    result = run_wtk(
        "new",
        "feature/local-override",
        "--base",
        "main",
        "--no-clipboard",
        cwd=repo,
        env={"HOME": str(home)},
        check=False,
    )
    linked = linked_worktree_path(repo, "feature/local-override")
    result.assert_failure()

    assert "Copy Patterns are supported only in global ~/.wtk/config.toml" in result.output
    assert not linked.exists()


@pytest.mark.skipif(os.name == "nt", reason="requires unix symlink semantics")
def test_overlapping_copy_patterns_dedupe_symlink_snapshot(
    run_wtk, repo_factory, tmp_path
) -> None:
    repo = repo_factory.init_repo("repo")
    repo_factory.commit_files(
        repo,
        {
            ".gitignore": "apps/web/.env\n",
            "apps/web/keep.txt": "web\n",
        },
        "add duplicate snapshot fixture",
    )

    shared_env = tmp_path / "shared.env"
    shared_env.write_text("WEB=value\n", encoding="utf-8")
    os.symlink(shared_env, repo / "apps" / "web" / ".env")

    home = tmp_path / "home"
    (home / ".wtk").mkdir(parents=True)
    (home / ".wtk" / "config.toml").write_text(
        """
copy = [
  "**/.env",
  "apps/web/.env",
]
""".lstrip(),
        encoding="utf-8",
    )

    out = run_wtk(
        "new",
        "feature/deduped-symlink",
        "--base",
        "main",
        "--no-clipboard",
        cwd=repo,
        env={"HOME": str(home)},
    ).output
    linked = linked_worktree_path(repo, "feature/deduped-symlink")
    wait_until("deduped symlink copy", lambda: linked.joinpath("apps/web/.env").exists())

    assert linked.joinpath("apps/web/.env").is_symlink()
    assert linked.joinpath("apps/web/.env").resolve() == shared_env.resolve()
    assert "copied 1 ignored files" in out
    assert out.count("apps/web/.env") == 0
