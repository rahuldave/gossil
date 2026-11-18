# Gest task values

The `gossil::gest_task_values` module converts eleven already decoded task fields
into an owned, validated record. Callers can use the record for complete task
snapshots before storage integration. The module does not complete the Gest API
or permit nonempty migration.

`TaskValueInputs` requires an explicit `serde_json::Value` for each field.
`task_from_values` validates these values and returns `TaskValues`, whose fields
are private. `task_to_values` returns the complete set of owned values.
The functions do not read a clock, generate identifiers or execute SQL.

## Use the library

Assemble every field after admitting the raw input. No field has a default.
The public input and output use the same `TaskValueInputs` type.

```rust
use gossil::gest_task_values::{TaskValueInputs, task_from_values, task_to_values};
use serde_json::json;

let fields = TaskValueInputs {
    gest_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
    project_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
    title: json!("Review task values"),
    priority: json!(255),
    status: json!("IN_PROGRESS"),
    description: json!(""),
    assigned_to: json!(null),
    metadata: json!([null, {"reviewed": false}]),
    resolved_at: json!(null),
    created_at: json!("2026-11-18T07:00:00-05:00"),
    updated_at: json!("2026-11-18T12:00:00Z"),
};
let task = task_from_values(fields).unwrap();
let values = task_to_values(&task);
assert_eq!(values.status, json!("in-progress"));
assert_eq!(values.created_at, json!("2026-11-18T12:00:00+00:00"));
assert_eq!(task_from_values(values).unwrap(), task);
```

The example uses array metadata and priority 255 to show the model's full value
types. Production callers must handle `TaskValueError` from validation.
Its display text names the first rejected field and the reason, without echoing
the supplied value.

`TaskValueError::parse_cause()` returns the original `chrono::ParseError`
when parsing the input or its canonical UTC output fails.
Callers can inspect its `kind()` to distinguish failures such as `TooShort`
and `OutOfRange`. Other validation failures return no parse cause.
Error equality includes the retained cause, so errors with equal display text
can compare unequal.
Chrono's alloc-only build does not implement `std::error::Error` for this parser
error, so callers use the accessor rather than `Error::source()`.
The validated record stays unchanged if a caller changes
the owned fields returned by `task_to_values`.

## Conversion contract

The input requires all eleven fields, even when a value is null.

| Field | Admitted value |
| --- | --- |
| `gest_id` | Full task ID string, called `id` in upstream `Model`. |
| `project_id` | Full project ID string. |
| `title` | Any string. |
| `priority` | Null or a JSON integer from 0 through 255. |
| `status` | A status string accepted by upstream `FromStr`. |
| `description` | Any string. |
| `assigned_to` | Null or a full author ID string. |
| `metadata` | Any `serde_json::Value`, including null. |
| `resolved_at` | A timestamp string for terminal status, otherwise null. |
| `created_at` | A timestamp string. |
| `updated_at` | A timestamp string. |

The task and project identifiers require exactly 32 lowercase letters from
`k` through `z`. An assignee is null or a full identifier. Title and description
preserve their strings, including empty strings, Unicode, whitespace and null
characters. The task field `gest_id` corresponds to upstream `Model.id`.

Priority is null or an integer from 0 through 255. Checked conversion rejects
negative integers, larger integers, floating point values and other JSON types.
The upstream task model uses `Option<u8>`. Its separate named priority enum
covers only 0 through 4 and is outside this module.

Status parsing follows upstream `FromStr`. The statuses are `open`,
`in-progress`, `done` and `cancelled`. Parsing accepts case differences and the
`in-progress`, `in_progress` and `inprogress` aliases. Output uses `in-progress`.
The adopted Gossil contract requires a resolution timestamp for `done` and
`cancelled`, and null for `open` and `in-progress`.
The converter checks that pairing without generating a timestamp.

Metadata retains every JSON value variant, including null. Explicit metadata
null is model data, independent of the nulls in optional model fields.
The converter preserves JSON value semantics without promising original JSON
spelling, object member order or duplicate-key detection.

Business timestamps use Gest's pinned chrono 0.4.44 RFC3339 parser and normalize
to Coordinated Universal Time (UTC), subject to Gossil's canonical output contract.
RFC3339 is the timestamp string format used at this boundary.
Output uses chrono's RFC3339 form with the `+00:00` offset and at most nine fractional digits.

The parser accepts offsets, lowercase separators, a space separator, year zero
and leap seconds. It truncates fractional input beyond nanoseconds, as upstream
does. Business timestamp rules are separate from the stricter artifact D-card time
grammar. The converter adds no ordering rule between timestamps.

A parsed instant must have a UTC representation that the RFC3339 parser can
read back. Offset normalization across the year zero or 9999 boundary can
produce a signed extended year. The converter rejects those results under the
adopted output contract. Upstream source parsing accepts their inputs.
Year zero and nearby offsets remain valid when their UTC result is representable.
The module does not claim parity with the full upstream timestamp
input domain.

Invalid field types, incomplete identifiers, invalid priorities or statuses,
unparseable timestamps and inconsistent status/resolution pairs return `TaskValueError`.
Normalized UTC output outside the adopted RFC3339 domain also returns that error.
Metadata has no further type restriction.

## Dependency scope

`Cargo.toml` pins chrono to 0.4.44 with default features disabled and `alloc`
as its only enabled feature. Clock access and chrono's serde support are disabled.
The lockfile adds chrono 0.4.44, num-traits 0.2.19 and autocfg 1.5.1.
The change updates no existing dependency package versions.
These additions support timestamp conversion in Rust and do not add a native
Fossil process or a SQL date conversion to this module.

## Caller responsibilities

The caller must decode and admit raw contributors before constructing the
input. It must check complete field presence, classify every raw J field,
reject duplicate lexemes where required and enforce operation byte budgets.
The explicit input struct is not an object decoder and cannot identify fields
that the caller discarded. Whole-history validation, reference closure and
unsupported-contributor policy also remain with the enclosing implementation.

The existing nonempty migration guard remains in place. This module supplies
complete task values for later before and after snapshots. Artifact creation,
replay, recipes, provenance, inverse operations, references and storage writes
remain separate work.

## Source and verification evidence

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

The README, site navigation and development plan link to this guide.
The API comments document the input fields, private record, conversion functions
and error type. Storage, artifacts, commands and schemas do not call this module.
Their contracts remain separate. Independent prose revision and the final
composed gates remain part of the parent-owned acceptance workflow.

This page follows the nossaifd writing guide, including principles credited to
Rosalind Reid, Shreya Shankar and Oskar Austegard.

## Recommendations

Use the explicit decoded-field converter for task snapshot values.
Next, independently review this candidate and run the composed tree's gates.
Keep nonempty migration closed until its enclosing contracts are implemented.
