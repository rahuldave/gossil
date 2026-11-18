# Local database ownership

The `storage` module opens local libSQL files and owns one write transaction
at a time. It supplies a storage boundary for later domain operations.
The current crate has no domain tables, migrations or storage adapters.

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
There are no public domain mutations yet.

`LocalStore::open(path)` builds a local database and retains one connection.
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
Fixed domain operations and orchestration of non-SQL domain errors remain
later work. Fixture SQL uses the same private execution and row-collection code as the
storage boundary.

Explicit commit and rollback return the connection only after SQLite reports
that no transaction remains active. A commit error triggers checked rollback
when the transaction remains active. Errors retain the original engine cause
and report a separate cleanup failure when one occurs.
If commit fails but checked rollback restores an idle connection, the store
can begin another write. The commit still returns its original error.

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

## Fixture scope and source basis

The dependency is pinned to `libsql = 0.9.30` with only its `core` feature.
The lockfile resolves `libsql-sys` and `libsql-ffi` to 0.9.30.
The executed macOS fixture reports SQLite 3.45.1 and this source identity:

```text
2024-01-30 16:01:20 e876e51a0ed5c5b3126f52e532044363a014bc594cfefa87ffb5b82257ccalt1
```

The implementation follows the registry sources for libSQL's local connection,
statement finalization and synchronous local calls.
It avoids `libsql::Transaction`, whose local destructor calls rollback with
`unwrap()` in this pinned version.
[SQLite's close contract](https://www.sqlite.org/c3ref/close.html) describes
connection destruction and rollback.

The private memory fixture checks transaction behavior in memory. It retains one connection and verifies memory journal
mode instead of WAL. An unpolled terminal future is a commit or rollback future
that the caller creates but never polls. The terminal method consumes the
transaction owner into that future. Dropping the future therefore drops its
owner, releases the writer and poisons the store. Open a new store before reuse.

Run the focused fixtures with:

```sh
just test --lib storage::tests -- --nocapture
just core-composition
just doctests
```

The fixtures cover settings on each open, JSON, owned BLOBs, unique and foreign
key constraints, integrity checks, persistence, explicit rollback and caught
operation errors. They also cover deferred foreign key failure at commit,
SQLite's automatic rollback, competing writers on OS threads and a WAL reader.
Drop, unpolled terminal futures and controlled cancellation must release the
writer so an independent connection can proceed.
The controlled await also has a non-cancelled control that resumes and terminates
the transaction explicitly.

A private test hook injects failures in transaction-control results to check
cleanup policy only.
These fixtures do not simulate failing disks or establish durability after
power loss. The close result is unavailable through this libSQL API.

### Composition fixture

The private test in `tests/support/storage_composition.rs` creates a delta,
packs the source and delta, and stores both with the target's SHA3 digest.
It reopens the file, unpacks the stored BLOBs, applies the delta and checks
both target bytes and digest. A later transaction changes one BLOB and inserts
a second row before a duplicate key fails. Reopening confirms that both writes
rolled back and the original stored bytes remain intact.

This fixture uses the existing private executor. Its table is a test fixture,
not a proposed domain schema. It establishes composition on local storage;
it does not rerun or extend the historical Fossil reference evidence.

### Historical test-first evidence

The test-first stub opened a real file with engine defaults and used deferred
BEGIN. It failed the WAL assertion with `delete` and allowed a competing writer
before DML. Foreign keys were already ON in this build.
A separate run without the failure latch let a helper execute after a caught
unique error. The retained assertions pass with configuration, immediate BEGIN
and the failure latch present.

## Native date provider

The relative Cargo patch selects a complete, reviewed `libsql-ffi` source
copy with bounded date corrections for the intended Fossil artifact date fields.
The artifact parser is still planned; the current date test issues private SQL. The base version and source-ID strings
above still apply; the patched source hash identifies the provider.
[Local native date provider](libsql-provider.md) records its provenance,
integrity checks, hook ownership and qualification limits.

## Recommendations

Use one owned transaction for each complete mutation and terminate it explicitly.
Next, review the schema contracts, then add fixed domain operations that roll
back when either SQL or another step of the mutation fails. Keep domain atomicity, recovery and server
cancellation claims separate from these local fixtures.
