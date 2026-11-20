# CLI reference

The `gossil` executable initializes empty repositories, opens native empty-root
checkouts and has three file diagnostics.
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
empty-root checkout. File recording and history commands remain planned.
This source interface is separate from the existing default mini-VCS preview;
this guide does not record a new installation or publication.

## Open a native checkout

This source command opens one native checkout at the initial empty root. A
checkout registration binds an existing directory to that root. The locator is
an owner-local lookup file at `ROOT/.gossil/checkout.json`. Neither operation
selects files, creates file history or restores working files. This 0.4 source adds open and locator recovery. Installation and publication
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

The native library admits catalog4 with exactly zero or one registration at the
initial empty root. Catalog1, populated states and a second registration are
outside this command. Native admission checks the repository and owner binding,
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
and repairs no database state. An incomplete marker forbids a new registration
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
and returns status 0. Failures return status 1 with a diagnostic on stderr.
Locator diagnostics name their publication phase. Internal error categories do
not change this process exit status. A receipt write failure can occur after
successful publication. It preserves the committed registration and locator,
so a failed command does not imply that nothing committed.

## Short names

The `gos` executable runs the same implementation as `gossil`. Both accept `v`
as a short name for `vcs`. For example, this runs the same round-trip check:

```sh
cargo run --locked --bin gos -- v test-delta FILE1 FILE2
```

Use either executable with either group name for initialization, native open
and all three diagnostics.
Usage errors retain the spelling `gossil vcs`, including calls through `gos v`.

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
three delta diagnostics and native open.

## Recommendations

Use the three diagnostics for file deltas. For a native checkout, initialize a
new repository, then register one explicit existing root with open. Next, reopen
without `--root` and preserve any reported committed state when a command fails.
