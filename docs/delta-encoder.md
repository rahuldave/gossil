# Bounded delta encoder

[API: `create`](api/delta.qmd#create) gives the exact contract.

`gossil::delta::create(source, target)` returns a Fossil delta that describes
`target` with literal bytes and copies from `source`. It returns
`Result<Vec<u8>, DeltaError>`. It opens no database and calls no C implementation.
The [decoder guide](delta-decoder.md) describes how to apply the result.

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

Cargo may print build messages on stderr before the executable runs.
The commands require exactly three and two paths, respectively.

Usage errors return status 1 and the matching usage line. Missing inputs produce
`no such file: PATH`. Output failures produce
`unable to open file "PATH" for writing`. Each diagnostic ends with LF.

Other read errors retain the path and underlying I/O error.
Usage and input-read failures preserve an existing output. Writing can truncate
an existing output before a later write fails.

These diagnostics have no root command aliases. They read and write file data
through paths. `test-delta` uses stdout only for its `ok` diagnostic. Reading
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

## Recommendations

Use the contracts above with explicit resource limits. Next, consult the
[generated Rust API](api/index.qmd) for the full signatures and errors.
