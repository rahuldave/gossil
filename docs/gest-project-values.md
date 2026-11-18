# Gest project values

The `gossil::gest_project_values` module converts all four canonical project
fields into an owned, validated record. Use it for complete decoded snapshots
before storage integration. Project persistence and CLI commands remain planned.

Gest is a task and artifact tracker. These converters use its task and project record model. A Gest project represents a tracked repository or directory. Here, canonical
means the four project fields in the reviewed field map.
In a source checkout, the map is `tests/fixtures/gest_project_values/source-map.json`.

`ProjectValueInputs` requires an explicit `serde_json::Value` for `gest_id`,
`archived_at`, `created_at` and `updated_at`. `project_from_values` returns a
`ProjectValues` record with private fields. `project_to_values` returns all four
fields as independent owned values. The local checkout root is a filesystem path and is not canonical data.

The [upstream project model](https://github.com/rahuldave/gest/blob/eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8/src/store/model/project.rs) defines the source record.
See [library setup](examples.md) for Cargo dependencies.

## Use the library

Before constructing `serde_json::Value` inputs, callers check raw JSON for required fields,
unknown fields and duplicate keys. Duplicate keys must be checked before decoding collapses them.
Then assemble every field for conversion. Nullable archive
state requires an explicit null. The converter does not read a clock, generate
an identifier, look up references or execute SQL.

```rust
use gossil::gest_project_values::{ProjectValueInputs, project_from_values, project_to_values};
use serde_json::json;

let fields = ProjectValueInputs {
    gest_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
    archived_at: json!(null),
    created_at: json!("2026-11-18T07:00:00-05:00"),
    updated_at: json!("2025-01-01T00:00:00Z"),
};
let project = project_from_values(fields).unwrap();
let values = project_to_values(&project);
assert_eq!(values.created_at, json!("2026-11-18T12:00:00+00:00"));
assert_eq!(project_from_values(values).unwrap(), project);
```

The example sets modification before creation to show that conversion imposes no time ordering.

## Validation and caller responsibilities

The identifier must contain exactly 32 lowercase letters from `k` through `z`.
Archive state accepts null or an RFC3339 timestamp. Creation and modification
timestamps require strings. Shared validation uses chrono 0.4.44, normalizes to
UTC and checks that the output can be parsed again. It imposes no time ordering.

`ProjectValueError` displays the field and a static reason without echoing input.
Its `parse_cause()` accessor retains the actual chrono error from input parsing
or canonical UTC output parsing. Other failures have no parser cause.

Callers own raw decoding, required-field presence, unknown-field classification,
duplicate keys in the original input, byte budgets and reference checks.
Callers also check the complete history of changes to the record.
The converter does not establish project uniqueness or implement lifecycle,
replay, migration, storage or the complete Gest API.

## Evidence and scope

The seven converter controls in `tests/gest_project_values.rs` check complete output, independent ownership, types,
identifiers, nullable archive state and timestamp boundaries with typed causes.
These tests cover the converter itself. They do not reproduce the upstream
project API or repository tests, whose behavior and persistence still need separate validation.
The project fixtures retain the reviewed field map and
frozen catalog source provenance in `tests/fixtures/gest_catalog_values/source-map.json`.
The broader map does not imply that other catalog models ship here.
Project storage and reads of caller-selected records remain planned
through the repository transaction API.

## Recommendations

Use the converter after checking the complete raw input. Next, adapt the example
to your validated fields and use `project_to_values` to read the owned result.
