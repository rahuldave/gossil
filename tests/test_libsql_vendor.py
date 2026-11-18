"""Adversarial controls for the project-owned native-source integrity gate."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    """Load project maintenance code without changing the public Rust API."""
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


VERIFY = load("vendor_verify", ROOT / "scripts/verify-libsql-vendor.py")
REGENERATE = load("vendor_regenerate", ROOT / "scripts/regenerate-libsql.py")


class DisposableGitTest(unittest.TestCase):
    """Keep synthetic repositories separate from the invoking Git hook."""

    def setUp(self):
        """Remove Git repository selectors before any disposable subprocess."""
        # git rev-parse documents this list for commands run in another repository.
        names = subprocess.check_output(
            ["git", "rev-parse", "--local-env-vars"], text=True
        ).splitlines()
        environment = {key: value for key, value in os.environ.items() if key not in names}
        isolated = patch.dict(os.environ, environment, clear=True)
        isolated.start()
        self.addCleanup(isolated.stop)


class VendorIntegrity(DisposableGitTest):
    """Mutate disposable copies; never bless changed product or staged files."""

    def setUp(self):
        """Copy the complete reviewed tree and its frozen control artifacts."""
        DisposableGitTest.setUp(self)
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        shutil.copytree(ROOT / VERIFY.VENDOR, self.root / VERIFY.VENDOR)
        manifest = json.loads((ROOT / VERIFY.MANIFEST).read_text())
        for path in [VERIFY.MANIFEST, *manifest["artifacts"]]:
            target = self.root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / path, target)

    def tearDown(self):
        """Release all test-owned repositories and copies."""
        self.temp.cleanup()

    def reject(self, indexed=False):
        """Demand a failure rather than only inspecting a diagnostic string."""
        with self.assertRaises((ValueError, OSError, KeyError, TypeError)):
            VERIFY.validate(self.root, indexed)

    def git(self, *args):
        """Create and stage only a disposable synthetic test repository."""
        subprocess.run(["git", *args], cwd=self.root, check=True, capture_output=True)

    def test_complete_copy_passes(self):
        """The exact complete tree is accepted before any tampering."""
        self.assertEqual(VERIFY.validate(self.root), 272)

    def test_missing_extra_changed_and_symlink_are_rejected(self):
        """Detect each independent inventory and byte mutation."""
        path = self.root / VERIFY.VENDOR / "Cargo.toml"
        original = path.read_bytes()
        path.unlink()
        self.reject()
        path.write_bytes(original)
        extra = self.root / VERIFY.VENDOR / "unexpected"
        extra.write_bytes(b"extra")
        self.reject()
        extra.unlink()
        path.write_bytes(original + b"\n")
        self.reject()
        path.unlink()
        path.symlink_to(ROOT / VERIFY.VENDOR / "Cargo.toml")
        self.reject()

    def test_symlinked_directory_is_rejected(self):
        """Reject an external subtree even if its byte inventory matches."""
        directory = self.root / VERIFY.VENDOR / "src"
        shutil.rmtree(directory)
        directory.symlink_to(ROOT / VERIFY.VENDOR / "src", target_is_directory=True)
        self.reject()

    def test_manifest_missing_corrupt_and_escaping_paths_are_rejected(self):
        """Fail closed on missing metadata and path traversal."""
        path = self.root / VERIFY.MANIFEST
        original = path.read_bytes()
        path.unlink()
        self.reject()
        path.write_bytes(b"{")
        self.reject()
        path.write_bytes(b'{"schema":1,"schema":1}')
        self.reject()
        manifest = json.loads(original)
        manifest["files"]["../escape"] = next(iter(manifest["files"].values()))
        path.write_text(json.dumps(manifest))
        self.reject()

    def test_wrong_provenance_and_artifact_are_rejected(self):
        """A complete source set cannot excuse changed source attribution."""
        path = self.root / VERIFY.MANIFEST
        manifest = json.loads(path.read_text())
        manifest["identity"]["native_source_sha256"] = "0" * 64
        path.write_text(json.dumps(manifest))
        self.reject()
        shutil.copyfile(ROOT / VERIFY.MANIFEST, path)
        (self.root / "evidence/libsql-provider/provenance.json").write_text("{}")
        self.reject()

    def test_index_and_working_bytes_must_agree(self):
        """Reject unstaged edits, stale staged manifest/source and indexed symlinks."""
        self.git("init", "-q")
        self.git("add", ".")
        self.assertEqual(VERIFY.validate(self.root, True), 272)
        path = self.root / VERIFY.VENDOR / "Cargo.toml"
        original = path.read_bytes()
        path.write_bytes(original + b"\n")
        self.reject(True)
        self.git("add", str(path.relative_to(self.root)))
        path.write_bytes(original)
        self.reject(True)
        self.git("add", str(path.relative_to(self.root)))
        manifest_path = self.root / VERIFY.MANIFEST
        original_manifest = manifest_path.read_bytes()
        manifest_path.write_bytes(original_manifest + b" ")
        self.git("add", VERIFY.MANIFEST)
        manifest_path.write_bytes(original_manifest)
        self.reject(True)
        self.git("add", VERIFY.MANIFEST)
        path.unlink()
        path.symlink_to(ROOT / VERIFY.VENDOR / "Cargo.toml")
        self.git("add", str(path.relative_to(self.root)))
        self.reject(True)

    def test_staged_extra_or_missing_source_is_rejected(self):
        """Validate the staged path set independently of the current tree."""
        self.git("init", "-q")
        self.git("add", ".")
        self.git("rm", "--cached", VERIFY.VENDOR + "/Cargo.toml")
        self.reject(True)
        self.git("add", ".")
        extra = self.root / VERIFY.VENDOR / "extra"
        extra.write_bytes(b"extra")
        self.git("add", ".")
        extra.unlink()
        self.reject(True)

    def test_wrong_regeneration_input_and_patch_counts_fail(self):
        """Maintenance refuses wrong inputs and incomplete substitutions."""
        bad = self.root / "wrong.c"
        bad.write_bytes(b"not the pinned source")
        output = self.root / "output.c"
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            REGENERATE.regenerate(bad, bad, bad, output)
        self.assertFalse(output.exists())
        malformed = b"--- a/bundled/src/sqlite3.c\n+++ b/bundled/src/sqlite3.c\n@@ -1,2 +1,1 @@\n-a\n+b\n"
        with self.assertRaisesRegex(ValueError, "count mismatch"):
            REGENERATE.apply(b"a\n", malformed)
        with self.assertRaisesRegex(ValueError, "hunk count"):
            REGENERATE.apply(b"a\n", b"--- a/bundled/src/sqlite3.c\n+++ b/bundled/src/sqlite3.c\n")


class LiteralWhitespacePolicy(DisposableGitTest):
    """Preserve frozen source bytes while rejecting ordinary authored whitespace."""

    def test_literal_paths_pass_and_ordinary_paths_fail(self):
        """Run the real staged diff checker under the tracked attribute policy."""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True, capture_output=True)
            shutil.copyfile(ROOT / ".gitattributes", root / ".gitattributes")
            literals = [
                "vendor/libsql-ffi/bundled/fixture.c",
                "evidence/libsql-provider/adapted.patch",
                "evidence/libsql-provider/fossil-configure.stdout",
                "evidence/libsql-provider/product-red.stdout",
                "docs/_extensions/atelier/fonts/dmsans-OFL.txt",
                "docs/_extensions/atelier/fonts/ibmplexmono-OFL.txt",
            ]
            for name in literals:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"literal captured bytes \t\n\n")
            subprocess.run(["git", "add", "."], cwd=root, check=True, capture_output=True)
            checked = subprocess.run(["git", "diff", "--cached", "--check"], cwd=root, capture_output=True)
            self.assertEqual(checked.returncode, 0, checked.stdout)
            ordinary = [
                "ordinary.rs", "docs/ordinary.md", "scripts/ordinary.py", "config.toml",
                "vendor/libsql-ffi-extra/file.c",
                "evidence/libsql-provider/product-red.stdout.more",
                "docs/_extensions/atelier/fonts/dmsans-OFL.txt.more",
                "docs/_extensions/atelier/fonts/ibmplexmono-OFL.txt.more",
                "docs/_extensions/atelier/fonts/ordinary.txt",
                "docs/_extensions/atelier/fonts-extra/dmsans-OFL.txt",
            ]
            for name in ordinary:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"authored trailing whitespace \t\n")
            subprocess.run(["git", "add", "."], cwd=root, check=True, capture_output=True)
            checked = subprocess.run(["git", "diff", "--cached", "--check"], cwd=root, capture_output=True)
            self.assertEqual(checked.returncode, 2)
            for name in ordinary:
                self.assertIn((name + ":1: trailing whitespace.").encode(), checked.stdout)
            for name in literals:
                self.assertNotIn((name + ":1:").encode(), checked.stdout)


class PreservedReaderPolicy(DisposableGitTest):
    """Exercise the installed page-map contract with disposable complete sources."""

    setUp = VendorIntegrity.setUp
    tearDown = VendorIntegrity.tearDown

    def policy(self):
        """Load the installed gate against the disposable project root."""
        return load("reader_gate", ROOT / ".agents/skills/nossaifd/scripts/prose_gate.py").Checkers(self.root)

    def test_exact_pages_use_source_checks_and_ordinary_prose_keeps_style(self):
        """All three exact pages pass through the map, including verbose mode."""
        gate = self.policy()
        pages = [
            "vendor/libsql-ffi/bundled/SQLite3MultipleCiphers/readme.md",
            "vendor/libsql-ffi/bundled/SQLite3MultipleCiphers/src/filelist.md",
            "vendor/libsql-ffi/bundled/sqlean/regexp/pcre2/README.md",
        ]
        for page in pages:
            self.assertEqual(gate.name(page), "scripts/check-preserved-libsql-prose.py")
            problems, output = gate.problems(page, True)
            self.assertEqual(problems, [], output)
            self.assertIn("preserved upstream prose", output)
        ordinary = self.root / "ordinary.md"
        ordinary.write_text("It wasn't ready.\n")
        self.assertTrue(gate.problems("ordinary.md")[0])
        new = self.root / "vendor/libsql-ffi/new.md"
        new.write_text("It wasn't ready.\n")
        self.assertEqual(gate.name("vendor/libsql-ffi/new.md"), "check.py of the skill")
        self.assertTrue(gate.problems("vendor/libsql-ffi/new.md")[0])

    def test_source_and_map_mutations_fail_closed(self):
        """Missing, changed and linked pages or broadened policy cannot pass."""
        page = "vendor/libsql-ffi/bundled/SQLite3MultipleCiphers/readme.md"
        path = self.root / page
        original = path.read_bytes()
        for mutation in ("missing", "changed", "symlink"):
            path.unlink()
            if mutation == "changed":
                path.write_bytes(original + b"\n")
            elif mutation == "symlink":
                path.symlink_to(ROOT / page)
            self.assertTrue(self.policy().problems(page)[0], mutation)
            path.unlink(missing_ok=True)
            path.write_bytes(original)
        mapping = self.root / ".agents/nossaifd-checkers.json"
        original_map = mapping.read_bytes()
        mapping.unlink()
        with self.assertRaises(OSError):
            VERIFY.validate(self.root)
        mapping.write_bytes(original_map)
        for value in (b"{", original_map.replace(b"readme.md", b"*.md")):
            mapping.write_bytes(value)
            self.assertTrue(self.policy().problems(page)[0])
            with self.assertRaises(ValueError):
                VERIFY.validate(self.root)
        mapping.write_bytes(original_map)
        checker = self.root / "scripts/check-preserved-libsql-prose.py"
        checker.unlink()
        self.assertTrue(self.policy().problems(page)[0])
        checker.write_text("raise RuntimeError('broken checker')\n")
        self.assertTrue(self.policy().problems(page)[0])

    def test_adapter_rejects_unlisted_page_and_changed_archive_hash(self):
        """Direct use cannot authorize a new path or a rewritten archive pin."""
        import sys
        checker = self.root / "scripts/check-preserved-libsql-prose.py"
        for page in ("ordinary.md", "vendor/libsql-ffi/new.md", "../README.md"):
            result = subprocess.run([sys.executable, str(checker), page], cwd=self.root, capture_output=True)
            self.assertEqual(result.returncode, 1)
            self.assertIn(b"HARD", result.stdout)
        manifest = self.root / VERIFY.MANIFEST
        value = json.loads(manifest.read_text())
        missing = dict(value)
        missing["artifacts"] = dict(value["artifacts"])
        del missing["artifacts"]["scripts/check-preserved-libsql-prose.py"]
        manifest.write_text(json.dumps(missing))
        with self.assertRaisesRegex(ValueError, "missing provenance artifacts"):
            VERIFY.validate(self.root)
        value["files"]["bundled/SQLite3MultipleCiphers/readme.md"]["sha256"] = "0" * 64
        manifest.write_text(json.dumps(value))
        self.assertTrue(self.policy().problems("vendor/libsql-ffi/bundled/SQLite3MultipleCiphers/readme.md")[0])


class HookIsolation(DisposableGitTest):
    """Run the real controls from a worktree pre-push hook."""

    def test_hook_preserves_calling_repository(self):
        """A successful hook leaves the caller's config, index and files intact."""
        import sys
        import shlex

        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            source, remote, worktree = (base / name for name in ("source", "remote", "worktree"))

            def git(root, *args):
                return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True)

            source.mkdir()
            git(source, "init", "-q")
            git(source, "config", "user.name", "Hook isolation control")
            git(source, "config", "user.email", "control@example.invalid")
            (source / "sentinel").write_bytes(b"caller bytes\n")
            git(source, "add", ".")
            git(source, "-c", "core.hooksPath=/dev/null", "commit", "-qm", "control")
            git(source, "init", "--bare", "-q", str(remote))
            git(source, "worktree", "add", "-qb", "hook-control", str(worktree))
            git(worktree, "remote", "add", "origin", str(remote))
            hook = source / ".git/hooks/pre-push"
            # Git itself supplies GIT_DIR for this real push. Explicitly export
            # the calling index too, as hook managers can do.
            hook.write_text(
                "#!/bin/sh\nexport GIT_INDEX_FILE=\"$(git rev-parse --path-format=absolute --git-path index)\"\nexec "
                + shlex.join([sys.executable, str(Path(__file__).resolve()),
                              "VendorIntegrity", "LiteralWhitespacePolicy", "PreservedReaderPolicy"])
                + "\n"
            )
            hook.chmod(0o755)
            index = Path(git(worktree, "rev-parse", "--path-format=absolute", "--git-path", "index").stdout.decode().strip())
            config = source / ".git/config"
            before = (index.read_bytes(), config.read_bytes(), (worktree / "sentinel").read_bytes(),
                      git(worktree, "status", "--porcelain").stdout,
                      git(worktree, "ls-files", "--stage", "-z").stdout)
            pushed = git(worktree, "push", "origin", "HEAD:refs/heads/hook-control")
            self.assertIn(b"Ran 12 tests", pushed.stderr)
            self.assertIn(b"OK", pushed.stderr)
            after = (index.read_bytes(), config.read_bytes(), (worktree / "sentinel").read_bytes(),
                     git(worktree, "status", "--porcelain").stdout,
                     git(worktree, "ls-files", "--stage", "-z").stdout)
            self.assertEqual(before, after)
            self.assertEqual(git(worktree, "rev-parse", "--is-bare-repository").stdout.strip(), b"false")


if __name__ == "__main__":
    unittest.main()
