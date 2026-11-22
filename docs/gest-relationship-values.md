# Gest relationship values

The `gossil::gest_relationship_values` module converts all eight canonical
relationship fields into an owned, validated record. Use it for complete decoded
snapshots before storage integration. Relationship persistence and CLI commands
remain planned.

Gest is a task and artifact tracker. A Gest relationship is one directed link
from a source entity to a target entity, for example a task that blocks another
task. Each entity is a task, an artifact or an iteration.
Here, canonical means the eight relationship fields in the reviewed field map.
In a source checkout, the map is `tests/fixtures/gest_relationship_values/source-map.json`.

`RelationshipValueInputs` requires an explicit `serde_json::Value` for `gest_id`,
`rel_type`, `source_id`, `source_type`, `target_id`, `target_type`, `created_at`
and `updated_at`. `relationship_from_values` returns a `RelationshipValues`
record with private fields. `relationship_to_values` returns all eight fields as
independent owned values.

The [upstream relationship model](https://github.com/rahuldave/gest/blob/eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8/src/store/model/relationship.rs) defines the source record.
See [library setup](examples.md) for Cargo dependencies.

## Use the library

Before you build the `serde_json::Value` inputs, check the raw JSON for required fields,
unknown fields and duplicate keys. Check duplicate keys before decoding, because decoding merges them.
Then assemble every field for conversion. The converter does not read a clock,
generate an identifier, look up references or execute SQL.

```rust
use gossil::gest_relationship_values::{
    RelationshipValueInputs, relationship_from_values, relationship_to_values,
};
use serde_json::json;

let fields = RelationshipValueInputs {
    gest_id: json!("klmnopqrstuvwxyzklmnopqrstuvwxyz"),
    rel_type: json!("Child-Of"),
    source_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
    source_type: json!("task"),
    target_id: json!("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"),
    target_type: json!("TASK"),
    created_at: json!("2026-11-18T07:00:00-05:00"),
    updated_at: json!("2026-11-18T12:00:00Z"),
};
let record = relationship_from_values(fields).unwrap();
let output = relationship_to_values(&record);
assert_eq!(output.rel_type, json!("child-of"));
assert_eq!(output.target_type, json!("task"));
assert_eq!(output.created_at, json!("2026-11-18T12:00:00+00:00"));
assert_eq!(relationship_from_values(output).unwrap(), record);
```

## Validation and caller responsibilities

The relationship ID, the source ID and the target ID must each contain exactly
32 lowercase letters from `k` through `z`. The source type and the target type
accept `artifact`, `iteration` and `task`.

The relationship type accepts the five Gest spellings `blocked-by`, `blocks`,
`child-of`, `parent-of` and `relates-to`. Gest ignores case when it parses a
relationship type or an entity type. So the converter accepts `Child-Of`, and
the output is the lowercase spelling. Underscore spellings such as `child_of`
and surrounding whitespace fail.

The record keeps the direction as given. Gest pairs each type with an inverse
type. `blocks` and `blocked-by` are inverses, `child-of` and `parent-of` are
inverses, and `relates-to` is its own inverse. The converter does not apply
the inverse and does not create the inverse row. A row from A to B and a row
from B to A are two different records.

Both timestamps must be strings. The shared timestamp validation uses `chrono`
0.4.44. It converts each timestamp to UTC and checks that the output can be
parsed again. It does not check the order of the two timestamps.

Upstream, the directed tuple of type, source type, source ID, target type and
target ID is unique among all relationships. A single record cannot show a
duplicate, so the converter does not check uniqueness. The Gossil contract
GEST-CONSTRAINT-01 requires that check across all live relationship records.
Gossil does not implement this check yet.

`RelationshipValueError` displays the field and a static reason, and does not repeat the input.
Its `parse_cause()` accessor returns the `chrono` error from the parse of the input
or from the parse of the canonical UTC output. Other failures have no parser cause.

Callers own raw decoding, required-field presence, unknown-field classification,
duplicate keys in the original input, byte budgets and the uniqueness of the directed tuple. Callers
also check that both endpoints exist in the selected project, and they check
the complete history of changes to the record. The converter does not implement
lifecycle, replay, migration, storage or the complete Gest API.

## Evidence and scope

The ten converter controls in `tests/gest_relationship_values.rs` check complete
output, independent ownership, types, identifiers, type spellings, direction and
timestamp boundaries with typed causes. One of them checks that the source map
names each field, and that each control that it names exists in the file.

The same control compares the map with the expected values in `tests/support/gest_source_map.rs`.
These values are the Gest commit, the hash of each cited Gest file and the exact list of fields.
They also include the hash of the test accounting addendum.
The control then makes copies of the map, each with one stale or malformed entry,
and checks that the comparison refuses each copy. The Gest files are not in this
repository, so the control compares the hashes that the map records with these values.
It does not hash the files themselves.

These tests cover the converter itself. They do not reproduce the upstream
relationship API or repository tests. The source map lists 15 upstream enum
tests as later API obligations. Five test the `EntityType` enum, and ten test
the `RelationshipType` enum, including its five inverse tests. The converter
calls none of those enum methods.

## Recommendations

Use the converter after checking the complete raw input. Next, check across the
live records that the directed tuple is unique and that both endpoints exist.
Do this before you store a converted relationship.
