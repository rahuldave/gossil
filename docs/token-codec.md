# Fossil token bytes

The `gossil::token_codec` module encodes and decodes arbitrary bytes for
Fossil artifact and protocol tokens.
It implements the token escape functions from Fossil revision
`b3a1c8efb2731dffbd5cc38c58ac349746de6da6`, `src/encode.c:392-458`.
It does not parse artifacts or select repository hash policy.

## API contracts

All three functions return `Result` and preserve allocation error causes.
`fossilize` encodes the entire slice, including embedded NUL bytes.
`fossilize_c_string` encodes bytes before the first NUL, or the whole slice
when no NUL occurs. Both return bytes without a C terminator.

The eight escaped bytes are NUL, space, tab, newline, carriage return,
vertical tab, form feed and backslash. Their escape letters are `0`, `s`,
`t`, `n`, `r`, `v`, `f` and backslash. Each follows a backslash.
Other bytes pass through unchanged.
No function requires UTF-8.

`defossilize` stops reading at the first source NUL or the slice end.
Unknown escapes discard the backslash. For example, `\q` decodes to `q`.
A terminal backslash survives.

The returned `DecodedToken.bytes` contains every emitted byte, including
bytes emitted after a decoded NUL. Its `c_string_len` is the first decoded
NUL index, or the output length. C-string consumers see only that prefix.


```rust
use gossil::token_codec::{defossilize, fossilize};
let encoded = fossilize(b"a b\0").unwrap();
let decoded = defossilize(&encoded).unwrap();
assert_eq!(decoded.bytes, b"a b\0");
assert_eq!(decoded.c_string_len, 3);
```

The implementation checks the exact encoded size and reserves output storage
before emitting bytes. `TokenError::Size` rejects arithmetic overflow or
lengths above Rust vector capacity. `TokenError::Allocation` retains the
allocator error. These errors are Rust resource policies. The C comparison
requires valid pointers, terminated C strings and explicit lengths within
allocated storage. It excludes signed C integer overflow and allocation exhaustion.

## Executed reference checks

The fixtures contain 538 native observations from a new characterization
harness, not an inherited Fossil test suite. They cover every byte, escape
suffixes, source NUL boundaries, explicit lengths and C-string lengths.
The native decoder returns `void`. Complete emitted bytes are derived from
the source algorithm and checked against native buffer mutation; their length
is not a native return value. Tests also compare the observed native `strlen`.
Stale bytes in the native backing buffer are not decoded output.

`tests/fixtures/token-codec/` preserves exact fixtures, the proposed reference
contract, source bodies, adapter, license and build provenance. `fixtures.json`
is the byte evidence used by the Rust tests. The contract's
`Vec` signatures describe the earlier proposal; the implemented APIs use `Result`.
All fifteen reference hashes and both pristine source hashes were verified
before import. The original hash manifest includes scratch programs and logs
that are not shipped here.

The hash manifest records the original experiment. It is not a
manifest of this directory. The adapter and source bodies are evidence only;
Rust does not invoke C. Compiler flags are recorded in `contract.json`.
`environment.json` identifies the compiler.

To verify a shipped input, compute
its SHA-256 and compare the matching entry in `hashes.json`. The source spans
and pristine source hashes are in `provenance.json`. Rebuilding the full
characterization requires the original scratch programs; this directory alone
does not supply them.

Run `just test --test token_codec` for the finite reference checks and
`just test token_codec::tests` for deterministic resource error checks.
The callable stub first failed an output assertion for `all256-explicit`;
the implementation then passed all cases. These focused checks do not establish
artifact parsing, storage integration or complete engine compatibility.

## Recommendations

Use `fossilize` for bounded byte slices and choose C-string behavior explicitly
when needed. Next, follow the [review workflow](development-plan.md#checkpoints-and-ownership),
including `just check`, before integrating this primitive with artifact parsing.
