# Gest iteration values

The `gossil::gest_iteration_values` module converts all nine canonical iteration
fields into an owned, validated record. Use it for complete decoded snapshots
before storage integration. Iteration persistence and CLI commands remain planned.

Gest is a task and artifact tracker. A Gest iteration is a time-boxed group of
tasks in one project. Here, canonical means the nine iteration fields in the
reviewed field map.
In a source checkout, the map is `tests/fixtures/gest_iteration_values/source-map.json`.

`IterationValueInputs` requires an explicit `serde_json::Value` for `gest_id`,
`project_id`, `title`, `status`, `description`, `metadata`, `completed_at`,
`created_at` and `updated_at`. `iteration_from_values` returns an
`IterationValues` record with private fields. `iteration_to_values` returns all
nine fields as independent owned values.

The [upstream iteration model](https://github.com/rahuldave/gest/blob/eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8/src/store/model/iteration.rs) defines the source record.
See [library setup](examples.md) for Cargo dependencies.

## Use the library

Before constructing `serde_json::Value` inputs, callers check raw JSON for required fields,
unknown fields and duplicate keys. Callers must check duplicate keys before decoding merges them.
Then assemble every field for conversion. An active iteration requires an
explicit null completion time. The converter does not read a clock, generate an
identifier, look up references or execute SQL.

```rust
use gossil::gest_iteration_values::{IterationValueInputs, iteration_from_values, iteration_to_values};
use serde_json::json;

let fields = IterationValueInputs {
    gest_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
    project_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
    title: json!("Sprint 4"),
    status: json!("COMPLETED"),
    description: json!(""),
    metadata: json!({}),
    completed_at: json!("2026-11-18T12:00:00Z"),
    created_at: json!("2026-11-18T07:00:00-05:00"),
    updated_at: json!("2026-11-18T12:00:00Z"),
};
let record = iteration_from_values(fields).unwrap();
let output = iteration_to_values(&record);
assert_eq!(output.status, json!("completed"));
assert_eq!(output.created_at, json!("2026-11-18T12:00:00+00:00"));
assert_eq!(iteration_from_values(output).unwrap(), record);
```

The example gives the status in capital letters to show that output uses the
lowercase spelling.

## Validation and caller responsibilities

Both identifiers must contain exactly 32 lowercase letters from `k` through `z`.
The title and the description accept any string, including an empty string,
Unicode and null characters. Metadata accepts any JSON value, including null,
scalars and arrays. The converter keeps the JSON value but not its raw spelling.

Status accepts `active`, `completed` or `cancelled` in any letter case, as the
upstream parser does. Surrounding spaces and other spellings fail. A terminal
status, which is `completed` or `cancelled`, requires an RFC3339
completion timestamp, and `active` requires null. The upstream row reader does
not check this pair, but the Gossil contract requires it. The converter does not
create a completion time.

Creation and modification timestamps require strings. Shared validation uses
chrono 0.4.44, normalizes to UTC and checks that the output can be parsed again.
It imposes no time ordering.

`IterationValueError` displays the field and a static reason without echoing input.
Its `parse_cause()` accessor keeps the chrono error from input parsing
or canonical UTC output parsing. Other failures have no parser cause.

Callers own raw decoding, required-field presence, unknown-field classification,
duplicate keys in the original input, byte budgets and reference checks.
Callers also check the complete history of changes to the record.
The converter does not check that the project exists. It does not implement
task membership, lifecycle, replay, migration, storage or the complete Gest API.

## Evidence and scope

The ten converter controls are in `tests/gest_iteration_values.rs`. They check
complete output, independent ownership, types, identifiers, metadata, status
spellings, the terminal timestamp pair and timestamp boundaries with typed causes. One
control checks the source map.

The same control compares the map with the expected values in `tests/support/gest_source_map.rs`.
These values are the Gest commit, the hash of each cited Gest file and the exact list of fields.
They also include the hash of the test accounting addendum.
The control then makes copies of the map, each with one stale or malformed entry,
and checks that the comparison refuses each copy. The Gest files are not in this
repository, so the control compares the hashes that the map records with these values.
It does not hash the files themselves.

These tests cover the converter itself, not the upstream iteration API or
repository tests. The source map lists the six upstream tests of the status
display and of the terminal check as later API work. The converter uses that
display and that check through the port of `IterationStatus` in the
[Gest model](gest-model.md). The tests of that port run the six tests.
Iteration storage and reads of caller-selected records remain planned
through the repository transaction API.

## Recommendations

Use the converter after checking the complete raw input. Next, adapt the example
to your validated fields and use `iteration_to_values` to read the owned result.
