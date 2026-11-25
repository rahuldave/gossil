# Read tasks with task list and task show

The `issue task list` command prints the tasks of a project, and `issue task show` prints one task. Both
read the private tasks that the [issue ticket writer](issue-ticket.md) stored. A task ID is the 32-letter
Gest ID of the task, with each letter from `k` to `z`.

The text output of both commands is provisional. It is plain and stable text with no color. It is not
the styled table of Gest, and a later build can change it. The JSON output follows Gest.

The code is in `src/product/issue_read.rs`. The parsers, the selection and the printers are pure
functions. The two handlers take a repository that the caller opened. No command calls the handlers
yet. The next commit connects them to `gos i task list` and `gos i task show`.

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
or `--status=done`. `-s=` gives an empty value, which is not a valid status. Each option can appear
only once, and so can `--no-color` and `--no-pager`. Only `-v` can repeat, because the parser counts it.
As in the Rust parser Clap, a value in its own argument cannot start with `-`, unless it is `-` alone. So
`-s -x` has a missing value.

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

The command exits with 74 when admission refuses the repository, or when the command cannot write the
output.

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

The unit controls in `tests/support/product_issue_read.rs` check the parsers, the lookup, the selection
and the exact bytes of each printer. Further controls build a catalog 6 repository through the library,
with a project and four tasks, two of them after a private check-in. Then they run both handlers and
check:

- the order, the default filter, `--all`, `--status` and the project filter
- the JSON, parsed back and compared with the stored fields
- a unique prefix, a missing one, an ambiguous one and an invalid one
- a changed row and a failed write, each of which exits with 74

`just issue-read-oracle` compares the JSON with a reference `gest`. It creates two tasks with `gest`,
stores the same values in a Gossil repository and requires equal bytes from `task show --json`, `task
list --json` and `task list --all --json`. It runs the `gest` that `GOSSIL_GEST_REFERENCE` names, or else
the `gest` on the path. The installed `gest` 0.6.0 is a fork, not the Gest pin `eaf821b`, so a pass
checks that build only. Without a `gest`, the recipe exits with 5, which means `not-run` and not a pass.

## Recommendations

Keep the selection and the lookup equal to Gest `repo::task::all` and `resolve_id`. Treat the text
output as provisional, and parse the JSON in scripts.
