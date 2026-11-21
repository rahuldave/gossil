# Rust library examples

These examples use the library in this checkout. The byte examples operate without
opening a database; the ancestry example uses an existing admitted repository. For full signatures and errors, browse the
[generated API reference](api/index.qmd).

## Add Gossil to an application

Keep a source checkout of Gossil and add these sections to your application's
root `Cargo.toml`. Replace `/absolute/path/to/gossil` with that checkout's path:

```toml
[dependencies]
gossil = { path = "/absolute/path/to/gossil" }
serde_json = "1"

[patch.crates-io]
libsql = { git = "https://github.com/rahuldave/gossil-libsql", rev = "c02db34ae8a7e91ca7ab8faee18fe41515626940" }
libsql-ffi = { path = "/absolute/path/to/gossil/vendor/libsql-ffi" }
```

Cargo does not inherit patches from dependencies. The application must select
both the corrected Rust wrapper and Gossil's bundled libSQL provider itself. This provider preserves the Fossil date
arithmetic used by artifact parsing and repository admission. The registry
provider alone does not establish that compatibility. Preserve the bundled
source and its [provenance record](https://github.com/rahuldave/gossil/blob/main/evidence/libsql-provider/provenance.json).

The [wrapper guide](libsql-wrapper.md) describes the exact TEXT behavior and source checks.
For a Cargo workspace, put both patches in the workspace root manifest. Save a
complete byte-operation example below as `src/main.rs` inside `fn main() { ... }`,
then run `cargo run`. The asynchronous storage example uses a separate runtime.

## Create and apply a delta

```rust
use gossil::delta::{Limits, apply, create};
let origin = b"before";
let target = b"after";
let delta = create(origin, target).unwrap();
let restored = apply(origin, &delta, Limits { max_output_bytes: 1024 }).unwrap();
assert_eq!(restored, target);
```

The limit caps decoded output. Handle `DeltaError` rather than unwrapping
results in an application. The [delta guides](delta-encoder.md) describe the
format and validation bounds.

## Pack bytes and compute a digest

```rust
use gossil::{compression, content_hash};
let packed = compression::pack(b"abc", -1).unwrap();
assert_eq!(compression::unpack(&packed, 3).unwrap(),
           compression::Unpacked::Bytes(b"abc".to_vec()));
assert_eq!(content_hash::sha1(b"abc").len(), 20);
```

The compression level `-1` selects the default. The unpack limit is three bytes.
See [byte primitives](blob-primitives.md) for empty inputs and digest choices.

## Escape a token

```rust
use gossil::token_codec::{fossilize, defossilize};
let escaped = fossilize(b"two words").unwrap();
let decoded = defossilize(&escaped).unwrap();
assert_eq!(decoded.bytes, b"two words");
```

Token escaping preserves the byte form described in the [token guide](token-codec.md).
Use the explicit C-string function when zero-byte truncation is required.

For asynchronous storage examples, see [local storage](local-storage.md).
The [artifact parser guide](artifact-parser.md) shows the date-provider boundary.

## Read complete recorded ancestry

Use an existing `LocalOwnerRepository` and explicit finite `AdmissionLimits`.
This helper propagates `StorageError` and returns every supported ancestor:

```rust
use gossil::storage::{AdmissionLimits, LocalOwnerRepository, RecordedCheckin, StorageError};

async fn recorded_history(
    owner: &mut LocalOwnerRepository,
    limits: AdmissionLimits,
) -> Result<Vec<RecordedCheckin>, StorageError> {
    owner.recorded_timeline("work", limits).await
}
```

Replace `work` with an existing checkout ID. Rows preserve native date, actor
and comment bytes, and full verified hashes. The current check-in comes first;
the shared empty root comes last. The shared empty root alone is a valid result before private
recording. A missing checkout or exhausted bound returns an error rather than
an empty or truncated history.

See [native storage](native-storage.md#read-recorded-checkout-ancestry)
for admission, transaction and ownership limits. To read the same ancestry from
the source CLI, use [timeline or log](cli.md#read-complete-recorded-ancestry)
with explicit repository and checkout selectors and an existing matching locator.
The CLI preserves metadata as JSON strings; the API fields remain byte vectors.

## Recommendations

Start with in-memory byte operations. Next, handle typed errors and choose
application limits before connecting these calls to stored or untrusted data.
