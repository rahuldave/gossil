# Byte digests and packed blobs

The library provides Fossil byte digests and packed-blob operations. Packing
adds a capacity header to compressed bytes. These
primitives do not select a repository hash policy or store content. The runner
and these primitives share a crate. CLI integration remains separate work.

## Use the library

```rust
use gossil::{compression, content_hash};

let digest = content_hash::sha1(b"abc");
assert_eq!(digest.len(), 20);
let packed = compression::pack(b"abc", -1).unwrap();
assert_eq!(compression::unpack(&packed, 3).unwrap(),
           compression::Unpacked::Bytes(b"abc".to_vec()));
```

In the example, `-1` selects the default compression level, and `3` limits
decoded output to three bytes.

`sha1(input: &[u8])` returns `[u8; 20]`. `pack(input: &[u8], level: i32)`
returns `Result<Vec<u8>, BlobError>`. `unpack(input: &[u8], max_output: usize)`
returns `Result<Unpacked, BlobError>`.
See the [runner prerequisites](conformance.md#local-prerequisites) for actual tool setup.

## Source contract

The port follows Fossil Git revision
`b3a1c8efb2731dffbd5cc38c58ac349746de6da6`. The source surfaces are
`src/blob.c:1372–1535`, `src/sha1.c:185–201`, and
`src/sha3.c:385–468,591–606,622–679`, and the SHA1DC initialization in
`src/sha1hard.c:1327–1342`.

`content_hash::sha1` accepts arbitrary bytes and returns twenty digest bytes.
SHA1DC detects known SHA1 collision attacks.
It preserves Fossil's collision detection and safe-hash defaults. For a
known collision attack, it returns the mitigated digest. The byte helper ignores
the separate collision indication, as `sha1sum_blob` does. It does not return
the ordinary vulnerable digest for those inputs.

`content_hash::sha3` accepts each width from 128 through 512 in steps of 32.
It returns the corresponding binary digest, or `None` for an unsupported width.
`content_hash::hex` renders bytes as lowercase hexadecimal.

SHA3 uses Fossil's width-dependent capacity and its `0x06` and `0x80` padding.
The small adapter handles byte absorption, padding and extraction in explicit
little-endian lane order. The maintained `keccak` crate supplies the permutation.
A shorter digest is not a truncated standard-width hash. The four standard
widths use FIPS-202 padding; the other widths preserve Fossil's extensions.

`compression::pack` accepts zlib levels -1 and zero through nine. Level -1
selects the backend default. `compression::pack_two` compresses the concatenated
slices at level nine, as Fossil does. Neither operation allocates a concatenated
input buffer. The compressed representation can differ between backends.

A packed blob contains a four-byte unsigned big-endian capacity followed by a
zlib stream. The capacity is an upper bound on decoded bytes. A larger header
than the actual output is valid. Bytes following the first complete zlib stream
are ignored. Invalid checksums, incomplete streams and output exceeding the
header produce errors.

`compression::unpack` accepts a caller-selected limit. It rejects a header above
that limit before decoding, then checks each output chunk against the header.
Output grows through fallible reservations. The limit is caller policy; the
format has no additional fixed small cap. Encoding rejects lengths above the
unsigned 32-bit header range. C overflow and undefined behavior are not ported.

Inputs of at most four bytes produce `Unpacked::NoPayload`. A caller that replaces an existing buffer
must retain those bytes for that result. A successfully decoded empty
stream produces `Unpacked::Bytes` with an empty vector. These outcomes differ.
Fossil's diagnostic `test-uncompress` ignores the C function's error return, so
CLI exit zero does not prove successful decoding.

Allocation and compression errors retain their underlying causes. Digest
results have a fixed small size; compressed input and output remain caller-owned.
The API does not define database, filesystem, publication or privacy behavior.

## Executed checks

The portable corpus is `tests/fixtures/blob/reference.json`. Its generator runs
native reference commands through `jlog`. It records the executable version and
pinned source identity in separate fields. The source revision is a Git identity;
the executable reports Fossil version 2.29 with identifier `7e8ecf0ce4`.
That Git commit contains a `manifest.uuid` beginning with `7e8ecf0ce4`,
so the two identifiers name the same source snapshot.

The corpus includes all thirteen SHA3 widths at six
lengths around their input-block boundaries, a binary SHA1 vector, a known collision pair, and both
native compression modes, for one slice and for two concatenated slices. It also records oversized headers, trailing bytes,
short inputs and invalid data, including the native CLI's unchanged output.

`just blob-reference` regenerates the finite corpus. The ordinary tests consume
the committed bytes without a Fossil installation. They check native-to-Rust
decoding, digest equality, every compression level, chunk boundaries and errors.
`just blob-interop` explicitly runs the opt-in native decoding test against the
locally installed reference. It checks Rust-to-native decoding at all levels
and for the concatenation of both input slices. The ordinary suite reports that test as ignored.

Both interoperability directions passed locally on macOS. Exact compressed-byte
identity is not claimed. These experiments establish byte-level primitives;
they do not establish a complete content store or inherited-suite acceptance.

## Dependencies and integration

The lockfile selects `sha1collisiondetection` 0.3.4 with its `std` feature,
`keccak` 0.1.6 and `flate2` 1.1.11. The SHA1 crate is the maintained Rust port
of Marc Stevens and Dan Shumow's SHA1 collision-detection implementation.
`flate2` uses its default Rust backend, `miniz_oxide` 0.9.1. No native Fossil
process supplies candidate digest or compression behavior.

The two `sha-mbles` collision fixtures come from `sha1collisiondetection` 0.3.4.
Their upstream MIT license and attribution accompany them in
`tests/fixtures/blob/SHA1CD-LICENSE.txt`. Both native reference outputs differ
from the common vulnerable digest and from each other.

The composed library exports these modules with the delta encoder, decoder,
token codec and [local storage boundary](local-storage.md). A private fixture
persists packed source and delta bytes, restores the target and checks its hash.
It also checks rollback of a failed transaction. This fixture adds composition
coverage without changing the native byte evidence above.

See the [development workflow](development-plan.md#checkpoints-and-ownership)
for review of this composition. Installed tool updates remain a separate refresh
run by the coordinator. Source
changes and passing local checks do not establish a remote release.

## Recommendations

Use these library primitives with an explicit caller output limit.
Next, follow the [review workflow](development-plan.md#checkpoints-and-ownership),
including `just check`, for the composed core before adding domain storage. Keep the finite native observations separate
from the local storage fixture.
