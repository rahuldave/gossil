# Store a private issue ticket

The issue ticket writer stores one Gest task or project in a native repository, so that stock Fossil
2.29 can read the ticket, check it and rebuild it. A ticket is the Fossil record of a piece of work.
Gossil writes each Gest task and project as a ticket, and the [task ticket](task-ticket.md) and
[Gest ticket](gest-ticket.md) emitters make its creation artifact. A creation artifact is the first and
only artifact of a ticket in this build.

The writer is a library method of `LocalOwnerRepository` in `src/storage/issue_ticket.rs`. No CLI command
calls it yet. The repository must be at catalog 6, the newest catalog of Gossil storage.

## Write and read a ticket

Emit the creation artifact first, with `task_creation_bytes` or `project_creation_bytes`. Then give the
bytes to the writer, with explicit limits:

```rust
use gossil::storage::{IssueLimits, PathScope, PrivateIssueRequest};

let limits = IssueLimits { admission, sql_rows: 100_000, sql_bytes: 64 << 20 };
let request = PrivateIssueRequest { artifact: &bytes, scope: PathScope::Private };
let stored = owner.create_private_issue(request, limits).await?;
let read = owner.read_private_issue(&stored.ticket_id, limits).await?;
assert_eq!(read.record, stored.record);
```

`admission` holds the content and parser limits that the whole call shares. `sql_rows` and `sql_bytes` limit
the rows and bytes that the SQL queries of one call can return. The result names the ticket ID, which is
the K card, and the native name of the artifact. It also holds the validated `TaskValues` or
`ProjectValues`.

`read_private_issue` parses the stored artifact again and derives every row from it. It then compares
those rows with the stored rows. It refuses any difference and writes nothing. It also refuses two
kinds of extra row that name the ticket. One is an `event` row with the ticket's `tagid` under another `objid`. The
other is a `ticketchng` row with the ticket's RID under another `tkt_id`.

## Rows that one write adds

The writer adds these rows in one exclusive transaction:

- **rcvfrom** gets one local receipt, with `uid` 0, a NULL nonce and a NULL IP address.
- **blob** gets the artifact, with `rcvid` set to that receipt, and **private** marks it.
- **tag** gets `tkt-K` with the next free `tagid`, and **tagxref** gets
  `(tagid, 1, rid, rid, NULL, mtime, rid)`.
- **ticket** and **ticketchng** get the decoded J fields as text, as Fossil `ticket_insert` writes them.
- **event** gets one row of type `t`. Its comment is `New ticket [K16|K10] <i>title</i>.`, where K16
  and K10 are the first 16 and 10 characters of K. The title is HTML-escaped, and a project has no title,
  so its comment holds `<i></i>`.
- **g_gest_tasks** or **g_gest_projects** gets the typed row, with the scope key `combined`.

The writer adds no `unclustered` row, no `unsent` row and no `backlink` row. Stock Fossil writes the same
rows for a private ticket, except a `modreq` row for moderation, which Gossil does not write. The three
local audit tables `transactions`, `transaction_events` and `g_command_audit` stay empty.

On a new repository, the first ticket takes `tagid` 13. A later branch tag then takes the next free
number.

## Shapes that the writer refuses

The writer accepts one complete task or project creation for each ticket ID. It refuses each other shape
as not supported in this build, and the refusal leaves every table unchanged:

- a second artifact for a ticket ID that the repository holds
- a change to a ticket, a ticket attachment or an artifact that is not a ticket
- an artifact that the emitter does not write again byte for byte, for example one with an unknown field
- a J value that contains `[`, because Gossil has no port of the Markdown backlinks of Fossil
- a request with a scope other than private

The reader refuses a second artifact or an attachment for the ticket ID in the same way.

## Limits of this build

`LocalOwnerRepository::open` still refuses a repository that holds an issue ticket. The writer keeps its
own admission proof up to date, so one open owner can write and read tickets. A write can fail and roll
back after its admission check passed. The writer then refreshes the change counter of the proof, so the
failed write does not lock out the stored tickets. A write that the admission check refuses as stale
leaves the proof stale, so the check refuses the next call of that owner too. A later change teaches
`open` and the recording validators to accept the rows above.

The writer writes no selectors, tags of Gest or links. The scope key is the literal `combined`, and not
the full key of the specification, because the full key needs selectors.

## Tests

`tests/issue_ticket.rs` checks a task and a project end to end (GEST-T01). It also checks the metadata,
priority and status values that one creation can hold (part of GEST-T02), and each refused shape. The unit
controls in `tests/support/storage_issue_ticket.rs` inject a failure after each write step. They check
that every byte of the repository files stays the same (GEST-T08), and that the reader refuses changed
rows.

`just issue-ticket-interop` runs the stock Fossil check with the pinned Fossil 2.29 at `7e8ecf0ce4`. It
writes a task and a project, then requires these results of stock Fossil:

1. `fossil test-integrity --parse` reports 0 errors.
2. `fossil ticket show` returns each J value byte for byte.
3. `fossil test-ticket-rebuild all` and `fossil rebuild` each write the same ticket, change, event, tag,
   private, unclustered, unsent and backlink rows as Gossil.

The comparison counts event times in whole milliseconds. Fossil formats `event.mtime` with its own
conversion, which adds noise after the ninth decimal place of the Julian day. The comment bytes must be
equal. If the reference is absent, the recipe exits 5, which means `not-run` and not a pass.

## Recommendations

Validate a record with its converter, emit its creation artifact, and store it with
`create_private_issue`. Run `just issue-ticket-interop` after a change to the writer, and report
`not-run` apart from a pass.
