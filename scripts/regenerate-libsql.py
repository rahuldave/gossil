#!/usr/bin/env python3
"""Reproduce the reviewed native source from separately acquired pinned inputs.

This maintenance tool is never called by Cargo. It does not download inputs.
"""
import argparse
import hashlib
from pathlib import Path
import re

ORIGINAL = "0eb52e017a09ddb955a7fbaa5c2e5c39ed238f7480ec485e45f64fbb624ee7fb"
NATIVE = "fa1ca7a8ac45af89632ab480a3133556a48120fc76bffdd2d5cbb54cfb2e7ab0"
PATCH = "613b3fbf276cfbdb5d0010f3d09e271c0bfbc1a115d72f387a8ac38beb8bd19d"
OUTPUT = "78516943f8216206dd48091daa8b69aea172b5090c878e12d8dcba6cd445c072"


def checked(path, expected):
    """Reject any input whose complete byte identity differs from its pin."""
    if path.is_symlink() or not path.is_file():
        raise ValueError("input must be a regular nonsymlinked file")
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != expected:
        raise ValueError("input hash mismatch: " + str(path))
    return data


def apply(original, patch):
    """Apply exactly seven reviewed unified hunks with exact old/new counts."""
    lines = original.decode("utf-8").splitlines(keepends=True)
    chunks = patch.decode("utf-8").splitlines(keepends=True)
    if chunks[:2] != ["--- a/bundled/src/sqlite3.c\n", "+++ b/bundled/src/sqlite3.c\n"]:
        raise ValueError("unexpected patch target")
    output, cursor, index, hunks = [], 0, 2, 0
    while index < len(chunks):
        match = re.fullmatch(r"@@ -(\d+),(\d+) \+(\d+),(\d+) @@\n", chunks[index])
        if not match:
            raise ValueError("invalid patch header")
        start, old_count, new_start, new_count = map(int, match.groups())
        if start - 1 < cursor:
            raise ValueError("overlapping patch")
        output.extend(lines[cursor:start - 1])
        cursor = start - 1
        if len(output) != new_start - 1:
            raise ValueError("new patch offset mismatch")
        index += 1
        old_seen = new_seen = 0
        while index < len(chunks) and not chunks[index].startswith("@@ "):
            line = chunks[index]
            if line[0] in " -":
                if cursor >= len(lines) or lines[cursor] != line[1:]:
                    raise ValueError("patch context mismatch")
                cursor += 1
                old_seen += 1
            if line[0] in " +":
                output.append(line[1:])
                new_seen += 1
            if line[0] not in " +-":
                raise ValueError("invalid patch body")
            index += 1
        if (old_seen, new_seen) != (old_count, new_count):
            raise ValueError("patch substitution count mismatch")
        hunks += 1
    if hunks != 7:
        raise ValueError("wrong patch hunk count")
    output.extend(lines[cursor:])
    data = "".join(output).encode("utf-8")
    if hashlib.sha256(data).hexdigest() != OUTPUT:
        raise ValueError("regenerated output hash mismatch")
    return data


def regenerate(original, native, patch, output):
    """Check both upstream inputs and reviewed patch before writing a new file."""
    old = checked(original, ORIGINAL)
    checked(native, NATIVE)
    changes = checked(patch, PATCH)
    data = apply(old, changes)
    # Exclusive creation prevents accidental changes to existing reviewed inputs.
    with output.open("xb") as handle:
        handle.write(data)


def main():
    """Accept explicit inputs; report regeneration errors without partial output."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original", type=Path)
    parser.add_argument("native", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    patch = Path(__file__).resolve().parents[1] / "evidence/libsql-provider/adapted.patch"
    try:
        regenerate(args.original, args.native, patch, args.output)
    except (OSError, ValueError, UnicodeError) as error:
        parser.exit(1, str(error) + "\n")
    print("Reproduced " + OUTPUT)


if __name__ == "__main__":
    main()
