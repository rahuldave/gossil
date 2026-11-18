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
| `invocation_plan` | Optional complete plan and corpus pins; omitted or `null` uses strict duplicate-name rejection. See the plan contract below. |

The runner locates `<suite>.test` beside `driver`. The example runs
`tests/fixtures/controls/pass.test`. For an upstream suite, keep the original
source tree and corpus unchanged. Put a copy of `tester.tcl` and the selected
suite beside each other in a separate harness directory. Apply
`tests/fixtures/fossil/enable.patch` only to the copied driver.

Prepend
`set argv0 /absolute/path/to/original/test/tester.tcl` to that enabled copy,
using the original driver path in the pinned source tree. This restores the
original corpus location when the driver derives `testdir` from `argv0`.

Set `driver` to the enabled copy. The executed `delta1` harness uses this layout;
its original driver, suite and corpus bytes remain unchanged.
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

Without a plan, assertion identity combines the source pin, suite path and
original name. The runner does not infer parameterized cases or unreported seeds.
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

## Optional complete invocation plan

The default parser rejects repeated original assertion names. Some inherited
suites reuse names for different source files. Such a run can opt into a
complete, ordered plan of expected source-derived invocation identities.
The plan preserves every original name, result, annotation and framework flag.

Add an `invocation_plan` object to the configuration:

```json
{
  "invocation_plan": {
    "path": "/absolute/path/invocation-plan.json",
    "sha256": "EXPECTED_PLAN_SHA256",
    "expected_invocations": 22804,
    "corpus_manifest": "/absolute/path/corpus-manifest.json",
    "corpus_manifest_sha256": "EXPECTED_MANIFEST_SHA256",
    "corpus_root": "/absolute/path/upstream"
  }
}
```

`expected_invocations` pins the complete row count and must be positive.
A count pin catches truncated plans during setup. It does not prove source correspondence.

Both expected hashes must be lowercase SHA-256 values. The runner checks them
before execution and parses those same verified bytes. The plan binds the
configured suite, upstream revision, exact suite bytes and corpus manifest.
The manifest binds every original corpus input. All physical paths are absolute.

The runner assumes that no process changes input files during a run. This
immutable-input assumption also applies to the existing input-hashing contract. Hash verification does not remove the race if another writer changes
an input after verification.

A plan uses schema `gossil.invocation-plan.v1`. Each row contains `name` and
`id`. An ID contains `suite` and a structured `position`. Corpus positions use
`kind: "corpus"`, an upstream-relative `source`, a zero-based `iteration` and
a positive `slot`. Empty controls use `kind: "empty"` with a positive `index`.
The final framework position uses `kind: "framework"`.

For `delta1`, `slot`
identifies one of the three assertions in each mutation iteration, numbered
1 through 3. The empty-control `index` identifies one of three final empty-input
assertions, also numbered 1 through 3.

The complete object shapes are shown below. These illustrative objects have one
corpus row. They are not a complete plan for the real `delta1` suite. Replace
all hash placeholders with hashes of the exact bytes and include every real row.

```json
{
  "schema": "gossil.invocation-plan.v1",
  "suite": "delta1",
  "revision": "PINNED_REVISION",
  "suite_sha256": "SUITE_SHA256",
  "corpus_manifest_sha256": "MANIFEST_SHA256",
  "rows": [
    {"name": "delta-example-0-1", "id": {"suite": "delta1", "position": {
      "kind": "corpus", "source": "test/example.txt", "iteration": 0, "slot": 1
    }}}
  ]
}
```

The corresponding manifest has this shape:

```json
{
  "suite": "delta1",
  "revision": "PINNED_REVISION",
  "files": {"test/example.txt": "FILE_SHA256"}
}
```

Source paths use forward slashes and contain no empty, dot or parent components.
IDs must be unique, and plan rows must cover every manifest source.

Review of the generator and source establishes correspondence for every invocation.
Hashes bind supplied bytes. Arbitrary identity labels alone do not prove provenance.

The runner compares the entire observed vector with the complete expected
original-name sequence and cardinality. It attaches `expected_invocation_id`
only after an exact match. Invalid syntax remains malformed in either mode.
No assertion is dropped, repaired, renamed or reordered.

Failures, skipped summaries, bad totals, invalid UTF-8, nonzero exits and timeouts still reject.
The report retains every plan row. The run directory also keeps exact verified
`invocation-plan.json` and `corpus-manifest.json` bytes.

Expected IDs describe correspondence derived from pinned source order.
The original upstream log does not emit these IDs. Matching names cannot detect
permutations or substitutions of whole groups with identical printed names.
The frozen suite and original corpus establish their expected order.

For the pinned `delta1` corpus, generate the plan from the original source tree:

```sh
just delta1-plan /absolute/path/fossil /absolute/path/new-plan-directory
```

The generator uses the selected Tcl interpreter's actual `lsort` order and
`file root` behavior. `lsort` determines the order of corpus paths.
`file root` removes the final filename extension to form the printed name stem. It preserves all non-directory corpus inputs and emits
all 22,804 rows for 76 files, three empty controls and the framework assertion.

The output directory must not exist. The source root must contain the original
`test/delta1.test` and adjacent corpus. The recipe uses `.jlog/bin/tclsh` and
labels the plan with revision `b3a1c8efb2731dffbd5cc38c58ac349746de6da6`.
It does not verify the source checkout against that revision. Supply the pinned
source and review its correspondence before using the plan.

The generator writes `invocation-plan.json`, `corpus-manifest.json` and
`generation.json`. Use the plan and manifest hashes from `generation.json`
for the configuration pins. Set `corpus_root` to the source root.
Generation metadata records the Tcl and generator hashes and complete source order.
The [development guide](development-plan.md#evidence-archive) links the executed qualification evidence.

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
