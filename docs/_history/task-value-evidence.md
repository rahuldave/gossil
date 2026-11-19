# Task value evidence archive

Historical development evidence for the decoded task values converter.
These runs qualify only their recorded trees.

### Task value evidence

The source pin is Gest `eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8`.
The corrected plan is `addendum.json` with SHA-256
`9206f47295ea635e5b8baa6ccb223b68f452e634100ee4966203f809ed951d78`.
Open the following source and evidence paths from the repository root.
They are outside this documentation site.

The fixture source map, `tests/fixtures/gest_task_values/source-map.json`,
records exact source identities, all fields and all 23 priority/status tests.
It maps five directly inherited checks and six semantic replacements.
The other twelve tests concern the named priority API outside the numeric task
model. This accounting does not claim 23 ports or full Gest compatibility.

The focused tests, `tests/gest_task_values.rs`, exercise the public conversion
functions. The records in `tests/fixtures/gest_task_values/records.json` include
complete snapshots and source-compatible timestamp controls.
Checked priority conversion and terminal/null rejection are Gossil contract
checks, rather than copied upstream SQL behavior.

The candidate evidence in `evidence/gest-task-values/candidate.json` records the
author's focused run, dependency delta and complete impact inventory.
It preserves the author tree's hashes. Later documentation and comment edits
do not change that evidence or make those hashes describe the later tree.
The preserved focused run passed 21 test functions with none failed or ignored.

The separate parse-cause correction in `evidence/gest-task-values-cause-fix/report.json`
records a failing regression before the fix and a passing run of 24 tests after it.
That run includes the unchanged original 21 tests and three new cause controls.
The controls check distinct parser causes, validation failures without a parser
cause and canonical UTC reparsing failures. This later evidence preserves the
original run and does not qualify the final composed tree.

Ordinary focused tests run in the worker's own target directory through
`jlog run just test-paths tests/gest_task_values.rs`. Parent-owned review and
formal gates qualify the later composed tree. No live Fossil probe ran for
this module.

The README and site navigation link to the task values guide.
The API comments document the input fields, private record, conversion functions
and error type. The task ticket emitter borrows canonical values from this module.
Storage, commands and schemas do not call this module.
Their contracts remain separate. Independent prose revision and the final
composed gates remain part of the parent-owned acceptance workflow.

## Dependency history

The converter dependency change added chrono 0.4.44, num-traits 0.2.19 and autocfg 1.5.1.
It updated no existing dependency package versions.

## Captured whitespace

Five frozen task-value captures retain literal whitespace through exact `.gitattributes` paths.
They are `green.log`, `guide.diff` and `source.diff` under `evidence/gest-task-values-cause-fix`,
plus `focused-test.log` and `source-edge-observation.log` under `evidence/gest-task-values`.
These paths preserve command output and patch context without changing the captured bytes.
Ordinary authored files, suffixed paths and neighboring names retain whitespace checks.

## Recommendations

Use these records to inspect the original experiments. Next, run current checks
for the tree you intend to change.
