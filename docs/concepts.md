# Library concepts

Gossil separates byte operations, artifact structure and local database ownership.
A byte operation does not create a repository, and a parsed artifact does not
establish its place in a repository graph.

For file selection, check-ins, branches and sharing, read
[working with repositories](working-with-repositories.md). This page describes
the library modules available to application authors.

## Files and issues in one operation

The intended product coordinates agent files and structured issue records in one
repository database. A local operation can commit a plan revision, its task
change and a private trace together. Its transaction includes canonical
artifacts, derived projections, scope, local command history and Gossil checkout
heads. A projection is stored state reconstructed from retained artifact records.

Git remains the external source authority; its commits are separate operations.
Checkout filesystem writes follow database commit through recoverable work.
This integrated workflow remains planned.

A historical view selects a permitted file check-in and
exact record boundaries for each issue or discussion. The record boundary
names the complete contributor set and projection version, with all dependencies
required by the claimed view. Contributors are immutable artifacts that determine
the record's state. The projection version identifies the reconstruction rules;
scope distinguishes shared access from an authorized owner's combined view.
Record-only changes create no file check-in, so a file hash
alone cannot identify issue history.

Later arrivals do not change the selected historical set. Access still depends on
permission and byte availability.

Typed links connect an issue to exact code versions and support reverse lookup
from code to issues. A Git target pins its registered repository, commit, path
and blob, with a versioned selector for a range. Captures can retain the
permitted bytes; an object ID alone does not guarantee their availability.

Shared history must be complete without private-only dependencies. Receiving it
does not automatically update a checkout or an owner's private issue selection.
Explicit update and publication enforce the relevant revision and scope checks.
A descriptor is a structured record naming a versioned link target and any
selected range. Its transferable encoding remains unfinished, so these guides
do not yet give a format that applications can exchange.

Fossil supplies immutable file and ticket history; Gest supplies task workflows.
Gossil's specified additions coordinate exact issue boundaries with shared/private
file heads and strengthen Gest's multi-statement helpers with one owned
transaction. Planned command undo validates complete inverse changes and rejects
stale dependencies; it retains artifacts and does not rewind Git.
Read [compare Gossil with Git](compare-with-git.md) for the plan/task example
and [working with repositories](working-with-repositories.md) for file selection.
The library modules below provide the implemented subset.

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
Its `initialize` constructor creates a repository without a checkout. The
[`vcs init` command](cli.md#initialize-a-repository) calls this constructor,
generates identities and a password, and selects a default administrator from
the environment. The API requires those values explicitly. Its `create`
constructor binds an existing catalog1 checkout directory. Catalog1 and catalog4
name storage formats; the [native storage guide](native-storage.md) defines their
creation and admission rules.

After `initialize`,
`register_checkout` can register one existing directory in catalog4.
`checkout_view` authenticates its ID; `checkout_view_for_root` authenticates its
exact root. These library methods change no working files and publish no locator.
The source [`vcs open` adapter](cli.md#open-a-native-checkout) adds locator
publication and recovery around those admitted database operations. Open does
not select additional files.

Generic private content writes require the bound catalog1 repository and refuse
catalog4. Catalog4 has a separate private recording API, described below. Its
shared root stays a check-in with no files.

Admission validates the stored root and authority before repository operations.
For catalog4, it also validates the complete supported tracking, scope and private
history state. `SharedView` restricts access to public content that passed this
validation. Read and write access to the database file through the operating
system establishes local owner authority.

The [native storage guide](native-storage.md) defines admission, content bounds
and access rules. General links between artifacts in the repository graph remain planned, so parsing
does not establish parent relationships or resolve references. Sync and team authentication also remain planned.

## Gest record values

Gest is a task and artifact tracker. These converters use its task, project and author record model.
The `gest_task_values` module validates eleven already decoded task fields.
It returns an owned record with a standardized status and UTC timestamps.
The [task values guide](gest-task-values.md) defines validation and caller responsibilities.

The `gest_project_values` module validates the four fields that make up the
canonical Gest project record,
`gest_id`, `archived_at`, `created_at` and `updated_at`.
It returns an owned record with an identifier, nullable archive time and UTC timestamps.
The [project values guide](gest-project-values.md) defines the complete field contract.
A local checkout root is outside the canonical project record.

The `gest_author_values` module validates six canonical author fields and returns
an owned record. It preserves names and nullable email exactly, normalizes author
types and converts timestamps to UTC. The [author values guide](gest-author-values.md)
defines this conversion separately from native actors and local principals.

The [task ticket emitter](task-ticket.md) produces bounded creation artifact bytes
from a validated task, a separate native ticket identity, actor bytes and a typed UTC date.
Before passing decoded task, project or author values to a converter, callers
must check raw field presence, duplicates and byte limits. Storage integration and task, project or author commands remain separate work.

## Explicit scope and private recording

Tracking and scope are separate choices. `add_files` enrolls paths without
staging their bytes. An absent scope row means unselected, not shared or private.
`select_path_scopes` persists an immutable `PathScope::Shared` or
`PathScope::Private` choice for an already tracked path. Repeating an equal
choice is a checked no-op; changing a choice is unsupported.

`commit_private_tracked_files` requires every tracked path to have private scope.
The caller supplies the complete path set and the latest `CheckoutView`.
The recorder captures current bytes and executable modes, so edits after add
need no second add. It writes one complete private check-in and updates the
combined head while leaving the empty shared head unchanged. The combined head
selects the owner's private tree together with the shared tree, which is empty
in this supported recording family.

The operation validates the full supported history and its derived rows.
A content budget bounds stored, decoded and retained bytes. A parser ledger
counts input bytes, cards, owned bytes and entries across parsed artifacts.
Both bound work across admission, capture, construction and final validation.

An unchanged tree returns `NoChange` without
a new receipt or changed head or counter. Working files remain unchanged.
Read [native storage](native-storage.md#private-recording-library) for request
metadata and limits. Shared recording, synchronization and integrated issue
operations remain planned.

The contracts above describe the source library. The new scope and commit command adapters
are deferred, and [final qualification](development-plan.md#private-recording-qualification)
is still pending.

## Recommendations

Choose the smallest module that fits the operation. Next, set resource limits
at every decoding or parsing boundary and read that module's error contract.
