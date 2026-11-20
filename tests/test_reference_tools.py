"""Security and retry controls for the private native reference provider."""

import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "reference_provider",
    Path(__file__).resolve().parents[1] / "scripts/setup-reference-tools.py",
)
PROVIDER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROVIDER)


def package(
    path: Path,  # Fresh output archive.
    files: dict[str, bytes],  # Regular archive members and their bytes.
) -> None:  # Writes only the requested temporary archive.
    """Make small data archives without constructing reference executables."""
    with tarfile.open(path, "w:gz") as output:
        for name, data in files.items():
            member = tarfile.TarInfo(name)
            member.size = len(data)
            output.addfile(member, io.BytesIO(data))


class ReferenceToolControls(unittest.TestCase):
    """Reject unsafe inputs and preserve user state across failed setup."""

    def test_wrong_runtime_observations_are_rejected(
        self: "ReferenceToolControls",  # Isolated parser rejection controls.
    ) -> None:  # No simulated output is used as positive native qualification.
        """Inject bad response text without constructing stand-in executables."""
        pins = json.loads(PROVIDER.LOCK.read_text())
        tcl = "8.6.18\n/runtime/library\n1\n"
        fossil = "This is fossil version 2.29 [7e8ecf0ce4]\n"
        cases = [
            ("8.6.17\n/runtime/library\n1\n", fossil),
            ("8.6.18\n/runtime/library\n0\n", fossil),
            (tcl, "This is fossil version 2.28 [7e8ecf0ce4]\n"),
            (tcl, "This is fossil version 2.29 [0000000000]\n"),
            (tcl, fossil + "SSL (unexpected)\n"),
            (tcl, fossil + "libfuse unexpected\n"),
        ]
        for tcl_text, fossil_text in cases:
            replies = [
                subprocess.CompletedProcess([], 0, stdout=tcl_text),
                subprocess.CompletedProcess([], 0, stdout=fossil_text),
            ]
            with self.subTest(tcl=tcl_text, fossil=fossil_text):
                with mock.patch.object(
                    PROVIDER.subprocess, "run", side_effect=replies
                ):
                    with self.assertRaises(ValueError):
                        PROVIDER.observe(
                            Path("not-executed-tcl"),
                            Path("not-executed-fossil"),
                            pins,
                        )

    def test_archive_download_and_member_limits_are_independent(
        self: "ReferenceToolControls",  # Independent input bound controls.
    ) -> None:  # Neither limit failure publishes cached or extracted data.
        """Correct digests cannot excuse oversized input or too many members."""
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            pin = {
                "archive": "source.tar.gz",
                "url": "https://invalid.example/not-requested",
                "sha256": hashlib.sha256(b"ab").hexdigest(),
            }
            with (
                mock.patch.object(PROVIDER, "MAX_ARCHIVE", 1),
                mock.patch.object(
                    PROVIDER, "urlopen", return_value=io.BytesIO(b"ab")
                ),
            ):
                with self.assertRaises(ValueError):
                    PROVIDER.archive(pin, root)
            self.assertEqual(list(root.iterdir()), [])
            source = root / "many.tar.gz"
            package(source, {f"root/{index}": b"" for index in range(10001)})
            with self.assertRaisesRegex(ValueError, "extraction bounds"):
                PROVIDER.unpack(source, root / "unpacked", "root")
            self.assertFalse((root / "unpacked").exists())

    def test_source_manifest_mismatches_stop_before_native_execution(
        self: "ReferenceToolControls",  # Source identity rejection controls.
    ) -> None:  # No native program or installation is published.
        """Archive hashes do not replace UUID and Fossil manifest checks."""
        for failure in ["uuid", "manifest-sha3"]:
            with (
                self.subTest(failure=failure),
                tempfile.TemporaryDirectory() as temp,
            ):
                root = Path(temp)
                cache = root / "cache"
                cache.mkdir()
                manifest = b"original source manifest\n"
                uuid = hashlib.sha3_256(manifest).hexdigest()
                pins = {}
                for name in ["tcl", "fossil"]:
                    identity = (
                        "wrong-uuid"
                        if failure == "uuid" and name == "tcl"
                        else uuid
                    )
                    files = {
                        f"{name}/manifest.uuid": (identity + "\n").encode()
                    }
                    if name == "fossil":
                        files["fossil/manifest"] = (
                            b"changed bytes"
                            if failure == "manifest-sha3"
                            else manifest
                        )
                    source = cache / f"{name}.tar.gz"
                    package(source, files)
                    pins[name] = {
                        "root": name,
                        "archive": source.name,
                        "uuid": uuid,
                        "sha256": PROVIDER.digest(source),
                    }
                lock = root / "pins.json"
                lock.write_text(json.dumps(pins))
                project = root / "project"
                with (
                    mock.patch.object(PROVIDER, "LOCK", lock),
                    mock.patch.object(PROVIDER.sys, "platform", "linux"),
                    mock.patch.object(PROVIDER, "run") as native,
                ):
                    with self.assertRaisesRegex(ValueError, "manifest"):
                        PROVIDER.setup(project, cache)
                    native.assert_not_called()
                self.assertEqual(list((project / ".jlog/tools").iterdir()), [])
                self.assertFalse((project / ".jlog/bin").exists())

    def test_cached_tampering_is_rejected_without_replacement(
        self: "ReferenceToolControls",  # Isolated unittest control.
    ) -> None:  # The original cached bytes survive.
        """A changed cache cannot bless itself or trigger a replacement download."""
        with tempfile.TemporaryDirectory() as temp:
            cache = Path(temp)
            original = b"changed archive bytes"
            (cache / "source.tar.gz").write_bytes(original)
            pin = {"archive": "source.tar.gz", "sha256": "0" * 64}
            with self.assertRaisesRegex(ValueError, "SHA256 mismatch"):
                PROVIDER.archive(pin, cache)
            self.assertEqual((cache / "source.tar.gz").read_bytes(), original)

    def test_bad_download_is_never_published(
        self: "ReferenceToolControls",  # Isolated unittest control.
    ) -> None:  # No cache entry is created for mismatched bytes.
        """A digest failure stops publication before extraction or compilation."""
        with tempfile.TemporaryDirectory() as temp:
            pin = {
                "archive": "source.tar.gz",
                "sha256": "0" * 64,
                "url": "https://invalid.example/not-requested",
            }
            with mock.patch.object(
                PROVIDER, "urlopen", return_value=io.BytesIO(b"wrong")
            ):
                with self.assertRaisesRegex(ValueError, "SHA256 mismatch"):
                    PROVIDER.archive(pin, Path(temp))
            self.assertEqual(list(Path(temp).iterdir()), [])

    def test_traversal_and_links_fail_before_any_extraction(
        self: "ReferenceToolControls",  # Isolated unittest control.
    ) -> None:  # Neither an earlier valid entry nor an escape is written.
        """Validate the complete inventory before publishing archive members."""
        cases = [
            ("/outside", tarfile.REGTYPE),
            ("root/../../outside", tarfile.REGTYPE),
            ("other/file", tarfile.REGTYPE),
            ("root/link", tarfile.SYMTYPE),
            ("root/link", tarfile.LNKTYPE),
        ]
        for name, kind in cases:
            with (
                self.subTest(name=name, kind=kind),
                tempfile.TemporaryDirectory() as temp,
            ):
                root = Path(temp)
                source = root / "input.tar.gz"
                destination = root / "unpacked"
                destination.mkdir()
                with tarfile.open(source, "w:gz") as output:
                    good = tarfile.TarInfo("root/good")
                    good.size = 1
                    output.addfile(good, io.BytesIO(b"x"))
                    bad = tarfile.TarInfo(name)
                    bad.type = kind
                    bad.linkname = "../../outside"
                    output.addfile(bad)
                with self.assertRaises(ValueError):
                    PROVIDER.unpack(source, destination, "root")
                self.assertEqual(list(destination.iterdir()), [])
                self.assertFalse((root / "outside").exists())

    def test_regular_source_is_complete_and_bounded(
        self: "ReferenceToolControls",  # Isolated unittest control.
    ) -> None:  # Valid source survives, while oversize source writes nothing.
        """Exercise successful extraction and the independent unpacked-byte limit."""
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / "input.tar.gz"
            package(source, {"root/data": b"abc"})
            destination = root / "valid"
            result = PROVIDER.unpack(source, destination, "root")
            self.assertEqual((result / "data").read_bytes(), b"abc")
            with mock.patch.object(PROVIDER, "MAX_SOURCE", 2):
                with self.assertRaisesRegex(ValueError, "extraction bounds"):
                    PROVIDER.unpack(source, root / "too-large", "root")
            self.assertFalse((root / "too-large").exists())

    def test_foreign_paths_are_kept_without_download_or_execution(
        self: "ReferenceToolControls",  # Isolated unittest control.
    ) -> None:  # Foreign directories and file bytes remain unchanged.
        """Refuse links outside the checkout and unowned existing tool files."""
        for relative in [
            ".jlog",
            ".jlog/tools",
            ".jlog/cache",
            ".jlog/cache/reference-tools",
        ]:
            with (
                self.subTest(relative=relative),
                tempfile.TemporaryDirectory() as temp,
            ):
                root = Path(temp)
                project = root / "project"
                project.mkdir()
                foreign = root / "foreign"
                foreign.mkdir()
                link = project / relative
                link.parent.mkdir(parents=True, exist_ok=True)
                link.symlink_to(foreign, target_is_directory=True)
                with mock.patch.object(PROVIDER.sys, "platform", "linux"):
                    with self.assertRaisesRegex(
                        ValueError, "shared reference"
                    ):
                        PROVIDER.setup(project, None)
                self.assertTrue(link.is_symlink())
                self.assertEqual(list(foreign.iterdir()), [])
        with tempfile.TemporaryDirectory() as temp:
            project = Path(temp)
            foreign = project / ".jlog/bin/fossil"
            foreign.parent.mkdir(parents=True)
            foreign.write_bytes(b"user-owned data, never executed")
            with mock.patch.object(PROVIDER.sys, "platform", "linux"):
                with self.assertRaisesRegex(ValueError, "replace existing"):
                    PROVIDER.setup(project, None)
            self.assertEqual(
                foreign.read_bytes(), b"user-owned data, never executed"
            )
            self.assertFalse((project / ".jlog/cache").exists())

    def test_failed_native_stage_can_be_retried(
        self: "ReferenceToolControls",  # Isolated unittest control.
    ) -> None:  # Failure leaves no published prefix, receipt or tool links.
        """Missing configure fails twice at the same stage without poisoning setup."""
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            cache = root / "cache"
            cache.mkdir()
            manifest = b"a test source manifest\n"
            uuid = hashlib.sha3_256(manifest).hexdigest()
            pins = {}
            for name in ["tcl", "fossil"]:
                files = {f"{name}/manifest.uuid": (uuid + "\n").encode()}
                if name == "fossil":
                    files["fossil/manifest"] = manifest
                else:
                    files["tcl/unix/data"] = b"no configure executable"
                source = cache / f"{name}.tar.gz"
                package(source, files)
                pins[name] = {
                    "root": name,
                    "archive": source.name,
                    "uuid": uuid,
                    "sha256": PROVIDER.digest(source),
                }
            lock = root / "pins.json"
            lock.write_text(json.dumps(pins))
            project = root / "project"
            with (
                mock.patch.object(PROVIDER, "LOCK", lock),
                mock.patch.object(PROVIDER.sys, "platform", "linux"),
            ):
                for _ in range(2):
                    with self.assertRaises(FileNotFoundError):
                        PROVIDER.setup(project, cache)
                    self.assertEqual(
                        list((project / ".jlog/tools").iterdir()), []
                    )
                    self.assertFalse((project / ".jlog/bin").exists())


if __name__ == "__main__":
    unittest.main()
