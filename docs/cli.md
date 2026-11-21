# CLI reference

The `gossil` executable initializes empty repositories, opens native checkouts
and has three file diagnostics. This source checkout also has
[explicit add and tracked status](#add-explicit-paths-and-inspect-pending-additions),
plus [scope selection and private recording](#select-scopes-and-record-private-files).
`gos` is an alias for `gossil`, and `v` is an alias for `vcs`.
Each diagnostic runs without a repository. Run it from this source checkout with `just gossil`, which builds
and runs the executable through Cargo. Cargo may print build messages on stderr.

| Command | Arguments | Successful output |
|---|---|---|
| `vcs test-delta-create` | `ORIGIN TARGET DELTA` | Writes `DELTA`; no stdout |
| `vcs test-delta-apply` | `ORIGIN DELTA TARGET` | Writes `TARGET`; no stdout |
| `vcs test-delta` | `FILE1 FILE2` | Prints `ok` and a newline |

```sh
just gossil vcs test-delta-create ORIGIN TARGET DELTA
just gossil vcs test-delta-apply ORIGIN DELTA TARGET
just gossil vcs test-delta FILE1 FILE2
```

Replace capitalized arguments with file paths. The commands accept exactly the
number of paths shown. Each diagnostic requires the `vcs` or `v` group. Neither executable has help subcommands.
File data comes from paths and goes to paths. Stdin and stdout do not carry it.

## Initialize a repository

```sh
just gossil vcs init --admin-user alice --date-override '2026-11-18 12:34:56' NEW_REPOSITORY
just gossil vcs new --sha1 ANOTHER_NEW_REPOSITORY
```

Initialization accepts exactly one destination. Both executable names and both
group names support `init` and its `new` alias. The destination must be new;
existing files, symlinks and SQLite recovery sidecars are refused.

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

Administrator bytes reach native validation unchanged; the storage API requires valid UTF-8.
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
Broader history commands remain deferred.

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
has no `--help`, `--register`, `--name=value` or default repository selection.
Use either `gossil` or `gos`, with either `vcs` or `v`.

For first registration, create the repository separately with `vcs init`, then
supply `--root`. Open requires an existing database and never creates one.
The root can contain ordinary files and nested Git repositories. Their bytes
remain unselected and unchanged. Native VCS open requires no issue, project or
author initialization. It does not grant Gest project authority.

The native library admits catalog4 with exactly zero or one registration and
the supported tracking, scope and private recording history. Catalog1, unrelated
file history and a second registration are outside this command. The [storage guide](native-storage.md) defines those catalog formats.
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
directory. Its canonical existing database must match `--repository`. The exact
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
not change this process exit status.

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

Both `--repository` and `--checkout` are required exactly once. Repeat `--path PATH` for each file;
there are no positional path lists. Paths are exact UTF-8 names relative to the
registered root, regardless of the invocation directory. Status accepts no paths.
Neither command accepts `--json`, `--root`, scope switches or recursive selection.
Both executable names and both group aliases use this grammar.

Add accepts existing regular files only. It rejects absolute paths, empty
components, `.` and `..`, backslashes, duplicate paths within a batch, case aliases,
symlinks and special files. On macOS, paths must also be ASCII. This implementation rejects non-ASCII
paths because their filesystem name comparisons have not been qualified.

The `.gossil` namespace, selected database, SQLite sidecars and
known hard-link aliases are protected. Parent directories and leaves are opened
without following symlinks, and their identities are rechecked before commit.
Keep the filesystem namespace stable during the command; these checks do not
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
once; an existing-only batch validates and rereads its inputs without changing
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
`private`; the path uses Rust debug-string syntax with quotes and escapes.
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

The choice is immutable. Repeating the same choice is a checked no-op; changing
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
reads the complete tracked inventory and current checkout view; the library
checks all seven view fields and reauthenticates owner, root, provider and scope.
No caller-supplied replacement authority is accepted.

Every commit rereads current bytes and executable modes. An edit needs no second
add. Saving a file alone creates no history. Mode-only changes count as changes;
mtime-only changes do not. Working files remain unchanged.

The first private
check-in names the empty shared root as its parent; later check-ins name both
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
A later add creates unselected paths again; select their private scope before
recording the expanded tree.

### Grammar, limits and failures

Both commands accept separate option values only. They reject positional
arguments, unknown or duplicate options, `--name=value`, `--help`, `--root` and
`--json`. Only `--path` may repeat, and only for scope selection. Commit accepts
no `--path`; scope accepts no recording metadata or `--all-tracked`.
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
add/status, scope/commit and all three diagnostics.
Usage messages use fixed spellings regardless of the invocation alias.
Add and status use `gos v`; the other command usage messages use `gossil vcs`.

## Exit status and errors

Success returns status 0. Usage, input, delta and output failures return status 1
with one diagnostic line on stderr. Missing input reports `no such file: PATH`.
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

## Planned commands

Broader version-control commands and Gest task commands are planned. They are
not accepted by `gossil` or `gos`. The short names above apply to initialization and the
three delta diagnostics, native open, add/status and scope/commit. Shared
recording, changing scopes, removal, rename, materialization, sync and Gest
record associations remain unsupported.

## Recommendations

Use the three diagnostics for file deltas. For a native checkout, initialize a
new repository, then register one explicit existing root with open. Next, reopen
without `--root`, add explicit files, select private scope and commit all tracked
paths. Follow the [source walkthrough](getting-started.md#record-private-files-from-source)
and preserve durable state when a command reports an output failure.
