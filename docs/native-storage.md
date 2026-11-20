# Native local storage

[API: `LocalOwnerRepository`](api/storage.qmd#LocalOwnerRepository) gives the exact contract.

The storage API creates an empty native repository without a checkout. Its
bound-checkout constructor also supports private file bytes through one owned
libSQL transaction. Its supported public
graph is the initial empty check-in, which contains no files.
General artifact crosslinks, Gest
projections and filesystem recovery remain planned.

## Initialize without a checkout

`LocalOwnerRepository::initialize(path, InitializeOptions)` creates a native
repository without registering a checkout. Supply the project and local owner
identities, administrator, password, SQLite date expression, SHA1 selection and
finite content/parser limits. The API requires an explicit administrator.
It does not read environment variables or a preference database.

Initialization normalizes the date for the root D card. It installs the native
default report, system users and administrator password digest. SHA1 selection
sets the native hash policy and hashes the actual root with SHA1.
Otherwise the root uses SHA3-256. `initialization_identity()` returns the stored
project-code and server-code after checking admission.

This constructor installs catalog4 with `checkout_count=0`. Its only nonempty
`g_*` tables are `g_storage`, `g_local_owner` and `g_public_receipt`.
Reopen authenticates the single canonical empty root, its native hash, date,
event, branch tags and receive/index facts. It checks the exact schema,
complete policy, owner binding and foreign keys.

All 54 other local tables must be empty. Extra native content or indexed
history also refuses admission, even when checkout counters remain zero.
Registered, populated, typed and mixed catalog4 states remain unsupported.
Catalog5 and catalog6 remain unsupported. No upgrade occurs on open.
Private content writes through this handle also refuse catalog4.

The destination and its recovery neighbors must remain stable through engine
cleanup. Existing destinations and recovery entries are preserved.
Failures retain the first cause and report cleanup uncertainty separately.
A possibly opened engine or an attempted commit prevents automatic main-file
removal. Pathname observations do not certify engine disposal or atomic binding.
Templates, global preferences and project display fields are outside this API.

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

`AdmissionLimits` bound root content reconstruction and parser allocations.
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

## Admission validity

Admission is the validation required before repository operations.

A sealed admission proof records SQLite `data_version`, connection
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

## Schema and remaining work

Each catalog installs six SQL inputs under `src/storage/schema/`. Catalog4
substitutes `adapter-catalog4.sql` for the catalog1 adapter. The required object
inventory and complete stored definitions are
checked before mutation. Known optional schema families return an unsupported
configuration error. Unregistered persistent triggers receive a separate error.

Dynamic ticket templates, extension schemas and their registry transitions
remain unimplemented. General public receipts, publication, native and typed
rebuild, retained-dependency administration and filesystem journals also remain
unimplemented. The public APIs do not offer those operations.

## Recommendations

Prefer `initialize` for a repository without a checkout and `create` for an
explicit bound checkout. Use finite admission limits. Next, consult the
[generated API](api/index.qmd) for repository creation, admission and content methods.
