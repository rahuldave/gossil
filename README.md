# Gossil

Gossil aims to keep agent work, issues and file history together alongside Git.
Fossil is a version-control system. This build exposes a Rust library for its delta bytes, artifact records
and local storage. Its `gossil` CLI creates, applies and checks deltas between files.
The library includes bounded compression, content hashes, token escaping,
artifact checksums and owned local database transactions.

Read the [product documentation](https://rahuldave.com/gossil/), start with
[a file round trip](docs/getting-started.md), or browse the
[CLI reference](docs/cli.md). The site generates the complete Rust API reference
from source comments.

## Try the CLI

From a source checkout with Rust, Cargo, Just, uv and `jlog` installed:

```sh
just setup
just gossil vcs test-delta FILE1 FILE2
```

Replace `FILE1` and `FILE2` with two existing paths. Success prints `ok` after
creating and applying deltas in both directions. The
[getting started guide](docs/getting-started.md) includes a complete example.
The `gos` executable and the `v` group name run the same diagnostics as
`gossil` and `vcs`. See [short names](docs/cli.md#short-names) to run `gos` from this source checkout.

## Use the library

```rust
use gossil::delta::{Limits, apply, create};
let delta = create(b"before", b"after").unwrap();
let restored = apply(b"before", &delta, Limits { max_output_bytes: 1024 }).unwrap();
assert_eq!(restored, b"after");
```

See [application setup and examples](docs/examples.md) for the required Cargo
provider patch, and [library concepts](docs/concepts.md) for module choices.

Gest is a task and artifact tracker. The two converters below use its task and project record model.
The [Gest task values converter](docs/gest-task-values.md) validates complete decoded
task snapshots in memory. The [Gest project values converter](docs/gest-project-values.md)
validates all four canonical project fields.

The [task ticket emitter](docs/task-ticket.md) turns validated task snapshots into bounded
task creation artifacts in Fossil's ticket format. Callers supply the identity, actor and date.
These modules do not store records or add commands.

Native storage initializes an empty public root without a checkout through the
library API. Bound checkout creation also supports bounded private content
operations. The root is a check-in with no files. File-containing check-ins, general
version-control commands, Gest task management, a server and a user interface
remain planned. Build this version from source.

## Contribute

Read [development and contributing](docs/development-plan.md) for setup,
verification, documentation generation and the evidence archive.
`gossil-conformance` is separate development infrastructure for explicit
inherited-test configurations. See its [runner guide](docs/conformance.md).

## Recommendations

Try the file round trip before integrating the library. Next, choose a module
guide and use the generated API for exact signatures and errors.
