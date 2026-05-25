import unittest
from unittest import mock

from scripts import release


class ResolveTargetVersionTests(unittest.TestCase):
    def test_accepts_explicit_version(self) -> None:
        self.assertEqual(release.resolve_target_version("1.2.3"), release.Version(1, 2, 3))

    def test_bumps_major_from_cargo_version(self) -> None:
        with mock.patch("scripts.release.read_cargo_version", return_value=release.Version(1, 2, 3)):
            self.assertEqual(release.resolve_target_version("major"), release.Version(2, 0, 0))

    def test_bumps_minor_from_cargo_version(self) -> None:
        with mock.patch("scripts.release.read_cargo_version", return_value=release.Version(1, 2, 3)):
            self.assertEqual(release.resolve_target_version("minor"), release.Version(1, 3, 0))

    def test_bumps_patch_from_cargo_version(self) -> None:
        with mock.patch("scripts.release.read_cargo_version", return_value=release.Version(1, 2, 3)):
            self.assertEqual(release.resolve_target_version("patch"), release.Version(1, 2, 4))


class PrepareReleaseTests(unittest.TestCase):
    def test_resolves_semantic_bump_after_syncing_base_branch(self) -> None:
        events: list[str] = []

        def run_side_effect(args: list[str], *, capture: bool = False) -> str:
            if args == ["git", "pull", "--ff-only", "origin", "main"]:
                events.append("pull")
            return ""

        def resolve_side_effect(value: str) -> release.Version:
            events.append(f"resolve:{value}")
            return release.Version(1, 2, 4)

        with (
            mock.patch("scripts.release.require_command"),
            mock.patch("scripts.release.ensure_clean_worktree"),
            mock.patch("scripts.release.ensure_base_branch"),
            mock.patch("scripts.release.run", side_effect=run_side_effect),
            mock.patch("scripts.release.resolve_target_version", side_effect=resolve_side_effect) as resolve_target_version,
            mock.patch("scripts.release.ensure_version_increases") as ensure_version_increases,
            mock.patch("scripts.release.ensure_tag_absent"),
            mock.patch("scripts.release.ensure_branch_absent"),
            mock.patch("scripts.release.update_version_files"),
            mock.patch("scripts.release.ensure_changes_exist"),
            mock.patch("scripts.release.ensure_release_label"),
        ):
            release.prepare_release("patch", base="main", remote="origin", skip_tests=True)

        resolve_target_version.assert_called_once_with("patch")
        ensure_version_increases.assert_called_once_with(release.Version(1, 2, 4))
        self.assertEqual(events, ["pull", "resolve:patch"])


if __name__ == "__main__":
    unittest.main()
