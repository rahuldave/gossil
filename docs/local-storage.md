# Local database ownership

[API: `LocalStore`](api/storage.qmd#LocalStore) gives the exact contract.

The `storage` module opens local libSQL files and owns one write transaction
at a time. Its low-level lifecycle API also supports the validated
[native storage boundary](native-storage.md). That boundary installs the complete
schema and implements empty-root initialization, one native directory registration,
explicit tracking and scopes, private file recording and bounded catalog1 content
operations. The [native storage guide](native-storage.md) defines the catalog formats.
Opening that higher-level repository validates its stored root and authority metadata.
Migration and broader adapters remain unimplemented.

Both `LocalOwnerRepository::create` and `LocalOwnerRepository::initialize` refuse existing `-journal`, `-wal` and `-shm`
entries beside the destination before creating the database. They preserve those
entries, including dangling symlinks and directories. Observation failures retain
the operating-system cause. The checks append literal native filename suffixes.
They observe the namespace before creation and again before SQLite entry.

Callers must keep that namespace stable through engine cleanup. The constructors
observe regular-file identity at separate boundaries. These observations do not
establish atomic binding to an engine handle. Checkout-free initialization retains
a failed main file if the engine might have opened it or COMMIT was attempted.
It reports cleanup uncertainty separately from the first operation error.

## Open and terminate a write

```rust
use gossil::storage::{LocalStore, StorageError};

async fn lifecycle(path: &std::path::Path) -> Result<(), StorageError> {
    let mut store = LocalStore::open(path).await?;
    store.begin_write().await?.commit().await?;
    store.begin_write().await?.rollback().await?;
    Ok(())
}
```

The example opens the database and terminates empty transactions.
Use `LocalOwnerRepository` for admitted native content operations. The example
above uses only the low-level lifecycle API.

`LocalStore::open(path)` builds a local database and retains one connection.
It treats nonempty paths as literal native filenames, including relative names
that begin with `file:` and names that contain `?`, `#` or `%`.
On Unix, it preserves filename bytes that are not valid UTF-8 when the filesystem
accepts them. A filesystem rejection remains an open error.
An embedded NUL returns an error with the original `std::ffi::NulError` cause.
An empty path retains SQLite's temporary-database behavior and fails the required
WAL check.

Every file open sets and reads back WAL, foreign keys ON, `synchronous=FULL`
and a 5000 ms busy timeout. WAL is SQLite's write-ahead journal mode.
An open fails if configuration differs from these requirements.

`begin_write(&mut self)` takes the connection before it issues `BEGIN IMMEDIATE`.
The returned `WriteTransaction`, the transaction owner, exclusively borrows the store.
A second owner cannot borrow it at the same time.
There is no public SQL, connection, statement or lazy query interface.

An internal execution error marks the owner rollback-only, which means later
internal operations cannot issue SQL and commit must reject the operation.
Queries materialize results by collecting rows into owned values before returning.
They consume their statements and rows inside the owner.
Preparation, stepping and value-decoding errors follow the same failure path.
The native content operations also latch non-SQL failures through the owner. Fixture SQL uses the same private execution and row-collection code as the
storage boundary.

Explicit commit and rollback return the connection only after SQLite reports
that no transaction remains active. A commit error triggers checked rollback
when the transaction remains active. Errors retain the original engine cause
and report a separate cleanup failure when one occurs.
If commit fails but checked rollback restores an idle connection, the store
can begin another write. The commit still returns its original error.

`commit_checked()` returns a `CommitFailure` with the original operation error
and an optional cleanup error. Both retain their typed source chains through
`StorageError::source()` when an underlying cause exists.
Inspect `operation` first and `cleanup` separately.
`commit()` keeps its existing `StorageError` result and stores cleanup diagnostics
as text.

A failed BEGIN or cleanup discards the connection and poisons the store.
A poisoned store requires an explicit new open.

## Drop and cancellation

Dropping a `WriteTransaction` without explicit termination poisons the store
and drops its sole connection.
The destructor executes no SQL and polls no future.
The pinned local libSQL connection uses SQLite's `close_v2` API.
SQLite rolls back an active transaction when the connection is destroyed.
This depends on the module keeping statements and other connection aliases
inside the owner.

Local SQL calls block their current asynchronous poll, including busy waits.
The cancellation fixture aborts at a controlled await between SQL calls and
awaits task destruction before checking writer progress.
Cancellation after a successful COMMIT leaves committed rows in the database.
It cannot undo a commit whose result the outer caller has not received.
A later server adapter must define its blocking scheduling and result-delivery
boundaries.

## Repository recording

`LocalStore` supplies transaction ownership. `LocalOwnerRepository` adds the
repository, checkout and history checks required by the
[private recording library](native-storage.md#private-recording-library).
Opening a `LocalStore` alone does not establish those prerequisites.

The recorder validates the complete supported history, captures every explicitly
private tracked file, and writes content, a manifest, its receipt and derived
rows in one transaction. Helpers borrow one content ledger and one parser ledger,
whose ceilings bound each file, artifact and row. Failure rolls back earlier
database writes, and recording never writes the working files.

The [qualification record](development-plan.md#private-recording-qualification)
distinguishes the checked library checkpoint from the CLI awaiting final gates.
It does not add public SQL access or change the lifecycle guarantees above.

## Recommendations

Use `LocalStore` to own a transaction, or `LocalOwnerRepository` for validated
repository operations. Prefer `commit_checked()` when a caller needs to inspect
both error causes. Next, consult the [generated Rust API](api/index.qmd)
for the chosen type's signatures and errors.
