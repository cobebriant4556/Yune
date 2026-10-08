"""Binary selection must not depend on a debug-only hard-coded path."""
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from yune_binary import resolve_binary


class BinaryResolutionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="yune builds ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.name = "yune.exe" if os.name == "nt" else "yune"

    def executable(self, relative):
        path = self.root / relative / self.name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"fixture")
        path.chmod(0o755)
        return path.resolve()

    def resolve(self, **kwargs):
        kwargs.setdefault("environ", {})
        return resolve_binary(self.root, **kwargs)

    def test_release_only_is_selected(self):
        expected = self.executable("target/release")
        self.assertEqual(self.resolve(), expected)

    def test_debug_only_is_selected(self):
        expected = self.executable("target/debug")
        self.assertEqual(self.resolve(), expected)

    def test_two_builds_require_explicit_selection(self):
        debug = self.executable("target/debug")
        release = self.executable("target/release")
        with self.assertRaisesRegex(ValueError, "Both debug and release"):
            self.resolve()
        self.assertEqual(self.resolve(profile="debug"), debug)
        self.assertEqual(self.resolve(profile="release"), release)

    def test_explicit_profile_never_falls_back(self):
        self.executable("target/debug")
        with self.assertRaises(FileNotFoundError):
            self.resolve(profile="release")

    def test_explicit_binary_never_falls_back(self):
        self.executable("target/debug")
        with self.assertRaises(FileNotFoundError):
            self.resolve(binary="missing/yune")

    def test_binary_environment_is_inherited(self):
        expected = self.executable("custom build")
        self.assertEqual(self.resolve(environ={"YUNE_BIN": str(expected)}), expected)

    def test_cli_binary_overrides_environment(self):
        expected = self.executable("selected")
        ignored = self.executable("ignored")
        self.assertEqual(self.resolve(binary=str(expected), environ={"YUNE_BIN": str(ignored)}), expected)

    def test_cli_profile_overrides_inherited_binary(self):
        expected = self.executable("target/release")
        ignored = self.executable("target/debug")
        self.assertEqual(self.resolve(profile="release", environ={"YUNE_BIN": str(ignored)}), expected)

    def test_explicit_binary_ignores_unrelated_profile_environment(self):
        expected = self.executable("chosen")
        self.assertEqual(self.resolve(binary=str(expected), environ={"YUNE_PROFILE": "invalid"}), expected)

    def test_empty_explicit_binary_rejected(self):
        self.executable("target/debug")
        with self.assertRaisesRegex(ValueError, "must not be empty"):
            self.resolve(binary="")

    def test_relative_paths_use_repo_root(self):
        expected = self.executable("custom build")
        self.assertEqual(self.resolve(binary=f"custom build/{self.name}"), expected)

    def test_custom_target_directory_and_triple(self):
        expected = self.executable("build output/test-triple/release")
        self.assertEqual(self.resolve(target_dir="build output", target="test-triple", profile="release"), expected)

    def test_cargo_environment(self):
        expected = self.executable("build/test-triple/release")
        env = {"CARGO_TARGET_DIR": "build", "CARGO_BUILD_TARGET": "test-triple", "YUNE_PROFILE": "release"}
        self.assertEqual(self.resolve(environ=env), expected)

    def test_missing_binary_has_build_instructions(self):
        with self.assertRaisesRegex(FileNotFoundError, "cargo build -p yune"):
            self.resolve()

    def test_invalid_profile(self):
        with self.assertRaisesRegex(ValueError, "profile"):
            self.resolve(profile="unknown")

    def test_target_cannot_escape_target_directory(self):
        for target in ("..", "../outside", "nested/path", "nested\\path"):
            with self.subTest(target=target), self.assertRaisesRegex(ValueError, "target triple"):
                self.resolve(target=target)

    @unittest.skipIf(os.name == "nt", "Unix executable permission check")
    def test_non_executable_rejected(self):
        expected = self.executable("target/release")
        with patch("yune_binary.os.access", return_value=False):
            with self.assertRaises(PermissionError):
                self.resolve(binary=str(expected))


if __name__ == "__main__":
    unittest.main()
