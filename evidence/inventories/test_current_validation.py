"""Exercise rejection of corrupted correction/provenance companions."""

import copy
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location('current_inventory', SOURCE / 'validate_current.py')
VALIDATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VALIDATOR)


class CorrectionValidationTests(unittest.TestCase):
    """Verify that effective enum and complete identity checks reject drift."""

    def setUp(self):
        """Copy companions into an isolated directory for each corruption check."""
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name) / 'inventories'
        shutil.copytree(SOURCE, self.root, ignore=shutil.ignore_patterns('__pycache__'))
        VALIDATOR.ROOT = self.root

    def tearDown(self):
        """Restore the validator root and remove the temporary companions."""
        VALIDATOR.ROOT = SOURCE
        self.temporary.cleanup()

    def change_json(self, name, change):
        """Apply one deliberate mutation to a copied JSON companion."""
        path = self.root / name
        data = json.loads(path.read_text())
        change(data)
        path.write_text(json.dumps(data))

    def test_current_companions_pass(self):
        """Accept the current correction companions without mutations."""
        self.assertEqual(VALIDATOR.validate()['corrected_family_count'], 14)

    def test_non_normative_family_rejected(self):
        """Reject a disposition outside the declared vocabulary."""
        self.change_json('fossil/family-corrections.json', lambda d: d['contract_families']['database'].update(disposition='adapt'))
        with self.assertRaises(ValueError):
            VALIDATOR.validate()

    def test_injected_contract_destination_and_followup_rejected(self):
        """Reject overlay fields that could overwrite archived contract evidence."""
        path = self.root / 'fossil/family-corrections.json'
        original = path.read_bytes()
        for field in ['required_contract', 'proposed_destination', 'missing_test_followup']:
            with self.subTest(field=field):
                self.change_json('fossil/family-corrections.json', lambda d: d['contract_families']['database'].update({field: ''}))
                with self.assertRaises(ValueError):
                    VALIDATOR.validate()
                path.write_bytes(original)

    def test_missing_family_correction_rejected(self):
        """Reject an overlay that omits a required family correction."""
        self.change_json('fossil/family-corrections.json', lambda d: d['contract_families'].pop('platform-windows'))
        with self.assertRaises(ValueError):
            VALIDATOR.validate()

    def test_missing_and_duplicate_paths_rejected(self):
        """Reject incomplete or duplicate entries in the pinned path set."""
        for duplicate in [False, True]:
            with self.subTest(duplicate=duplicate):
                path = self.root / 'fossil/pinned-git-tree.json'
                original = path.read_bytes()
                data = json.loads(original)
                if duplicate:
                    data['files'][-1] = copy.deepcopy(data['files'][0])
                else:
                    data['files'].pop()
                path.write_text(json.dumps(data))
                with self.assertRaises(ValueError):
                    VALIDATOR.validate()
                path.write_bytes(original)

    def test_mode_kind_blob_and_byte_drift_rejected(self):
        """Reject changes to each recorded mode, kind, blob or byte identity."""
        path = self.root / 'fossil/pinned-git-tree.json'
        original = path.read_bytes()
        for field, value in [('git_mode', '100755'), ('file_kind', 'symlink'), ('git_object_id', '0' * 40), ('sha256', '0' * 64)]:
            with self.subTest(field=field):
                self.change_json('fossil/pinned-git-tree.json', lambda d: d['files'][0].update({field: value}))
                with self.assertRaises(ValueError):
                    VALIDATOR.validate()
                path.write_bytes(original)

    def test_original_archive_change_rejected(self):
        """Reject modified bytes in an uncompressed historical artifact."""
        with (self.root / 'fossil/short-report.json').open('ab') as stream:
            stream.write(b'\n')
        with self.assertRaises(ValueError):
            VALIDATOR.validate()

    def test_compressed_storage_change_rejected(self):
        """Reject changed gzip storage even when decompressed bytes agree."""
        import gzip
        path = self.root / 'fossil/inventory.json.gz'
        path.write_bytes(gzip.compress(gzip.decompress(path.read_bytes()), mtime=123))
        with self.assertRaises(ValueError):
            VALIDATOR.validate()

    def test_raw_tree_change_rejected(self):
        """Reject a changed raw Git tree capture."""
        with (self.root / 'fossil/pinned-git-tree.ls-tree').open('ab') as stream:
            stream.write(b'bad')
        with self.assertRaises(ValueError):
            VALIDATOR.validate()


if __name__ == '__main__':
    unittest.main()
