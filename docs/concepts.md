# Library concepts

Gossil separates byte operations, artifact structure and local database ownership.
A byte operation does not create a repository, and a parsed artifact does not
establish its place in a repository graph.

## Bytes and identities

The `delta` module creates and applies Fossil deltas. Application takes an
explicit output limit. The `compression` module packs and unpacks blobs, which
are stored byte sequences. Unpacking also takes a decoded size limit.

The `content_hash` module computes content digests. A digest identifies bytes.
It does not grant access to those bytes. The `artifact_checksum` module computes
legacy MD5 checksums for artifact records. It does not choose the repository's
content hash policy.

## Tokens and artifacts

The `token_codec` module escapes byte values for Fossil record tokens.
Its byte and C-string forms have different zero-byte behavior.
The [token guide](token-codec.md) defines those contracts.

The `artifact` module parses eight structural artifact types with explicit
limits. A caller supplies the date provider, which converts date fields.
Parsing checks structure and checksums. Repository graph validation requires
more storage rules. See the [parser guide](artifact-parser.md).

## Local storage and authority

`LocalStore` owns a local libSQL connection and one exclusive write transaction.
A transaction must commit or roll back before the connection returns to the
store. The public interface does not expose arbitrary SQL.

`LocalOwnerRepository` initializes a complete empty public root.
It also supports bounded private content operations. The empty root is a
check-in with no files. A private content write does not create a file-containing
check-in.

Admission validates the stored root before repository operations.
`SharedView` restricts access to admitted public content.
Operating-system access establishes local owner authority.

The [native storage guide](native-storage.md) defines admission, content bounds
and access rules. General crosslinks, sync and team authentication remain planned.

## Task values

The `gest_task_values` module validates eleven already decoded task fields.
It returns an owned record with canonical status and UTC timestamps.
The [task values guide](gest-task-values.md) defines validation and caller responsibilities.
Raw input admission, storage integration and task commands remain separate work.

## Recommendations

Choose the smallest module that fits the operation. Next, set resource limits
at every decoding or parsing boundary and read that module's error contract.
