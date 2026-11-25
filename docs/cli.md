# CLI reference

The `gossil` executable initializes empty repositories, opens native checkouts
and has three delta file diagnostics. The prepared source also has a
[glob diagnostic](#inspect-a-fossil-glob-list), with a
[focused source checkpoint](glob.md#current-source-status). This source checkout also has
[explicit add and tracked status](#add-explicit-paths-and-inspect-pending-additions),
plus [scope selection and private recording](#select-scopes-and-record-private-files)
and [complete recorded ancestry](#read-complete-recorded-ancestry).
`gos` is an alias for `gossil`, and `v` is an alias for `vcs`.
The [`issue` group](#issue-commands), or `i`, recognises each Gest command.
This build runs [`issue init`](#create-the-gest-project-of-a-checkout),
[`issue task list`](#list-the-tasks-of-the-project) and [`issue task show`](#show-one-task),
and refuses the others.

Each diagnostic runs without a repository. Run it from this source checkout with `just gossil`, which builds
and runs the executable through Cargo. Cargo may print build messages on stderr.

| Command | Arguments | Successful output |
|---|---|---|
| `vcs test-delta-create` | `ORIGIN TARGET DELTA` | Writes `DELTA`; no stdout |
| `vcs test-delta-apply` | `ORIGIN DELTA TARGET` | Writes `TARGET`; no stdout |
| `vcs test-delta` | `FILE1 FILE2` | Prints `ok` and a newline |
| `vcs test-glob` | `PATTERN STRING ...` | Prints SQL, patterns and match indices; prepared source |

```sh
just gossil vcs test-delta-create ORIGIN TARGET DELTA
just gossil vcs test-delta-apply ORIGIN DELTA TARGET
just gossil vcs test-delta FILE1 FILE2
```

Replace capitalized arguments with file paths. The commands accept exactly the
number of paths shown. Each diagnostic requires the `vcs` or `v` group.
The [help section](#get-help) says how to list the commands.
File data comes from paths and goes to paths. Stdin and stdout do not carry it.

## Get help

Both executables print help. The help lists the groups and commands that this
build runs. It also names the planned groups, and marks them as not available.

| Command | Output |
|---|---|
| `gossil`, `gossil --help`, `gossil -h`, `gossil help` | Product help: the groups and the commands of `vcs` and `issue` |
| `gossil vcs`, `gossil vcs --help`, `gossil vcs -h`, `gossil vcs help` | Help of the `vcs` group |
| `gossil vcs COMMAND --help`, `gossil vcs COMMAND -h` | Usage of one command |
| `gossil vcs help COMMAND`, `gossil help vcs COMMAND` | The same usage of one command |
| `gossil issue`, `gossil issue --help`, `gossil issue -h`, `gossil issue help`, `gossil help issue` | Help of the `issue` group |
| `gossil issue task create --help`, `gossil issue help task create` | Usage of one issue command |

Each form works with `gos` in place of `gossil`, `v` in place of `vcs` and `i`
in place of `issue`. Help goes to stdout, and the executable exits with status 0.
A help request reads and writes no file.

The product help also names the planned groups `ui` (`u`) and `serve` (`s`).
This build does not run them.

Before a group or a command name, `--help`, `-h` and `help` mean the same.
Several of them in a row count as one. So `gossil help help` prints the product help, and
`gossil vcs --help open` and `gossil vcs help help open` print the usage of `open`.

`init`, `new`, `info`, `timeline` and `log` print their own help, which the
sections below describe. So do `issue init`, `issue task list` and `issue task show`. Every other command prints its usage line and one
sentence about what it does.

After a `vcs` command, only a lone `--help` or `-h`
asks for help. The [issue section](#issue-commands) gives the rule after an issue command. With more arguments, the command parses `--help` by its own
grammar, so `vcs init -- --help` creates a file named `--help`. The word `help`
after a command is an ordinary argument, so `vcs init help` creates a repository
named `help`.

An unknown name fails with status 2 and two lines on stderr. The second line
points to the help:

```text
gossil: unknown command "frob"
Run "gossil --help" to list the commands.
```

An unknown command after `vcs` gives `gossil: unknown vcs command "NAME"`, then
`Run "gossil vcs --help" to list the vcs commands.` A planned group gives
`gossil: the group "NAME" is planned and not available in this build`. The
[issue section](#issue-commands) gives the errors of the `issue` group. A diagnostic
without its group, such as `gossil test-delta-apply`, is an unknown command too.
A help request with an argument after the command name, such as
`vcs help open extra` or `help vcs open status`, fails with status 2. The first
line on stderr is then `Usage: gossil vcs help COMMAND`.

## Issue commands

The `issue` group, or `i`, recognises every Gest command that the Gossil CLI
specification lists (CLI-02).
The top-level commands are `artifact`, `config`, `generate`, `init`, `iteration`, `migrate`, `project`,
`purge`, `search`, `self-update`, `tag`, `task`, `undo` and `version`, with
their subcommands. They keep the aliases of the Gest source:

- `a`, `i`, `p`, `t` and `u` stand for `artifact`, `iteration`, `project`, `task` and `undo`.
- `grep` stands for `search`.
- `new`, `rm`, `ls`, `view` and `edit` stand for `create`, `delete`, `list`, `show` and `update`.
  Under `iteration`, `rm` stands for `remove`.
- `delete` stands for `unset` under `meta`.

Help shows the aliases that Gest shows, and hides `a`, `i`, `t`, `u`, `grep`
and `delete` under `meta`, as Gest does.

This build runs `init`, `task list` and `task show`, which the
[sections below](#create-the-gest-project-of-a-checkout) describe. It recognises
each other command and refuses it:

```text
$ gos i task create "First task"
gossil: issue task create is not available in this build
```

The refused command then exits with status 69, the code of an invalid state. It reads and
writes no file, and it does not open or create a database. The parser checks
the command names only. Arguments after the last name, such as `"First task"`,
are not checked.

A `--help` or `-h` among the arguments prints the help of the command. So
does a cluster of short flags that reaches `h`, such as `-ah` after `task list`. Each letter
before `h` must be a flag of the command that takes no value, and no letter can come twice.
Only `v` can come twice, because Clap counts it. So `-vvh` asks for help, and `-aah` and
`-jjh` do not.

As in Clap, an option that takes a value
reads the rest of the cluster as its value, so `-sh` gives the status `h`.
Only `init`, `task create`, `task list` and `task show` read such clusters. After each
other command, only `--help` and `-h` print its help.
Clap also rejects a letter that repeats a flag of an earlier argument, but this build checks
each argument on its own. So Gest exits with 2 on `task list -a -ah`, and this build prints the help.
After `--`, `--help` and `-h` are ordinary arguments, so the build refuses
`gos i task create -- -h` like any other task creation.

| Input | Status | Message on stderr |
|---|---|---|
| A recognised command that this build does not run, such as `i undo` or `i t new` | 69 | `gossil: issue PATH is not available in this build` |
| `artifact`, `config`, `generate`, `iteration`, `task` or a `note` group without a subcommand | 2 | `gossil: issue PATH needs a command` |
| An unknown name, such as `i frob` or `i task frob` | 2 | `gossil: unknown issue command "NAME"` |
| `i serve` or `i s` | 2 | The unknown-name message, because the root groups `ui` and `serve` replace Gest's server command |
| A help request with an argument after a command that has no subcommands, such as `i help task create x` | 2 | `Usage: gossil issue help COMMAND` |

`PATH` is the canonical command path, such as `task create`, even when the
user typed an alias. In the unknown-name message, `NAME` starts with the
canonical path of its group, so `i t frob` gives `"task frob"`. Each status 2
message has a second line that points to the help.

Gest runs `project`, `tag` and each `meta` without a subcommand, so each of
them gives status 69 when it stands alone. A name after one of them that is
not its subcommand is an argument, so `i p frob` also gives status 69. A help
request reads that name as a command name, not an argument, so `i help p frob`
and `i help t meta ID` give status 2 with the unknown-name message. A `--help`
or `-h` right after `project`, `tag` or `meta` also makes the next name a
command name. Then `i p --help frob` and `i t meta -h ID` give status 2 too. After
the argument, as in `i p frob --help`, the `--help` prints the help of the
command.

This build does not yet recognise the global options `-v`, `--no-color` and
`--no-pager`, or `--version` at the group root. Before a command name, they
fail as unknown names. `init` accepts the first three after its name, as the
next section says. `task list` and `task show` accept `--no-color` and
`--no-pager` after their names, and refuse `-v` with status 69.

### Create the Gest project of a checkout

`gos i init`, or `gossil issue init`, creates the Gest project of the open
checkout that holds the current directory. It stores the project as a private
project ticket, and it binds the checkout to that project. A project ticket is
the Fossil ticket that holds one Gest project, as the
[issue ticket guide](issue-ticket.md) says. All four forms of executable and group
run the same command.

First, make the checkout with `vcs init` and `vcs open --root`. Then run `init`
in the checkout root or in a folder below it:

```text
$ cd /home/alice/project
$ gos v init /home/alice/project.fossil
$ gos v open --repository /home/alice/project.fossil --checkout work --root /home/alice/project
Opened checkout work at /home/alice/project (repository fc0dd44fb5dcb4aaecff68e771149f0698cbec87, head 397b39fc09ae52025628af5f7fbddefae0bbf53301664e2965cc279e89141c4f)
$ gos i init
  ✓  initialized project  lkpunroy

  root  /home/alice/project
```

The short ID is the first 8 letters of the Gest ID of the project. The line
`root` names the checkout root. Gest prints the current directory there, and Gossil
prints the checkout root. The output has no color.

`init` takes no repository or checkout selection. It finds the nearest `.gossil`
marker at or above the current directory, and reads the locator that `vcs open`
published. The locator selects the repository and the checkout. The command then
admits them with the same rules as `vcs open`, configuration included. It never
creates a repository, a checkout registration or a locator.

In one transaction, `init` writes these rows:

- the rows of a private project ticket, as the
  [issue ticket guide](issue-ticket.md#bind-a-project-to-a-checkout) lists them
- one `g_project_binding` row, with the checkout ID, the Gest ID and the checkout root
- one `g_checkout_project` row, with the checkout ID and the Gest ID

The ticket ID is 40 random hexadecimal digits, and the Gest ID is random. The
date and both timestamps hold the current time. The user is the local login, the
first nonempty value of `GOSSIL_USER`, `USER`, `LOGNAME` and `USERNAME`, or else
`root`. A failure before the commit leaves every byte of the repository unchanged.

A second `init`, or an `init` in a folder below the root, prints the same message
for the bound project. It changes no byte. Gest also keeps the project that it
finds. After `init`, every `vcs` command still works, and `init` works
both before and after the first commit.

`init` accepts `--local`, `--no-color`, `--no-pager` and `-v`, `-vv` or
`--verbose`, and ignores each of them. The locator already selects the
repository, so `--local` changes nothing. Every other argument fails with
status 2:

```text
gossil: unexpected argument "--json" for issue init
Usage: gossil issue init [OPTIONS]
```

A lone `--help` or `-h` prints the help of `init`. The other exit statuses
follow CLI-05, the list of exit codes in the Gossil CLI specification:

| Case | Status | Message on stderr |
|---|---|---|
| Success, also for a checkout with a bound project | 0 | None |
| An argument that `init` does not take | 2 | `gossil: unexpected argument "X" for issue init` |
| No marker, a marker without a locator, or a repository without `vcs open` | 78 | `gossil: no open checkout holds this directory; run gos v init and gos v open first` |
| A malformed locator | 78 | The locator error |
| A configured data directory that conflicts with the locator, or that holds no `gossil.db` | 78 | `configured storage conflicts with explicit repository` |
| A repository at an older catalog | 78 | The older-catalog message of the [exit section](#exit-status-and-errors) |
| A local login that is not UTF-8 | 78 | `gossil: the local login is not UTF-8; set GOSSIL_USER` |
| A failure of storage or admission, for example a changed binding row | 74 | The storage error, such as `unsupported project binding state` |
| A repository that already holds a project ticket, while the checkout has no project | 74 | The refusal below |
| A missing repository file, or another I/O failure | 74 | The I/O error |
| A failed write of the success message to stdout, after the commit | 74 | `gossil: issue init kept the project, and stdout failed: ERROR` |

For a repository that already holds a project, the storage error holds this text:

```text
issue init needs one project creation, a registered checkout with no bound project, and a repository with no project; binding an existing project is not available in this build
```

This build has these limits:

- The project is private only.
- `init` writes no audit row and no undo history.
- A text field cannot hold `[`.
- `init` writes no selectors, tags or links.
- `init` does not bind a project that the repository already holds. Gest binds
  the one live project, or refuses several of them as ambiguous. Gossil refuses
  both cases with status 74.
- Repositories of older Gossil versions are not supported.

### List the tasks of the project

`gos i task list`, or `gos i t ls`, prints the tasks of the project that `init`
bound to the open checkout. It finds the repository as `init` does, and it writes
nothing. The newest task comes first, and the list leaves out each task that is
`done` or `cancelled`:

```text
$ gos i task list
lqrooooo  in-progress  P1    Task o
lmommmmm  open         P1    Task m
```

The text output is provisional, as the [issue read guide](issue-read.md#text-output)
says. `--json` prints the JSON of Gest. The guide gives the order, the filters and
both forms of output. `task list` accepts these options:

- `-a` or `--all` adds the tasks that are `done` or `cancelled`.
- `-s STATUS` or `--status STATUS` keeps the tasks in that status. Without `--all`, a
  `done` or `cancelled` task stays out.
- `-j` or `--json` prints JSON.
- `--no-color` and `--no-pager` change nothing.

The Gest pin is the Gest commit that this build follows. Its other options are
`--assigned-to`, `--limit`, `-t` or `--tag`, `-q` or `--quiet`, `-r` or `--raw`, and
`-v` or `--verbose`. The command exits with status 69 on each of them:

```text
gossil: issue task list option --limit is not available in this build
```

### Show one task

`gos i task show ID`, or `gos i t view ID`, prints one task. The ID can be the
full 32-letter Gest ID or a prefix of it that names one task. The command searches
the tasks of every project, as Gest does, but it still needs a bound project.
It accepts `-j` or `--json`, `--no-color` and `--no-pager`. It refuses `-q` or
`--quiet`, `-r` or `--raw`, and `-v` or `--verbose` with status 69. The
[issue read guide](issue-read.md#find-a-task-by-prefix) gives the search and the
output.

The two read commands exit with these statuses of CLI-05:

| Case | Status | Message on stderr |
|---|---|---|
| Success | 0 | None |
| A parser failure, such as an unknown option, an extra argument or no ID | 2 | `gossil: ` and the problem, then the usage line |
| An option that this build does not support | 69 | `gossil: issue task COMMAND option --NAME is not available in this build`, where `COMMAND` is `list` or `show` |
| An invalid prefix, such as `L` | 64 | `gossil: Id prefix must contain only characters k-z, found 'L'` |
| A prefix that names no task | 66 | `gossil: not found: id prefix 'PREFIX'` |
| A prefix that names more than one task | 66 | `gossil: ambiguous id prefix 'PREFIX': matches N entities` |
| No open checkout holds the directory | 78 | `gossil: no open checkout holds this directory; run gos v init and gos v open first` |
| The checkout has no bound project | 78 | `gossil: this checkout has no Gest project; run gos i init first` |
| A failure of storage or admission | 74 | The storage error |
| A failed write to stdout | 74 | `gossil: cannot write the output: ERROR` |

The other failures of the locator and the configuration give the statuses that the
[`init` table](#create-the-gest-project-of-a-checkout) lists. The parser checks the
arguments before the command opens the repository. So a parser failure or an
unsupported option exits with 2 or 69 even outside a checkout.

## Initialize a repository

```sh
just gossil vcs init --admin-user alice --date-override '2026-11-18 12:34:56' NEW_REPOSITORY
just gossil vcs new --sha1 ANOTHER_NEW_REPOSITORY
```

Initialization accepts exactly one destination. Both executable names and both
group names support `init` and its `new` alias. The destination must be new.
The command refuses existing files, symlinks and SQLite recovery sidecars.

| Option | Behavior |
|---|---|
| `--admin-user USERNAME`, `-A USERNAME` | Set the initial administrator |
| `--date-override DATETIME` | Set the root date; default `now` |
| `--sha1` | Use SHA1 for the initial root and native hash policy |
| `--help`, `-h` | Print initialization help and exit successfully |
| `--` | Treat following arguments as literal destination names |

Long value options also accept `--name=value`. Without an explicit administrator,
the command selects the first nonempty value from `GOSSIL_USER`, `USER`,
`LOGNAME` and `USERNAME`, in that order. If all are absent or empty, it uses `root`.
No preference file is read or created.

Administrator bytes reach native validation unchanged. The storage API requires valid UTF-8.
Repository filenames retain native bytes, including names beginning with `file:`.
Dates must be UTF-8 and accepted by the native date provider.

The receipt prints the committed project and server identifiers, administrator
and initial remote-access password. Keep that password private. A receipt write
failure returns status 1 after commit and leaves the repository initialized.
Other initialization failures also return status 1. Initialization help returns 0.

The command creates an empty native root without a checkout. Templates, project
display fields and global preference options are unsupported and rejected before
creation. Use the separate open command below to register or reopen one
empty-root checkout. The scope and commit commands below record private history.
The bounded `timeline` and `log` commands below expose the
[`recorded_timeline` API](native-storage.md#read-recorded-checkout-ancestry).
Revision selection and broader history commands remain deferred.

An already installed mini-VCS preview has a separate
[command contract](working-with-repositories.md#try-the-installed-development-preview).
This guide does not record a new installation or publication.

## Open a native checkout

First registration binds one existing directory to the initial empty root.
Reopen also admits the supported private history written by the library. The locator is
an owner-local lookup file at `ROOT/.gossil/checkout.json`. Neither operation
selects files, creates file history or restores working files. The source supports open and locator recovery. Installation and publication
require separate delivery evidence.

```sh
just gossil vcs open --repository PATH --checkout ID --root ROOT
```

| Option | Behavior |
|---|---|
| `--repository PATH` | Required existing local repository database |
| `--checkout ID` | Required exact checkout ID, with UTF-8 text |
| `--root ROOT` | Existing directory for explicit first registration or same-root reopen |

Options take separate values and each may appear once, in any order. Unknown
options, duplicate options, positional arguments and missing values fail. Open
has no `--register`, `--name=value` or default repository selection. A lone
`--help` or `-h` prints the usage of `open`, as the [help section](#get-help) says.
Use either `gossil` or `gos`, with either `vcs` or `v`.

For first registration, create the repository separately with `vcs init`, then
supply `--root`. Open requires an existing database and never creates one.
The root can contain ordinary files and nested Git repositories. Their bytes
remain unselected and unchanged. Native VCS open requires no issue, project or
author initialization. It does not grant Gest project authority.

The native library admits catalog6 with exactly zero or one registration,
the supported tracking, scope and private recording history, and private issue
tickets. Catalog1, unrelated file history and a second registration are outside
this command. The command refuses a repository at an older Gossil catalog, 2, 4
or 5, before any effect, with exit 78. The [storage guide](native-storage.md)
defines those catalog formats.

Native admission checks the repository and owner binding,
canonical root, device/inode and complete supported state. The locator cannot
replace those checks. See [native storage](native-storage.md#register-one-existing-directory)
for identity, root and resource limits.

### Reopen and recover

For an existing registration, omit `--root` and invoke open from its root or a
nested directory:

```sh
cd ROOT
/path/to/gos v open --repository PATH --checkout ID
```

Use an absolute database path when changing directories. Open searches from the
canonical current directory toward the filesystem root and stops at the nearest
`.gossil` marker. It does not skip an incomplete, unsafe or conflicting marker
to select an ancestor. Explicit repository and checkout flags must agree with
both the locator and admitted database registration.

An explicit `--root` preserves that exact requested root. If discovery finds a
marker at an ancestor, first registration refuses rather than reusing the
ancestor checkout. Repeating open with the same root and an existing valid
locator reuses the admitted registration and syncs the locator directory.

If `checkout.json` is missing, rootless open can reconstruct it from the exact
existing registration. When no marker exists, run from the registered root
itself. Recovery authenticates exactly one matching registration and preserves
its identities, native keys and registration count. It allocates no checkout
and repairs no database state. An incomplete marker prevents a new registration
through `--root`. Malformed or conflicting locators remain conflicts and are
never replaced.

### Configuration before mutation

Open loads inherited Gest configuration from the invocation directory, even
when `--root` names another directory. Explicit repository flags do not bypass
configuration checks. Layers merge in this order, with later values taking
precedence:

1. The file named by `GEST_CONFIG`, or the platform configuration directory's
   `gest/config.toml` when that variable is absent.
2. Each ancestor from the filesystem root through the canonical invocation
   directory, reading `.config/gest.toml` then `.gest.toml` in each directory.
3. Environment values with the `GEST_` prefix. Double underscores separate
   nested keys, and key names become lowercase.

A usable absolute `storage.data_dir` explicitly selects `gossil.db` inside that
directory. Its canonical existing database must match `--repository`. A
directory that holds no `gossil.db` conflicts too. The exact
uppercase `GEST_STORAGE__DATA_DIR` accepts an absolute native-byte path without
UTF-8 loss. An effective relative path is not an explicit selection and does
not revive a value from an earlier configuration layer.

Malformed TOML, unreadable configuration files and invalid table or value types
fail before marker or database mutation. A configured `database.url` or
`database.host` is unsupported and also fails, including with explicit CLI
flags. These errors do not print the configured provider value. Typed I/O, TOML
and JSON causes remain inspectable through the retained error chain. Configuration
supplies no implicit database creation or fallback through an incomplete marker.

The configuration adapter retains Gest's MIT attribution at pinned source
`eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8` in `src/checkout_config.rs`.

### Locator bounds and permissions

The locator contains exactly `version`, `repository_path_hex`, `repository_id`
and `checkout_id`. Version is integer 1. The path is the actual absolute Unix
database filename, encoded as lowercase hexadecimal without losing native
bytes. The repository ID is the admitted 40-character lowercase identity.
The checkout ID names the real persisted registration.

Reading is bounded to 65536 bytes, including whitespace. Parsing rejects
unknown or duplicate fields, trailing data and malformed versions, identities
or path encodings. Decoded database paths are bounded to 16384 bytes and must
be absolute and NUL-free. Locator checkout IDs are bounded to 8192 UTF-8 bytes.
The native registration API separately requires its smaller 1024-byte ID limit.

The marker and locator must belong to the effective user and must not be
symlinks. A marker must be a directory without group or other write permission.
A locator must be a regular file without group or other permissions. New marker
directories use mode 0700, and new temporary files use mode 0600. Open refuses
unsafe objects and preserves their bytes and permissions.

### Publication failures

First registration prepares complete bounded locator bytes in an exclusively
created temporary file inside the marker directory. It flushes and syncs that
file before changing the database. After checked registration commit, it
publishes through a hard link that cannot overwrite `checkout.json`, then syncs
the marker directory. An identical existing locator also requires directory sync.
There is no replacement or copy fallback when linking fails.

| Failure phase | Preserved result and next action |
|---|---|
| `Preflight` | No new registration was committed by this locator attempt; resolve the input or namespace conflict |
| `CommittedUnpublished` | Registration remains committed; retry without `--root` from the registered root to reconstruct the missing locator |
| `DurabilityUnconfirmed` | Locator is present but directory durability is unconfirmed; preserve it and retry the same admitted open |
| `Published` | Locator is present and its directory was synced; owned temporary cleanup failed |

Cleanup targets only this attempt's temporary pathname with its recorded
identity. Unrelated crash leftovers and replacement objects are preserved.
Callers must keep the namespace stable. The contract does not promise atomic
filesystem/database commit or protection against hostile concurrent replacement.
After all durable locator pins are lost, recovery cannot prove historical
identity against malicious database substitution.

Success prints `Opened checkout ID at ROOT (repository REPOSITORY_ID, head HEAD)`
and returns status 0. `HEAD` is the shared head, which stays at the empty root
when the library records private history. Failures return status 1 with a diagnostic on stderr.
Locator diagnostics name their publication phase. Internal error categories do
not change this process exit status. The one exception is a repository at an
older Gossil catalog, which returns status 78, as
[Exit status and errors](#exit-status-and-errors) says.

A receipt write failure can occur after
successful publication. It preserves the committed registration and locator,
so a failed command does not imply that nothing committed.

## Add explicit paths and inspect pending additions

This source checkout provides `vcs add` and `vcs status` at the initial root
or an admitted private recorded head. Add records tracking metadata. Status lists
all tracked paths, their pending or recorded state, and their actual scope.
Use the separate scope and commit commands below to select scopes and record files.

First, use `vcs open` to publish or recover the locator.
Then run these commands from the registered checkout root or one of its descendants:

```sh
/path/to/gos v add --repository /absolute/history.gossil --checkout work --path notes/plan.md --path notes/review.md
/path/to/gos v status --repository /absolute/history.gossil --checkout work
```

Both `--repository` and `--checkout` are required exactly once. Repeat `--path PATH` for each file.
There are no positional path lists. Paths are exact UTF-8 names relative to the
registered root, regardless of the invocation directory. Status accepts no paths.

Neither command accepts `--json`, `--root`, scope switches or recursive selection.
A lone `--help` or `-h` prints the usage of the command.
Both executable names and both group aliases use this grammar.

Add accepts existing regular files only. It rejects absolute paths, empty
components, `.` and `..`, backslashes, duplicate paths within a batch, case aliases,
symlinks and special files. On macOS, paths must also be ASCII. This implementation rejects non-ASCII
paths because their filesystem name comparisons have not been qualified.

The `.gossil` namespace, selected database, SQLite sidecars and
known hard-link aliases are protected. Parent directories and leaves are opened
without following symlinks, and their identities are rechecked before commit.
Keep the filesystem namespace stable during the command. These checks do not
promise protection against hostile concurrent replacement.

The CLI bounds the complete tracked inventory and each batch to 4096 paths.
Each path has at most 4096 bytes and 128 components. Add temporarily captures files to validate the batch. It limits those initial
captures to 64 MiB of working bytes and uses a 64 KiB read buffer.
The final recheck temporarily holds a second copy of one file alongside those
captures, so captured bytes can total 128 MiB. These bounds exclude other process
allocations.

Native admission has separate finite bounds. If any bound is
exceeded, the command returns an error.

Add persists tracking metadata only. Earlier additions remain tracked across
batches and reopen. A successful batch with new paths advances `pending_version`
once. An existing-only batch validates and rereads its inputs without changing
rows or counters. A failed batch rolls back its metadata changes.

Working files are never written. Temporary reads do not create a stored content
snapshot, a check-in or a recording receipt.

Both commands print the complete tracked inventory in pathname order:

```text
Tracked paths (A: pending addition, R: recorded):
A	unselected	"notes/plan.md"
A	unselected	"notes/review.md"
```

Each row starts with `A` for a pending addition or `R` for a recorded path.
A tab separates the state, scope and path. Scope is `unselected`, `shared` or
`private`. The path uses Rust debug-string syntax with quotes and escapes.

A selected scope assigns a path to shared or owner-local private history.
`unselected` means that no choice has been stored.
Empty status prints only the heading.

Status does not discover untracked files or report modified, missing, removed
or renamed files. It does not reread working file content.
An output failure returns status 1, and an add may already have committed.

New paths have unselected scope. Add grants no recording or sync authority.
The [library API](native-storage.md#private-recording-library) can persist explicit
immutable scopes and record all tracked files when every scope is private.
The scope and commit commands below use that API, including rereading files
edited after add.

## Select scopes and record private files

Run `scope` and `commit` from the registered root or a descendant. Both require
`--repository PATH` and `--checkout ID` exactly once, plus a complete matching
locator. Use `vcs open` first to publish or recover that locator. Neither command
infers a repository, checkout or scope. The configuration, marker and identity
checks described for open also apply.

### Select immutable path scopes

```sh
/path/to/gos v scope --repository /absolute/history.gossil --checkout work --scope private --path notes/plan.md --path notes/review.md
```

`--scope shared|private` is required exactly once. Supply at least one
`--path PATH`; repeat that option for each exact tracked path relative to the
registered root. The one chosen scope applies to every path in this request.
Paths must already be tracked. Duplicate paths, conflicting choices and unsupported
path spellings fail without persisting a partial batch.

The choice is immutable. Repeating the same choice is a checked no-op, but changing
it is refused. New choices advance the pending version once.

A checkout can reopen with some scopes unselected or with both shared and private
choices. Those states grant no recording or synchronization authority.
Choosing shared scope prevents private recording of the complete tracked set.
Shared recording and changing a persisted scope are not supported.

Success prints `Selected scopes: N` followed by a newline. `N` is the number of
requested paths, including choices already stored unchanged.

### Record the complete private tree

```sh
/path/to/gos v commit --repository /absolute/history.gossil --checkout work --scope private --all-tracked --user alice --comment 'Save notes' --date-override '2026-11-21T12:00:00'
```

| Required option | Meaning |
|---|---|
| `--scope private` | Explicit request for owner-local private recording |
| `--all-tracked` | Include the complete tracked set; no path subset is accepted |
| `--user ACTOR` | Explicit nonempty actor text, at most 4096 UTF-8 bytes |
| `--comment TEXT` | Explicit nonempty comment, at most 1048576 UTF-8 bytes |
| `--date-override DATE` | Explicit native date text, at most 64 UTF-8 bytes |

Commit requires a nonempty tracked set with a stored private scope for every
path. An unselected, shared or mixed set is refused before mutation. The command
reads the complete tracked inventory and current checkout view. The library
checks all seven view fields and reauthenticates owner, root, provider and scope.
No caller-supplied replacement authority is accepted.

Every commit rereads current bytes and executable modes. An edit needs no second
add. Saving a file alone creates no history. Mode-only changes count as changes,
but mtime-only changes do not. Working files remain unchanged.

The first private
check-in names the empty shared root as its parent. Later check-ins name both
the previous private check-in and that shared root. The shared head stays unchanged.

Dates must satisfy the [native date grammar](task-ticket.md#date-domain) and be
strictly later than every parent under native SQL date conversion. Actor, comment
and date text must be UTF-8 without NUL. Comments containing `[` are refused,
including on a no-change request. This restriction narrows Fossil's comment grammar.

The command prints one of these lines, followed by a newline:

```text
Recorded private check-in: HASH
No changes
```

`HASH` is the new native check-in hash. An unchanged tree returns `No changes`
without a new receipt or any changed head, counter, content or classification.
A later add creates unselected paths again. Select their private scope before
recording the expanded tree.

### Grammar, limits and failures

Both commands accept separate option values only. They reject positional
arguments, unknown or duplicate options, `--name=value`, `--root` and
`--json`. A lone `--help` or `-h` prints the usage of the command. Only
`--path` may repeat, and only for scope selection. Commit accepts
no `--path`, and scope accepts no recording metadata or `--all-tracked`.
Both executable names and both group names use the same grammar.

Checkout IDs are nonempty UTF-8, bounded to 1024 bytes. Scope selection and
recording use the tracking limits above. These allow 4096 paths, 4096 bytes per
path, 128 components, 64 MiB of retained working bytes and a 64 KiB read buffer.
The temporary final file recheck can require another captured copy. Native
content and parser limits also bound cumulative work across the complete operation.
The [library contract](native-storage.md#private-recording-library) defines those
budgets, immutable history checks, protected files and rollback guarantees.

A capture, validation or transaction failure leaves no partial recording.
An output failure returns status 1 and can occur after a durable scope change
or private commit. A failed exit does not prove rollback. Reopen the
same explicit binding and retry with unchanged files, valid metadata and a date
later than the stored parents. After a durable commit, that retry prints
`No changes`. Scope retries with the same choices preserve them unchanged.

## Short names

The `gos` executable runs the same implementation as `gossil`. Both accept `v`
as a short name for `vcs`. For example, this runs the same round-trip check:

```sh
cargo run --locked --bin gos -- v test-delta FILE1 FILE2
```

Use either executable with either group name for initialization, native open,
add/status, scope/commit, info, timeline/log, the three delta diagnostics and
the prepared glob diagnostic.
Usage messages use fixed spellings regardless of the invocation alias.
Add and status use `gos v`, and the other command usage messages use `gossil vcs`.
The usage lines of the help all use `gossil vcs`.

## Exit status and errors

Success and help return status 0. Usage, input, delta and output failures return status 1
with one diagnostic line on stderr. A recognised issue command that this build does not run returns status 69,
as the [issue section](#issue-commands) says. `issue init` returns the
[statuses of CLI-05](#create-the-gest-project-of-a-checkout), such as 78 and 74, and so do
[`issue task list` and `issue task show`](#show-one-task). An unknown group or command returns status 2,
as the [help section](#get-help) says. A help request with an argument after
the command name also returns status 2, with two lines on stderr.

A repository whose catalog marker names an older Gossil catalog, 2, 4 or 5,
returns status 78 before any effect. The three lines on stderr name the
repository, the catalog found and catalog6, and say to create a new repository.
The catalog marker is the `storage_version` value of the `g_storage` table, and
not the `.gossil` checkout marker.

Missing input reports `no such file: PATH`.
An output-open failure reports `unable to open file "PATH" for writing`.
Invalid delta application reports `Delta application failed.`.
A failed round trip reports `delta test failed`.

Create and apply read their inputs before opening the output. This permits an
output path to name an input file. Input or validation failures preserve an
existing output, but an output write failure can leave it truncated.
Parent folders must already exist.

The [encoder guide](delta-encoder.md) and [decoder guide](delta-decoder.md)
describe format bounds and the library errors behind these diagnostics.
The [getting started example](getting-started.md) uses distinct output files.

## Development runner

`gossil-conformance` is a separate development executable. It runs one explicit
inherited-test configuration through `just conformance CONFIG`.
It does not implement a Gossil engine. Its configuration, prerequisites and
acceptance rules are in the [runner guide](conformance.md).

## Inspect current recorded checkout identity

Run `info` from the registered root or a descendant with a complete checkout locator:

```sh
/path/to/gossil vcs info --repository /absolute/history.gossil --checkout work
/path/to/gos v info --repository /absolute/history.gossil --checkout work
```

Both executable names accept both `vcs` and `v`. Repository and checkout selections
are required exactly once. The command accepts no revision argument, hash prefix,
tag, `--root` or `--json`. Standalone `--help` or `-h` succeeds without reading a checkout.

Success prints exactly five lines in this order:

```text
repository: "/absolute/history.gossil"
local-root: "/absolute/work"
checkout: FULL_CURRENT_NATIVE_ID
shared-head: FULL_SHARED_NATIVE_ID
private-head: FULL_PRIVATE_NATIVE_ID_OR_none
```

Each line has one ASCII space after the colon and ends with LF, including the
last line. Both paths are standard JSON strings with escaping for special
characters. `repository` preserves the supplied UTF-8 spelling, including relative
paths and symlink aliases, after successful admission. Non-UTF-8 repository input
is rejected without lossy conversion. `local-root` is the authenticated canonical
registered root.

All heads are complete native IDs. `checkout` is the private combined head when
present, otherwise the shared head. Before the first private recording,
`private-head` is the literal `none`. The shared head stays at the initial public
check-in after private recordings. Uncommitted edits do not change these recorded identities.

Info authenticates the existing checkout before producing stdout. Wrong selections,
missing or incomplete nearest locators, configuration conflicts and corrupted
native state fail through the existing admission checks. Errors retain their
underlying causes. Success and failure preserve repository logical state, locator
bytes and working files. Database file bytes can change during provider admission.
An output failure returns status 1 and can leave partial stdout.

This command adapts Fossil's current-tree checkout identity assertion from
`test/set-manifest.test:68-73` and the current-checkout branch of `src/info.c`.
Fossil prints a 40-character checkout prefix, and its `same_uuid` helper accepts matching prefixes.
The inherited test is copyright 2016 D. Richard Hipp under the Simplified BSD License.
Gossil controls compare complete output IDs against an independently authenticated checkout view.

This bounded subset does not implement Fossil's full info interface, general timeline filters,
parent traversal or historical revision selection. Unrelated inherited suites
are outside this qualification.

## Planned commands

Broader version-control commands are planned. Neither `gossil` nor `gos`
accepts them, and each fails with status 2, as the [help section](#get-help)
says. This build recognises the Gest commands other than `init`, `task list` and `task show`, and refuses them with status 69, as the
[issue section](#issue-commands) says. The short names above apply to initialization and the
three delta diagnostics, the prepared glob diagnostic, native open, add/status,
scope/commit, info and timeline/log. Shared
recording, changing scopes, removal, rename, materialization, sync and Gest
record associations remain unsupported.

## Read complete recorded ancestry

Run from the registered checkout root or a descendant after `vcs open` has
published a complete matching locator:

```sh
just gossil vcs timeline --repository REPO --checkout ID
just gossil v log --checkout ID --repository REPO
```

Replace `REPO` and `ID` with the existing database and exact checkout identity.
Both selectors are required, each once, with separate values in either order.
Repository text is bounded to 4096 UTF-8 bytes and checkout text to 1024 bytes.
Empty values, NUL, invalid UTF-8, duplicate or unknown options, positional
arguments and `--name=value` forms fail. There are no filters, revision selectors,
`--limit` or `--json` options. Output is always complete newline-delimited JSON (NDJSON), with one object per line.

The eight forms are `gossil vcs timeline`, `gossil vcs log`, `gossil v timeline`,
`gossil v log`, `gos vcs timeline`, `gos vcs log`, `gos v timeline` and `gos v log`.
They emit identical bytes for the same admitted history. Standalone `--help` or
`-h` after either command prints usage and exits 0 without opening storage.
Help combined with selectors fails.

Each compact JSON object has the keys `hash`, `date`, `actor`, `comment` and
`private`, in exactly that order. The first four values are strings, and `private` is a boolean.
Every object ends with one LF, with no optional whitespace. Hashes are full native
identities. Strict UTF-8 conversion and standard JSON escaping preserve the exact
native date and decoded actor and comment, without normalization or date
reformatting.

The current check-in comes first and the immutable shared root occurs
exactly once, last. Only the root has `private: false`. Before private recording,
the root is the sole row.

Opening uses existing-locator admission and publishes or repairs no locator.
The API then authenticates the complete supported history in its own transaction.
Neither operation reads working-file contents. Pending additions and dirty or
deleted working files do not change the recorded output. Missing checkout,
corrupt or unsupported state and exhausted bounds fail without a partial history.

Opening admission, the API operation and serialization each have a separate
finite allowance. They do not form one aggregate budget for the whole invocation.
For serialization, N counts the UTF-8 source bytes of all four string values,
including each hash once. The checked worst-case bound adds `6*N` to the exact syntax overhead.
That overhead is 61 bytes per private row or 62 per root row and includes keys,
quotes, punctuation, boolean and LF. The cumulative bound must fit the
CLI decoded-content limit and the platform allocation limit.

The adapter reserves
space once before serializing the complete response.

The API records its result and completes its checked transaction before the
adapter prepares output. Stdout receives no bytes until complete preflight and
serialization succeed. Success exits 0. Failures exit 1 with a diagnostic and
retain the original cause. A repository at an older Gossil catalog exits 78. A stdout write failure can leave partial stdout.

The read preserves logical database state and working files. This contract makes
no claim that physical database bytes remain identical.

See [qualification](development-plan.md#recorded-ancestry-cli-qualification)
before treating this source command as released or installed. General Fossil
timeline behavior and `cat` remain separate work.

## Inspect a Fossil glob list

The source accepts all four forms:

```text
gossil vcs test-glob PATTERN STRING ...
gossil v test-glob PATTERN STRING ...
gos vcs test-glob PATTERN STRING ...
gos v test-glob PATTERN STRING ...
```

Supply one pattern-list argument and at least one candidate argument. Arguments
are positional. There are no switches, settings lookup or JSON mode. A token
such as `--help` is a literal pattern or candidate when arity is valid. A lone
`--help` or `-h` after `test-glob` prints its usage.

A pattern
list beginning with `@` fails with `test-glob setting lookup is not implemented`.
Missing arguments fail with `Usage: gossil vcs test-glob PATTERN STRING ...`
for every alias.

For `PATTERN` equal to `*.rs,src/*` and candidates `src/lib.rs` and `notes.md`,
the source-defined output is:

```text
SQL expression: (x GLOB '*.rs' OR x GLOB 'src/*')
pattern[0] = [*.rs]
pattern[1] = [src/*]
1 1 src/lib.rs
0 0 notes.md
```

Pattern labels start at zero. Match indices start at one, and zero means no match.
Each candidate row prints reusable then one-shot results, followed by the
original candidate bytes and LF. Pattern and candidate bytes are not escaped.
The diagnostic constructs SQL with trusted expression `x` but executes no SQL.

The adapter caps list and candidate inputs at 1 MiB each, pattern count at
65,536 and SQL output at 16 MiB. These bounds do not limit engine matching time
or stack use. Unix arguments preserve raw bytes. Other platforms accept Unicode
as UTF-8 and reject unrepresentable arguments with
`test-glob argument is not valid Unicode`.

Success exits 0. Usage, settings, encoding, budget, allocation and stdout errors
exit 1. Budget messages use `glob NAME limit exceeded`, and capacity overflow uses
`glob capacity overflow`. Allocation messages begin `glob allocation:`.
Stdout failures print `unable to open file "<stdout>" for writing`, preserve
the original I/O cause and may leave partial stdout.
The command reads no repository, settings or files and changes no tracking state.

The [glob guide](glob.md) defines quotes, first-NUL truncation and the SQL
parser's extra advance after closing quotes. Historical controls used 0.9.30.
The [2026-11-21 focused checkpoint](glob.md#current-source-status) exercised all
four forms with the libSQL 0.10.0 engine. Complete frozen-tree and delivery qualification are
separate. These source forms make no claim about installed previews.

## Recommendations

Use the three diagnostics for file deltas. For a native checkout, initialize a
new repository, then register one explicit existing root with open. Next, reopen
without `--root`, add explicit files, select private scope and commit all tracked
paths. Use `info` to inspect the recorded checkout identity. Follow the [source walkthrough](getting-started.md#record-private-files-from-source)
and preserve durable state when a command reports an output failure.
