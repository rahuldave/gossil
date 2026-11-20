# CLI reference

The `gossil` executable initializes empty repositories and has three file diagnostics.
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
creation. Checkout registration, file recording and history commands remain planned.
This source interface is separate from the existing default mini-VCS preview;
this guide does not record a new installation or publication.

## Short names

The `gos` executable runs the same implementation as `gossil`. Both accept `v`
as a short name for `vcs`. For example, this runs the same round-trip check:

```sh
cargo run --locked --bin gos -- v test-delta FILE1 FILE2
```

Use either executable with either group name for all three diagnostics.
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
three delta diagnostics.

## Recommendations

Use the three diagnostics for file deltas. Next, follow the round-trip example
before using an existing file as an output path.
