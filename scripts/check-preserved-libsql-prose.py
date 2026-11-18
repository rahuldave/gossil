#!/usr/bin/env python3
"""Check exact frozen upstream reader pages by full source and provenance identity.

This project policy preserves readable third-party prose verbatim. It does not
remove its historical style findings or apply to authored project prose.
"""
import argparse
import importlib.util
from pathlib import Path
import subprocess


def check(root, page):
    """Reject unlisted paths and verify the complete frozen source composition."""
    spec = importlib.util.spec_from_file_location("libsql_vendor", root / "scripts/verify-libsql-vendor.py")
    verify = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(verify)
    if page not in verify.PRESERVED_READERS:
        raise ValueError("page is outside the exact preserved-reader allowlist")
    return verify.validate(root)


def main():
    """Implement the installed python CHECKER [-v] PAGE contract, failing closed."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("-v", action="store_true")
    parser.add_argument("page")
    args = parser.parse_args()
    try:
        count = check(Path(__file__).resolve().parents[1], args.page)
    except (OSError, ValueError, KeyError, TypeError, ImportError, subprocess.CalledProcessError) as error:
        print("HARD: preserved upstream prose verification failed: " + str(error))
        return 1
    print(f"{args.page}: preserved upstream prose; verified {count} source files and frozen provenance")
    if args.v:
        print("Source preservation policy; original style findings remain recorded, not corrected.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
