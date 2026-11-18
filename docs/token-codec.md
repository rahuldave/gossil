# Fossil token bytes

[API: `fossilize`](api/token_codec.qmd#fossilize) gives the exact contract.

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

## Recommendations

Use the contracts above with explicit resource limits. Next, consult the
[generated Rust API](api/index.qmd) for the full signatures and errors.
