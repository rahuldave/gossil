# Byte digests and packed blobs

[API: `pack`](api/compression.qmd#pack) gives the exact contract.

The library has Fossil byte digests and packed-blob operations. Packing
adds a capacity header to compressed bytes. These
primitives do not select a repository hash policy or store content. The runner
and these primitives share a crate. CLI integration remains separate work.

Legacy MD5 checksums have a separate [artifact checksum module](artifact-checksum.md).
The [artifact parser](artifact-parser.md) uses that module for Z-card validation.
The checksum module does not select repository content identities.

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
widths use FIPS-202 padding. The other widths preserve Fossil's extensions.

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
Output grows through fallible reservations. The caller sets the limit. The
format has no additional fixed small cap.

Encoding rejects lengths above the
unsigned 32-bit header range. C overflow and undefined behavior are not ported.

Inputs of at most four bytes produce `Unpacked::NoPayload`. A caller that replaces an existing buffer
must leave its existing buffer contents unchanged for that result. A successfully decoded empty
stream produces `Unpacked::Bytes` with an empty vector. These outcomes differ.
Fossil's diagnostic `test-uncompress` ignores the C function's error return, so
CLI exit zero does not prove successful decoding.

Allocation and compression errors retain their underlying causes. Digest
results have a fixed small size. Compressed input and output remain caller-owned.
The API does not define database, filesystem, publication or privacy behavior.

## Recommendations

Use the contracts above with explicit resource limits. Next, consult the
[generated Rust API](api/index.qmd) for the full signatures and errors.
