# Gossil

Gossil aims to keep agent work, issues and file history together alongside Git.
Fossil is a version-control system. This build exposes a Rust library for Fossil's delta bytes, artifact records
and local storage. Its `gossil` CLI creates, applies and checks deltas between files.
It also initializes empty repositories, opens one native checkout,
adds or lists explicitly tracked files, selects their scopes and records private history.

The CLI and library can select immutable
path scopes and record the complete private tracked tree. A scope assigns a path
to shared or owner-local private history; recording shared history remains planned.

The library includes bounded compression, content hashes, token escaping,
artifact checksums and owned local database transactions.

Read the [product documentation](https://rahuldave.com/gossil/), start with
[a file round trip](docs/getting-started.md), or browse the
[CLI reference](docs/cli.md). The site generates the complete Rust API reference
from source comments.

## Try the CLI

Use a source checkout with Rust, Cargo, Just, uv and `jlog` installed.
Linux setup also needs a C compiler, Make, zlib development headers and CA
certificates. Other hosts need the existing pinned Tcl and Fossil references
described in the [setup guide](docs/conformance.md#local-prerequisites).

```sh
just setup
just gossil vcs test-delta FILE1 FILE2
```

Replace `FILE1` and `FILE2` with two existing paths. Success prints `ok` after
creating and applying deltas in both directions. The
[getting started guide](docs/getting-started.md) includes a complete example.
The `gos` executable and the `v` group name run the same diagnostics as
`gossil` and `vcs`. See [short names](docs/cli.md#short-names) to run `gos` from this source checkout.

## Initialize an empty repository

From this source checkout, run `just gossil vcs init NEW_REPOSITORY`.
The destination must be new. `vcs new` is an alias. Initialization creates the
native empty root and prints project and server identifiers, the administrator
name and an initial remote-access password. It creates no checkout.
See [initialization options](docs/cli.md#initialize-a-repository) for dates and SHA1.

To register an existing directory and publish its lookup locator, use the source
[`vcs open` command](docs/cli.md#open-a-native-checkout). It selects no files.
Open can also recover a missing locator. Build the commands from this source
checkout; installation and publication require separate delivery checks.
An already installed mini-VCS preview has a separate
[command contract](docs/working-with-repositories.md#try-the-installed-development-preview).

## Use the library

```rust
use gossil::delta::{Limits, apply, create};
let delta = create(b"before", b"after").unwrap();
let restored = apply(b"before", &delta, Limits { max_output_bytes: 1024 }).unwrap();
assert_eq!(restored, b"after");
```

See [application setup and examples](docs/examples.md) for the required Cargo
provider patch, and [library concepts](docs/concepts.md) for module choices.

Gest is a task and artifact tracker. The converters below use its task, project and author record model.
The converters validate complete decoded snapshots in memory. They cover
[task values](docs/gest-task-values.md), all four canonical
[project fields](docs/gest-project-values.md), and all six canonical
[author fields](docs/gest-author-values.md).

The [task ticket emitter](docs/task-ticket.md) turns validated task snapshots into bounded
task creation artifacts in Fossil's ticket format. Callers supply the identity, actor and date.
These modules do not store records or add commands.

Native storage initializes an empty public root without a checkout through the
library API. It can then register one existing directory and authenticate that
registration by ID or exact root. Registration changes the database only; it
selects no files and publishes no checkout locator. See the
[native storage guide](docs/native-storage.md#register-one-existing-directory).

Catalog names identify storage formats. Catalog1 creation binds a checkout and
supports bounded private content operations. Catalog4 starts at an empty shared
check-in and accepts the private history described below. General history commands, shared recording, Gest task
management, a server and a user interface remain planned.

## Contribute

Read [development and contributing](docs/development-plan.md) for setup,
verification, documentation generation and the evidence archive.
`gossil-conformance` is separate development infrastructure for explicit
inherited-test configurations. See its [runner guide](docs/conformance.md).

## Track and record private files

Add enrolls paths without staging their bytes. New paths have unselected scope.
The library's `select_path_scopes` persists an explicit immutable choice for each
tracked path. `commit_private_tracked_files` requires every tracked path to be
private and rereads the complete tree, including edits made without re-adding.
It writes a private check-in while preserving the empty shared root.

Use [`vcs info`](docs/cli.md#inspect-current-recorded-checkout-identity) with explicit
repository and checkout selections to inspect the current recorded checkout.
It prints complete shared and private heads, preserves the supplied repository
path spelling as a JSON string, and requires a complete existing locator.
This bounded current-checkout command does not offer timeline or revision selection.

The source CLI offers [add and status](docs/cli.md#add-explicit-paths-and-inspect-pending-additions).
Status distinguishes pending additions from recorded paths and prints their
actual scopes. Use [`vcs scope` and `vcs commit`](docs/cli.md#select-scopes-and-record-private-files)
with explicit repository, checkout and scope choices. Commit also requires
`--all-tracked`, `--user`, `--comment` and `--date-override`.
The [source walkthrough](docs/getting-started.md#record-private-files-from-source)
records an edit without adding the file again.
See [native storage](docs/native-storage.md#private-recording-library) for the
request, limits and no-change result, and [development status](docs/development-plan.md#private-recording-qualification)
for the remaining qualification work.

The library can read [complete recorded checkout ancestry](docs/native-storage.md#read-recorded-checkout-ancestry)
through `LocalOwnerRepository::recorded_timeline` and `RecordedCheckin`.
It returns full native identities and exact metadata bytes, current first and
shared root last, within explicit operation limits. CLI `timeline` and `log`
remain deferred. See the [API checkpoint status](docs/development-plan.md#recorded-ancestry-api-qualification)
before treating this source feature as released or installed.

## Recommendations

Try the file round trip before integrating the library. Next, choose a module
guide and use the generated API for exact signatures and errors.
