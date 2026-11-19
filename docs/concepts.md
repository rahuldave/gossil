# Library concepts

Gossil separates byte operations, artifact structure and local database ownership.
A byte operation does not create a repository, and a parsed artifact does not
establish its place in a repository graph.

For file selection, check-ins, branches and sharing, read
[working with repositories](working-with-repositories.md). This page describes
the library modules available to application authors.

## Bytes and identities

The `delta` module creates and applies Fossil deltas. Application takes an
explicit output limit. The `compression` module packs and unpacks blobs, which
are stored byte sequences. Unpacking also takes a decoded size limit.

The `content_hash` module computes content digests. A digest identifies bytes
but does not grant access to them. The `artifact_checksum` module computes
legacy MD5 checksums for artifact records. It does not choose the repository's
content hash policy.

## Tokens and artifacts

The `token_codec` module escapes byte values for Fossil record tokens.
Its byte and C-string forms have different zero-byte behavior.
The [token guide](token-codec.md) defines those contracts.

The `artifact` module parses eight structural artifact types with explicit
limits and checks their structure and checksums. A caller implements `DateProvider`, whose `julianday` method converts date fields
to Julian day numbers. Repository graph validation requires
more storage rules. See the [parser guide](artifact-parser.md).

## Local storage and authority

`LocalStore` owns a local libSQL connection and one exclusive write transaction.
A transaction must commit or roll back before the connection returns to the
store. The public interface does not expose arbitrary SQL.

`LocalOwnerRepository` initializes a complete empty public root.
It also supports bounded private content operations. The empty root is a
check-in with no files. A private content write does not create a check-in that contains files.

Admission is the validation of the stored root required before repository
operations. `SharedView` restricts access to public content that passed this
validation. Read and write access to the database file through the operating
system establishes local owner authority.

The [native storage guide](native-storage.md) defines admission, content bounds
and access rules. General links between artifacts in the repository graph remain planned, so parsing
does not establish parent relationships or resolve references. Sync and team authentication also remain planned.

## Task and project values

Gest is a task and artifact tracker. These converters use its task and project record model.
The `gest_task_values` module validates eleven already decoded task fields.
It returns an owned record with a standardized status and UTC timestamps.
The [task values guide](gest-task-values.md) defines validation and caller responsibilities.

The `gest_project_values` module validates the four fields that make up the
canonical Gest project record,
`gest_id`, `archived_at`, `created_at` and `updated_at`.
It returns an owned record with an identifier, nullable archive time and UTC timestamps.
The [project values guide](gest-project-values.md) defines the complete field contract.
A local checkout root is outside the canonical project record.

The [task ticket emitter](task-ticket.md) produces bounded creation artifact bytes
from a validated task, a separate native ticket identity, actor bytes and a typed UTC date.
For task and project conversion, callers must check raw field presence, duplicates
and byte limits before passing
decoded values to a converter. Storage integration and task or project commands remain separate work.

## Recommendations

Choose the smallest module that fits the operation. Next, set resource limits
at every decoding or parsing boundary and read that module's error contract.
