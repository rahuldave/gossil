"""Drift controls for the immutable Git wrapper and unchanged engine composition."""
import importlib.util
import gzip
from pathlib import Path
import shutil
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('wrapper_pin', ROOT / 'scripts/verify-libsql-wrapper.py')
VERIFIER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFIER)


class WrapperPinTests(unittest.TestCase):
    """The wrapper correction cannot silently change the selected engine or source."""

    def setUp(self):
        """Copy only the static composition inputs, without build or vendor source."""
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        for name in ['Cargo.toml', 'Cargo.lock', 'vendor/libsql-ffi.manifest.json',
                     'evidence/libsql-wrapper/provenance.json', 'evidence/libsql-wrapper/companion-correction.patch.gz']:
            destination = self.root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, destination)
        VERIFIER.ROOT = self.root

    def tearDown(self):
        """Restore module state and remove only fixture-owned composition files."""
        VERIFIER.ROOT = ROOT
        self.temp.cleanup()

    def test_exact_composition(self):
        """The reviewed Git revision and original sys/provider identities pass."""
        VERIFIER.verify()

    def test_changed_git_revision(self):
        """A moving or different Git source cannot replace the reviewed object."""
        path = self.root / 'Cargo.toml'
        path.write_text(path.read_text().replace(VERIFIER.IDENTITY['revision'], '0' * 40))
        with self.assertRaisesRegex(ValueError, 'Git revision'):
            VERIFIER.verify()

    def test_changed_registry_sys_checksum(self):
        """The wrapper correction cannot substitute the registry sys package."""
        path = self.root / 'Cargo.lock'
        path.write_text(path.read_text().replace(VERIFIER.IDENTITY['libsql_sys_checksum'], '0' * 64))
        with self.assertRaisesRegex(ValueError, 'libsql-sys identity'):
            VERIFIER.verify()

    def test_changed_provider_manifest(self):
        """Any provider manifest byte change needs separate ownership and review."""
        (self.root / 'vendor/libsql-ffi.manifest.json').write_text('{}')
        with self.assertRaisesRegex(ValueError, 'provider manifest'):
            VERIFIER.verify()

    def test_changed_feature_set(self):
        """Unqualified wrapper features cannot enter through this correction."""
        path = self.root / 'Cargo.toml'
        path.write_text(path.read_text().replace('features = ["core"]', 'features = ["core", "remote"]'))
        with self.assertRaisesRegex(ValueError, 'feature/version'):
            VERIFIER.verify()

    def test_changed_authored_patch(self):
        """The retained production patch must match the reviewed source object."""
        (self.root / 'evidence/libsql-wrapper/companion-correction.patch.gz').write_bytes(gzip.compress(b'changed patch'))
        with self.assertRaisesRegex(ValueError, 'authored wrapper patch'):
            VERIFIER.verify()


if __name__ == '__main__':
    unittest.main()
