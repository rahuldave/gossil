# Gest task values

The `gossil::gest_task_values` module converts eleven already decoded task fields
into an owned, validated record. Callers can use the record for complete task
snapshots before storage integration. Pass validated snapshots to the
[task ticket emitter](task-ticket.md) for bounded creation artifact bytes.
The module does not implement the full Gest API or migrate stored repositories.

Gest is a task and artifact tracker. This converter uses its task record model.
Here, canonical values use the status and UTC timestamp forms defined below.

`TaskValueInputs` requires an explicit `serde_json::Value` for each field.
`task_from_values` validates these values and returns `TaskValues`, whose fields
are private. `task_to_values` returns the complete set of owned values.
The functions do not read a clock, generate identifiers or execute SQL.

## Use the library

Check raw input field presence, duplicates and byte limits before assembling decoded values. No field has a default.
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
| `gest_id` | Full task ID string, called `id` in Gest's task `Model`. |
| `project_id` | Full project ID string. |
| `title` | Any string. |
| `priority` | Null or a JSON integer from 0 through 255. |
| `status` | A status string accepted by Gest's task status `FromStr` implementation. |
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

Status parsing uses the parser of `TaskStatus` in the [Gest model](gest-model.md),
which is a port of upstream `FromStr`. The statuses are `open`,
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

`Cargo.toml` pins chrono to 0.4.44 with default features disabled. Its enabled
features are `alloc` and `serde`. The [Gest model](gest-model.md) needs `serde`
to read and write its timestamps. The `clock` feature stays off.
The lockfile includes chrono 0.4.44, num-traits 0.2.19 and autocfg 1.5.1.
These additions support timestamp conversion in Rust and do not add a native
Fossil process or a SQL date conversion to this module.

## Caller responsibilities

The caller must validate and decode raw fields before constructing the input.
For ticket artifacts, it must classify each J card, which assigns a named field.
It must check complete field presence, reject duplicate field spellings where
required and enforce byte limits for the operation.
The explicit input struct is not an object decoder and cannot identify fields
that the caller discarded. The enclosing implementation must validate history, resolve references to other
records and decide which unsupported fields or records to reject.

This module supplies complete task values for before and after snapshots.
The separate task ticket emitter supplies creation bytes. Applying history, recording operation recipes and the source of changes,
undoing operations and writing storage remain
separate work. The converter does not enable migration of stored repositories.

## Source and verification evidence

The converter follows Gest source commit
`eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8` with the Gossil validation rules
specified above. Source mappings, test counts and historical run qualifications
are preserved in the [task value evidence archive](https://github.com/rahuldave/gossil/blob/main/docs/_history/task-value-evidence.md).
Those runs qualify their recorded trees and do not establish full Gest or Fossil compatibility.

The task ticket emitter takes its list of canonical fields from this module.
The list borrows the title, description and metadata, and it holds short owned
strings for the identifiers, the status and the timestamps.
Storage and commands do not call this module. Database schemas do not
integrate its task record format.

This page follows the nossaifd writing guide, including principles credited to
Rosalind Reid, Shreya Shankar and Oskar Austegard.

## Recommendations

Use the explicit decoded-field converter for complete task snapshots.
Next, handle `TaskValueError` and use `task_to_values` to inspect the normalized
values or pass the validated record to the task ticket emitter.
