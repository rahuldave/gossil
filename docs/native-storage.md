# Native local storage

The storage candidate creates a complete empty native repository and stores
private file bytes through one owned libsql transaction. Its supported public
graph is the initial empty check-in, which contains no files.
General artifact crosslinks, Gest
projections and filesystem recovery remain outside this candidate.

The composed implementation remains subject to its ordered acceptance gates.
Focused results in `evidence/native-storage/` describe the source tests. The
composed run in `evidence/storage-provider-integration/report.json` records
69 unit tests, 62 integration tests and one explicit pre-existing native interop
ignore. Coverage records 2,914 executable changed lines with none missing,
including full line coverage for all seven storage-owned Rust files.
These results do not qualify the full accepted storage specification.

This checkpoint adopts parser and storage source on the accepted provider base;
the separate parser branch is not an ancestor of this checkpoint. Parser
acceptance is recorded independently at commit
`36cecd622510ce616a2524145ce887667e79ba2a`. The composed storage gates remain
open. There is no partial-policy product mode.

## Local owner and shared view

`LocalOwnerRepository::create` exclusively creates the database file and binds
its repository, owner and checkout identities. The initial root contains native
C, D, R, T, U and Z records. The parser validates those bytes with date conversion
on the same owned SQL connection.

Creation installs all six accepted schema files, native markers, compressed
root content, native crosslinks, owner, checkout and public receipt together.
The final complete-policy marker follows those writes. Phase-failure tests
reopen the database and find no remaining user schema after rollback.

Root construction computes the encoded user length and complete canonical
length before allocating either output. The size calculation and encoder use
the same existing token escape rules. An allocation-sensitive regression first
observed a 512-byte request with a 256-byte ceiling, then passed after correction.
Exact-fit and one-byte-under tests also check the complete root bound.

Private decoding boundaries check fully stepped native date values and owned
catalog fields. Tests pair selected-engine values with malformed typed results
and verify that a caught failure rolls back an earlier write. Metadata error
tests preserve the underlying operating-system cause.

`LocalOwnerRepository::open` requires an existing readable and writable regular
file. Its explicit `AdmissionLimits` bound root reconstruction and parsing.
Admission checks canonical bytes, native branch tags and event, the public
receipt, checkout bindings, complete policy and native versions.
The receipt must identify the root itself with validation class
`local-explicit-publication`. The event fields and both branch tags must match
the canonical root, including absent event overrides.

Admission rejects native or valid typed projections that claim the empty root
produces another artifact type or requires a graph dependency. It also rejects
current file, merge, path-selection and journal rows outside the empty checkout.
A producer identifies the artifact that creates a fact. A target identifies
what a fact refers to. Admission distinguishes those roles rather than rejecting
every row that mentions the root.

Operational and provenance rows, invalid diagnostic records and historical
selection records can remain. Unrelated private blobs and physical deltas can
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

Supported private file operations refresh only `total_changes` after checked
commit or rollback. They preserve the admitted root and policy, and they never
advance the saved `data_version`. Future root or policy mutation APIs must
invalidate or replace the proof after full validation.

Content operations do not decode the root through a hidden admission budget.
They check durable authority and charge requested content to caller limits.
Creation and open use their separate, explicit admission limits.

## Content bounds and rollback

Use `put_private_file_content` to store private bytes and `read_content` to
retrieve them through `LocalOwnerRepository`. Every content read or write receives finite `Limits`. Those limits bound each
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
The tests include a valid write followed by a caught hash failure, then reopen
the database to verify that the valid write also disappeared.

Full bytes can replace a phantom or a delta target whose old baseline is
missing. Replacement removes the stale delta row and clears phantom state.
Typed dependents are records in the adapter projections that require the content.
Required orphan, delta or typed dependents cause an explicit refusal until
their native and typed crosslinks are implemented.

## Schema and remaining work

The six SQL files under `src/storage/schema/` match the accepted evidence
copies. The required object inventory and complete stored definitions are
checked before mutation. Known optional schema families return an unsupported
configuration error. Unregistered persistent triggers receive a separate error.

Dynamic ticket templates, extension schemas and their registry transitions
remain unimplemented. General public receipts, publication, native and typed
rebuild, retained-dependency administration and filesystem journals also remain
unimplemented. The public APIs do not offer those operations.

The test inventory records each executed boundary separately. A source mapping
or an installed schema is not evidence that its mapped behavior passed.
The original hash-latch failure, packing-bound failure and setup failures remain
in the evidence directory beside later results.

## Recommendations

Use explicit local admission and finite content limits for this bounded API.
Next, finish the composed acceptance gates, then implement the first private
file child with real parent and file links under the accepted companion
`private-file-amendment.md` specification. That amendment, its catalog changes
and journal operations remain future implementation work.
File-containing check-ins, traversal and persistent selection commands, issue adapters, UI, sync, publication,
general graphs, migration, rebuild and filesystem recovery remain unimplemented.
