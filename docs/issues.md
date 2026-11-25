# Track issues beside your files

Gossil keeps the history of your files and the issues of your project in one
repository file. This page is for a reader who has built `gos` from source and
wants to see the issue commands work. It walks through one session from an
empty directory to tasks and check-ins in the same repository. Then it reads
that repository with stock Fossil, the Fossil 2.29 program that Gossil follows.

The issue commands follow Gest, a task tracker. A Gest project holds tasks,
and each task has a title, a status, a priority and a description. Gossil
stores each task as a Fossil ticket, a record that stock Fossil can read.

The output below is real output of this build. The only edit shortens the paths
to start with `~/demo`. The IDs, hashes and times differ in each run. The test
`tests/issue_demo.rs` runs the same steps and checks each output.

## Make a repository and record a file

Make the repository, then open a checkout in an empty directory. A checkout is
a directory whose files the repository tracks:

```text
$ mkdir -p ~/demo/work
$ gos v init ~/demo/demo.gossil
project-id: e11a10948be0b79e1159f51d7b89693bab4186fb
server-id:  1f07f5bb7fcdd26f943fb75ffea4c227d9518a34
admin-user: me (initial remote-access password is "LXaR3stVbQ")
$ cd ~/demo/work
$ gos v open --repository ~/demo/demo.gossil --checkout work --root ~/demo/work
Opened checkout work at ~/demo/work (repository e11a10948be0b79e1159f51d7b89693bab4186fb, head ea969fa75ea66defa9a7f57d31cec800b18c45437af074fc4789d169343b2e30)
```

Add a file, give it the private scope and record it. Private history stays in
this repository and is not shared. The date of a check-in must come after the
date of the empty check-in that `gos v init` made. Otherwise `gos v commit`
exits with status 1 and prints `private manifest date must follow all parents`.

The dates are in UTC, and each check-in must also come after the one before it.
So give a UTC date after the time of your own run, not the dates below. `me` is
the user name that `gos v init` prints as `admin-user`, so give `--user` your
own user name:

```text
$ echo hello > README.txt
$ gos v add --repository ~/demo/demo.gossil --checkout work --path README.txt
Tracked paths (A: pending addition, R: recorded):
A	unselected	"README.txt"
$ gos v scope --repository ~/demo/demo.gossil --checkout work --scope private --path README.txt
Selected scopes: 1
$ gos v commit --repository ~/demo/demo.gossil --checkout work --scope private --all-tracked --user me --comment "Add the readme" --date-override 2026-11-25T14:00:00
Recorded private check-in: 4a2cc7ea4fcaf7d31b299f58c9d9058571d28842e162e1f803561b6b4290e2b7
```

## Make the Gest project

Run `gos i init` in the checkout. It makes one Gest project and binds it to the
checkout. The later issue commands then use the project of the checkout:

```text
$ gos i init
  ✓  initialized project  szxwzssq

  root  ~/demo/work
```

The short ID `szxwzssq` is the first 8 letters of the project ID.

## Create tasks

`gos i task create TITLE` stores one private task. The second task below has a
description, a priority and the status `done`. With `--json`, the command
prints the task as Gest prints it:

```text
$ gos i task create "Write the guide"
  ✓  created task  lpswtmum

  title  Write the guide
$ gos i task create "Fix the parser" --description "The parser drops the last line." --priority high --status done
  ✓  created task  zsoopoor

  title  Fix the parser
$ gos i task create "Check stock Fossil" --json
{
  "assigned_to": null,
  "created_at": "2026-11-25T12:21:52.339358Z",
  "description": "",
  "id": "uwllrqzlnvkwuxlxworwvnqmvryzlmyt",
  "metadata": {},
  "priority": null,
  "project_id": "szxwzssqvlryqrtrztyotnrwspvtwurz",
  "resolved_at": null,
  "status": "open",
  "title": "Check stock Fossil",
  "updated_at": "2026-11-25T12:21:52.339358Z",
  "notes": [],
  "relationships": [],
  "tags": []
}
```

Gossil stores the priority `high` as the number 1. The [task create
section](cli.md#create-a-task) of the CLI reference gives every option.

## List the tasks

`gos i task list` prints the tasks of the project, the newest first. It leaves
out each task that is `done` or `cancelled`. With `--all`, it shows them too:

```text
$ gos i task list
uwllrqzl  open         --    Check stock Fossil
lpswtmum  open         --    Write the guide
$ gos i task list --all
uwllrqzl  open         --    Check stock Fossil
zsoopoor  done         P1    Fix the parser
lpswtmum  open         --    Write the guide
```

Each line gives the short ID, the status, the priority and the title. `--`
means that the task has no priority. With `--json`, the command prints a list
of the tasks:

```text
$ gos i task list --json
[
  {
    "assigned_to": null,
    "created_at": "2026-11-25T12:21:52.339358Z",
    "description": "",
    "id": "uwllrqzlnvkwuxlxworwvnqmvryzlmyt",
    "metadata": {},
    "priority": null,
    "project_id": "szxwzssqvlryqrtrztyotnrwspvtwurz",
    "resolved_at": null,
    "status": "open",
    "title": "Check stock Fossil",
    "updated_at": "2026-11-25T12:21:52.339358Z",
    "relationships": [],
    "tags": []
  },
  {
    "assigned_to": null,
    "created_at": "2026-11-25T12:21:50.901739Z",
    "description": "",
    "id": "lpswtmumnntmtkmlzwwklsulwswylvqz",
    "metadata": {},
    "priority": null,
    "project_id": "szxwzssqvlryqrtrztyotnrwspvtwurz",
    "resolved_at": null,
    "status": "open",
    "title": "Write the guide",
    "updated_at": "2026-11-25T12:21:50.901739Z",
    "relationships": [],
    "tags": []
  }
]
```

The JSON of a task is the same in `create`, `list` and `show`. Only the list
leaves out the empty `notes` field, as Gest does.

## Show one task

`gos i task show ID` prints one task. The ID can be a prefix of the task ID,
when no other task starts with it. Here `zs` names the done task. Use a prefix
of an ID from your own run:

```text
$ gos i task show zs
id           zsoopoorkotyulsvmpsmzwkyurrvnlzz
title        Fix the parser
status       done
priority     1
project      szxwzssqvlryqrtrztyotnrwspvtwurz
assigned to  none
metadata     {}
created at   2026-11-25T12:21:51.577902Z
updated at   2026-11-25T12:21:51.577902Z
resolved at  2026-11-25T12:21:51.577902Z
ticket       e3faa7a638bcd39e3b0cb02489be6026b357236b
artifact     675b687bb4d07b122f8c037e617747ae515579f7474357f63524c1c7a4f98d41
description
    The parser drops the last line.
```

The `ticket` line gives the ID of the Fossil ticket that holds the task. The
`--json` form prints the same fields as `create`. The [issue read
guide](issue-read.md) explains each field and the search by prefix.

## Record more files after the tasks

The file commands work as before after you create tasks. Record a second file, then read
the history and the tracked files:

```text
$ echo notes > NOTES.txt
$ gos v add --repository ~/demo/demo.gossil --checkout work --path NOTES.txt
Tracked paths (A: pending addition, R: recorded):
A	unselected	"NOTES.txt"
R	private	"README.txt"
$ gos v scope --repository ~/demo/demo.gossil --checkout work --scope private --path NOTES.txt
Selected scopes: 1
$ gos v commit --repository ~/demo/demo.gossil --checkout work --scope private --all-tracked --user me --comment "Add the notes" --date-override 2026-11-25T14:05:00
Recorded private check-in: a3e4de678d864919a194b6b46a139ac47c41637fb42e26caac2d751c026697db
$ gos v timeline --repository ~/demo/demo.gossil --checkout work
{"hash":"a3e4de678d864919a194b6b46a139ac47c41637fb42e26caac2d751c026697db","date":"2026-11-25T14:05:00","actor":"me","comment":"Add the notes","private":true}
{"hash":"4a2cc7ea4fcaf7d31b299f58c9d9058571d28842e162e1f803561b6b4290e2b7","date":"2026-11-25T14:00:00","actor":"me","comment":"Add the readme","private":true}
{"hash":"ea969fa75ea66defa9a7f57d31cec800b18c45437af074fc4789d169343b2e30","date":"2026-11-25T12:21:48.289","actor":"me","comment":"initial empty check-in","private":false}
$ gos v status --repository ~/demo/demo.gossil --checkout work
Tracked paths (A: pending addition, R: recorded):
R	private	"NOTES.txt"
R	private	"README.txt"
```

`gos v timeline` lists the check-ins only, and not the tasks.

## Read the repository with stock Fossil

Stock Fossil reads the same repository file. `fossil test-integrity` checks
every stored artifact. It finds the 3 check-ins and 4 tickets, which are the
project and the three tasks:

```text
$ fossil test-integrity --parse -R ~/demo/demo.gossil
all 9 blobs checked: 0 errors
0 control artifacts failed to parse
7 total control artifacts
  3 manifests
  4 tickets
low-level database integrity-check: ok
```

`fossil timeline` shows the check-ins and the tickets in one list:

```text
$ fossil timeline -R ~/demo/demo.gossil
=== 2026-11-25 ===
14:05:00 [a3e4de678d] *MERGE* *UNPUBLISHED* Add the notes (user: me tags: gossil-private/1f07f5bb7fcdd26f943fb75ffea4c227d9518a34)
14:00:00 [4a2cc7ea4f] *UNPUBLISHED* Add the readme (user: me tags: gossil-private/1f07f5bb7fcdd26f943fb75ffea4c227d9518a34)
12:21:52 [72f1dbc799] *UNPUBLISHED* New ticket [9dfb5cf383] "Check stock Fossil". (user: me)
12:21:51 [675b687bb4] *UNPUBLISHED* New ticket [e3faa7a638] "Fix the parser". (user: me)
12:21:50 [55d2174fdd] *UNPUBLISHED* New ticket [09303112e3] "Write the guide". (user: me)
12:21:50 [e2239de3dd] *UNPUBLISHED* New ticket [12f8ca15f2] . (user: me)
12:21:48 [ea969fa75e] initial empty check-in (user: me tags: trunk)
+++ no more data (7) +++
```

`*UNPUBLISHED*` marks private content. The second private check-in shows
`*MERGE*`, because it names the shared root as a second parent, as the [native
storage guide](native-storage.md) explains. The ticket without a title is the
project.

`fossil ticket show` prints the ticket fields of each task, one row for each
task and one column for each field. The command below keeps four of the
columns. The `--user` value must be a user of the repository, such as the
`admin-user`, or Fossil prints `No such user` and the name. Do not add a ticket with
`fossil ticket add`. It crashes in the pinned Fossil:

```text
$ fossil ticket show 0 "gossil_kind='task'" --user me -R ~/demo/demo.gossil | cut -f 2,6,8,13
tkt_uuid	status	priority	title
09303112e37e88bb29792eeda4767a6b961b0b3d	"open"	null	"Write the guide"
e3faa7a638bcd39e3b0cb02489be6026b357236b	"done"	1	"Fix the parser"
9dfb5cf38352b84a4d505c81f4b41b05da412200	"open"	null	"Check stock Fossil"
```

Each value is the JSON value that Gest stores, so a text value is in quotes.

## Rebuild a copy with stock Fossil

Run `fossil rebuild` on a copy only, and never on the repository that you use.
`fossil rebuild` makes the tables of Fossil again from the stored artifacts.
It drops each table whose name starts with `g_`, and Gossil keeps its own state
in those tables:

```text
$ cp ~/demo/demo.gossil ~/demo/copy.gossil
$ fossil rebuild ~/demo/copy.gossil
  100.0% complete...
$ fossil sql -R ~/demo/demo.gossil "SELECT count(*) FROM sqlite_schema WHERE name GLOB 'g_*'"
96
$ fossil sql -R ~/demo/copy.gossil "SELECT count(*) FROM sqlite_schema WHERE name GLOB 'g_*'"
0
```

Stock Fossil still reads the copy. `fossil test-integrity` reports 0 errors,
and `fossil timeline` and `fossil ticket show` list the same check-ins and
tasks. Only the row numbers of the tickets can change. `gos v open` refuses the
copy and leaves it unchanged:

```text
$ mkdir ~/demo/other
$ cd ~/demo/other
$ gos v open --repository ~/demo/copy.gossil --checkout work --root ~/demo/other
SQLite failure: `no such table: g_storage`
```

The command exits with status 1 and writes no file in `~/demo/other`. The
message is the raw storage error of this build.

Run `just issue-demo` in the source checkout to repeat these checks with the
pinned Fossil 2.29. It runs the ignored test of `tests/issue_demo.rs`.

## Limits of this build

This build is the first part of the issue commands, and it has these limits:

- **Four commands run.** They are `init`, `task create`, `task list` and
  `task show`. Each other issue command exits with status 69.
- **Tasks are private.** No command shares a task or syncs it.
- **No command changes a task.** There is no edit, undo or delete, and a task
  has no tags, links, notes, iteration or assignment.
- **A text field cannot hold `[`.** `task create` refuses a `[` in the title,
  the description or the metadata, because Markdown backlinks are not ported.
- **The text output is provisional.** Its bytes can change. Use `--json` in a
  script.
- **Run `fossil rebuild` on a copy only.** Gossil refuses a rebuilt repository,
  as the section above shows.
