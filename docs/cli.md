# CLI reference

The `gossil` executable has three file diagnostics. Each command runs without
a repository. Run it from this source checkout with `just gossil`, which builds
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
not accepted by `gossil` or `gos`. The short names above apply only to the
three delta diagnostics.

## Recommendations

Use the three diagnostics for file deltas. Next, follow the round-trip example
before using an existing file as an output path.
