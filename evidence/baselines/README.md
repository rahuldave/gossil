# Reference baseline evidence

These files record executed reference experiments on 2026-11-17. They do not
establish Gossil engine compatibility. The Fossil version-control and Gest task-management experiments are stored
in separate folders. Each has an archive index that maps original experiment
paths to captured files and exact byte hashes.
See the [Fossil index](fossil/archive-index.json) and
[Gest index](gest/archive-index.json).

Large artifacts use gzip, and their index hash covers the uncompressed bytes.
Original absolute source and output paths describe the historical experiment.
They are not runtime dependencies of the Rust runner.

The Fossil matrix recorded all 37 suite outcomes with a 240-second limit.
The [source and test inventory](../inventories/README.md) identifies the full suite set.
It has 25 passes, five failures, four disabled-feature results, one dormant
suite and two timeouts. The dormant `merge5` suite reported a skip and no
real assertion, so its status is `not-run`.

Real assertions check the behavior under test. They have 23,247 passes and
16 failures. The 35 synthetic framework passes check driver execution and
remain separate from behavioral results. Completed counts reconcile in the
[matrix record](fossil/baseline-240s.json).
The [supplemental 600-second delta attempt](fossil/supplemental-delta.json)
also timed out and remains separate from those matrix totals.

The reference build lacks TLS, FUSE, JSON and optional scripting features.
The [capability observations](fossil/runtime-features.json) record feature evidence.
Those configurations remain acceptance work.

The Gest integration run passed 347 tests. A default parallel unit run and a
separate serial unit run each passed 867 tests. The counts apply to each run,
not a sum of all attempts. The [run receipts](gest/runs.json) identify each
command and result.

An earlier unit run failed a task-row rendering assertion. Bounded stress
reproduced that failure in the test for pipes in sibling columns.
Test helpers change a styling setting shared by the whole process.
A race on that setting is an inference supported by source inspection,
as detailed in the [Gest report](gest/report.json) and
[failing stress output](gest/color-target-05.txt).
Serial success does not repair the defect, and the original assertions remain unchanged.

The Fossil archive includes pristine-file hashes, tool hashes, capability
observations, suite logs and individual-result records. The Gest archive
includes source hashes, per-run assertions, command receipts, CLI inventory
and Axum route inventory. Their reports state the limits of each observation.

Reproduction requires the pinned source trees and existing local native tools.
The [Fossil provenance](fossil/provenance.json) gives both its Git mirror
commit and native Fossil revision. The [Gest report](gest/report.json) gives
its reference revision, and [tool identities](fossil/tool-identities.json)
record the Fossil and Tcl binaries.

Source and native-tool acquisition are not automated here. Use the Rust runner
with an explicit suite configuration after supplying those prerequisites.

The checked-in control driver applies the
[enable patch](../../tests/fixtures/fossil/enable.patch), which removes the
upstream driver's immediate success message and exit. That removal lets the
driver execute assertions. It does not change their procedure.
See the [fixture provenance](../../tests/fixtures/provenance.json) for exact hashes.

## Recommendations

Keep failures and disabled features visible beside passing results. Next,
make source and native-tool acquisition reproducible, then qualify the missing
reference configurations and investigate each failing inherited contract.
