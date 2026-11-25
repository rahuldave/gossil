# Native local storage

[API: `LocalOwnerRepository`](api/storage.qmd#LocalOwnerRepository) gives the exact contract.

The storage API creates an empty native repository without a checkout. After
registration, its catalog6 library can track paths, select explicit scopes and
record the complete private tracked tree through one owned libSQL transaction.
The shared history remains the initial empty check-in. General artifact
crosslinks, Gest projections and filesystem recovery remain planned.

Catalog names identify storage formats. Catalog1 binds a checkout at creation
and supports bounded private content bytes. Catalog6 starts without a checkout
and supports one later registration. Generic catalog1 content writes remain
separate from catalog6 recording.

## Initialize without a checkout

`LocalOwnerRepository::initialize(path, InitializeOptions)` creates a native
repository without registering a checkout. Supply the project and local owner
identities, administrator, password, SQLite date expression, SHA1 selection and
finite content/parser limits. The API requires an explicit administrator.
It does not read environment variables or a preference database. The
[`vcs init` CLI adapter](cli.md#initialize-a-repository) supplies generated
identities and a password, and chooses the administrator from options or the
environment. It uses this strict catalog6 contract; it does not bind a checkout.

Initialization normalizes the date for the root D card. It installs the native
default report, system users and administrator password digest. SHA1 selection
sets the native hash policy and hashes the actual root with SHA1.
Otherwise the root uses SHA3-256. `initialization_identity()` returns the stored
project-code and server-code after checking admission.

This constructor installs catalog6 with `checkout_count=0`. Its only nonempty
`g_*` tables are `g_storage`, `g_local_owner` and `g_public_receipt`.
Reopen authenticates the single canonical empty root, its native hash, date,
event, branch tags and receive/index facts. It checks the exact schema,
complete policy, owner binding and foreign keys.

The remaining local tables must be empty, except for the exact registration,
tracking, scope and private recording inventories described below. Extra native
content or indexed history outside that family refuses admission. Admission also
accepts private issue tickets written before, between or after check-ins. It checks each row of a
ticket against the artifact of that ticket, as the [issue ticket guide](issue-ticket.md) says.

No upgrade occurs on open. Open refuses a repository before any effect when its
catalog marker, the `storage_version` value of `g_storage`, names catalog2,
catalog4 or catalog5. The message names the catalog found and catalog6, and the CLI exits 78.
Gossil has no upgrade path before its first real release, so the fix is a new
repository. Open refuses an unknown marker, such as 3 or 7, and a missing marker
with exit 1.
Generic private content writes through this handle refuse catalog6.

The destination and its recovery neighbors must remain stable through engine
cleanup. Existing destinations and recovery entries are preserved.
Failures retain the first cause and report cleanup uncertainty separately.
A possibly opened engine or an attempted commit prevents automatic main-file
removal. Pathname observations do not certify engine disposal or atomic binding.
Templates, global preferences and project display fields are outside this API.

## Register one existing directory

After checkout-free initialization, call
`LocalOwnerRepository::register_checkout(checkout_id, root, limits)` to register
one existing directory against the initial empty root. This API supports only
catalog6 with no existing registration. It refuses catalog1 and a second
registration, including a retry with the same ID or an alias of the same root.

Supply a nonempty, NUL-free checkout ID of at most 1024 UTF-8 bytes. Supply an
existing directory with a nonempty, NUL-free UTF-8 path of at most 4096 bytes,
both before and after canonicalization. Both ceilings also use the smaller
`limits.parser.input_bytes`. Relative paths and caller aliases resolve to the
canonical directory.

Persisted roots must retain their exact canonical spelling,
device, inode and owner binding on every admission. Canonical components are
observed without following links. Callers must keep the namespace stable.
These pathname observations do not protect against hostile concurrent replacement.

Registration uses one exclusive database transaction. It checks the saved
admission proof and complete native state before writing one checkout row, one
owner row and `checkout_count=1`. It writes exactly the three native keys
`checkout`, `checkout-hash` and `fingerprint`. These keys retain the actual initial root RID,
its hash and the native fingerprint derived from the receive record. The API
validates the complete result and captures its proof before checked commit.
It replaces the saved proof only after known success.

Earlier writes roll back on failure. The lifecycle rules still govern commit
and cleanup uncertainty.

The returned `CheckoutView` owns the repository ID, checkout ID, canonical root,
shared head, combined head, generation and pending version.
The combined head selects the owner's private tree together with the shared
tree, which is empty in this recording family.
At registration, the shared head is the public initial root, the combined head
is absent, and both counters are zero. A later private recording advances
the combined head while the shared head stays fixed.

`checkout_view(checkout_id, limits)` authenticates
that exact persisted registration. `checkout_view_for_root(root, limits)`
authenticates the sole registration for the explicitly chosen canonical root.
Both methods revalidate complete native state and refuse stale authority,
invalid bindings or unsupported inventory. Root lookup refuses absence, another
root, unauthorized ownership and multiple registrations. It allocates no identity
and repairs no registration or native key.

Ordinary files and nested Git repositories in the directory remain unselected
and unchanged. Registration creates no file check-in, materializes no files and
publishes no `ROOT/.gossil/checkout.json` locator. These methods require no Gest
project, issue or author initialization. The source
[`vcs open` adapter](cli.md#open-a-native-checkout) composes these library methods
with locator publication and recovery. Its explicit flags still use the same
admission and publisher rules. Root lookup itself only gives authenticated
facts and does not recover a locator.

Reopen accepts exactly zero or one such catalog6 registration. It independently
checks the complete checkout, owner and key inventory against `checkout_count`,
the native root and the supported recording history. Generic private content
writes still refuse catalog6.

## Track native pending additions

This source checkout exposes `LocalOwnerRepository::add_files` and
`LocalOwnerRepository::pending_adds`, with `TrackingLimits` and `PendingAdd`.

Call `add_files(checkout, paths, wanted, limits, tracking)` with the exact
`CheckoutView` from this owner as `wanted`, plus finite admission and tracking
limits. The `wanted` view states the complete checkout state that the caller
expects before mutation.
The method validates the whole batch and cumulative inventory, reads and
rechecks each regular file, then returns the committed view. The method rejects the batch if the heads,
generation, pending version or admission authority are stale.
Use the returned view for the next mutation.

`pending_adds(checkout, limits, tracking)` returns the complete bounded pending
inventory in pathname order. `tracked_paths` lists both pending and recorded
paths, and `path_scopes` reads only explicit scope choices.
`PendingAdd::executable` records the mode at first enrollment. Later mode changes
do not alter that flag until recording captures the current file.

Only this producer's exact catalog6 state is admitted. New `g_vfile` rows use
the current combined-head `vid`, or the initial root before recording. They use
`rid=mrid=0`, regular-file link flags, executable mode
and observed modification time. A RID is a native artifact row identifier.
New batches retain earlier rows and advance
`pending_version` once. Existing-only batches recheck files but change no state.

`g_pending_path` remains empty. New paths have no `g_path_scope` row, and add
preserves earlier explicit choices. Catalog1 rules remain unchanged.

No file bytes, canonical artifacts, receipts or file-containing check-ins are
stored by add. Working bytes remain unchanged.

`TrackingLimits` bounds the complete inventory and batch count, bytes per path,
path components, initial captured working bytes and read buffer size.
The final recheck temporarily retains another copy of one captured file.
The captured-byte limit therefore does not bound total process memory.

The methods reject zero structural or buffer bounds. Capture, recapture and
comparison also charge the operation's borrowed content budget, including retained
parent-directory checks. The cumulative work bounds do not measure
all process allocations. Captured bytes exist only for transaction validation.

Paths and descriptor identities
must satisfy the [CLI path contract](cli.md#add-explicit-paths-and-inspect-pending-additions).
The protected database identities come from the store's retained private
`FileIdentity`; callers cannot supply replacement authority.

New additions have unselected scope. An absent scope row means neither shared
nor private. Add alone authorizes no recording or sync.

## Private recording library

Call `initialize`, `register_checkout`, `add_files`, `select_path_scopes`, then
`commit_private_tracked_files`. Registration, add, scope selection and changed
recording return checkout views. Pass the most recent returned view as the
expected view and set `request.scope` to `PathScope::Private`.
The [storage API](api/storage.qmd#LocalOwnerRepository) gives the exact signatures.
Supply finite `AdmissionLimits` for content and parsing, plus `TrackingLimits`
for path counts, path sizes and retained working bytes.

Private recording saves the current bytes and executable modes of every tracked
file as one complete private history snapshot. Scope is each path's explicit
shared or private choice. Private means owner-local history; shared marks a path
for shared history, whose recording and synchronization are not supported here.
Every tracked path must have private scope before this recorder can run.

The library checkpoint has passed its recorded checks; the CLI checkpoint still
needs final qualification. The examples describe source contracts and establish
no release or installation.
See [development status](development-plan.md#private-recording-qualification) for
the accepted tracking baseline and remaining work.

`select_path_scopes(checkout, wanted, selections, limits, tracking)` persists
immutable explicit choices for tracked paths. An equal batch is a checked no-op;
a conflicting choice rejects the whole batch. A batch with new choices advances
`pending_version` once. `path_scopes` reads only explicit choices; `tracked_paths`
returns the complete ordered tracked inventory. Neither read infers missing scopes.

`commit_private_tracked_files(checkout, request, limits, tracking)` requires
`PathScope::Private` and a nonempty, duplicate-free set of all tracked paths.
It rejects an empty path set, including when the checkout has no tracked paths.
Each path must have a persisted private choice. `request.expected` supplies
all seven `CheckoutView` fields. They are repository ID, checkout ID, registered root,
shared head, combined head, generation and pending version. Stale expectations
or failed owner validation cause the transaction to reject the request.

Catalog1 stores loose private content bytes without a file tree. Catalog6
recording stores a complete file tree and its admitted history.

The request supplies actor, comment and native date bytes. The actor is nonempty
UTF-8 without NUL. The comment is UTF-8 with no NUL or literal left bracket.
Fossil's comment format uses left brackets to start links. Excluding them keeps
the backlink table outside this bounded family. No alternative escape for a literal left bracket is supported.

The date must use the [native date grammar](task-ticket.md#date-domain) and
be strictly later than every parent, including under native SQL date conversion. The recorder captures all working files again, retaining their bytes and
executable modes within finite limits. Edits
after add are included without a second add. Capture and parser helpers borrow
one content budget and one parser work ledger, which count cumulative work
across the whole operation.

A request explicitly supplies all tracked paths and the current view:

```rust
use gossil::storage::{CheckoutView, PathScope, PrivateRecordingRequest};

fn request<'a>(
    view: &'a CheckoutView,
    paths: &'a [&'a str], // Every tracked path, not a selected subset.
) -> PrivateRecordingRequest<'a> {
    PrivateRecordingRequest {
        expected: view,
        paths,
        scope: PathScope::Private,
        actor: b"Owner",
        comment: b"Save notes",
        date: b"2026-11-20T12:00:01",
    }
}
```

Select private scope first. Use a date later than the root and previous private
recording. Pass the request to `commit_private_tracked_files` with explicit limits.

Call the initial shared empty root S and private recordings L1, L2 and so on.
For example, recording edited notes creates L2 while S stays the shared view.
The shared empty root S remains immutable. The first private manifest L1 has
parent S; later manifests have parents Lprev then S. Every private manifest
contains the complete tree.

Keeping S as a parent preserves the explicit shared base while the first parent
follows private history. Admission validates the full history and exact native
and local projections, including private content, parent, event, leaf,
file-link and checkout facts. Later additions use the current combined-head RID
as their `vid`.

A projection is a derived database row. Admission rejects a file link whose
content hash differs from the hash in its manifest.
Catalog6 recording accepts full compressed content and refuses physical deltas.
Catalog1 retains its separate delta reconstruction contract.

Content that already
exists is verified and reused without rewriting its receipt or privacy class.
General removal and rename remain unsupported.

`Recorded.native_name` is the new manifest/check-in hash. `Recorded` also
returns the refreshed view after checked commit.
`NoChange` validates metadata, dates, bytes and modes but creates no receipt
and changes no head or counter.

Changed recording writes one local receipt, payloads, manifest and projections
atomically. It advances both generation and pending version once. Working files remain unchanged.
Shared recording and synchronization remain unsupported.

Catalog1 content
operations keep their separate contract. The source CLI exposes
[scope selection and private recording](cli.md#select-scopes-and-record-private-files)
and inspects recorded states through [status](cli.md#add-explicit-paths-and-inspect-pending-additions).
A failed write of the CLI receipt to standard output leaves the recorded history durable.

## Local owner and shared view

`LocalOwnerRepository::create` installs catalog1, exclusively creates the database file and binds
its repository, owner and checkout identities. The initial root contains native
C, D, R, T, U and Z records. The parser validates those bytes with date conversion
on the same owned SQL connection.

Creation installs all six accepted schema files, native markers, compressed
root content, native crosslinks, owner, checkout and public receipt together.
The final complete-policy marker follows those writes. A failed creation
rolls back the schema writes.

Root construction computes the encoded user length and complete canonical
length before allocating either output. The size calculation and encoder use
the same existing token escape rules.
Private decoding boundaries check fully stepped native date values and owned
catalog fields. A failure rolls back earlier writes in the transaction.
Metadata errors preserve the underlying operating-system cause.

`LocalOwnerRepository::open` requires an existing readable and writable regular
file. Its explicit `AdmissionLimits` bound root reconstruction and parsing.
For the bound catalog1 constructor, admission checks canonical bytes, native
branch tags and event, the public receipt, checkout bindings, complete policy
and native versions.
The receipt must identify the root itself with validation class
`local-explicit-publication`. The event fields and both branch tags must match
the canonical root, including absent event overrides.

Admission rejects native or valid typed projections that claim the empty root
produces another artifact type or requires a graph dependency. It also rejects
current file, merge, path-selection and journal rows outside the empty checkout.
A producer identifies the artifact that creates a fact. A target identifies
what a fact refers to. Admission distinguishes those roles rather than rejecting
every row that mentions the root.

In catalog1, operational and provenance rows, invalid diagnostic records and
historical selection records can remain. Unrelated private blobs and physical deltas can
remain, including a private delta whose baseline is the public root. These rows
do not extend the supported public graph or implement their corresponding
product operations.

`AdmissionLimits` bound content reconstruction and parser work. Catalog1 admission
validates its root; catalog6 admission also checks the complete supported history
and charges retained metadata to the same operation budget.
Fixed scalar conflict queries bound the data they return. Neither mechanism
sets a ceiling on SQLite scan work across stored tables.

Operating-system access establishes the local owner boundary. A stored owner
ID is a binding, and callers cannot turn an ID string into permission.
`shared_view()` returns a restricted local view with no owner operations.
Network authentication and team principals remain unimplemented.

The shared view reads the initial public root only. An absent name and a
private file name return the same unavailable error. A private association with
already public root bytes preserves their public classification. A receipt for
an unsupported general graph cannot grant shared access.

## Check admission before operations

Admission is the validation required before repository operations. A sealed
admission proof records SQLite `data_version`, connection
`total_changes` and exact durable authority metadata. Each content operation
checks that proof under its writer reservation before a private lookup.
The saved metadata includes the root-conflict predicates. Refreshing only the
connection counter cannot accept a newly contradictory projection.

An unrelated external commit conservatively requires explicit readmission.
Unexpected writes on the same connection also require readmission.

In catalog1, supported private file operations refresh only `total_changes` after checked
commit or rollback. They preserve the admitted root and policy, and they never
advance the saved `data_version`. Future root or policy mutation APIs must
invalidate or replace the proof after full validation.

Content operations do not decode the root through a hidden admission budget.
They check durable authority and charge requested content to caller limits.
Creation and open use their separate, explicit admission limits.

## Content bounds and rollback

For a bound catalog1 repository, use `put_private_file_content` to store private
bytes. Use `read_content` to retrieve content through `LocalOwnerRepository`.
Every content read or write receives finite `Limits`. Those limits bound each
stored envelope, decoded output, chain depth and aggregate stored and decoded
work. Reconstruction checks every intermediate native size and full hash.

A RID is a native artifact row identifier. A visited-RID set rejects cycles.
A phantom is an artifact whose content is missing. Shunned artifacts are excluded
by repository policy. Missing, phantom and shunned baselines reject
successful reconstruction.

The four-byte big-endian envelope retains native codec behavior. Excess
capacity and bytes after the first complete zlib stream are accepted. Exact
repository size and hash still have to match. A short `NoPayload` result cannot
stand in for valid empty content.

`compression::pack_bounded` uses the existing encoder and checks the full
stored length before each output reservation. Its ceiling includes the header.
The encoder also uses a fixed 8192-byte working buffer and backend state.
The ceiling is not a bound on every allocation in the process.

A hash, policy, codec, schema or resource error makes the borrowed owner
rollback-only. Catching the error cannot permit later SQL or successful commit.
Earlier writes in that transaction also roll back.

Full bytes can replace a phantom or a delta target whose old baseline is
missing. Replacement removes the stale delta row and clears phantom state.
Typed dependents are records in the adapter projections that require the content.
Required orphan, delta or typed dependents cause an explicit refusal until
their native and typed crosslinks are implemented.

## Read recorded checkout ancestry

`LocalOwnerRepository::recorded_timeline(checkout, limits)` returns a complete
`Vec<RecordedCheckin>` for an explicit existing catalog6 checkout. Each row has
`hash: String`, `date: Vec<u8>`, `actor: Vec<u8>`, `comment: Vec<u8>` and
`private: bool`. The generated [Rust API](api/index.qmd) gives the exact signature.

Results start with the current private check-in and follow its ancestors to the
immutable shared empty root S. The root occurs exactly once, last, with
`private == false`; every private row has `private == true`. Before any private
recording, S is the sole row. The reader follows ancestry rather than sorting
rows by date. Accepted recordings have strictly increasing dates; equal or
backward dates still refuse admission.

Hashes are full verified native identities. Dates preserve native D-card bytes;
actor and comment preserve the decoded native U-card and C-card bytes exactly.
The supported metadata is UTF-8 without NUL, but these public fields remain byte
vectors. The reader does not normalize dates, abbreviate hashes or use edited
display metadata. Complete native equality admission first authenticates the
original event user and comment. Only then does the reader get their owned
bytes in the same transaction.

One content budget and one parser ledger cover freshness, complete repository
admission, metadata queries and result retention. The complete result capacity
is charged before allocation. Full admission retains every existing authority,
provider, foreign-key, registration, native-root and history projection check.
Existing admission callers keep their checks and accounting unchanged.

Success returns the entire ancestry only after checked transaction completion.
There is no truncation or successful empty history. Missing checkout, stale
authority, unsupported or corrupt state, invalid or exhausted limits, allocation,
provider and transaction failures return `StorageError`. The first operation
cause and separate cleanup failure retain their existing precedence.

This operation reads no working-file contents, makes no durable mutation and
publishes no locator. Pending additions and dirty or deleted working files do
not change its recorded result.

The source [timeline and log commands](cli.md#read-complete-recorded-ancestry)
expose this reader as complete compact NDJSON. Existing-locator opening admission
is separate from the API operation. Serialization has a third finite allowance
derived from the unchanged CLI decoded-content limit. No whole-invocation
aggregate budget is claimed. Revision selection, filters and broader Fossil
timeline behavior remain deferred.

## Schema and remaining work

Each catalog installs six SQL inputs under `src/storage/schema/`. Catalog6
substitutes `adapter-catalog6.sql` for the catalog1 adapter. That file adds the
local audit tables `transactions`, `transaction_events` and `g_command_audit`,
which stay empty in this build. Initialization also stores the SQL of the
installed ticket tables in the native `ticket-table` setting. A stock
`fossil rebuild` recreates the ticket tables from that setting, so they keep
their Gest columns.

`just catalog-interop` checks a new repository with the pinned Fossil reference.
It runs `fossil test-integrity`, then `fossil rebuild` on a copy, and compares the
ticket columns. It then checks a repository with private check-ins and an issue
between them, with `fossil test-integrity`, `fossil timeline` and `fossil ticket show`.
When the reference is absent, it exits 5, the status for not-run.

The [issue ticket writer](issue-ticket.md) stores one private task or project
ticket in a catalog 6 repository. `open` admits a repository that holds such
tickets beside private check-ins.

The required object
inventory and complete stored definitions are
checked before mutation. Known optional schema families return an unsupported
configuration error. Unregistered persistent triggers receive a separate error.

Dynamic ticket templates, extension schemas and their registry transitions
remain unimplemented. General public receipts, publication, native and typed
rebuild, retained-dependency administration and filesystem journals also remain
unimplemented. The public APIs do not offer those operations.
The internal modules for [strict local records](local-records.md) are the base
for the record readers of catalog 6, which are not built yet.

## Recommendations

Prefer `initialize`, then `register_checkout` when one native empty-root
registration is needed. Use `create` for catalog1 private content operations. For private recording,
select scopes explicitly and pass the latest complete view and every tracked
path to the library recorder. Set finite admission limits, then consult the
[generated API](api/index.qmd) for repository creation, admission and content methods.
