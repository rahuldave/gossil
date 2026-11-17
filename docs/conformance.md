# Inherited-test runner

`gossil-conformance` executes one Fossil Tcl suite against an explicit program.
It is development infrastructure and implements no Gossil engine behavior.
A reference run tests the pinned upstream Fossil program. A candidate run tests
the replacement program whose compatibility is under development.
The runs have separate configurations and output directories.
A missing candidate never falls back to C Fossil.

Run `just conformance-self-test` to check the runner controls. These tests pass
when the runner correctly rejects deliberate failures and incomplete runs.
That result does not mean the inherited Fossil suites pass.

## Local prerequisites

Install Rust with Cargo, Git, Just and uv on `PATH` before using the recipes.
Rust builds the runner, Just runs `Justfile` recipes, and uv runs the installed
workflow tools. `just setup` fetches Cargo dependencies. `just install --check`
checks the saved workflow installation rather than supplying native test tools.

This checkout exposes Tcl 8.6.18 and Fossil 2.29 through `.jlog/bin`.
The Fossil binary is revision `7e8ecf0ce4`, with TLS and FUSE disabled.
These ignored tools are local prerequisites. A fresh clone does not acquire
them, so continuous integration from a fresh clone remains unproved.

Supply the pinned Fossil source and native tools yourself before running
reference suites. The repository has no automated acquisition procedure.
The user temporarily permitted the local Tcl installation to use shared
Homebrew storage rather than keeping all native dependencies inside the checkout.

## Run a suite

Write a JSON configuration with absolute paths. The following example uses the
checked-in positive control. Replace `/path/to/gossil` with the checkout path.
The output directory must not exist. Save this example as `CONFIG.json`.

```json
{
  "role": "reference",
  "revision": "b3a1c8efb2731dffbd5cc38c58ac349746de6da6",
  "tcl": "/path/to/gossil/.jlog/bin/tclsh",
  "driver": "/path/to/gossil/tests/fixtures/controls/tester.tcl",
  "executable": "/path/to/gossil/.jlog/bin/fossil",
  "suite": "pass",
  "run_dir": "/path/to/gossil/target/conformance/example",
  "timeout_ms": 3000,
  "seed": 20261117,
  "inputs": [
    "/path/to/gossil/tests/fixtures/fossil/tester.tcl",
    "/path/to/gossil/tests/fixtures/fossil/enable.patch"
  ],
  "capabilities": {"tls": "disabled", "fuse": "disabled"}
}
```

### Configuration fields

| Field | Meaning and effect |
| --- | --- |
| `role` | Required label `reference` or `candidate`. It does not select a binary. |
| `revision` | Nonempty source identifier retained in evidence. The runner does not verify a source checkout against it. |
| `tcl` | Absolute path to an executable Tcl interpreter. |
| `driver` | Absolute path to the enabled `tester.tcl` driver. |
| `executable` | Absolute path to the program under test, used without fallback. |
| `suite` | Suite basename without a slash, for example `pass` for `pass.test`. |
| `run_dir` | Absolute path to a new output directory whose parent already exists. |
| `timeout_ms` | Positive deadline in milliseconds. A timeout rejects the run. |
| `seed` | Unsigned 32-bit value passed to Tcl `srand` before the driver runs. It controls Tcl randomness, not every source of randomness in the tested program. |
| `inputs` | Absolute paths to extra source, fixture or patch files whose bytes need hashing. |
| `capabilities` | Caller-supplied string map retained as evidence. It neither enables features nor verifies their availability. |

The runner locates `<suite>.test` beside `driver`. The example runs
`tests/fixtures/controls/pass.test`. For an upstream suite, supply its enabled
driver and adjacent suite files from the pinned source tree. Apply
`tests/fixtures/fossil/enable.patch` to a copy of that driver before running it.
The runner hashes the driver, suite, interpreter and tested executable
automatically, in addition to every path in `inputs`.

The example's `revision` is the Git mirror commit for the inherited source.
Fossil identifies the same source snapshot with its native revision, starting
`7e8ecf0ce4`. The full pair and driver hashes are in the
repository file `evidence/baselines/fossil/provenance.json`.
The label alone does not prove that an executable was built from that source.

Create the parent output folder, then run `just conformance CONFIG.json`.
Exit status 0 means this suite passed, status 1 means it failed acceptance,
and status 2 means setup failed. The runner prints the `report.json` path.

The report retains decoded stdout and stderr, commands, environment, hashes, source pin,
suite, original assertion names, seed, capabilities and process status.
The log files retain exact bytes and their hashes. Invalid UTF-8 blocks
acceptance. Only stdout supplies assertion records, while stderr remains evidence.

An assertion identity combines the source pin, suite path and original name.
The runner does not enumerate parameterized cases or infer unreported seeds.
Callers must supply all fixture and source inputs that need hashing.

Framework checks remain separate from real assertions. Known-bug annotations
remain visible, but an assertion reported as failing never becomes a pass. Skipped summaries
produce `not-run` with the original reason.

The runner has no automatic `feature-disabled` mapping. A separate analysis
must link the skipped reason to a capability observation, as the
reference archive described in `evidence/baselines/README.md` does. The configuration
map alone does not establish that classification. Timeouts and incomplete output produce `blocked`.

Only `pass` contributes to acceptance. A skip or blocked result prevents
acceptance of the entire suite, even if other assertions pass.
A framework-only run fails acceptance.
The runner also rejects missing summaries, inconsistent counts and crashes.
For example, the final summary for one passing assertion is
`***** Final results: 0 errors out of 1 tests`.

A final summary invalidates the run if a count is nonnumeric or exceeds
the range of `usize`, Rust's unsigned pointer-sized integer type. Every final-summary occurrence counts, including
malformed ones. A duplicate summary or a later valid summary cannot clear
that rejection.

## Process isolation and limits

The runner implements Unix process groups. The current checks ran on macOS, while Linux execution remains unqualified.
Each invocation uses a fresh directory, a cleared environment, isolated HOME
and temporary paths, and a fixed system utility PATH of `/usr/bin:/bin`.
The runner observes completion without reaping the process-group leader.
It checks the group identity before terminating owned descendants, then reaps
the direct child. A timeout always rejects the run.

Execution assumes trusted local programs with the same user credentials.
The runner rejects setuid and setgid targets. Descendants that change their
credentials, process group or session are unsupported. The runner is not a
sandbox and does not promise a deadline for uninterruptible kernel work.
It reaps its direct child and terminates descendants in the same process group.

On macOS, XNU returns EPERM for a group containing only zombies. After the
leader exits, the runner records that outcome as `no-signalable-members`
under the trusted-program assumptions above. Other permission failures block
the run. The [XNU signal implementation](https://raw.githubusercontent.com/apple-oss-distributions/xnu/main/bsd/kern/kern_sig.c)
defines that behavior.

## Sources of the fixtures

`tests/fixtures/fossil/tester.tcl` preserves the pristine upstream driver.
`tests/fixtures/controls/tester.tcl` removes only its initial `puts Ok` and
`exit` lines. Upstream disables this Tcl driver because it is unmaintained and difficult
to debug, as its opening comment states. An unmodified invocation exits
before it executes assertions.
The patch `tests/fixtures/fossil/enable.patch` records only that change.
Both copies retain upstream notices, and the adjacent BSD license covers the inherited file.

The fixture provenance record in `tests/fixtures/provenance.json` identifies
the source and both driver hashes. The control suites are new runner tests,
separate from the upstream corpus.
They exercise the inherited driver without changing its assertion procedure.

The full reference baseline remains separate. See the checked-in evidence
under `evidence/baselines` for classified results and limits.

## Recommendations

To run a suite, use an explicit configuration and retain failed results.
For development, finish the native-tool acquisition contract and run every
required reference build configuration before claiming inherited compatibility.
