#!/usr/bin/env python3
"""Verify the reviewed complete native dependency, optionally against the index."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import tomllib
import subprocess

VENDOR = "vendor/libsql-ffi"
MANIFEST = "vendor/libsql-ffi.manifest.json"
IDENTITY = {'crate': 'libsql-ffi', 'version': '0.10.0', 'archive_sha256': 'f232292810b6298d17466b59bf8077d075fb016d89c281ca9dc968ea4a3b9d03', 'crate_vcs': 'd8bc862d20bedc72945a1da27c8b7045414c0261', 'original_source_sha256': '0eb52e017a09ddb955a7fbaa5c2e5c39ed238f7480ec485e45f64fbb624ee7fb', 'native_source_sha256': 'fa1ca7a8ac45af89632ab480a3133556a48120fc76bffdd2d5cbb54cfb2e7ab0', 'patched_source_sha256': '78516943f8216206dd48091daa8b69aea172b5090c878e12d8dcba6cd445c072'}


PRESERVED_READERS = {'vendor/libsql-ffi/bundled/SQLite3MultipleCiphers/readme.md': 'fd2e51292e7be006a3ed781422045701f532b1e5aa224e9f233564484e360782', 'vendor/libsql-ffi/bundled/SQLite3MultipleCiphers/src/filelist.md': '113a3c0eb9c38669ebef8c5821ce437a01e70f09dd313a6be2a58bf816726348', 'vendor/libsql-ffi/bundled/sqlean/regexp/pcre2/README.md': 'cbc112fe687da99471dc6befddda9ae13412725f3a0b8a272d1992a113f93b96'}
READER_MAP = ".agents/nossaifd-checkers.json"
READER_POLICY = {
    "version": 1,
    "checkers": [{
        "pages": list(PRESERVED_READERS),
        "checker": "scripts/check-preserved-libsql-prose.py",
    }],
}


OFFICIAL_ABI_FILES = {'build.rs': '452e349bbcfcc4367565352650341eea33a0a2492d90f4709ca4ce3a843cd2ed', 'bundled/src/sqlite3.h': 'ee6eab8416938332fb2aa93e257a07773cb2721661a293ce4e227c7a1ea9aa63', 'bundled/src/sqlite3ext.h': '79b7bb7d2e25a7c7b0960d25a2be07e49f1f9cfd54b869214592e93b37c26914', 'bundled/bindings/bindgen.rs': 'b03594f88895b4386165388bf50e426cdd1279d6c825b5a380022d113e6419d5', 'bundled/bindings/session_bindgen.rs': '652b23963ebb5bc967070344e813e8f03fe10b24752235662863a5ffe4443f34'}

def digest(data):
    """Return the full byte identity used by the frozen manifest."""
    return {"length": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def strict_json(data):
    """Reject duplicate keys rather than silently trusting the last digest."""
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError("duplicate manifest/provenance key")
            result[key] = value
        return result
    return json.loads(data, object_pairs_hook=unique)


def safe_path(value):
    """Reject noncanonical, absolute or escaping manifest paths."""
    if not isinstance(value, str) or not value or "\\" in value:
        raise ValueError("invalid manifest path")
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in (".", "..") for part in path.parts) or str(path) != value:
        raise ValueError("unsafe manifest path: " + value)
    return value


def regular(root, relative):
    """Read a regular file only after checking every ancestor for symlinks."""
    current = root
    for part in safe_path(relative).split("/"):
        current = current / part
        if current.is_symlink():
            raise ValueError("symlink: " + relative)
    if not stat.S_ISREG(current.stat().st_mode):
        raise ValueError("not a regular file: " + relative)
    return current.read_bytes()


def file_set(root):
    """Enumerate the whole vendor tree, including unexpected untracked files."""
    base = root / VENDOR
    if base.is_symlink() or (root / "vendor").is_symlink() or not base.is_dir():
        raise ValueError("missing or symlinked vendor root")
    files = set()
    for directory, dirs, names in os.walk(base, followlinks=False):
        for name in dirs + names:
            path = Path(directory) / name
            if path.is_symlink():
                raise ValueError("symlink in vendor tree: " + str(path))
            if name in names:
                if not stat.S_ISREG(path.stat().st_mode):
                    raise ValueError("nonregular vendor entry: " + str(path))
                files.add(path.relative_to(base).as_posix())
    return files


def index_entries(root):
    """Read staged object identities without interpreting worktree bytes as staged."""
    raw = subprocess.check_output(["git", "ls-files", "--stage", "-z"], cwd=root)
    result = {}
    for entry in raw.split(b"\0"):
        if not entry:
            continue
        head, path = entry.split(b"\t", 1)
        mode, oid, stage = head.decode("ascii").split()
        path = path.decode("utf-8")
        if stage != "0":
            raise ValueError("unmerged index")
        result[path] = (mode, oid)
    return result


def validate(root, indexed=False):
    """Validate inventory, byte identity, provenance and optional staged equality."""
    root = Path(root).resolve()
    index = index_entries(root) if indexed else None

    def read(path):
        working = regular(root, path)
        if index is None:
            return working
        mode, oid = index.get(path, (None, None))
        if mode not in ("100644", "100755"):
            raise ValueError("missing/nonregular staged file: " + path)
        staged = subprocess.check_output(["git", "cat-file", "blob", oid], cwd=root)
        if working != staged:
            raise ValueError("staged/worktree disagreement: " + path)
        return staged

    if strict_json(read(READER_MAP)) != READER_POLICY:
        raise ValueError("reader policy must name exactly the three reviewed upstream pages")
    for page, expected_hash in PRESERVED_READERS.items():
        if hashlib.sha256(read(page)).hexdigest() != expected_hash:
            raise ValueError("original upstream reader hash disagreement: " + page)
    manifest = strict_json(read(MANIFEST))
    if set(manifest) != {"schema", "identity", "files", "artifacts"} or manifest["schema"] != 1:
        raise ValueError("unsupported manifest")
    if manifest["identity"] != IDENTITY:
        raise ValueError("wrong source provenance")
    files = manifest["files"]
    if not isinstance(files, dict) or not files or ".cargo-ok" in files:
        raise ValueError("invalid archive inventory")
    expected = set(map(safe_path, files))
    if file_set(root) != expected:
        raise ValueError("vendor path-set disagreement")
    if index is not None:
        staged = {path[len(VENDOR) + 1:] for path in index if path.startswith(VENDOR + "/")}
        if staged != expected:
            raise ValueError("staged vendor path-set disagreement")
    artifacts = manifest["artifacts"]
    required = {
        "evidence/libsql-provider/provenance.json",
        "evidence/libsql-provider/adapted.patch",
        "scripts/regenerate-libsql.py",
        "scripts/verify-libsql-vendor.py",
        READER_MAP,
        "scripts/check-preserved-libsql-prose.py",
        "tests/test_libsql_vendor.py",
        "tests/test_libsql_provider_compile.py",
        "evidence/libsql-provider/upstream-style-original.txt",
        'tests/fixtures/provider-compile/abi-010.c',
        'tests/fixtures/provider-compile/provenance.json',
        'tests/fixtures/provider-compile/libsql-0.9.30-adapted-before-stdint.c.gz.part1',
        'tests/fixtures/provider-compile/libsql-0.9.30-adapted-before-stdint.c.gz.part2',
        'tests/fixtures/provider-compile/libsql-0.9.30-adapted-before-stdint.c.gz.part3',
        'evidence/libsql-provider/upgrade-0.10/adaptation-map.json',
        'evidence/libsql-provider/upgrade-0.10/adapted-0.9.30.patch',
        'evidence/libsql-provider/upgrade-0.10/provenance-0.9.30.json',
    }
    if not isinstance(artifacts, dict) or not required.issubset(artifacts):
        raise ValueError("missing provenance artifacts")
    entries = {VENDOR + "/" + name: row for name, row in files.items()}
    for path, row in artifacts.items():
        safe_path(path)
        if path == MANIFEST or path.startswith(VENDOR + "/"):
            raise ValueError("invalid artifact path")
        entries[path] = row
    for path, row in entries.items():
        if not isinstance(row, dict) or set(row) != {"length", "sha256"}:
            raise ValueError("invalid digest entry: " + path)
        if type(row["length"]) is not int or row["length"] < 0 or not re.fullmatch("[0-9a-f]{64}", str(row["sha256"])):
            raise ValueError("invalid digest: " + path)
        if digest(read(path)) != row:
            raise ValueError("byte identity disagreement: " + path)
    package = tomllib.loads(read(VENDOR + "/Cargo.toml").decode("utf-8"))["package"]
    if package["name"] != "libsql-ffi" or package["version"] != "0.10.0":
        raise ValueError("official provider package identity differs")
    for path, expected_hash in OFFICIAL_ABI_FILES.items():
        if hashlib.sha256(read(VENDOR + "/" + path)).hexdigest() != expected_hash:
            raise ValueError("official provider ABI/build bytes differ: " + path)
    provenance = strict_json(read("evidence/libsql-provider/provenance.json"))
    if provenance["identity"] != IDENTITY:
        raise ValueError("wrong provenance artifact")
    if files["bundled/src/sqlite3.c"]["sha256"] != IDENTITY["patched_source_sha256"]:
        raise ValueError("wrong candidate source")
    return len(files)


def main():
    """Fail closed; pre-commit uses the index, ordinary required checks use working files."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--index", action="store_true")
    args = parser.parse_args()
    try:
        count = validate(Path(__file__).resolve().parents[1], args.index)
    except (OSError, ValueError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        parser.exit(1, "libsql vendor verification failed: " + str(error) + "\n")
    print(f"Verified {count} libsql archive files and frozen provenance ({'index + worktree' if args.index else 'worktree'}).")


if __name__ == "__main__":
    main()
