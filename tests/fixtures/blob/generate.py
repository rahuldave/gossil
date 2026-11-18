"""Regenerate finite Fossil byte vectors through the explicit reference tool."""
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
REVISION = "b3a1c8efb2731dffbd5cc38c58ac349746de6da6"

def fossil(*args):
    """Keep each native reference invocation in jlog's execution history."""
    return subprocess.check_output(["jlog", "run", "fossil", *map(str, args)], text=True)

def main():
    """Write portable packed bytes and every accepted SHA3 width boundary."""
    results = {"source_revision": REVISION, "reference_version": fossil("version").strip(), "sha3": [], "packed": []}
    with tempfile.TemporaryDirectory(prefix="gossil-blob-vectors-") as directory:
        scratch = Path(directory)
        source = scratch / "input"
        other = scratch / "other"
        target = scratch / "packed"
        for bits in range(128, 513, 32):
            rate = 200 - bits // 4
            for length in [0, 3, rate - 1, rate, rate + 1, 2 * rate + 1]:
                data = bytes((i * 73 + 11) % 256 for i in range(length))
                source.write_bytes(data)
                digest = fossil("sha3sum", "--size", bits, source).split()[0]
                results["sha3"].append({"bits": bits, "input_hex": data.hex(), "digest_hex": digest})
        for data in [b"", b"abc", bytes(range(256)), b"Fossil\x00blob\n" * 2000]:
            source.write_bytes(data)
            fossil("test-compress", source, target)
            packed = target.read_bytes()
            results["packed"].append({"input_hex": data.hex(), "packed_hex": packed.hex(), "mode": "single"})
            split = len(data) // 2
            source.write_bytes(data[:split]); other.write_bytes(data[split:])
            fossil("test-compress-2", source, other, target)
            results["packed"].append({"input_hex": data.hex(), "packed_hex": target.read_bytes().hex(), "mode": "two"})
        source.write_bytes(b"a\x00b\xff")
        results["sha1"] = {"input_hex": source.read_bytes().hex(), "digest_hex": fossil("sha1sum", source).split()[0]}
        results["sha1_collisions"] = []
        for name in ["sha-mbles-1.bin", "sha-mbles-2.bin"]:
            source.write_bytes((ROOT / name).read_bytes())
            results["sha1_collisions"].append({"file": name, "digest_hex": fossil("sha1sum", source).split()[0]})
        # The native CLI ignores uncompress errors. Check bytes, not just exit zero.
        normal = bytes.fromhex(results["packed"][2]["packed_hex"])
        for name, data in [("oversized", (100).to_bytes(4, "big") + normal[4:]), ("trailing", normal + b"trailing"), ("short", b"abcd"), ("invalid", b"\0\0\0\x03invalid")]:
            source.write_bytes(data)
            fossil("test-uncompress", source, target)
            results.setdefault("native_uncompress", []).append({"case": name, "input_hex": data.hex(), "output_hex": target.read_bytes().hex()})
    (ROOT / "reference.json").write_text(json.dumps(results, indent=2) + "\n")

if __name__ == "__main__":
    main()
