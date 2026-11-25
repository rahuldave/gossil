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

## The proof of one issue

`LocalOwnerRepository::verify_issue` proves one task or project from its own rows, and
[`gos v verify --issue ID`](cli.md#verify-a-repository) runs it. It first finds the one ticket whose
`gest_id` holds the ID.

The proof of one issue checks only a subset of the repository. It proves the rows that
belong to the issue and that an index reaches. The rows are its artifact, ticket, change row, event, tag, tag row,
private mark, typed row and, for a task, its project. It does not claim that no other row in the
repository names the issue. Only the full proof, `gos v verify` without `--issue`, makes that claim.

It then checks that each table that admission keeps empty in every repository has no row, for example `delta`,
`cherrypick`, `g_gest_notes` and `g_gest_relationships`. The check takes one fixed query per table. A row
in one of these tables makes the proof refuse with `unsupported nonempty catalog6 table:` and the name
of the table. A repository with no checkout must also keep `unsent`, `filename`, `mlink`, `plink`,
`g_path_scope`, `g_project_binding` and `g_checkout_project` empty. The proof does not check those
seven whole tables. It refuses only an `unsent`, `mlink` or `plink` row that names the issue, as the next
paragraphs say, and only the full proof refuses every other row there.

The `tkt-K` tag must then have one `tagxref` row. The RID of that row must name a
private blob with a local receive record. The proof does not look for another blob with the same
receipt, because no index reaches such a blob. The full proof refuses a shared receipt.

No row of the `unsent`, `unclustered`, `leaf`, `phantom`, `orphan` or `backlink` table may name that
blob. The same holds for an `attachment` row whose target is K, and for a `g_public_receipt` row with
the hash of the blob. No `mlink` row may have the blob as its `mid`, `fid` or `pid`, and no `plink` row
may have it as its `pid` or `cid`. No index reaches `mlink.pmid` or `plink.baseid`, so the proof does not check them.

The proof replays the one artifact, which must be a canonical creation with this K. The `event`, `tagxref`, `ticket` and `ticketchng` rows of the issue must equal the replay.
So must its typed row in the `combined` scope. The typed table of the other kind must hold no
`combined` row with the Gest ID of the issue.

For a task, the proof checks only that a `combined` project row with its project ID exists. When none
exists, it refuses with `a stored task names a project that the repository does not hold`. The full
proof admits such a task, because the library writer may store a task before its project.

The proof replays no other artifact. It reads the rows of this issue by an index or a primary key,
except for the one lookup of the ticket by its Gest ID, which scans the `ticket` table. Each row read
also measures the size of the text of the whole repository, as every read of admission does. So the cost of the proof
still grows with the repository, but more slowly than the cost of the full proof.

The proof does not see a row that no index of the issue reaches, for example an unrelated tag or a
ticket row without an artifact. It does not see a typed row in another scope either. Only the full proof
of admission, or `gos v verify` without `--issue`, refuses those rows.

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

## How many issues a repository holds

A write runs the full admission twice, and each admission reads and parses every issue artifact. So the
work of one write grows with the count of issues.

Some reads in admission take a whole surface of the repository. Each bounded SQL read takes the text
inventory, the text of 12 native and local tables such as `blob`, `event`, `tag` and `tagxref`. Each
read of stored content checks the owner again, and that check takes the schema and the policy tables.
The content ledger charges each whole surface once per admission. A later read of that surface charges
only the bytes that it has grown by. The other charges are the same for each issue, so the
decoded-byte charge of one admission grows linearly with the count of issues.

The `gos v` and `gos i` commands use one parser ledger of 4096 cards and 4096 entries for a whole call.
A write parses every task twice. So under those ceilings a repository holds 113 tasks with 21-byte
titles, which make about 57 KB of artifacts. The card ceiling refuses the 114th write. With wider parser
ledgers, the decoded-byte ceiling of 64 MiB refuses the write after 378 such tasks. The `gos v`
commands parse every issue in their admissions too, so the same ceilings bound them.

`gos i task create` refuses a task artifact of more than 65,536 bytes, so that one large task cannot
use up the budgets of every later write.

## Tests

`tests/issue_ticket.rs` checks a task and a project end to end (GEST-T01). It also checks the metadata,
priority and status values that one creation can hold (part of GEST-T02), and each refused shape.

The unit controls in `tests/support/storage_issue_ticket.rs` inject a failure after each write step.
They check that every byte of the repository files stays the same (GEST-T08). They also check that
admission refuses a changed or extra row in each admitted table, for example a `tagxref` row with no RID.
They check too that admission refuses a project that reuses the Gest ID of a task. For each changed row,
the ticket reader, the next write and a new open each refuse, and leave the repository files unchanged.

Each of the 29 cases of a changed row also runs the proof of one issue on the task and on the project.
It checks the exact result of each proof. In 22 of the cases, at least one of the two proofs refuses.

In the other 7 cases, both proofs admit the change and only the full proof refuses it. Those 7 cases add
a row that no index of the issue reaches. Examples are an unrelated tag, a second event of the ticket
and a change row of the artifact under another ticket. One of the 7 cases adds a second typed row in
another scope.

In the case of an artifact whose K is not the K of its tag, the proof of the K of the tag also refuses,
with `stored issue rows differ from the ticket artifact`.

`the_proof_has_no_total_ceiling` stores a project and two tasks. It sets total content ceilings of 64 bytes
and a parser ceiling of 20 entries, which open refuses. Under them `verify` passes and counts 0
check-ins and 3 issues. A parser input ceiling of 64 bytes makes `verify` refuse.

Under the same small ceilings, `verify_issue` proves a task and a project, and `find_issue` finds each of them by a prefix.
`find_issue` refuses the prefixes `""`, `"a"` and 33 letters with `invalid issue ID prefix`. No step
changes a byte.

Most cases of `the_issue_proof_refuses_each_row_that_names_its_artifact` add one row that names the
task, by its artifact or by its Gest ID. A `phantom`, `orphan`, `delta`, `cherrypick`, `g_gest_notes` or
`g_gest_relationships` row makes the proof of the task refuse with the message of that nonempty table.
An `mlink` row with the artifact as its `mid`, `fid` or `pid` makes it refuse with `stored issue rows
differ from the ticket artifact`. So does a `plink` row with the artifact as its `pid` or `cid`. So do a
`g_public_receipt` row with the hash of the artifact and a `combined` project row with the Gest ID of the task.

One more case deletes the ticket row of the task, which gives the same refusal. The test also checks the
exact refusal of the full proof for each case.

`only_the_full_proof_refuses_rows_that_no_index_reaches` adds each of these in turn:

- a second blob under the receipt of the task
- an `mlink` row whose `pmid` is the task artifact
- a `plink` row whose `baseid` is the task artifact

The proof of the task admits each, and `verify` refuses each.

`only_the_issue_proof_refuses_a_task_without_its_project` stores a task alone. `verify` passes and
counts one issue. The proof of the task and `verify_issue` both refuse the task with the message of a
missing project. `verify_issue` also refuses an ID that no ticket holds.

The unit test `a_per_artifact_parser_ledger_does_not_accumulate` in
`src/storage/repository.rs` checks that a per-artifact parser ledger keeps its whole allowance after each
parse, and still refuses one artifact over its ceiling.

`tests/vcs_verify_cli.rs` checks `gos v verify` in all four forms. It checks the exact summary
`verified: 1 check-ins, 4 issues, 15 tables compared`. It proves one task by its full ID and by a prefix,
and the project by a prefix. It checks the usage line and the status of a usage error. It also checks the statuses of an invalid,
missing or ambiguous prefix, and the exact count of an ambiguous prefix.

A changed typed row makes both forms exit with status 74 and the fixed message. In this build, open
refuses the row at admission, before either proof runs. So this test checks the message and the
status, and the unit tests above check the proofs themselves.

`verify_counts_no_check_in_and_refuses_an_older_catalog` checks a repository without a commit. It prints
`verified: 0 check-ins, 3 issues, 15 tables compared`. With stdout closed, the command exits with 1 and
prints `gossil: unable to write the summary: `. A repository at catalog 4 makes both forms exit with 78
and print three lines. No case changes a byte of the repository.

A change to the `ticket`, `ticketchng`, `g_gest_tasks` or `g_gest_projects` table also makes the saved
admission proof stale. The admission check of the next write then refuses that write.

Further unit controls cover `init_private_project`. A failure injected after each write step, the
binding step included, leaves every byte unchanged and the checkout unbound. The method refuses a task,
an unknown checkout, a bound checkout, a repository that holds an unbound project and a repository
without a registration.

Admission refuses each changed, missing or extra binding row, a binding to a
task or to an archived project, and a value that is not text. Before each change, the list call returns
all three issues of the bound repository. After the change, the list call refuses with `unsupported project
binding state`. A change to a binding row also makes the admission proof stale.

`tests/issue_init_cli.rs` checks the command `gos i init` in all four forms, its second run, its
refusals and the `vcs` commands before and after it. `tests/issue_task_cli.rs` checks
`gos i task create` in the same way, and reads each stored task back with `read_private_issue`. It
also creates a task in each of the four forms. Then it checks that `gos i task list --json` lists all
four, and that `gos i task show --json` shows the stored ID, title and priority of each. It runs two
`task create` commands at the same time, four times, and requires that both exit with 0.

A further test in `tests/issue_task_cli.rs` holds the write lock for 7 seconds. A `task list` and a
`task show` that start meanwhile retry and exit with 0. Then the test runs a `task create` beside a
`task list`.

The unit tests of `src/product/issue_task.rs` inject a failure after each write step of a new task. Then
they check that every byte of the repository stays unchanged (GEST-T08).

Admission parses every artifact under one parser ledger, so the entry ceiling of that ledger limits the
count of issues. A unit control sets a ceiling that admits two tasks, and checks that admission refuses a
third.

A unit control in `src/storage/repository/recorded_history.rs` measures the decoded-byte charge of one
admission at 3, 6 and 12 tasks, and checks that it grows linearly. Further unit controls there check
that repeated owner checks charge the schema once per admission. They check that a second admission on
the same ledger charges each whole surface again. They also check that the ledger charges exactly the
TEXT and BLOB bytes of a result.

A unit control in `src/storage/content.rs` checks that the ledger charges a surface once per admission,
and later only its growth. It also checks that the ledger charges and holds nothing for a refused
reservation.

`tests/issue_capacity.rs` sets a decoded-byte ceiling of 4 MiB. The old charge, which took the whole
text for each ticket, met that ceiling at the fifth write. The test writes 12 tasks, then opens, lists
and reads them. Then it writes until the ceiling refuses a write, and checks that the refusal changes no
byte of the repository files. It also runs the `gos v` commands in a repository with 12 tasks.

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
