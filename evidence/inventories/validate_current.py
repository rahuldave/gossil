"""Validate correction companions and immutable inventory identities.

Use --git-source PATH to compare every Fossil tree entry and blob to the pin.
No engine assertions execute, and no historical artifacts are rewritten.
"""

import argparse
import collections
import gzip
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent
PIN = 'b3a1c8efb2731dffbd5cc38c58ac349746de6da6'
ALLOWED = {'port', 'replace-with-equivalent', 'supporting-source'}


def digest(data):
    """Return the SHA-256 identity of bytes."""
    return hashlib.sha256(data).hexdigest()


def load(path):
    """Read JSON, decompressing historical inventory captures when needed."""
    data = path.read_bytes()
    return json.loads(gzip.decompress(data) if path.suffix == '.gz' else data)


def tree_rows(raw):
    """Parse complete NUL-terminated Git tree output without path quoting."""
    if not raw.endswith(b'\0'):
        raise ValueError('tree output must be NUL terminated')
    rows = []
    for entry in raw[:-1].split(b'\0'):
        metadata, path = entry.split(b'\t', 1)
        mode, kind, oid = metadata.decode().split()
        rows.append((path.decode(), mode, kind, oid))
    return rows


def dispositions(value):
    """Yield every effective disposition field, including nested mappings."""
    if isinstance(value, dict):
        for key, child in value.items():
            if key == 'disposition':
                yield child
            else:
                yield from dispositions(child)
    elif isinstance(value, list):
        for child in value:
            yield from dispositions(child)


def validate(git_source=None):
    """Check all originals, corrected dispositions and pinned file identities."""
    checks = []

    def require(name, passed):
        """Record a successful identity check or reject its named failure."""
        if not passed:
            raise ValueError(name)
        checks.append(name)

    originals = {}
    inventories = {}
    for family, expected_count in [('fossil', 10), ('gest', 13)]:
        index = load(ROOT / family / 'relocation-index.json')
        require(f'{family}: complete original artifact count', len(index['files']) == expected_count)
        for item in index['files'].values():
            path = ROOT / family / item['archive']
            stored = path.read_bytes()
            data = gzip.decompress(stored) if item.get('compression') == 'gzip' else stored
            require(f'{family}/{item["archive"]}: original bytes', digest(data) == item['sha256'] and len(data) == item['bytes'])
            originals[str(path.relative_to(ROOT))] = {'stored_sha256': digest(stored), 'original_sha256': digest(data), 'original_bytes': len(data)}
        inventories[family] = load(ROOT / family / 'inventory.json.gz')

    receipt = load(ROOT / 'correction-receipt.json')
    require('all 23 originals preserve stored and decompressed bytes', originals == receipt['preserved_originals'])
    fossil = inventories['fossil']
    overlay = load(ROOT / 'fossil/family-corrections.json')
    require('overlay pinned to archived inventory', overlay['source_revision'] == PIN == fossil['source_revision'] and overlay['archived_inventory_sha256'] == digest(gzip.decompress((ROOT / 'fossil/inventory.json.gz').read_bytes())))
    families = fossil['contract_families']
    expected = {key for key, row in families.items() if row['disposition'] not in ALLOWED}
    require('exact fourteen family corrections', set(overlay['contract_families']) == expected and len(expected) == 14)
    for key, correction in overlay['contract_families'].items():
        require(f'{key}: exact correction fields preserve archived contracts', set(correction) == {'original_disposition', 'disposition', 'adaptation_action', 'status', 'explanation'})
        require(f'{key}: correction retains original and pending explanation', correction['original_disposition'] == families[key]['disposition'] and correction['adaptation_action'] == correction['original_disposition'] and bool(correction['status']) and bool(correction['explanation']))
        require(f'{key}: normalization follows declared vocabulary', correction['disposition'] == {'adapt': 'replace-with-equivalent', 'reference': 'supporting-source', 'defer-required': 'port'}[correction['original_disposition']])
        families[key].update({k: v for k, v in correction.items() if k != 'original_disposition'})
    for family, inventory in inventories.items():
        values = list(dispositions(inventory))
        require(f'{family}: every effective disposition is normative', bool(values) and all(v in ALLOWED for v in values))
    require('Windows remains required port scope', families['platform-windows']['disposition'] == 'port' and 'required' in families['platform-windows']['explanation'])

    manifest = load(ROOT / 'fossil/pinned-git-tree.json')
    raw = (ROOT / 'fossil/pinned-git-tree.ls-tree').read_bytes()
    require('raw tree capture identity', digest(raw) == manifest['acquisition']['raw_tree_sha256'])
    require('Git mirror and native identities remain separate', manifest['git_revision'] == PIN and manifest['native_fossil_manifest_id'] == fossil['native_revision'] and manifest['git_revision'] != manifest['native_fossil_manifest_id'])
    parsed = tree_rows(raw)
    rows = manifest['files']
    tuples = [(x['path'], x['git_mode'], x['git_object_type'], x['git_object_id']) for x in rows]
    inventory = {x['path']: x for x in fossil['files']}
    source_inventory = {x['path']: x for x in load(ROOT / 'fossil/source-inventory-input.json')['files']}
    require('full 984-path mode/type/blob identity set', len(parsed) == len(set(parsed)) == len(rows) == manifest['file_count'] == 984 and set(parsed) == set(tuples) and len({x['path'] for x in rows}) == 984)
    require('complete path sets match both byte inventories', {x['path'] for x in rows} == set(inventory) == set(source_inventory))
    require('all manifest byte hashes match both inventories', all(x['sha256'] == inventory[x['path']]['sha256'] == source_inventory[x['path']]['sha256'] for x in rows))
    require('all kinds and modes valid including link text semantics', all(x['git_object_type'] == 'blob' and x['git_mode'] in {'100644', '100755', '120000'} and x['file_kind'] == ('symlink' if x['git_mode'] == '120000' else 'regular') for x in rows))
    require('mode totals match complete manifest', manifest['mode_counts'] == dict(collections.Counter(x['git_mode'] for x in rows)))
    if git_source:
        def git(*args):
            """Read pinned Git objects from the supplied source checkout."""
            return subprocess.check_output(['git', '-C', str(git_source), *args])
        require('actual pinned Git tree agrees byte for byte', git('ls-tree', '-r', '-z', '--full-tree', PIN) == raw)
        require('actual root tree identity', git('rev-parse', PIN + '^{tree}').decode().strip() == manifest['git_tree_object_id'])
        for row in rows:
            blob = git('cat-file', 'blob', row['git_object_id'])
            oid = hashlib.sha1(b'blob ' + str(len(blob)).encode() + b'\0' + blob).hexdigest()
            require(f'{row["path"]}: pinned blob bytes and object identity', digest(blob) == row['sha256'] and len(blob) == row['bytes'] and oid == row['git_object_id'])
    return {'kind': 'exhaustive inventory correction/provenance checks; not compatibility acceptance', 'check_count': len(checks), 'checks': checks, 'original_artifacts': originals, 'effective_fossil_family_count': len(families), 'corrected_family_count': len(expected), 'fossil_file_count': len(rows), 'mode_counts': manifest['mode_counts'], 'actual_git_blobs_verified': len(rows) if git_source else 0, 'limits': 'No native Fossil manifest reconstruction, semantic source audit, configuration qualification or engine assertions.'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--git-source', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    result = validate(args.git_source)
    if args.output:
        args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({k: v for k, v in result.items() if k not in {'checks', 'original_artifacts'}}, indent=2))
