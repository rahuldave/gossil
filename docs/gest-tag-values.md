# Gest tag values

The `gossil::gest_tag_values` module converts all four canonical tag fields into
an owned, validated record. Use it for complete decoded snapshots before storage
integration. Tag persistence and CLI commands remain planned.

Gest is a task and artifact tracker. A Gest tag is a label that Gest stores once
and attaches to tasks, artifacts and iterations. It is not a Fossil control tag.
Here, canonical means the four tag fields in the reviewed field map.
In a source checkout, the map is `tests/fixtures/gest_tag_values/source-map.json`.

`TagValueInputs` requires an explicit `serde_json::Value` for `gest_id`, `label`,
`created_at` and `updated_at`. `tag_from_values` returns a `TagValues` record
with private fields. `tag_to_values` returns all four fields as independent owned values.

The [upstream tag model](https://github.com/rahuldave/gest/blob/eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8/src/store/model/tag.rs) defines the source record.
See [library setup](examples.md) for Cargo dependencies.

## Use the library

Before you build the `serde_json::Value` inputs, check the raw JSON for required fields,
unknown fields and duplicate keys. Check duplicate keys before decoding, because decoding merges them.
Then assemble every field for conversion. The converter does not read a clock,
generate an identifier, look up references or execute SQL.

```rust
use gossil::gest_tag_values::{TagValueInputs, tag_from_values, tag_to_values};
use serde_json::json;

let fields = TagValueInputs {
    gest_id: json!("klmnopqrstuvwxyzklmnopqrstuvwxyz"),
    label: json!("Urgent"),
    created_at: json!("2026-11-18T07:00:00-05:00"),
    updated_at: json!("2026-11-18T12:00:00Z"),
};
let record = tag_from_values(fields).unwrap();
let output = tag_to_values(&record);
assert_eq!(output.label, json!("Urgent"));
assert_eq!(output.created_at, json!("2026-11-18T12:00:00+00:00"));
assert_eq!(tag_from_values(output).unwrap(), record);
```

## Validation and caller responsibilities

The identifier must contain exactly 32 lowercase letters from `k` through `z`.
The label accepts any string, including an empty string, Unicode and null
characters. The converter keeps the label exactly as given, so `Urgent` and
`urgent` stay two different labels.

Both timestamps must be strings. The shared timestamp validation uses `chrono`
0.4.44. It converts each timestamp to UTC and checks that the output can be
parsed again. It does not check the order of the two timestamps.

Upstream, each label is unique among all tags. A single record cannot show a
duplicate, so the converter does not check uniqueness. The Gossil contract
GEST-CONSTRAINT-01 requires that check across all live tag records.

Under that contract, two tags with equal labels and distinct IDs are a conflict. The check keeps
both records, marks them as conflicts and hides the conflicted rows. Gossil does
not implement the check or the hiding of those rows yet. The converter handles a
single record and has no conflict state.

`TagValueError` displays the field and a static reason, and does not repeat the input.
Its `parse_cause()` accessor returns the `chrono` error from the parse of the input
or from the parse of the canonical UTC output. Other failures have no parser cause.

Callers own raw decoding, required-field presence, unknown-field classification,
duplicate keys in the original input, byte budgets and label uniqueness.
Callers also check the complete history of changes to the record.
The converter does not attach tags to entities. It does not implement
lifecycle, replay, migration, storage or the complete Gest API.

## Evidence and scope

The nine converter controls in `tests/gest_tag_values.rs` check complete output,
independent ownership, types, identifiers, exact labels and timestamp boundaries
with typed causes. One of them checks that the source map names each field, and
that each control that it names exists in the file.

The same control compares the map with the expected values in `tests/support/gest_source_map.rs`.
These values are the Gest commit, the hash of each cited Gest file and the exact list of fields.
They also include the hash of the test accounting addendum.
The control then makes copies of the map, each with one stale or malformed entry,
and checks that the comparison refuses each copy. The Gest files are not in this
repository, so the control compares the hashes that the map records with these values.
It does not hash the files themselves.

These tests cover the converter itself. They do not reproduce the upstream tag
API or repository tests. The upstream constructor test calls `Model::new`, which
generates an ID and reads the clock. The source map lists it as a later API obligation.

## Recommendations

Use the converter after checking the complete raw input. Next, check label
uniqueness across the live tag records before you store a converted tag.
