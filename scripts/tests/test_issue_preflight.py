"""Offline checks for legacy issue-script preconditions; no GitHub calls."""

import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock


SCRIPTS = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "legacy_issue_generator", SCRIPTS / "generate_all_issues.py"
)
generator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generator)


class IssuePreflightTests(unittest.TestCase):
    def test_missing_source_roots_stop_before_issue_creation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "sdk/src").mkdir(parents=True)
            with mock.patch.object(generator, "PROJECT_ROOT", root):
                with mock.patch.object(generator, "create_issue") as create:
                    with self.assertRaises(SystemExit) as error:
                        generator.main()
                    self.assertIn("backend/src", str(error.exception))
                    self.assertIn("mobile/src", str(error.exception))
                    create.assert_not_called()

    def test_issue_command_runs_in_the_generator_checkout(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in generator.REQUIRED_SOURCE_ROOTS:
                (root / name).mkdir(parents=True)
            with mock.patch.object(generator, "PROJECT_ROOT", root):
                generator.require_source_layout()
                result = subprocess.CompletedProcess([], 0, stdout="", stderr="")
                with mock.patch.object(generator.subprocess, "run", return_value=result) as run:
                    with mock.patch("builtins.print"):
                        self.assertTrue(generator.create_issue("title", "body", 1, 1))
                    self.assertEqual(run.call_args.kwargs["cwd"], root)

    @unittest.skipUnless(shutil.which("bash"), "Bash is required for the wrapper check")
    def test_missing_replacement_stops_wrapper_before_any_gh_call(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / "checkout/scripts"
            scripts.mkdir(parents=True)
            wrapper = scripts / "close_and_recreate_all.sh"
            shutil.copyfile(SCRIPTS / wrapper.name, wrapper)
            binaries = root / "bin"
            binaries.mkdir()
            marker = root / "gh-called"
            fake_gh = binaries / "gh"
            fake_gh.write_text('#!/bin/sh\n: > "$GH_MARKER"\nexit 99\n')
            fake_gh.chmod(0o755)
            unrelated = root / "unrelated"
            unrelated.mkdir()
            environment = dict(os.environ, PATH=str(binaries) + os.pathsep + os.environ.get("PATH", ""), GH_MARKER=str(marker))
            result = subprocess.run(["bash", str(wrapper)], cwd=unrelated, env=environment, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(str(scripts / "create_all_70_detailed.py"), result.stderr)
            self.assertFalse(marker.exists())


if __name__ == "__main__":
    unittest.main()
