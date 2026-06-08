from __future__ import annotations


def assert_usage_error(run_wtk, repo, args: tuple[str, ...], reason: str, usage: str) -> None:
    result = run_wtk(*args, cwd=repo, check=False)
    result.assert_failure()
    assert result.stdout == ""
    assert reason in result.stderr
    assert "Usage:" in result.stderr
    assert usage in result.stderr
    assert "Flags:" in result.stderr


def test_cli_usage_and_flag_errors(run_wtk, repo_factory) -> None:
    repo = repo_factory.init_repo("repo")
    cases = [
        (("new",), "missing required argument: branch", "wtk new <branch> [flags]"),
        (("new", "feature/a", "feature/b"), "too many arguments: expected 1 branch", "wtk new <branch> [flags]"),
        (("create",), "missing required argument: branch", "wtk create <branch> [flags]"),
        (("create", "feature/a", "feature/b"), "too many arguments: expected 1 branch", "wtk create <branch> [flags]"),
        (("checkout",), "missing required argument: branch", "wtk checkout <branch> [flags]"),
        (("checkout", "feature/a", "feature/b"), "too many arguments: expected 1 branch", "wtk checkout <branch> [flags]"),
        (("remove", "one", "two"), "too many arguments: expected at most 1 path", "wtk remove [path] [flags]"),
        (("send-out", "extra"), "unexpected argument: extra", "wtk send-out [flags]"),
        (("bring-in",), "missing required argument: branch", "wtk bring-in <branch> [flags]"),
        (("completion", "tcsh"), "unsupported shell: tcsh", "wtk completion <bash|zsh|fish|powershell> [flags]"),
        (("new", "--wat"), "unknown flag: --wat", "wtk new <branch> [flags]"),
        (("create", "--wat"), "unknown flag: --wat", "wtk create <branch> [flags]"),
        (("checkout", "--wat"), "unknown flag: --wat", "wtk checkout <branch> [flags]"),
        (("upgrade", "--wat"), "unknown flag: --wat", "wtk upgrade [flags]"),
    ]
    for args, reason, usage in cases:
        assert_usage_error(run_wtk, repo, args, reason, usage)
