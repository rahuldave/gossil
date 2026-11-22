# Gest artifact values

The `gossil::gest_artifact_values` module converts all eight canonical artifact
fields into an owned, validated record. Use it for complete decoded snapshots
before storage integration. Artifact persistence and CLI commands remain planned.

Gest is a task and artifact tracker. A Gest artifact is a document with a title
and a body, such as a spec or a plan. It is not a Fossil artifact. Here, canonical
means the eight artifact fields in the reviewed field map.
In a source checkout, the map is `tests/fixtures/gest_artifact_values/source-map.json`.

`ArtifactValueInputs` requires an explicit `serde_json::Value` for `gest_id`,
`project_id`, `title`, `body`, `metadata`, `archived_at`, `created_at` and
`updated_at`. `artifact_from_values` returns an `ArtifactValues` record with
private fields. `artifact_to_values` returns all eight fields as independent owned values.

The [upstream artifact model](https://github.com/rahuldave/gest/blob/eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8/src/store/model/artifact.rs) defines the source record.
See [library setup](examples.md) for Cargo dependencies.

## Use the library

Before constructing `serde_json::Value` inputs, callers check raw JSON for required fields,
unknown fields and duplicate keys. Callers must check duplicate keys before decoding merges them.
Then assemble every field for conversion. The archive time can be null, but the
caller must give the null explicitly. The converter does not read a clock, generate an identifier,
look up references or execute SQL.

```rust
use gossil::gest_artifact_values::{ArtifactValueInputs, artifact_from_values, artifact_to_values};
use serde_json::json;

let fields = ArtifactValueInputs {
    gest_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
    project_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
    title: json!(""),
    body: json!("Complete document body é"),
    metadata: json!([null, {"reviewed": false}]),
    archived_at: json!(null),
    created_at: json!("2026-11-18T07:00:00-05:00"),
    updated_at: json!("2026-11-18T12:00:00Z"),
};
let record = artifact_from_values(fields).unwrap();
let output = artifact_to_values(&record);
assert_eq!(output.body, json!("Complete document body é"));
assert_eq!(output.created_at, json!("2026-11-18T12:00:00+00:00"));
assert_eq!(artifact_from_values(output).unwrap(), record);
```

The example uses an empty title to show that strings have no content rule.

## Validation and caller responsibilities

Both identifiers must contain exactly 32 lowercase letters from `k` through `z`.
The title and the body accept any string, including an empty string, Unicode
and null characters. Metadata accepts any JSON value, including null, scalars
and arrays. The converter keeps the JSON value but not its raw spelling.

Archive state accepts null or an RFC3339 timestamp. Creation and modification
timestamps require strings. Shared validation uses chrono 0.4.44, normalizes to
UTC and checks that the output can be parsed again. It imposes no time ordering.
An archived artifact keeps its body. The archive timestamp does not mark the
record as deleted.

`ArtifactValueError` displays the field and a static reason without echoing input.
Its `parse_cause()` accessor keeps the chrono error from input parsing
or canonical UTC output parsing. Other failures have no parser cause.

Callers own raw decoding, required-field presence, unknown-field classification,
duplicate keys in the original input, byte budgets and reference checks.
Callers also check the complete history of changes to the record.
The converter does not check that the project exists. It does not implement
lifecycle, replay, migration, storage or the complete Gest API.

## Evidence and scope

The nine converter controls in `tests/gest_artifact_values.rs` check complete output,
independent ownership, types, identifiers, metadata, archive state and timestamp
boundaries with typed causes. One control checks that the source map names each
field, and that it names only controls that exist in the file.
These tests cover the converter itself. They do not reproduce the upstream
artifact API or repository tests, whose behavior and persistence still need separate validation.
Artifact storage and reads of caller-selected records remain planned
through the repository transaction API.

## Recommendations

Use the converter after checking the complete raw input. Next, adapt the example
to your validated fields and use `artifact_to_values` to read the owned result.
