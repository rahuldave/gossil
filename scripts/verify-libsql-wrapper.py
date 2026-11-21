#!/usr/bin/env python3
"""Verify the immutable wrapper pin and unchanged registry/sys/provider identities."""
import hashlib
import gzip
import json
from pathlib import Path
import tomllib

ROOT = Path(__file__).resolve().parents[1]
IDENTITY = {
    'crate': 'libsql', 'version': '0.10.0',
    'repository': 'https://github.com/rahuldave/gossil-libsql',
    'revision': 'c02db34ae8a7e91ca7ab8faee18fe41515626940',
    'baseline': '243281163e4a39303322385d5f9537736277026e',
    'archive_sha256': 'd6bca1704194048f0f0bbc8a8e05ba85c0c67628b03bd59346dd2fdd946a1588',
    'upstream_commit': 'd8bc862d20bedc72945a1da27c8b7045414c0261',
    'libsql_sys_checksum': 'cf4a7f2d76f49442c273a892a574a272b8b35f4bf0b1b25a475f2cc95885fe0f',
    'ffi_manifest_sha256': '86134dea69ef341167417309f74b0473352932046219f9de1609ff60413a7a65',
    'correction_patch_sha256': '35223fd9fafbb00dc67fd500392a68d44270581fda29c626a7a3789171689194',
}


def require(condition, message):
    """Reject a changed identity even with Python optimization enabled."""
    if not condition:
        raise ValueError(message)


def verify():
    """Check every declared wrapper/source identity before normal Cargo checks."""
    provenance = json.loads((ROOT / 'evidence/libsql-wrapper/provenance.json').read_text())
    require(provenance['schema'] == 1 and provenance['identity'] == IDENTITY, 'wrapper provenance changed')
    manifest = tomllib.loads((ROOT / 'Cargo.toml').read_text())
    require(manifest['dependencies']['libsql'] == {'version': '=0.10.0', 'default-features': False, 'features': ['core']}, 'wrapper feature/version contract changed')
    patches = manifest['patch']['crates-io']
    require(patches['libsql'] == {'git': IDENTITY['repository'], 'rev': IDENTITY['revision']}, 'wrapper Git revision changed')
    require(patches['libsql-ffi'] == {'path': 'vendor/libsql-ffi'}, 'provider path patch changed')
    packages = tomllib.loads((ROOT / 'Cargo.lock').read_text())['package']
    selected = {}
    for name in ['libsql', 'libsql-sys', 'libsql-ffi']:
        matches = [item for item in packages if item['name'] == name]
        require(len(matches) == 1, f'expected one resolved {name} identity')
        selected[name] = matches[0]
        require(matches[0]['version'] == '0.10.0', f'{name} version changed')
    wrapper = selected['libsql']
    expected = f"git+{IDENTITY['repository']}?rev={IDENTITY['revision']}#{IDENTITY['revision']}"
    require(wrapper.get('source') == expected and 'checksum' not in wrapper, 'resolved wrapper source changed')
    system = selected['libsql-sys']
    require(system.get('source') == 'registry+https://github.com/rust-lang/crates.io-index' and system.get('checksum') == IDENTITY['libsql_sys_checksum'], 'registry libsql-sys identity changed')
    require('source' not in selected['libsql-ffi'] and 'checksum' not in selected['libsql-ffi'], 'resolved provider is not the existing path patch')
    require(hashlib.sha256((ROOT / 'vendor/libsql-ffi.manifest.json').read_bytes()).hexdigest() == IDENTITY['ffi_manifest_sha256'], 'provider manifest changed')
    require(hashlib.sha256(gzip.decompress((ROOT / 'evidence/libsql-wrapper/companion-correction.patch.gz').read_bytes())).hexdigest() == IDENTITY['correction_patch_sha256'], 'authored wrapper patch changed')
    print(f"Verified libsql 0.10.0 at {IDENTITY['revision']}; registry libsql-sys and existing provider identities unchanged.")


if __name__ == '__main__':
    verify()
