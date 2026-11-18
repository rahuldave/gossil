# Bounded delta encoder

`gossil::delta::create(source, target)` returns a Fossil delta that describes
`target` with literal bytes and copies from `source`. It returns
`Result<Vec<u8>, DeltaError>`. It opens no database and calls no C implementation.
The [decoder guide](delta-decoder.md) describes application of the result.

## Use the library

```rust
use gossil::delta::{Limits, apply, create};

let delta = create(b"before", b"after").unwrap();
let restored = apply(b"before", &delta, Limits { max_output_bytes: 1024 }).unwrap();
assert_eq!(restored, b"after");
```

## Use the diagnostics

From the repository root, replace the capitalized arguments with file paths:

```sh
cargo run --locked --bin gossil -- vcs test-delta-create ORIGIN TARGET DELTA
cargo run --locked --bin gossil -- vcs test-delta FILE1 FILE2
```

`test-delta-create` reads both inputs before opening `DELTA` for writing.
The output can share either input path. Success returns status 0 with empty
stdout and stderr. `test-delta` creates and applies deltas in both directions,
compares both results with the original bytes, and prints `ok` followed by LF.
A failed comparison, creation or round-trip application returns status 1
with `delta test failed` on stderr.
File-buffer reservation failures return `Delta application failed.`.

The commands require exactly three and two paths, respectively.

Usage errors return status 1 and the matching usage line. Missing inputs produce
`no such file: PATH`. Output failures produce
`unable to open file "PATH" for writing`. Each diagnostic ends with LF.

The regular-file tests cover missing inputs, output-open failure and paths
where the output aliases an input.
Other read errors retain the path and underlying I/O error.
Usage and input-read failures preserve an existing output. Writing can truncate
an existing output before a later write fails.

These diagnostics have no root command aliases. They read and write file data
through paths; `test-delta` uses stdout only for its `ok` diagnostic. Reading
file data from stdin, writing file data to stdout, automatic parent-folder
creation and complete native I/O diagnostic parity are outside this qualification.

## Preserved encoder choices

The encoder follows the pinned Fossil `src/delta.c` algorithm. It indexes source
landmarks, scans target windows for matching source bytes, and emits copies
when they cover their encoding overhead. Other bytes become literals. Its rolling hash
uses sixteen signed bytes, matching the selected native build. Checksums sum
unsigned big-endian four-byte words with wrapping arithmetic and right-zero
padding for the last short word.

Sources of at most sixteen bytes produce one literal, including a zero-length
literal for an empty target. Longer sources omit that zero literal. The hash
tables omit the last complete source block and visit newer landmarks first.
Each target window examines at most 250 candidates.

Backward extension stops before source offset zero. A match equal to its encoding overhead is accepted.
Equal-length candidates retain the first visited match, and the first target
window with a winning match emits it immediately.

Both inputs must fit within `i32::MAX` bytes. The encoder counts integer digits
with unsigned arithmetic. The native signed shift above `2^30` is undefined. Unsigned counting there
is a safe Rust policy and does not promise giant-input native parity. Slice lengths and addresses remain bounded by existing inputs.

Every output append reserves storage before extending the vector. Both hash
tables also reserve before initialization. `EncoderAllocationFailed` records
the buffer stage, requested element count and original `TryReserveError` cause.
The source and target size checks reuse `InputTooLarge`. Tests exercise genuine
capacity-overflow errors without asking the allocator for giant storage.
Fallible reservation does not promise recovery from an operating-system kill.

## Evidence and limits

`tests/fixtures/encoder/` preserves 28 exact native source, target and delta
triples. They include sixteen regular cases, three boundary cases and nine
earlier empty-source checksum cases. Tests compare every created delta byte for byte
and then apply it. The subprocess tests replay all 28 create outputs and check
two-way round trips. Provenance records preserve the native observations and
source/build identities; tests need only the checked-in bytes.

The finite observations distinguish signed incoming and outgoing rolling bytes,
the 250-candidate limit, landmark endpoints, equal-match retention, literals,
copies and checksum tails. The backward-zero exclusion and acceptance equality
have separate source-derived tests. These tests establish finite compatibility,
not universal encoder parity or checksum-enabled decoder behavior.

The pinned native reference is Fossil 2.29 at revision `7e8ecf0ce4`. Its
provenance records at `tests/fixtures/encoder/provenance.json` in the
repository root identify the
observations and build.

The candidate runs the unchanged inherited `delta1` suite through the
[existing runner](conformance.md). The evidence under `evidence/delta-encoder/`
records the exact artifact, corpus, seed, deadline, results and harness changes.
The original corpus includes every non-directory entry in `testdir`, with no
file-type filtering. The runner's launch script seeds Tcl randomness; this is
an explicit harness adaptation to the otherwise unseeded upstream suite.
A namespace wrapper inserts `vcs` before each diagnostic command.
The enabled driver is separate from the corpus, so enabling execution does
not alter any original corpus bytes.

The runner supports an optional [complete pinned invocation plan](conformance.md#optional-complete-invocation-plan).
Here, frozen means the exact runner, candidate, source and input hashes recorded
together in the qualification evidence.

The final run of the frozen runner used the complete plan with its expected
count pinned at 22,804 rows. It accepted all 22,804 assertions with zero errors or skips.
It retained all original names and assigned each occurrence a unique expected
source-derived ID. The plan binds the suite, source revision, all 76 original
corpus files, exact source order and expected count. The runner verifies expected
hashes before parsing the same bytes and preserves all other rejection rules.

`evidence/delta-encoder/planned-delta1-results.json` records the final runner,
encoder, plan, source and corpus hashes together.
`evidence/delta-encoder/plan-source-mapping.json` maps the generator rules
to the original source and enumerates all 76 corpus files.

### Preserved earlier runs

The two original strict-mode runs reported all 22,804 assertions as passing,
including 22,803 direct assertions and one framework assertion. Each exited 0
with no errors, ignored assertions or timeout. Their reports remain blocked because the default parser rejects repeated
assertion names.
Three original corpus files share the root `link-tester`, so the upstream suite
repeats 300 original assertion names three times each.


The portable archives preserve complete reports,
logs, source snapshots, all original corpus bytes and generation evidence.
Only `run-plan-final` qualifies the frozen runner with the expected count pin.
An earlier planned run remains intermediate evidence. No original blocked run
has been relabeled. Strict default parsing still rejects repeated names.

Expected IDs describe correspondence from the reviewed source and sorted corpus.
The original log does not emit them. Name matching cannot detect whole-group
permutations or substitutions with identical printed names. Count and hash pins
bind supplied bytes; the generator and source review establish completeness.
The tests relocate the original corpus and regenerate a plan with the same bytes.

## Recommendations

Use `create` for bounded in-memory encoding and an explicit decoder output
limit when applying the result. Next, complete the independent documentation, correctness and test reviews
of the encoder, runner correction and recorded `delta1` qualification
before accepting the encoder task.
