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


if __name__ == "__main__":
    unittest.main()
