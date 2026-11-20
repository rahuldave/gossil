# Gest author values

The `gossil::gest_author_values` module validates all six canonical author fields
in memory. Use it for complete decoded snapshots before storage integration.
Gest is a task and artifact tracker. An author record identifies a human or agent
in its record model. Accepting the `human` value here does not deliver native
Human workflows.

`AuthorValueInputs` requires an explicit `serde_json::Value` for `gest_id`,
`author_type`, `name`, `email`, `created_at` and `updated_at`.
`author_from_values` returns an `AuthorValues` record with private fields.
`author_to_values` returns all six fields as independent owned values.
See [library setup](examples.md) for Cargo dependencies.

## Use the library

Before decoding raw input, check required fields, unknown fields and duplicate
keys. Decoding can collapse duplicate keys. Then supply every field, including
an explicit null for absent email.

```rust
use gossil::gest_author_values::{AuthorValueInputs, author_from_values, author_to_values};
use serde_json::json;

let author = author_from_values(AuthorValueInputs {
    gest_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
    author_type: json!("AGENT"),
    name: json!(" Alice "),
    email: json!(null),
    created_at: json!("2026-11-19T01:02:03.000000001+02:00"),
    updated_at: json!("2020-01-01T00:00:00Z"),
}).unwrap();
let values = author_to_values(&author);
assert_eq!(values.author_type, json!("agent"));
assert_eq!(values.name, json!(" Alice "));
assert_eq!(values.email, json!(null));
assert_eq!(values.created_at, json!("2026-11-18T23:02:03.000000001+00:00"));
assert_eq!(author_from_values(values).unwrap(), author);
```

The modification time precedes creation in this example because conversion
imposes no time ordering. It generates no identity or time and performs no SQL.

## Validation and errors

The identifier must contain exactly 32 lowercase letters from `k` through `z`.
The author type accepts case-insensitive `human` or `agent` and emits lowercase.
It rejects surrounding whitespace and other values.

Names accept any string, including empty strings, whitespace and control characters.
Email accepts an explicit null or any string. Null and an empty string remain
distinct. Conversion does not check email syntax or trim either field.

Both timestamps require strings that chrono 0.4.44 can parse as RFC3339.
Conversion normalizes each instant to UTC and checks that its output can be
parsed again. It rejects output outside that canonical RFC3339 domain.

`AuthorValueError` displays the field and a static reason without echoing input.
Its `parse_cause()` accessor returns the actual chrono error from input parsing
or canonical output parsing. Other failures have no parser cause.

## Scope and controls

Callers own raw decoding, field admission, duplicate detection, byte limits,
reference checks and the history of changes to the record.
An author record is separate from native actor bytes and local principals.
Conversion does not establish uniqueness, authorization, storage, lifecycle,
replay, migration or the complete Gest API.

The seven controls in `tests/gest_author_values.rs` cover field types, identifiers,
author types, exact strings, nullable email, timestamps, parser causes and owned
output. These controls exercise the pure converter. They do not test storage or
native workflows. Their presence does not establish an executed test pass.

## Recommendations

Use the converter after checking the complete raw input. Next, adapt the example
to your admitted fields and read the result with `author_to_values`.
