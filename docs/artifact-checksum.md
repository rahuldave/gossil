# Artifact byte checksums

[API: `md5`](api/artifact_checksum.qmd#md5) gives the exact contract.

`artifact_checksum::md5` computes the sixteen MD5 digest bytes for a byte slice.
The [artifact parser](artifact-parser.md) uses this legacy checksum for its Z-card check. The Z card is the artifact checksum record.
It does not select repository content identities. SHA-1 and SHA3 remain in
`content_hash`. Structural parsing has its own explicit limits and validation rules.

`Md5Checksum::new` starts an owned computation. `update` appends the complete
slice, including zero bytes and bytes outside UTF-8. Empty updates do nothing.
`finish` consumes the computation and returns `[u8; 16]`. Each value has fixed
memory use and can be interleaved with other computations. No input copy,
file access or process-global state is required.

Slice lengths bound each call.
The caller chooses how much data to supply. There is no artifact size policy
or cancellation mechanism in this byte primitive.

```rust
use gossil::artifact_checksum::{md5, Md5Checksum};
let mut checksum = Md5Checksum::new();
checksum.update(b"a");
checksum.update(b"bc");
assert_eq!(checksum.finish(), md5(b"abc"));
```

## Source contract

The inspected Fossil mirror is `b3a1c8efb2731dffbd5cc38c58ac349746de6da6`.
The native build reports `7e8ecf0ce4a3c05aa39de7ffe2828d23e334efe446ef2387e09f205ef00b2227`.
In `src/manifest.c`, `verify_z_card` hashes the bytes before the final
35-byte Z card through `md5sum_init`, `md5sum_step_text` and `md5sum_finish`.
This module supplies that checksum operation. It does not inspect a Z card.

The byte result maps to `md5sum_blob` in `src/md5.c`. The incremental operations
map to the positive-length and zero-length byte behavior of `md5sum_step_text`.
Rust ownership replaces the native shared context and overwritten static result.

The consuming `finish` requires a fresh value for another computation. It does
not reproduce the native implicit reset. Negative lengths and C string scanning
are excluded. `md5sum_step_blob` needs no separate Rust adapter because slices
already describe bytes and length.

Excluded native functions are `md5sum_current_state`, `md5sum_file` and
`md5sum_test`. `md5sum_current_state` sums words of the internal context for
troubleshooting. Its two callers in `vfile.c` are commented out. It is not a
snapshot of the MD5 digest. The native command and filesystem behaviors,
repository R-card aggregation, ETags and all artifact producers remain outside
this module. Native `MD5Init`, `MD5Update`, `MD5Final`, `MD5Transform` and
`DigestToBase16` are not translated into Rust here.

The implementation uses RustCrypto's exact-pinned `md-5` version `0.10.6`.
This version shares the existing `digest` 0.10 dependency with SHA-2.
The lockfile records its registry checksum. Its [versioned documentation](https://docs.rs/md-5/0.10.6/md5/)
describes the API used here. RustCrypto's [current documentation](https://docs.rs/md-5/0.11.0/md5/)
identifies legacy interoperability as the purpose of MD5. This module adds no
identity or security policy and forwards no computation to C.

## Recommendations

Use the contracts above with explicit resource limits. Next, consult the
[generated Rust API](api/index.qmd) for the full signatures and errors.
