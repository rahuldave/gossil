"""Compile the complete adapted provider with the core Cargo build's C flags."""
import hashlib
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
# Keep these feature definitions aligned with build_bundled in upstream build.rs.
DEFINITIONS = (
    "SQLITE_CORE", "SQLITE_DEFAULT_FOREIGN_KEYS=1", "SQLITE_ENABLE_API_ARMOR",
    "SQLITE_ENABLE_COLUMN_METADATA", "SQLITE_ENABLE_DBSTAT_VTAB", "SQLITE_ENABLE_FTS3",
    "SQLITE_ENABLE_FTS3_PARENTHESIS", "SQLITE_ENABLE_FTS5", "SQLITE_ENABLE_JSON1",
    "SQLITE_ENABLE_LOAD_EXTENSION=1", "SQLITE_ENABLE_MEMORY_MANAGEMENT",
    "SQLITE_ENABLE_RTREE", "SQLITE_ENABLE_STAT2", "SQLITE_ENABLE_STAT4", "SQLITE_SOUNDEX",
    "SQLITE_THREADSAFE=1", "SQLITE_USE_URI", "HAVE_USLEEP=1", "_POSIX_THREAD_SAFE_FUNCTIONS",
)


class ProviderCompile(unittest.TestCase):
    """Exercise real amalgamation initializers without optional header defines."""

    def compile(self, source, output):
        """Use the production core definitions and preserve compiler diagnostics."""
        command = [*shlex.split(os.environ.get("CC", "cc")), "-O0", "-fPIC", "-w",
                   "-std=c11", *["-D" + value for value in DEFINITIONS],
                   "-UHAVE_STDINT_H", "-UHAVE_INTTYPES_H", "-c", str(source), "-o", str(output)]
        return subprocess.run(command, capture_output=True, text=True, timeout=180,
                              env={**os.environ, "LC_ALL": "C"})

    def test_core_amalgamation_compiles(self):
        """The actual provider must produce an object without HAVE_STDINT_H hints."""
        with tempfile.TemporaryDirectory(prefix="gossil-provider-compile-") as directory:
            output = Path(directory) / "sqlite3.o"
            result = self.compile(ROOT / "vendor/libsql-ffi/bundled/src/sqlite3.c", output)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertGreater(output.stat().st_size, 0)

    @unittest.skipUnless(sys.platform == "linux", "historical failure needs Linux headers")
    def test_exact_previous_provider_fails_without_integer_header(self):
        """Reconstruct and compile the complete failing pre-fix adapted source."""
        source = (ROOT / "vendor/libsql-ffi/bundled/src/sqlite3.c").read_bytes()
        closure = (b"/* Pinned SQLite numerical closure for date text only; original "
                   b"length/encoding API remains unchanged. */\n#include <math.h>\n")
        fixed = closure + b"#include <stdint.h>\n"
        self.assertEqual(source.count(fixed), 1)
        previous = source.replace(fixed, closure, 1)
        self.assertEqual(hashlib.sha256(previous).hexdigest(),
                         "1c99ebdc5a6fccdecb30c7702f7f6b07fa8258eea7d4216d878863cb3b7deefd")
        with tempfile.TemporaryDirectory(prefix="gossil-provider-before-") as directory:
            baseline = Path(directory) / "previous.c"
            baseline.write_bytes(previous)
            result = self.compile(baseline, Path(directory) / "previous.o")
            self.assertNotEqual(result.returncode, 0, "pre-fix Linux provider unexpectedly compiled")
            self.assertIn("UINT64_C", result.stderr)
            self.assertRegex(result.stderr, r"initializer element is not (a compile-time )?constant")


if __name__ == "__main__":
    unittest.main()
