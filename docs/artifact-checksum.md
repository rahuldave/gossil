# Artifact byte checksums

`artifact_checksum::md5` computes the sixteen MD5 digest bytes for a byte slice.
The [artifact parser](artifact-parser.md) uses this legacy checksum for its Z-card check. The Z card is the artifact checksum record.
It does not select repository content identities. SHA-1 and SHA3 remain in
`content_hash`. Structural parsing has its own explicit limits and validation rules.

`Md5Checksum::new` starts an owned computation. `update` appends the complete
slice, including zero bytes and bytes outside UTF-8. Empty updates do nothing.
`finish` consumes the computation and returns `[u8; 16]`. Each value has fixed
memory use and can be interleaved with other computations. No input copy,
file access or process-global state is required.

Slice lengths bound each call;
the caller chooses how much data to supply. There is no artifact size policy
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
This module supplies that checksum operation; it does not inspect a Z card.

The byte result maps to `md5sum_blob` in `src/md5.c`. The incremental operations
map to the positive-length and zero-length byte behavior of `md5sum_step_text`.
Rust ownership replaces the native shared context and overwritten static result.
The consuming `finish` requires a fresh value for another computation; it does
not reproduce the native implicit reset. Negative lengths and C string scanning
are excluded. `md5sum_step_blob` needs no separate Rust adapter because slices
already describe bytes and length.

Excluded native functions are `md5sum_current_state`, `md5sum_file` and
`md5sum_test`. `md5sum_current_state` sums words of the internal context for
troubleshooting; its two callers in `vfile.c` are commented out. It is not a
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

## Executed checks and limits

The three test functions in `tests/artifact_checksum.rs` first compiled
against a zero-result stub and failed digest assertions. They then passed with
the RustCrypto implementation, as did both module doctests. The seven standard vectors come from
[RFC 1321, Appendix A.5](https://www.rfc-editor.org/rfc/rfc1321).
The native corpus adds fourteen files with empty bytes, all byte values,
embedded zero and high bytes. Their lengths include 55, 56, 57, 63, 64, 65,
119, 120, 127, 128 and 129.
Each input is checked whole and in nine chunk sizes with empty updates.

A separate test interleaves two owned computations.
The fixture directory preserves the failing stub run in `red.log`.
`green.log.gz` stores the exact successful raw log in deterministic gzip form.
Its provenance checks the original uncompressed bytes; the archive is not a
live test log.

`tests/fixtures/artifact-checksum/native-captures.json` preserves each
`jlog run fossil md5sum` command, raw output, exit code, length and input SHA-256.
All native inputs were existing regular files; no native artifact parser ran.
`cases.json` holds the expected byte results and the `.bin` files retain inputs.
These fixtures are new checks, separate from the inherited Fossil Tcl suite.
No inherited suite acceptance or full-engine compatibility is claimed.

The same directory retains the native caller search and two AST match reports.
The search covered every file under the pinned `src` and `test` directories;
no MD5 text matched under `test`. Initial variadic AST patterns returned false
empty results. Explicit two-argument patterns found blob and text consumers.
The text search retains the other helper calls and commented diagnostic calls.
It does not establish coverage of those callers' behavior.

## Recommendations

Use this module for bounded byte checksums required by legacy artifacts.
Next, use the parser guide to select limits for structural artifact validation.
Keep repository content identity policy in its existing module.
