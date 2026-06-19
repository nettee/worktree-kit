from __future__ import annotations

import os
import stat

import pytest

from conftest import linked_worktree_path, wait_until
from conftest import run_git


def test_root_and_nested_ignored_env_files_copy(run_wtk, repo_factory) -> None:
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

    out = run_wtk("new", "feature/envs", "--base", "main", "--no-clipboard", cwd=repo).output
    linked = linked_worktree_path(repo, "feature/envs")
    wait_until("copied env files", lambda: linked.joinpath(".env").exists() and linked.joinpath("apps/web/.env").exists())

    assert linked.joinpath(".env").read_text(encoding="utf-8") == "ROOT=value\n"
    assert linked.joinpath("apps/web/.env").read_text(encoding="utf-8") == "WEB=value\n"
    assert linked.joinpath("services/api/.env").read_text(encoding="utf-8") == "API=value\n"
    assert "initializing worktree asynchronously" in out


def test_root_and_nested_ignored_secrets_auto_tfvars_copy(run_wtk, repo_factory) -> None:
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

    out = run_wtk("new", "feature/tfvars", "--base", "main", "--no-clipboard", cwd=repo).output
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


def test_ignored_only_dirs_non_ascii_and_tracked_env_behavior(run_wtk, repo_factory) -> None:
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

    out = run_wtk("new", "feature/mixed", "--base", "main", "--no-clipboard", cwd=repo).output
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

    run_wtk("new", "feature/root-env-symlink", "--base", "main", "--no-clipboard", cwd=repo)
    linked = linked_worktree_path(repo, "feature/root-env-symlink")
    wait_until("symlink copy", lambda: linked.joinpath(".env").exists())
    assert linked.joinpath(".env").is_symlink()
    assert linked.joinpath(".env").resolve() == shared_env.resolve()

    run_git(repo, "worktree", "remove", str(linked))
    os.unlink(repo / ".env")
    (repo / ".env").write_text("ROOT=value\n", encoding="utf-8")
    os.chmod(repo / ".env", 0o600)

    run_wtk("new", "feature/root-env-mode", "--base", "main", "--no-clipboard", cwd=repo)
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


def test_global_copy_config_controls_recursive_and_exact_files(
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
[copy]
recursive = [".env.local"]
exact = ["notes/secret.txt"]
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


def test_repo_local_copy_config_overrides_global_copy_config(
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
[copy]
recursive = [".env.local"]
exact = ["specs/change/active"]
""".lstrip(),
        encoding="utf-8",
    )
    (repo / ".wtk").mkdir()
    (repo / ".wtk" / "config.toml").write_text(
        """
[copy]
recursive = [".env"]
exact = []
""".lstrip(),
        encoding="utf-8",
    )

    run_wtk(
        "new",
        "feature/local-override",
        "--base",
        "main",
        "--no-clipboard",
        cwd=repo,
        env={"HOME": str(home)},
    )
    linked = linked_worktree_path(repo, "feature/local-override")
    wait_until("local override recursive copy", lambda: linked.joinpath(".env").exists())

    assert linked.joinpath(".env").read_text(encoding="utf-8") == "DEFAULT=value\n"
    assert not linked.joinpath(".env.local").exists()
    assert not linked.joinpath("specs/change/active").exists()
