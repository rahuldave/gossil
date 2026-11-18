#!/usr/bin/env python3
"""Verify the reviewed complete native dependency, optionally against the index."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess

VENDOR = "vendor/libsql-ffi"
MANIFEST = "vendor/libsql-ffi.manifest.json"
IDENTITY = {
    "crate": "libsql-ffi",
    "version": "0.9.30",
    "archive_sha256": "0be1da6f123ceb2cd23f469883415cab9ee963286a85d61e22afb8b12e15e681",
    "crate_vcs": "0653c5788d77ef16a97c56ff3e9fdc11717a72d9",
    "original_source_sha256": "d9791dc493f043172d75c4fe6ef48307db6534c98f2106c843551ca5cbfa2912",
    "native_source_sha256": "fa1ca7a8ac45af89632ab480a3133556a48120fc76bffdd2d5cbb54cfb2e7ab0",
    "patched_source_sha256": "1c99ebdc5a6fccdecb30c7702f7f6b07fa8258eea7d4216d878863cb3b7deefd",
}


PRESERVED_READERS = {'vendor/libsql-ffi/bundled/SQLite3MultipleCiphers/readme.md': '451308d2943483efcd506effdde8d5362a25a81a844beb37ef7c034a15ceb123',
 'vendor/libsql-ffi/bundled/SQLite3MultipleCiphers/src/filelist.md': '113a3c0eb9c38669ebef8c5821ce437a01e70f09dd313a6be2a58bf816726348',
 'vendor/libsql-ffi/bundled/sqlean/regexp/pcre2/README.md': 'cbc112fe687da99471dc6befddda9ae13412725f3a0b8a272d1992a113f93b96'}
READER_MAP = ".agents/nossaifd-checkers.json"
READER_POLICY = {
    "version": 1,
    "checkers": [{
        "pages": list(PRESERVED_READERS),
        "checker": "scripts/check-preserved-libsql-prose.py",
    }],
}


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
        "evidence/libsql-provider/upstream-style-original.txt",
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
