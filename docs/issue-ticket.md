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

`read_private_issue` runs the full admission of `LocalOwnerRepository::open`, then parses the artifact of
the ticket ID and returns its record. It writes nothing. `create_private_issue` runs the same admission
before its write and again after it, in the same transaction and under the same limits.

## List every issue

`list_private_issues` returns every task and project of the repository, in the order of the receipts
that stored them:

```rust
let issues = owner.list_private_issues(limits).await?;
for issue in &issues {
    println!("{} {}", issue.ticket_id, issue.native_name);
}
```

The call runs the full admission of `LocalOwnerRepository::open` first. Then it runs the issue part of
admission once more, with the same queries, and takes each record from that second pass. It writes nothing,
and it uses no SQL that admission does not use. An empty repository gives an empty list.

Both passes charge the same content and parser ledgers. So the limits must cover each issue artifact
twice. For example, three issues that an open admits with 37 parser entries need 72 entries for a
list. The shared root takes 2 of those entries, and only the first pass parses it. So the list
needs 2 + 2 × 35 entries. The list does not use the SQL limits of `IssueLimits`.

The handlers of `issue task list` and `issue task show` read the tasks through this call. The
[issue read guide](issue-read.md) gives their output.

## How admission checks issues

Admission finds each issue from its `tkt-` tag, parses its artifact again and derives every row from it.
The `ticket`, `ticketchng`, `g_gest_tasks` and `g_gest_projects` tables must hold exactly the derived rows.
The `event`, `tagxref`, `tag`, `blob`, `private`, `leaf`, `unclustered`, `unsent` and `rcvfrom` tables
must hold exactly the rows of the check-ins and of the issues, with nothing else. If any of these tables
holds another row, for example a changed or extra row, open refuses the repository.

Issues and private check-ins can come in any order in one repository. Each check-in and each issue owns
one receipt, and the receipts after the root's have no gaps. Tags above the 12 built-in tags take their
IDs in the order of the receipts that created them. So admission looks up the private branch tag by name.
A Gest ID names one task or one project. Admission refuses a project that reuses the Gest ID of a task,
and a second ticket with the same ticket ID.

The checkout fingerprint names a receipt. Registration takes the newest receipt, so when an issue comes
before registration, the fingerprint names the receipt of that issue. After the first check-in, the
fingerprint must name the receipt of the selected check-in, and open refuses an older fingerprint.

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

Tag IDs above 12 follow the order of the writes. When a ticket comes before the first check-in, that
ticket takes `tagid` 13, and the first check-in gives its branch tag the next free number. When the
first check-in comes before the first ticket, its branch tag takes 13, and the first ticket takes 14.

## Shapes that the writer refuses

The writer accepts one complete task or project creation for each ticket ID. It refuses each other shape
as not supported in this build, and the refusal leaves every table unchanged:

- a second artifact for a ticket ID that the repository holds
- a task whose Gest ID a project holds, or a project whose Gest ID a task holds
- a change to a ticket, a ticket attachment or an artifact that is not a ticket
- an artifact that the emitter does not write again byte for byte, for example one with an unknown field
- a J value that contains `[`, because Gossil has no port of the Markdown backlinks of Fossil
- a request with a scope other than private

## Limits of this build

Issues are private only. A write can fail and roll back after its admission check passed. The writer then
refreshes the change counter of the proof, so the failed write does not lock out the stored tickets. A
write that the admission check refuses as stale leaves the proof stale, so the check refuses the next call
of that owner too.

The binding tables `g_project_binding`, `g_checkout_project`, `g_identity` and `g_contributor` stay empty,
and admission refuses a row in any of them. The writer writes no audit row and no undo history.

The writer writes no selectors, tags of Gest or links. The scope key is the literal `combined`, and not
the full key of the specification, because the full key needs selectors.

## Tests

`tests/issue_ticket.rs` checks a task and a project end to end (GEST-T01). It also checks the metadata,
priority and status values that one creation can hold (part of GEST-T02), and each refused shape.

The unit controls in `tests/support/storage_issue_ticket.rs` inject a failure after each write step.
They check that every byte of the repository files stays the same (GEST-T08). They also check that
admission refuses a changed or extra row in each admitted table, for example a `tagxref` row with no RID.
They check too that admission refuses a project that reuses the Gest ID of a task. For each changed row,
the ticket reader, the next write and a new open each refuse, and leave the repository files unchanged.

A change to the `ticket`, `ticketchng`, `g_gest_tasks` or `g_gest_projects` table also makes the saved
admission proof stale. The admission check of the next write then refuses that write.

Admission parses every artifact under one parser ledger, so the entry ceiling of that ledger limits the
count of issues. A unit control sets a ceiling that admits two tasks, and checks that admission refuses a
third.

A unit control lists two tasks and a project in receipt order, and checks that the list changes no row.
It also finds the smallest entry ceiling and the smallest decoded-byte ceiling of the list and of an
open. A list call with the decoded-byte ceiling of the open refuses and changes no row. In each tamper
case above, the list call also refuses with the same message. In each of its three orders, the CAT-T07
subset below also lists the issues after each step.

The CAT-T07 subset in `tests/issue_ticket.rs` writes issues and check-ins in three orders. In the first,
an issue comes before the checkout is registered. In the second, issues come before the first check-in,
and in the third, issues come after check-ins. After each step, a new open admits the repository, and
the commands `status`, `add`, `commit` and `timeline` work. With check-ins between the issues, admission
still refuses a changed issue, tag or receipt row, for example swapped tag IDs or swapped receipts. A
further test checks which receipt the fingerprint names before and after the first check-in.

`just issue-ticket-interop` runs the stock Fossil check with the pinned Fossil 2.29 at `7e8ecf0ce4`. It
writes a task and a project, then requires these results of stock Fossil:

1. `fossil test-integrity --parse` reports 0 errors.
2. `fossil ticket show` returns each J value byte for byte.
3. `fossil test-ticket-rebuild all` and `fossil rebuild` each write the same ticket, change, event, tag,
   private, unclustered, unsent and backlink rows as Gossil.

It then writes check-ins with `gos` and an issue between them. On that repository, stock Fossil must
report 0 errors from `fossil test-integrity --parse`. It must list both check-ins and the new ticket in
`fossil timeline`, and show the ticket in `fossil ticket show`. `just catalog-interop` runs this check too.

The comparison counts event times in whole milliseconds. Fossil formats `event.mtime` with its own
conversion, which adds noise after the ninth decimal place of the Julian day. The comment bytes must be
equal. If the reference is absent, the recipe exits 5, which means `not-run` and not a pass.

## Recommendations

Validate a record with its converter, emit its creation artifact, and store it with
`create_private_issue`. Run `just issue-ticket-interop` after a change to the writer, and report
`not-run` apart from a pass.
