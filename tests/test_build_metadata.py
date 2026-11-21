"""Cached builds must track real Git revisions exported by the shipping script."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
import uuid


PROJECT = Path(__file__).resolve().parents[1]
# This live session leader reserves the owned group until the real command and
# every inherited stdout/stderr pipe holder finishes. It forwards real output.
SUPERVISOR = """import subprocess, sys
result = subprocess.run(sys.argv[1:], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
sys.stdout.buffer.write(result.stdout)
sys.stderr.buffer.write(result.stderr)
raise SystemExit(result.returncode)
"""


class BuildMetadata(unittest.TestCase):
    """Compile the exact shipping script in isolated dependency-free Git fixtures."""

    def setUp(self):
        """Keep fixture identity unique while inheriting the guard's Cargo environment."""
        self.temporary = tempfile.TemporaryDirectory(prefix="gossil-build-metadata-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.crate = self.root / "crate"
        (self.crate / "src").mkdir(parents=True)
        self.package = "gossil-build-metadata-" + uuid.uuid4().hex[:12]
        self.environment = {key: value for key, value in os.environ.items()
                            if not key.startswith("GIT_")}
        self.environment.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1")
        self.cargo = shutil.which("cargo")
        self.git = shutil.which("git")
        self.assertIsNotNone(self.cargo)
        self.assertIsNotNone(self.git)
        (self.crate / "Cargo.toml").write_text(
            f'[package]\nname = "{self.package}"\nversion = "0.0.0"\n'
            'edition = "2024"\npublish = false\n', encoding="utf-8")
        (self.crate / "src/main.rs").write_text(
            'fn main() { println!("{}\\n{}", env!("GIT_SHA"), env!("BUILD_DATE")); }\n',
            encoding="utf-8")
        shutil.copyfile(PROJECT / "build.rs", self.crate / "build.rs")
        self.assertEqual((self.crate / "build.rs").read_bytes(), (PROJECT / "build.rs").read_bytes())
        # Create the actual zero-dependency lock before any cached-build comparison.
        self._cargo("generate-lockfile", "--offline")
        self.script = None

    def _run(self, arguments, *, environment=None, timeout=60):
        """Bound one owned subprocess group without touching another fixture or cache."""
        process = subprocess.Popen([sys.executable, "-c", SUPERVISOR, *arguments], cwd=self.crate,
                                   env=self.environment if environment is None else environment,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   text=True, start_new_session=True)
        try:
            stdout, stderr = process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            # A live, unreaped direct child reserves this newly created group identity.
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
            process.communicate(timeout=5)
            raise
        self.assertEqual(process.returncode, 0, f"{arguments!r}\n{stdout}\n{stderr}")
        return stdout

    def _cargo(self, *arguments):
        """Use the inherited target/profile/Cargo settings, never a competing target."""
        return self._run([self.cargo, *arguments, "--manifest-path", str(self.crate / "Cargo.toml")])

    def _git(self, *arguments):
        """Keep actor configuration local to each real fixture command."""
        return self._run([self.git, "-c", "user.name=Build Metadata Fixture",
                          "-c", "user.email=build-metadata-fixture@example.invalid",
                          *arguments], timeout=15).strip()

    def _commit(self, message):
        """Separate reference mtimes on filesystems with coarse timestamp precision."""
        time.sleep(1.05)
        self._git("commit", "--no-gpg-sign", "--allow-empty", "-m", message)
        return self._git("rev-parse", "HEAD")

    def _build(self):
        """Read metadata from the actual compiled fixture binary and build-script artifact."""
        date_before = self._run(["date", "+%Y-%m-%d"], timeout=10).strip()
        output = self._cargo("build", "--offline", "--locked", "--message-format=json")
        binary = None
        for line in output.splitlines():
            row = json.loads(line)
            if row.get("reason") != "compiler-artifact":
                continue
            if "bin" in row["target"]["kind"]:
                binary = row.get("executable")
            if "custom-build" in row["target"]["kind"]:
                for name in row["filenames"]:
                    candidate = Path(name)
                    if candidate.is_file() and os.access(candidate, os.X_OK):
                        self.script = candidate
        self.assertIsNotNone(binary)
        self.assertIsNotNone(self.script)
        self.assertTrue(self.script.is_file())
        metadata = self._run([binary], timeout=10).splitlines()
        self.assertEqual(len(metadata), 2)
        date_after = self._run(["date", "+%Y-%m-%d"], timeout=10).strip()
        self.assertIn(metadata[1], {date_before, date_after})
        return metadata[0]

    def _source_state(self):
        """Prove cached-build controls change Git metadata rather than package source."""
        return {name: ((self.crate / name).stat().st_mtime_ns,
                       hashlib.sha256((self.crate / name).read_bytes()).hexdigest())
                for name in ["Cargo.toml", "Cargo.lock", "build.rs", "src/main.rs"]}

    def _initialize_git(self):
        """Create only a private repository, with no user templates or hooks."""
        template = self.root / "empty-template"
        template.mkdir()
        self._git("init", "--initial-branch=metadata", f"--template={template}")
        self._git("add", "Cargo.toml", "Cargo.lock", "build.rs", "src/main.rs")
        return self._commit("first source")

    def _assert_cached_transitions(self, first, branch):
        """Exercise the same unchanged-source contract in ordinary and linked worktrees."""
        self.assertEqual(self._build(), self._git("rev-parse", "--short", "HEAD"))
        unchanged = self._source_state()
        second = self._commit("empty attached revision")
        self.assertEqual(self._source_state(), unchanged)
        self.assertEqual(self._build(), self._git("rev-parse", "--short", "HEAD"))
        time.sleep(1.05)
        self._git("switch", "-c", "alternate-" + branch, first)
        self.assertEqual(self._source_state(), unchanged)
        self.assertEqual(self._build(), self._git("rev-parse", "--short", "HEAD"))
        time.sleep(1.05)
        self._git("switch", "--detach", second)
        self.assertEqual(self._build(), self._git("rev-parse", "--short", "HEAD"))
        self._commit("empty detached revision")
        self.assertEqual(self._build(), self._git("rev-parse", "--short", "HEAD"))
        time.sleep(1.05)
        self._git("switch", branch)
        self._git("pack-refs", "--all")
        loose = Path(self._git("rev-parse", "--git-path", "refs/heads/" + branch))
        if not loose.is_absolute():
            loose = self.crate / loose
        self.assertFalse(loose.exists())
        self.assertEqual(self._build(), self._git("rev-parse", "--short", "HEAD"))
        self._commit("packed branch becomes loose")
        self.assertTrue(loose.is_file())
        self.assertEqual(self._source_state(), unchanged)
        self.assertEqual(self._build(), self._git("rev-parse", "--short", "HEAD"))

    def test_cached_git_transitions_update_the_compiled_revision(self):
        """Track actual attached, switched, detached and packed-to-loose Git refs."""
        first = self._initialize_git()
        self.assertTrue((self.crate / ".git").is_dir())
        self._assert_cached_transitions(first, "metadata")

    def test_linked_worktree_uses_its_head_and_common_branch_refs(self):
        """A real linked worktree must refresh cached metadata at every Git transition."""
        first = self._initialize_git()
        primary = self.crate
        linked = self.root / "linked"
        self._git("worktree", "add", "-b", "metadata-linked", str(linked), first)
        self.crate = linked
        self.assertTrue((linked / ".git").is_file())
        head = Path(self._git("rev-parse", "--git-path", "HEAD"))
        self.assertNotEqual(head.resolve(), (linked / ".git/HEAD").resolve())
        self._assert_cached_transitions(first, "metadata-linked")
        actual_primary = self._run([self.git, "-C", str(primary), "rev-parse", "HEAD"], timeout=15).strip()
        self.assertEqual(actual_primary, first)

    def test_timeout_stops_a_descendant_after_the_real_command_exits(self):
        """The reserved supervisor group remains owned while an orphan holds output pipes."""
        receipt = self.root / "descendant.pid"
        program = ("import pathlib, subprocess, sys; "
                   "child=subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)']); "
                   "pathlib.Path(sys.argv[1]).write_text(str(child.pid))")
        with self.assertRaises(subprocess.TimeoutExpired):
            self._run([sys.executable, "-c", program, str(receipt)], timeout=0.5)
        child = int(receipt.read_text())
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            status = subprocess.run(["ps", "-p", str(child), "-o", "stat="],
                                    capture_output=True, text=True, timeout=5).stdout.strip()
            if not status:
                break
            time.sleep(0.05)
        self.assertEqual(status, "", "owned descendant survived timeout cleanup")

    def test_outside_git_has_unknown_revision_and_a_real_build_date(self):
        """Source archives outside Git preserve the explicit unknown revision fallback."""
        self.assertFalse((self.crate / ".git").exists())
        self.assertEqual(self._build(), "unknown")

    def test_missing_commands_keep_unknown_metadata(self):
        """Run the real compiled script with an empty PATH, without substitute programs."""
        self._build()
        empty_path = self.root / "empty-path"
        empty_path.mkdir()
        environment = dict(self.environment, PATH=str(empty_path))
        output = self._run([str(self.script)], environment=environment, timeout=10)
        self.assertIn("cargo:rustc-env=GIT_SHA=unknown\n", output)
        self.assertIn("cargo:rustc-env=BUILD_DATE=unknown\n", output)


if __name__ == "__main__":
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(BuildMetadata)
    expected = 5
    if suite.countTestCases() != expected:
        raise SystemExit(f"expected {expected} metadata controls, found {suite.countTestCases()}")
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if result.testsRun != expected or result.skipped or not result.wasSuccessful():
        raise SystemExit(1)
