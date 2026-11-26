# Read tasks with task list and task show

The `issue task list` command prints the tasks of a project, and `issue task show` prints one task. Both
read the private tasks that the [issue ticket writer](issue-ticket.md) stored. A task ID is the 32-letter
Gest ID of the task, with each letter from `k` to `z`.

The text output of both commands is provisional. It is plain and stable text with no color. It is not
the styled table of Gest, and a later build can change it. The JSON output follows Gest.

The code is in `src/product/issue_read.rs`. The parsers, the selection and the printers are pure
functions. The two handlers take a repository that the caller opened. The commands `gos i task list`
and `gos i task show` open the repository of the checkout and call the handlers, as the
[CLI reference](cli.md#list-the-tasks-of-the-project) says. `task list` lists the tasks of the project
bound to the checkout. Before `gos i init` binds a project, both commands exit with 78.

## Options

`task list` accepts these options:

- `-a` or `--all` shows tasks in every status. Without it, the list leaves out each task that is `done`
  or `cancelled`.
- `-s STATUS` or `--status STATUS` keeps only the tasks in that status. As in Gest, the default list
  leaves out a `done` task also when `--status done` asks for it. To list the done tasks, use
  `--all --status done`.
- `-j` or `--json` prints JSON.

`task show` takes one task ID or prefix, and `-j` or `--json`. Both commands accept the global
`--no-color` and `--no-pager`. The two options change nothing, because the output has no color and no
pager.

Short options can share one argument, as in `-aj`, and a value can follow its option, as in `-sdone`
or `--status=done`. `-s=` gives an empty value, which is not a valid status. As in the Rust parser
Clap, a value in its own argument cannot start with `-`, unless it is `-` alone. So `-s -x` has a
missing value.

Each option can appear only once, and so can `--no-color` and `--no-pager`. Only `-v` can repeat,
because the parser counts it. As in Clap, the parser reports a repeated option at the letter that
repeats it, also when the first copy is in an earlier argument. It does so before it reads a later
unknown letter. So `-aah` and `-a -ax` both fail with `the argument '--all' cannot be used more than
once`.

The parser also accepts the other options of the Gest pin, which is the Gest commit `eaf821b` that this
build follows. This build does not support those options, so the command then exits with 69:

- `task list`: `--assigned-to`, `--limit`, `-t` or `--tag`, `-q` or `--quiet`, `-r` or `--raw`, and `-v`
  or `--verbose`
- `task show`: `-q` or `--quiet`, `-r` or `--raw`, and `-v` or `--verbose`

The parser fails, with exit status 2, on each of these arguments:

- an unknown option, a repeated option, a missing value, or a value given to an option that takes none
- an invalid status, or a `--limit` value that is not a whole number
- an extra argument, or no ID for `task show`
- an argument that is not UTF-8

The parser checks every argument before the command refuses an unsupported option. So `--limit x`
exits with 2, and `--limit 5` exits with 69.

## What task list and task show check

`task list` reads each typed task row of the repository with its ticket ID and artifact name. It
replays no artifact, so its cost grows with the count of tasks and not with the rest of the
repository. It does not compare the rows with the artifacts. `gos v verify` does.

`task show` reads only the task rows whose ID starts with the prefix. Then it proves the one task, as
`gos v verify --issue` does. It replays the artifact of the task, compares the rows of the task with that
replay, and prints the task from its artifact. Both commands first run the checks of each command, which
the [native storage guide](native-storage.md#check-a-repository) lists.

## Order of the list

The newest `created_at` comes first, as in Gest. When two tasks have the same `created_at`, the task that
was stored later comes first. The caller can give a project ID, and then the list keeps only the tasks
of that project.

## Find a task by prefix

`task show` finds a task as Gest `resolve_id` does:

1. The prefix must have 1 to 32 letters, each from `k` to `z`. So an uppercase letter is invalid, and the
   command exits with 64.
2. The command searches the tasks whose status is not `done` or `cancelled`. If exactly one task ID
   starts with the prefix, that task is the result.
3. If none of those matches, the command searches every task.

The search covers the tasks of every project. It does not match a project ID. The command exits with
66 when the prefix matches no task, or when the first search with a match finds more than one task. The
messages are Gest's:

```text
not found: id prefix 'lz'
ambiguous id prefix 'lq': matches 2 entities
Id prefix must contain only characters k-z, found 'L'
```

The command exits with 74 when open, its checks or the proof of the task refuse the repository, or
when the command cannot write the output.

Both commands take the write lock of the repository, as every issue command does. A read can wait for
that lock past its timeout of 15 seconds and refuse with `database is locked`. Then the command opens the repository
again and tries once more. After a second refusal, it exits with 74. A commit of another process between
two commands needs no retry, because each command checks what it reads in its own transaction.

## Text output

`task list` prints one line for each task. Two spaces separate the four fields of the line:

- the short ID, which is the first 8 letters of the task ID
- the status, padded to 11 characters
- the priority, as `P` and its number, padded to 4 characters
- the title

A missing priority prints `--`. An empty list prints nothing.

```text
lqsppppp  in-progress  P1    Task p
lqrooooo  open         P1    Task o
lmommmmm  open         P1    Task m
```

`task show` prints one labelled line for each stored field, then the description. It indents each line
of the description by four spaces:

```text
id           lqrooooooooooooooooooooooooooooo
title        Task o
status       open
priority     1
project      kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk
assigned to  none
metadata     {}
created at   2026-11-18T12:00:03Z
updated at   2026-11-18T12:00:03Z
resolved at  none
ticket       0000000000000000000000000000000000000004
artifact     <the 64-character native name>
description
    First line of the description
```

The `ticket` line gives the K card, and the `artifact` line gives the native name of the creation
artifact. A missing value prints `none`. The metadata is compact JSON. Times are RFC 3339 in UTC, as
the JSON writes them.

A control character in a title prints as a Rust escape, such as `\t`, so that one field cannot break
the layout. The description keeps its line breaks and escapes each other control character.

## JSON output

The JSON matches Gest's envelopes, pretty-printed, with a final newline. `task list --json` prints an
array, and an empty list prints `[]`. Each object has the eleven fields of the Gest task model. Their
order is `assigned_to`, `created_at`, `description`, `id`, `metadata`, `priority`, `project_id`,
`resolved_at`, `status`, `title` and `updated_at`. Then come `relationships` and `tags`. `task show
--json` prints one object, with `notes` before `relationships`, as Gest does.

This build stores no notes, relationships or tags. So the three lists are always empty, as Gest prints
them for a task without any. The status `in-progress` prints as `inprogress`, because Gest's serializer
spells it that way.

A task in this build has no field that Gest's JSON lacks. So every field matches, as long as Gest's own
output is the reference.

## Tests

`tests/issue_read_cli.rs` runs the two commands through both executables and both group names. It
builds a checkout with `gos v init`, `gos v open` and `gos i init`, and stores four tasks through the
library, one of them in another project. A task needs its project, so the test stores each project
first. It checks the output of each form, each exit status from 2
to 78, the help forms and a closed stdout. After each read it checks that no byte of the repository
changed.

`tests/issue_task_cli.rs` also reads back the tasks that `gos i task create` stores, through
both commands in each form. It compares the listed IDs with the stored IDs as a set.

A further test in that file holds the write lock from another connection for 7 seconds. A `task list`
and a `task show` that start meanwhile both exit with 0 and print the task. Each gets the lock within the busy
timeout of 15 seconds. The unit test of `crate::vcs_open::retry_locked` checks the one retry after a
timeout. The test then runs a `task create` and a `task list` at the same time, and both exit with 0.

The unit controls in `tests/support/product_issue_read.rs` check the parsers, the lookup, the selection
and the exact bytes of each printer. Further controls build a catalog 6 repository through the library,
with a project and four tasks, two of them after a private check-in. Then they run both handlers and
check:

- the order, the default filter, `--all`, `--status` and the project filter
- the JSON, parsed back and compared with the stored fields
- a unique prefix, a missing one, an ambiguous one and an invalid one
- a failed write, which exits with 74
- a task row that another connection changes, which the list prints and the show refuses with 74
  before any output. A new open admits the repository, and `verify` refuses it.

`just issue-read-oracle` compares the JSON with a reference `gest`. It creates two tasks with `gest`,
stores the same values in a Gossil repository and requires equal bytes from `task show --json`, `task
list --json` and `task list --all --json`. It runs the `gest` that `GOSSIL_GEST_REFERENCE` names, or else
the `gest` on the path. The installed `gest` 0.6.0 is a fork, not the Gest pin `eaf821b`, so a pass
checks that build only. Without a `gest`, the recipe exits with 5, which means `not-run` and not a pass.

## Recommendations

Keep the selection and the lookup equal to Gest `repo::task::all` and `resolve_id`. Treat the text
output as provisional, and parse the JSON in scripts.
