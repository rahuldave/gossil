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

`read_private_issue` runs the checks of each command, which the
[native storage guide](native-storage.md#check-a-repository) lists. Then it reads the artifact that the
`tkt-` tag of the ticket ID names, parses it and returns its record. It compares no row and writes
nothing.

`create_private_issue` runs the same checks before its write and again after it, in the same
transaction and under the same limits. It parses its artifact and checks that it is one canonical
creation. It checks through an index that no row holds its ticket ID, tag, artifact or Gest ID. A task also
needs a project that the repository holds and that is not archived. Each check uses an index, so the
cost of a write does not grow with the count of issues.

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

Like `create_private_issue`, the method runs the checks of each command before and after its write. Before it
changes anything, it refuses an artifact that is not one project creation, a checkout that is not registered and
a checkout that already has a project. It also refuses a repository that already holds a project
ticket, for example one that `create_private_issue` wrote. The refusal for these cases is
`issue init needs one project creation, a registered checkout with no bound project, and a repository
with no project; binding an existing project is not available in this build`. A failure before the
commit leaves every byte of the repository files unchanged.

`bound_project` returns the Gest ID of the project bound to a checkout, or `None` when the checkout has
no binding. It runs the checks of each command first, and it writes nothing.
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

The call runs the checks of each command first. Then it reads each `combined` typed row with its
ticket ID and artifact name. It replays no artifact, so it passes under a parser entry ceiling of one.
It writes nothing. An empty repository gives an empty list. The SQL row ceiling of `IssueLimits` bounds
the count of issues, and the parser input ceiling bounds the width of one row.

The cost of the list grows with the count of issues, and not with the rest of the repository. The list
does not compare the typed rows with the artifacts. `gos v verify` does.

The `issue task list` handler reads the tasks through this call. The `issue task show` handler does
not use it. It finds one task with `find_task` and proves it with `verify_issue`. The
[issue read guide](issue-read.md) gives the output of both commands.

## How the full proof checks issues

The full proof of `gos v verify` finds each issue from its `tkt-` tag, parses its artifact again and derives every row from it.
The `ticket`, `ticketchng`, `g_gest_tasks` and `g_gest_projects` tables must hold exactly the derived rows.
The `event`, `tagxref`, `tag`, `blob`, `private`, `leaf`, `unclustered` and `rcvfrom` tables
must hold exactly the rows of the check-ins and of the issues, with nothing else. The `unsent` table may
hold any subset of the rows that replay gives, because a stock `fossil push` or `fossil sync` that sends
nothing deletes rows from it. If any of these tables holds another row, for example a changed or extra
row, the full proof refuses the repository.

Issues and private check-ins can come in any order in one repository. Each check-in and each issue owns
one receipt, and the receipts after the root's have no gaps. Tags above the 12 built-in tags take their
IDs in the order of the receipts that created them. So the full proof looks up the private branch tag by
name. A Gest ID names one task or one project. The full proof refuses a project that reuses the Gest ID of a task,
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

It then checks that each table that the full proof keeps empty in every repository has no row, for example `delta`,
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
proof admits such a task. The writer refuses it, so only a change outside Gossil can make one.

The proof replays no other artifact. It reads the rows of this issue by an index or a primary key,
except for the one lookup of the ticket by its Gest ID, which scans the `ticket` table. So apart from that
lookup, the cost of the proof does not grow with the repository.

The proof does not see a row that no index of the issue reaches, for example an unrelated tag or a
ticket row without an artifact. It does not see a typed row in another scope either. Only the full proof,
`gos v verify` without `--issue`, refuses those rows.

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

Issues are private only. A write can fail and roll back after its checks passed. The next command runs
its checks again in its own transaction, so a failed write does not lock out the stored tickets.

Open and each command accept the binding tables `g_project_binding` and `g_checkout_project` in two states only.
In the first, both tables are empty. In the second, each table holds the one row that
`init_private_project` writes. Both rows then name the registered checkout and the Gest ID of one
project that is not archived. The row of `g_project_binding` also holds the exact root of the
checkout, and each ID must be text. Open refuses every other state with
`unsupported project binding state`, and so does each command.

In a repository without a checkout registration, open refuses every binding row.

The tables `g_identity` and `g_contributor` stay empty, and the full proof refuses a row in either of
them.
The writer writes no audit row and no undo history. `init_private_project` does not bind a project that
the repository already holds.

The writer writes no selectors, tags of Gest or links. The scope key is the literal `combined`, and not
the full key of the specification, because the full key needs selectors.

## How many issues a repository holds

No command reads the whole repository, so no count of issues makes a write refuse. `task create`,
`status` and `commit` each read only the rows that they need, by an index. `task show` finds its ticket
by a scan of the `ticket` table, since no index holds the Gest ID, and then replays only its one
artifact. `task list` reads every typed row, and `gos v verify` replays every artifact, so their cost
grows with the count of issues. The scan of `task show` is cheap, as the times below show.

`just issue-capacity` stores 5,000 tasks in a release build and times each command at 100, 1,000 and
5,000 tasks. One run on macOS, with 8 CPUs and load averages of about 9 before and 7 after, gave these
times in milliseconds:

| Tasks | create | list | show | status | commit | verify |
|---|---|---|---|---|---|---|
| 100 | 15 | 15 | 15 | 13 | 17 | 101 |
| 1,000 | 15 | 23 | 16 | 13 | 17 | 860 |
| 5,000 | 15 | 68 | 16 | 13 | 19 | 4,267 |

The times of create, show, status and commit stay flat. The times of list and verify grow with the count
of tasks.

`gos i task create` refuses a task artifact of more than 65,536 bytes.

## Tests

`tests/issue_ticket.rs` checks a task and a project end to end (GEST-T01). It also checks the metadata,
priority and status values that one creation can hold (part of GEST-T02), and each refused shape.

The unit controls in `tests/support/storage_issue_ticket.rs` inject a failure after each write step.
They check that every byte of the repository files stays the same (GEST-T08). They also check that
the full proof refuses a changed or extra row in each admitted table, for example a `tagxref` row with no
RID. They check too that the full proof refuses a project that reuses the Gest ID of a task, while open
and the read of the project admit it.

For each changed row, the ticket reader, the list and the next write refuse only a change to a fact that
each one reads. For example, the reader refuses a second artifact under the tag, and the write refuses a
missing project row. A new open admits each change, because none is a fact that open reads. No case
changes a byte of the repository files.

Each of the 29 cases of a changed row also runs the proof of one issue on the task and on the project.
It checks the exact result of each proof. In 22 of the cases, at least one of the two proofs refuses.

In the other 7 cases, both proofs admit the change and only the full proof refuses it. Those 7 cases add
a row that no index of the issue reaches. Examples are an unrelated tag, a second event of the ticket
and a change row of the artifact under another ticket. One of the 7 cases adds a second typed row in
another scope.

In the case of an artifact whose K is not the K of its tag, the proof of the K of the tag also refuses,
with `stored issue rows differ from the ticket artifact`.

`the_proof_has_no_total_ceiling` stores a project and two tasks. It sets total content ceilings of 64 bytes
and a parser ceiling of 20 entries, far below what the whole repository needs. Under them `verify` passes and counts 0
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

Other tamper tests of check-ins, tracking, scopes and the timeline take the exact refusal of the full
proof from `tests/fixtures/tamper-refusals.tsv`. Each line of that table holds a tamper statement, a tab
and the first line of the refusal. `assert_refusal` in `tests/support/proof.rs` fails on a statement
that the table does not hold, or on a different message.

`only_the_full_proof_refuses_rows_that_no_index_reaches` adds each of these in turn:

- a second blob under the receipt of the task
- an `mlink` row whose `pmid` is the task artifact
- a `plink` row whose `baseid` is the task artifact

The proof of the task admits each, and `verify` refuses each.

`a_task_needs_its_project` checks that the writer refuses a task whose project the repository does not
hold, or holds archived, and changes no byte. Then it deletes the typed row of the project. The proof of
the task and `verify_issue` both refuse the task with the message of a missing project. `verify_issue`
also refuses an ID that no ticket holds.

The unit test `a_per_artifact_parser_ledger_does_not_accumulate` in
`src/storage/repository.rs` checks that a per-artifact parser ledger keeps its whole allowance after each
parse, and still refuses one artifact over its ceiling.

`tests/vcs_verify_cli.rs` checks `gos v verify` in all four forms. It checks the exact summary
`verified: 1 check-ins, 4 issues, 15 tables compared`. It proves one task by its full ID and by a prefix,
and the project by a prefix. It checks the usage line and the status of a usage error. It also checks the statuses of an invalid,
missing or ambiguous prefix, and the exact count of an ambiguous prefix.

A changed typed row makes both forms exit with status 74 and the fixed message. Open admits the row,
so the proof itself refuses it.

`open_refuses_each_fact_that_it_reads` changes one fact that open reads in each case. The cases are a
wrong catalog marker, a wrong fingerprint, a missing `server-code` or `project-code` config row, a
missing owner row, a wrong checkout count and lost Gossil tables. `verify` and `status` each refuse
at open, before their own work, and print the same message, and neither changes a byte. Lost tables give exit
78 to both. Lost tables also make `gos i task list` and `gos i task show` exit with 78 and the same
message.

`verify_counts_no_check_in_and_refuses_an_older_catalog` checks a repository without a commit. It prints
`verified: 0 check-ins, 3 issues, 15 tables compared`. With stdout closed, the command exits with 1 and
prints `gossil: unable to write the summary: `. A repository at catalog 4 makes both forms exit with 78
and print three lines. No case changes a byte of the repository.

Further unit controls cover `init_private_project`. A failure injected after each write step, the
binding step included, leaves every byte unchanged and the checkout unbound. The method refuses a task,
an unknown checkout, a bound checkout, a repository that holds an unbound project and a repository
without a registration.

Open refuses each changed, missing or extra binding row, a binding to a
task or to an archived project, and a value that is not text. Before each change, the list call returns
all three issues of the bound repository. After the change, the reader of the binding, the list call and
a new open each refuse with `unsupported project binding state`.

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

A further unit test there opens a second owner, and then the first owner commits. The second owner
needs no second open. `task create` writes its task, and `gos i init` finds the project of the first
owner. When no project is bound and the write of an init refuses, the init exits with 74 and the
message of the write, and stores no project.

A unit control in `src/storage/repository/recorded_history.rs` measures the charge of the checks of
each command at 3, 6 and 12 issues, and checks that it stays the same. Further unit controls there check
that repeated owner checks charge the schema once per admission. They also check that the ledger charges
exactly the TEXT and BLOB bytes of a result.

A unit control in `src/storage/content.rs` checks that the ledger charges a surface once per admission,
and later only its growth. It also checks that the ledger charges and holds nothing for a refused
reservation.

`tests/issue_capacity.rs` uses the ceilings of the CLI. Its ordinary test stores 300 tasks, then runs
`task create`, `task list`, `task show`, `status`, `commit` and `verify`, and checks the output of
each. The ignored test stores 5,000 tasks and prints the time of each command at 100, 1,000 and 5,000
tasks. `just issue-capacity` runs the ignored test in a release build.

A unit control lists two tasks and two projects in receipt order under a parser entry ceiling of one, and
checks that the list changes no row. It finds the least SQL row ceiling under which the list passes, and
checks that one more task makes the list refuse under that ceiling. It also checks that `find_task`
refuses an empty prefix, a prefix of 33 letters and a prefix outside the letters k to z, with
`invalid issue ID prefix`. A further control stores the ID of each task as a blob. A prefix still
matches such an ID, and then `find_task` refuses the task with
`a stored typed row does not hold a valid task or project` and changes no row of any table. In each
of its three orders, the CAT-T07 subset below also lists the issues after each step.

The CAT-T07 subset in `tests/issue_ticket.rs` writes issues and check-ins in three orders. In the first,
an issue comes before the checkout is registered. In the second, issues come before the first check-in,
and in the third, issues come after check-ins. After each step, a new open admits the repository, and
the commands `status`, `add`, `commit` and `timeline` work.

With check-ins between the issues, the
full proof still refuses a changed issue, tag or receipt row, for example swapped tag IDs or swapped
receipts. Open admits each change but one, the swapped receipt of the selected check-in, because that
is the only fact among them that open reads. A further test checks which receipt the fingerprint names
before and after the first check-in.

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
