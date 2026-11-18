# Bounded delta decoder

[API: `apply`](api/delta.qmd#apply) gives the exact contract.

`gossil::delta` applies Fossil deltas to byte slices. A delta describes output
with literal bytes and copies from an original byte slice. The library validates
the commands before allocating output, then returns the decoded bytes.
The `gossil vcs test-delta-apply` diagnostic exposes that decoder through files.
It opens no repository or database and does not forward work to C Fossil.

## Apply a delta from files

From the repository root, run:

```sh
cargo run --locked --bin gossil -- vcs test-delta-apply ORIGIN DELTA TARGET
```

Replace `ORIGIN`, `DELTA` and `TARGET` with file paths. Quote paths that contain
spaces. The command reads both inputs and finishes decoding before it opens
`TARGET`. The target can be the same path as either input.
On success, it writes exactly the decoded bytes, returns status 0 and leaves
stdout and stderr empty.

Cargo may print build messages on stderr before the executable runs.
The command returns status 1 for usage, read, decode, allocation or write errors.
The characterized diagnostics are:

| Failure | Stderr line |
| --- | --- |
| Unsupported command or argument count | `Usage: gossil vcs test-delta-apply ORIGIN DELTA TARGET` |
| Missing input | `no such file: PATH` |
| Delta rejection or buffer reservation failure | `Delta application failed.` |
| Target open or write failure | `unable to open file "PATH" for writing` |

Each diagnostic ends with one LF byte, and stdout remains empty. Other input
read errors include the path and the underlying I/O error. The command has no
root-level `test-delta-apply` alias.
Usage, input-read and decode failures preserve an existing target because it
has not been opened. A write failure can leave a truncated or partially written
target. The command does not replace files atomically.

## Use the library

The decoder API exposes `output_size`, `apply`, `Limits` and `DeltaError`.
The same module also exposes the encoder function [`create`](delta-encoder.md).
`output_size(delta)` parses only the advertised output size and its LF delimiter.
It does not validate the commands or enforce the output limit used by `apply`.

```rust
use gossil::delta::{Limits, apply, output_size};

let delta = b"3\n3:ABC0;";
assert_eq!(output_size(delta).unwrap(), 3);
let output = apply(b"", delta, Limits { max_output_bytes: 1024 }).unwrap();
assert_eq!(output, b"ABC");
```

`apply(source, delta, limits)` returns `Result<Vec<u8>, DeltaError>`.
The result has exactly the advertised size, with no trailing NUL byte.
Both input slices must fit within `i32::MAX` bytes. The output ceiling is the
smaller of `limits.max_output_bytes` and `i32::MAX`.
`Limits::default()` uses `i32::MAX`, as does the file diagnostic.

The first pass checks command syntax, source ranges, literal lengths, cumulative
output length and the final size. After validation, the decoder reserves output
storage with a fallible operation and replays the checked slices in a second pass.
It stores no list of parsed operations. Invalid commands reach no output
reservation. An empty result needs no reservation.

The decoder returns these `DeltaError` variants:

| Error variant | Meaning |
| --- | --- |
| `MissingHeaderNewline` | Header digits are not followed by LF. |
| `InputTooLarge` | A source or delta input exceeds the signed-length envelope. |
| `OutputTooLarge` | The advertised output exceeds the effective ceiling. |
| `TruncatedCommand` | A numeric command has no following operator. |
| `MissingCopyComma` | A copy offset has no present comma delimiter. |
| `UnknownOperator` | An operator byte is not defined. |
| `SourceRange` | A copy exceeds the source, including a zero copy beyond its end. |
| `OutputRange` | A command would exceed the advertised output. |
| `TruncatedLiteral` | A literal has fewer bytes than its count. |
| `Unterminated` | The input ends or reaches a command-start NUL before a semicolon. |
| `SizeMismatch` | A semicolon arrives before the advertised output is complete. |
| `AllocationFailed` | Output reservation failed, with the original `TryReserveError` retained as its cause. |

Fallible reservation reports recoverable allocation failures. It does not
guarantee recovery from an operating-system kill or memory exhaustion after a
successful reservation.

## Preserved format behavior

A delta starts with the encoded output length and LF. The form `COUNT:BYTES` inserts a literal, `COUNT@OFFSET,` copies from
the source, and `CHECKSUM;` ends the delta. All numbers use the alphabet below.

The decoder uses the exact unsigned-byte digit alphabet:

```text
0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ_abcdefghijklmnopqrstuvwxyz~
```

Numbers wrap modulo 2^32, and empty numeric fields mean zero. High-bit bytes
are not aliases for ASCII digits or delimiters. Zero-count copies still require
a valid source offset and a comma. Literal payloads can contain arbitrary bytes.
The decoder requires a semicolon and an exact final output size, then ignores
trailing bytes.

Checksum verification is disabled, as it is in the pinned reference build.
The numeric field before the semicolon is parsed but not checked against the
output. Accepted wrong-checksum fixtures remain accepted.
Checked ranges and slice access reject truncation at boundaries where the C
implementation can read beyond its logical input. Those C pointer behaviors
are not format guarantees.

## Recommendations

Use the contracts above with explicit resource limits. Next, consult the
[generated Rust API](api/index.qmd) for the full signatures and errors.
