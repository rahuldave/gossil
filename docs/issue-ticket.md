# Store a private issue ticket

The issue ticket writer stores one Gest task or project in a native repository, so that stock Fossil
2.29 can read the ticket, check it and rebuild it. A ticket is the Fossil record of a piece of work.
Gossil writes each Gest task and project as a ticket, and the [task ticket](task-ticket.md) and
[Gest ticket](gest-ticket.md) emitters make its creation artifact. A creation artifact is the first and
only artifact of a ticket in this build.

The writer is a library method of `LocalOwnerRepository` in `src/storage/issue_ticket.rs`. A second
method, `init_private_project`, writes a project and binds it to the registered checkout. The command
[`gos i init`](cli.md#create-the-gest-project-of-a-checkout) calls that method, and
[`gos i task create`](cli.md#create-a-task) calls `create_private_issue` for a task in the bound
project. The repository must be at catalog 6, the newest catalog of Gossil storage.

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

## Bind a project to a checkout

`init_private_project` stores one project creation and binds it to the registered checkout, in one
exclusive transaction:

```rust
use gossil::storage::ProjectInitRequest;

let request = ProjectInitRequest { artifact: &bytes, checkout_id: "work" };
let stored = owner.init_private_project(request, limits).await?;
let project = owner.bound_project("work", limits).await?;
assert!(project.is_some());
```

The method writes the rows that `create_private_issue` writes for a project, as the section
[rows that one write adds](#rows-that-one-write-adds) lists them. It also writes two binding rows:

- **g_project_binding** gets the checkout ID, the Gest ID of the project and the exact root of the
  checkout.
- **g_checkout_project** gets the checkout ID and the Gest ID of the project.

Like `create_private_issue`, the method runs the full admission before and after its write. Before it
changes anything, it refuses an artifact that is not one project creation, a checkout that is not registered and
a checkout that already has a project. It also refuses a repository that already holds a project
ticket, for example one that `create_private_issue` wrote. The refusal for these cases is
`issue init needs one project creation, a registered checkout with no bound project, and a repository
with no project; binding an existing project is not available in this build`. A failure before the
commit leaves every byte of the repository files unchanged.

`bound_project` returns the Gest ID of the project bound to a checkout, or `None` when the checkout has
no binding. It runs the full admission of `LocalOwnerRepository::open` first, and it writes nothing.
`bound_project` does not use the SQL limits in `limits`.

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

Admission accepts the binding tables `g_project_binding` and `g_checkout_project` in two states only.
In the first, both tables are empty. In the second, each table holds the one row that
`init_private_project` writes. Both rows then name the registered checkout and the Gest ID of one
admitted project that is not archived. The row of `g_project_binding` also holds the exact root of the
checkout. Admission refuses every other state with `unsupported project binding state`.

In a repository without a checkout registration, admission refuses every binding row. The admission proof also holds
the rows of both tables, so a later change to them makes the proof stale.

The tables `g_identity` and `g_contributor` stay empty, and admission refuses a row in either of them.
The writer writes no audit row and no undo history. `init_private_project` does not bind a project that
the repository already holds.

The writer writes no selectors, tags of Gest or links. The scope key is the literal `combined`, and not
the full key of the specification, because the full key needs selectors.

Each write checks every stored ticket twice, before and after the write, against one parser budget and
one budget of decoded content. A task of about 300 KB passes its own write and makes every later write
refuse. For this reason, `gos i task create` refuses a task artifact of more than 65,536 bytes.

In this build, the budget of decoded content runs out after about 96 tasks of any size. Large tasks use
it up sooner. With descriptions of 10,000 bytes, the write of the 32nd task refuses. The cause is in
admission. For each stored ticket, it charges the text of the whole repository against that budget. So the
cost grows with the number of tickets times the size of the repository.

After that, every write refuses with `total decoded content limit`. `gos i task create` then exits with
status 74, and the `vcs` writes, such as `gos v add`, exit with status 1. The refused write changes no byte, so
nothing stored is damaged or lost.

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

Further unit controls cover `init_private_project`. A failure injected after each write step, the
binding step included, leaves every byte unchanged and the checkout unbound. The method refuses a task,
an unknown checkout, a bound checkout, a repository that holds an unbound project and a repository
without a registration. Admission refuses each changed, missing or extra binding row, a binding to a
task or to an archived project, and a value that is not text. A change to a binding row also makes the
admission proof stale.

`tests/issue_init_cli.rs` checks the command `gos i init` in all four forms, its second run, its
refusals and the `vcs` commands before and after it. `tests/issue_task_cli.rs` checks
`gos i task create` in the same way, and reads each stored task back with `read_private_issue`. The
unit tests of `src/product/issue_task.rs` inject a failure after each write step of a new task. Then
they check that every byte of the repository stays unchanged (GEST-T08).

Admission parses every artifact under one parser ledger, so the entry ceiling of that ledger limits the
count of issues. A unit control sets a ceiling that admits two tasks, and checks that admission refuses a
third.

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

Last, the recipe runs the ignored test `stock_fossil_reads_a_repository_after_issue_init` in
`tests/issue_init_cli.rs`. The test runs `gos v init`, `gos v open`, `gos i init`, `gos i task create`,
one commit and a second `gos i task create`. Stock Fossil must then report 0 errors from
`fossil test-integrity --parse`, and `fossil ticket show` must show the private project and both tasks.

The comparison counts event times in whole milliseconds. Fossil formats `event.mtime` with its own
conversion, which adds noise after the ninth decimal place of the Julian day. The comment bytes must be
equal. If the reference is absent, the recipe exits 5, which means `not-run` and not a pass.

## Recommendations

Validate a record with its converter, emit its creation artifact, and store it with
`create_private_issue`. Bind a project to a checkout only with `init_private_project`. Run
`just issue-ticket-interop` after a change to the writer, and report `not-run` apart from a pass.
