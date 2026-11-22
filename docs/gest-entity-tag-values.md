# Gest entity tag values

The `gossil::gest_entity_tag_values` module converts all four canonical
`entity_tag` fields into an owned, validated record. Use it for complete decoded
snapshots before storage integration. Tag attachment and CLI commands remain
planned.

Gest is a task and artifact tracker. An `entity_tag` row attaches one Gest tag to
one task, artifact or iteration. The row is a join record, which links two
other records and has no Gest ID of its own.
Here, canonical means the four `entity_tag` fields in the reviewed field map.
In a source checkout, the map is `tests/fixtures/gest_entity_tag_values/source-map.json`.

`EntityTagValueInputs` requires an explicit `serde_json::Value` for `entity_id`,
`entity_type`, `tag_id` and `created_at`. `entity_tag_from_values` returns an
`EntityTagValues` record with private fields, and `entity_tag_to_values` returns
all four fields as independent owned values.

The [upstream `entity_tag` model](https://github.com/rahuldave/gest/blob/eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8/src/store/model/entity_tag.rs) defines the source record.
See [library setup](examples.md) for Cargo dependencies.

## Use the library

Before you build the `serde_json::Value` inputs, check the raw JSON for required fields,
unknown fields and duplicate keys. Check duplicate keys before decoding, because decoding merges them.
Then assemble every field for conversion. The converter does not read a clock,
generate an identifier, look up references or execute SQL.

```rust
use gossil::gest_entity_tag_values::{
    EntityTagValueInputs, entity_tag_from_values, entity_tag_to_values,
};
use serde_json::json;

let fields = EntityTagValueInputs {
    entity_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
    entity_type: json!("Artifact"),
    tag_id: json!("klmnopqrstuvwxyzklmnopqrstuvwxyz"),
    created_at: json!("2026-11-18T07:00:00-05:00"),
};
let record = entity_tag_from_values(fields).unwrap();
let output = entity_tag_to_values(&record);
assert_eq!(output.entity_type, json!("artifact"));
assert_eq!(output.created_at, json!("2026-11-18T12:00:00+00:00"));
assert_eq!(entity_tag_from_values(output).unwrap(), record);
```

## Validation and caller responsibilities

The entity ID and the tag ID must each contain exactly 32 lowercase letters from
`k` through `z`. The entity type accepts `artifact`, `iteration` and `task`.
Gest ignores case when it parses an entity type, so the converter accepts `Artifact`
and returns the lowercase spelling. A value with whitespace around it fails.

The timestamp must be a string. The shared timestamp validation uses `chrono`
0.4.44. It converts the timestamp to UTC and checks that it can parse the
output again.

The Gossil contract gives each join record an immutable key, the JSON tuple
`[entity_type,entity_id,tag_id]`. Each attachment of a tag also has its own
ticket, called an incarnation. The tuple is stored in the field `gossil_key`
when that ticket is created. At most one incarnation for a tuple may be live.

The converter returns the three key fields with the entity type in lowercase. So two inputs
that differ only in the case of the entity type give equal records. The converter
does not build the key, choose the incarnation or check that only one is live.

`EntityTagValueError` displays the field and a static reason, and does not repeat the input.
Its `parse_cause()` accessor returns the `chrono` error from the parse of the input
or from the parse of the canonical UTC output. Other failures have no parser cause.

Callers own raw decoding, required-field presence, unknown-field classification,
duplicate keys in the original input and byte budgets. Callers also own the key,
the incarnation, the attachment and removal of a tag, and the check that the
entity and the tag exist. The converter does not implement lifecycle, replay, migration,
storage or the complete Gest API.

## Evidence and scope

The eight converter controls in `tests/gest_entity_tag_values.rs` check complete
output, independent ownership, types, identifiers, entity-type spellings, the
key fields and timestamp boundaries with typed causes. One of them checks that
the source map names each field, and that each control that it names exists in
the file.

The same control compares the map with the expected values in `tests/support/gest_source_map.rs`.
These values are the Gest commit, the hash of each cited Gest file and the exact list of fields.
They also include the hash of the test accounting addendum.
The control then makes copies of the map, each with one stale or malformed entry,
and checks that the comparison refuses each copy. The Gest files are not in this
repository, so the control compares the hashes that the map records with these values.
It does not hash the files themselves.

These tests cover the converter itself. They do not reproduce the tests of the
upstream tag API or repository. The upstream `EntityType` enum has five tests of its
`Display` and `FromStr` methods. The converter calls neither method, so the
source map lists the five tests as later API obligations. The upstream
constructor `Model::new` reads the clock and has no test.

## Recommendations

Use the converter after you check the complete raw input. Next, build the key
tuple and check that only one incarnation for it is live before you store a
converted attachment.
