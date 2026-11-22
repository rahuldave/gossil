# Gest note values

The `gossil::gest_note_values` module converts all seven canonical note fields
into an owned, validated record. Use it for complete decoded snapshots before
storage integration. Note persistence and CLI commands remain planned.

Gest is a task and artifact tracker. A Gest note is a text comment that is
attached to one task, artifact or iteration, with an author when the author is
known. It is not a Fossil forum post.
Here, canonical means the seven note fields in the reviewed field map.
In a source checkout, the map is `tests/fixtures/gest_note_values/source-map.json`.

`NoteValueInputs` requires an explicit `serde_json::Value` for `gest_id`,
`entity_id`, `entity_type`, `author_id`, `body`, `created_at` and `updated_at`.
`note_from_values` returns a `NoteValues` record with private fields.
`note_to_values` returns all seven fields as independent owned values.

The [upstream note model](https://github.com/rahuldave/gest/blob/eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8/src/store/model/note.rs) defines the source record.
See [library setup](examples.md) for Cargo dependencies.

## Use the library

Before you build the `serde_json::Value` inputs, check the raw JSON for required fields,
unknown fields and duplicate keys. Check duplicate keys before decoding, because decoding merges them.
Then assemble every field for conversion. The converter does not read a clock,
generate an identifier, look up references or execute SQL.

```rust
use gossil::gest_note_values::{NoteValueInputs, note_from_values, note_to_values};
use serde_json::json;

let fields = NoteValueInputs {
    gest_id: json!("klmnopqrstuvwxyzklmnopqrstuvwxyz"),
    entity_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
    entity_type: json!("Task"),
    author_id: json!(null),
    body: json!("Checked the *build*.\n"),
    created_at: json!("2026-11-18T07:00:00-05:00"),
    updated_at: json!("2026-11-18T12:00:00Z"),
};
let record = note_from_values(fields).unwrap();
let output = note_to_values(&record);
assert_eq!(output.entity_type, json!("task"));
assert_eq!(output.author_id, json!(null));
assert_eq!(output.created_at, json!("2026-11-18T12:00:00+00:00"));
assert_eq!(note_from_values(output).unwrap(), record);
```

## Validation and caller responsibilities

The note ID and the entity ID must each contain exactly 32 lowercase letters
from `k` through `z`. The author ID is JSON null for an unknown author, or a full
ID of the same form. The converter keeps null as null. An empty string is not
an unknown author, so it fails.

The entity type accepts `artifact`, `iteration` and `task`. Gest ignores case
when it parses an entity type. So the converter accepts `Task` and returns the
lowercase spelling. A project, a tag and an author are not entity types here.
Surrounding whitespace fails.

The body accepts any string, including an empty string, Unicode, line endings
and null characters. Gest stores Markdown in the body, but the converter does
not check a format or a size. It keeps the body exactly as given.

Both timestamps must be strings. The shared timestamp validation uses `chrono`
0.4.44. It converts each timestamp to UTC and checks that the output can be
parsed again. It does not check the order of the two timestamps.

`NoteValueError` displays the field and a static reason, and does not repeat the input.
Its `parse_cause()` accessor returns the `chrono` error from the parse of the input
or from the parse of the canonical UTC output. Other failures have no parser cause.

Callers own raw decoding, required-field presence, unknown-field classification,
duplicate keys in the original input and byte budgets. Callers also check that
the entity and the author exist in the selected project, and they check the
complete history of changes to the record. The converter does not implement
lifecycle, replay, migration, storage or the complete Gest API.

## Evidence and scope

The ten converter controls in `tests/gest_note_values.rs` check complete output,
independent ownership, types, identifiers, the nullable author, entity-type
spellings, exact bodies and timestamp boundaries with typed causes. One of them
checks that the source map names each field, and that each control that it
names exists in the file.

These tests cover the converter itself. They do not reproduce the upstream note
API or repository tests. The upstream `EntityType` enum has five tests of its
`Display` and `FromStr` methods. The converter calls neither method, so the
source map lists the five tests as later API obligations.

## Recommendations

Use the converter after checking the complete raw input. Next, check that the
entity and the author of each converted note exist before you store the note.
