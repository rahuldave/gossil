"""Compile the complete adapted provider with the core Cargo build's C flags."""
import gzip
import hashlib
import os
import re
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
        """Compile frozen exact 0.9.30 pre-fix bytes; this is historical evidence."""
        # Historical 0.9.30 bytes are independent of the current 0.10 provider.
        fixture = ROOT / "tests/fixtures/provider-compile"
        parts = (
            ("libsql-0.9.30-adapted-before-stdint.c.gz.part1",
             "5171481098a6ea993ff33d15b6417194859c7b9067c77525dd730f9c16ba4287"),
            ("libsql-0.9.30-adapted-before-stdint.c.gz.part2",
             "71de6b5fc180af1837825b339b41916257ffbc89b907a6e55fb890ef30982016"),
            ("libsql-0.9.30-adapted-before-stdint.c.gz.part3",
             "ae08a908fa0db8ac6bc33ba0b08daae283d92aa915c7876d00e92534095652b3"),
        )
        compressed = bytearray()
        for name, checksum in parts:
            part = (fixture / name).read_bytes()
            self.assertLess(len(part), 1000 * 1024)
            self.assertEqual(hashlib.sha256(part).hexdigest(), checksum)
            compressed.extend(part)
        self.assertEqual(hashlib.sha256(compressed).hexdigest(),
                         "a24ede40d3d24a529a15093a69d7e7ca96a91d321edfaec5f93be19812d0a48b")
        previous = gzip.decompress(compressed)
        self.assertEqual(hashlib.sha256(previous).hexdigest(),
                         "1c99ebdc5a6fccdecb30c7702f7f6b07fa8258eea7d4216d878863cb3b7deefd")
        with tempfile.TemporaryDirectory(prefix="gossil-provider-before-") as directory:
            baseline = Path(directory) / "previous.c"
            baseline.write_bytes(previous)
            result = self.compile(baseline, Path(directory) / "previous.o")
            self.assertNotEqual(result.returncode, 0, "pre-fix Linux provider unexpectedly compiled")
            self.assertIn("UINT64_C", result.stderr)
            self.assertRegex(result.stderr, r"initializer element is not (a compile-time )?constant")


    @unittest.skipUnless(sys.platform == "linux", "integer-header negative needs Linux headers")
    def test_current_010_requires_explicit_integer_header(self):
        """Removing only the current closure's integer header exposes UINT64_C."""
        source = (ROOT / "vendor/libsql-ffi/bundled/src/sqlite3.c").read_bytes()
        marker = (b"/* Pinned SQLite numerical closure for date text only; original "
                  b"length/encoding API remains unchanged. */\n#include <math.h>\n")
        fixed = marker + b"#include <stdint.h>\n"
        self.assertEqual(source.count(fixed), 1)
        with tempfile.TemporaryDirectory(prefix="gossil-provider-010-stdint-") as directory:
            negative = Path(directory) / "without-stdint.c"
            negative.write_bytes(source.replace(fixed, marker, 1))
            result = self.compile(negative, Path(directory) / "negative.o")
            self.assertNotEqual(result.returncode, 0, "0.10 missing-header control compiled")
            self.assertIn("UINT64_C", result.stderr)
            self.assertRegex(result.stderr, r"initializer element is not (a compile-time )?constant")

    def test_current_010_requires_math_header_before_date_closure(self):
        """The real date closure must obtain INFINITY/HUGE_VAL before first use."""
        source = (ROOT / "vendor/libsql-ffi/bundled/src/sqlite3.c").read_bytes()
        marker = (b"/* Pinned SQLite numerical closure for date text only; original "
                  b"length/encoding API remains unchanged. */\n")
        fixed = marker + b"#include <math.h>\n#include <stdint.h>\n"
        self.assertEqual(source.count(fixed), 1)
        with tempfile.TemporaryDirectory(prefix="gossil-provider-010-math-") as directory:
            negative = Path(directory) / "without-math.c"
            negative.write_bytes(source.replace(fixed, marker + b"#include <stdint.h>\n", 1))
            result = self.compile(negative, Path(directory) / "negative.o")
            self.assertNotEqual(result.returncode, 0, "0.10 missing-math control compiled")
            self.assertRegex(result.stderr, r"(HUGE_VAL|INFINITY)")

    def test_current_010_wal_bindings_match_official_abi_two(self):
        """Both shipped Rust binding modes retain the authoritative C WAL layout."""
        vendor = ROOT / "vendor/libsql-ffi/bundled"
        header = (vendor / "src/sqlite3.h").read_text()
        body = header.split("typedef struct libsql_wal_methods {", 1)[1].split(
            "} libsql_wal_methods;", 1)[0]
        fields = ["iVersion", *re.findall(r"^  \S[^\n]*?\(\*(x\w+)\)", body, re.MULTILINE)]
        self.assertEqual(len(fields), 29)
        self.assertEqual(fields[-2:], ["xDb", "xSavepointForget"])
        self.assertRegex(body, r"void \(\*xSavepointForget\)\(wal_impl\* pWal, unsigned int \*aWalData\);")
        # Official ffi 0.10.0 archive bindings: compare complete files, not a mock ABI.
        checksums = {
            "bindgen.rs": "b03594f88895b4386165388bf50e426cdd1279d6c825b5a380022d113e6419d5",
            "session_bindgen.rs": "652b23963ebb5bc967070344e813e8f03fe10b24752235662863a5ffe4443f34",
        }
        for name, checksum in checksums.items():
            with self.subTest(bindings=name):
                raw = (vendor / "bindings" / name).read_bytes()
                self.assertEqual(hashlib.sha256(raw).hexdigest(), checksum)
                rust = raw.decode().split("pub struct libsql_wal_methods {", 1)[1].split("\n}", 1)[0]
                self.assertEqual(re.findall(r"pub (\w+):", rust), fields)
                self.assertRegex(rust, r"pub xSavepointForget: ::std::option::Option<\s*"
                                 r'unsafe extern "C" fn\(pWal: \*mut wal_impl, '
                                 r"aWalData: \*mut ::std::os::raw::c_uint\),\s*>")

    def test_current_010_linked_wal_savepoint_and_numeric_contract(self):
        """Real core flags must link and execute version-two WAL release callbacks."""
        with tempfile.TemporaryDirectory(prefix="gossil-provider-010-abi-") as directory:
            directory = Path(directory)
            output = directory / "sqlite3.o"
            result = self.compile(ROOT / "vendor/libsql-ffi/bundled/src/sqlite3.c", output)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            executable = directory / "provider-abi"
            command = [*shlex.split(os.environ.get("CC", "cc")), "-std=c11", "-O0",
                       "-Wall", "-Wextra", "-Werror", "-I",
                       str(ROOT / "vendor/libsql-ffi/bundled/src"),
                       str(ROOT / "tests/fixtures/provider-compile/abi-010.c"),
                       str(output), "-o", str(executable), "-lm", "-lpthread", "-ldl"]
            linked = subprocess.run(command, capture_output=True, text=True, timeout=180,
                                    env={**os.environ, "LC_ALL": "C"})
            self.assertEqual(linked.returncode, 0, linked.stdout + linked.stderr)
            run = subprocess.run([str(executable), str(directory / "wal.db")],
                                 capture_output=True, text=True, timeout=30,
                                 env={**os.environ, "LC_ALL": "C"})
            self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
            self.assertEqual(run.stdout, "sqlite-3.47.0 wal-abi-2 savepoint-forget numeric-ok\n")


if __name__ == "__main__":
    unittest.main()
