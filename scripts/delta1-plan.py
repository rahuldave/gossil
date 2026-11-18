#!/usr/bin/env python3
"""Generate complete source-derived delta1 identities in actual Tcl lsort order."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("source", type=Path)
parser.add_argument("output", type=Path)
parser.add_argument("--tcl", required=True, type=Path)
parser.add_argument("--revision", required=True)
args = parser.parse_args()
source = args.source.resolve()
args.output.mkdir(parents=True, exist_ok=False)
# Tcl supplies both ordering and file-root behavior. No extension filter is applied.
program = 'set root [lindex $argv 0]\nforeach f [lsort [glob $root/test/*]] {if {[file isdir $f]} continue; puts "$f\\t[file root [file tail $f]]"}\n'
result = subprocess.run([str(args.tcl), "/dev/stdin", str(source)], input=program, text=True, check=True, capture_output=True)
files = {}
rows = []
order = []
for line in result.stdout.splitlines():
    absolute, base = line.split("\t")
    path = Path(absolute)
    relative = path.relative_to(source).as_posix()
    files[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
    order.append(relative)
    for iteration in range(100):
        for slot in range(1, 4):
            rows.append({"name": f"delta-{base}-{iteration}-{slot}", "id": {"suite": "delta1", "position": {"kind": "corpus", "source": relative, "iteration": iteration, "slot": slot}}})
for index in range(1, 4):
    rows.append({"name": f"delta-empty-{index}", "id": {"suite": "delta1", "position": {"kind": "empty", "index": index}}})
rows.append({"name": "test-framework-delta1", "id": {"suite": "delta1", "position": {"kind": "framework"}}})
manifest = {"suite": "delta1", "revision": args.revision, "files": files}
manifest_bytes = (json.dumps(manifest, indent=2) + "\n").encode()
(args.output / "corpus-manifest.json").write_bytes(manifest_bytes)
plan = {"schema": "gossil.invocation-plan.v1", "suite": "delta1", "revision": args.revision, "suite_sha256": hashlib.sha256((source / "test/delta1.test").read_bytes()).hexdigest(), "corpus_manifest_sha256": hashlib.sha256(manifest_bytes).hexdigest(), "rows": rows}
plan_bytes = (json.dumps(plan, indent=2) + "\n").encode()
(args.output / "invocation-plan.json").write_bytes(plan_bytes)
(args.output / "generation.json").write_text(json.dumps({"source_revision": args.revision, "source": str(source), "tcl": str(args.tcl), "tcl_sha256": hashlib.sha256(args.tcl.read_bytes()).hexdigest(), "generator_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "tcl_enumeration_program": program, "ordered_corpus": order, "N": len(files), "rows": len(rows), "plan_sha256": hashlib.sha256(plan_bytes).hexdigest(), "corpus_manifest_sha256": hashlib.sha256(manifest_bytes).hexdigest(), "expected_identity_limit": "Source-derived correspondence; output cannot detect invisible same-name whole-group substitutions or permutations."}, indent=2) + "\n")
print(f"{len(files)} corpus files; {len(rows)} complete ordered rows")
