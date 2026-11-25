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
The library modules below form the implemented subset.

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
constructor creates catalog1 and binds an existing checkout directory. Catalog1 and catalog6
name storage formats; the [native storage guide](native-storage.md) defines their
creation and admission rules.

After `initialize`,
`register_checkout` can register one existing directory in catalog6.
`checkout_view` authenticates its ID; `checkout_view_for_root` authenticates its
exact root. These library methods change no working files and publish no locator.
The source [`vcs open` adapter](cli.md#open-a-native-checkout) adds locator
publication and recovery around those admitted database operations. Open does
not select additional files.

Generic private content writes require the bound catalog1 repository and refuse
catalog6. Catalog6 has a separate private recording API, described below. Its
shared root stays a check-in with no files.

Admission validates the stored root and authority before repository operations.
For catalog6, it also validates the complete supported tracking, scope and private
history state, and each private issue ticket against its artifact. `SharedView`
restricts access to public content that passed this validation. Read and write
access to the database file through the operating system establishes local owner
authority.

The [native storage guide](native-storage.md) defines admission, content bounds
and access rules. General links between artifacts in the repository graph remain planned, so parsing
does not establish parent relationships or resolve references. Sync and team authentication also remain planned.

## Gest record values

Gest is a task and artifact tracker. These converters use its record model for tasks, projects, authors, artifacts, iterations, tags, notes, relationships, entity tags and iteration tasks.
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

The `gest_artifact_values` module validates eight canonical artifact fields and
returns an owned record. A Gest artifact is a document such as a spec, not a
Fossil artifact. The module keeps the title and body exactly, and it accepts a null
or RFC3339 archive time. The [artifact values guide](gest-artifact-values.md)
defines the field contract.

The `gest_iteration_values` module validates nine canonical iteration fields and
returns an owned record with a lowercase status and UTC timestamps. A completed
or cancelled iteration requires a completion time, and an active one requires
null. The [iteration values guide](gest-iteration-values.md) defines the field contract.

The `gest_tag_values` module validates four canonical tag fields and returns an
owned record. It keeps the label exactly as given, including its case, and
converts timestamps to UTC. The caller checks that labels are unique across records. The
[tag values guide](gest-tag-values.md) defines the field contract.

The `gest_note_values` module validates seven canonical note fields. It keeps
the body exactly as given, keeps a null author as null and returns the entity
type in lowercase. The [note values guide](gest-note-values.md) defines the field contract.

The `gest_relationship_values` module validates eight canonical relationship
fields. It returns the relationship type and both entity types in lowercase,
and keeps the direction as given. The caller checks that each directed link is
unique, and it checks the inverse row. The [relationship values guide](gest-relationship-values.md) defines the field contract.

The `gest_entity_tag_values` and `gest_iteration_task_values` modules validate
the four canonical fields of each join record. A join record links two other
records and has no Gest ID of its own. The entity tag converter returns the
entity type in lowercase. The iteration task converter checks that the phase
is an unsigned 32-bit integer. The caller builds the key tuple of each join
record and owns each incarnation, which is the ticket of one attachment or
membership. The [entity tag values guide](gest-entity-tag-values.md) and the
[iteration task values guide](gest-iteration-task-values.md) define the field contracts.

The `gest_model` module is a copy of the Gest record types and of their primitive
values, such as `Id` and `TaskStatus`. The converters parse each identifier and
each kind of status or type with these primitives, so the converters and the
model accept the same text. A record that a converter has validated converts into
its model record through the `From` trait. The [Gest model guide](gest-model.md) describes the module.

The [task ticket emitter](task-ticket.md) produces bounded creation artifact bytes
from a validated task, a separate native ticket identity, actor bytes and a typed UTC date.
The [Gest ticket emitter](gest-ticket.md) takes the same inputs for a validated artifact
or project, and the three kinds share one emitter.
Before passing decoded values to a converter, callers
must check raw field presence, duplicates and byte limits. The [issue ticket writer](issue-ticket.md)
stores a task or project creation privately. The command `gos i init` stores a project this way, and
`gos i task create` stores a task.
The other commands for these records remain separate work.

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

The source [scope and commit commands](cli.md#select-scopes-and-record-private-files)
use these library contracts. They require the complete locator binding and explicit
repository, checkout and scope selections. Commit also requires all-tracked consent
and explicit actor, comment and date. [Final CLI qualification](development-plan.md#private-recording-qualification)
is still pending.

The library reader `recorded_timeline` returns the complete current-first ancestry
through the shared empty root. It authenticates all native history before reading
original metadata in the same transaction. It reads no working-file contents and
returns no partial result when a bound fails. See the
[recorded ancestry contract](native-storage.md#read-recorded-checkout-ancestry).
The source [timeline and log commands](cli.md#read-complete-recorded-ancestry)
print those rows as compact NDJSON after existing-locator admission and checked
API completion. Opening, the API and serialization have separate finite bounds.

## Glob lists and SQL expressions

The prepared `glob` module parses Fossil pattern lists and calls the selected
libSQL engine for matching. A result is the first matching one-based pattern
index, or zero. Its finite limits bound bytes and pattern counts, not engine
matching time or stack use. Matching selects no files and traverses no directories.

`glob_expr` builds SQL from a trusted expression and escaped patterns. Bind
untrusted candidate values through SQL parameters in a caller-owned connection;
never put those bytes in the expression argument. The storage API still exposes
no arbitrary SQL. The [glob guide](glob.md) describes the distinct SQL parser
advance and the current qualification limits.

## Recommendations

Choose the smallest module that fits the operation. Next, set resource limits
at every decoding or parsing boundary and read that module's error contract.
