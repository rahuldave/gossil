# Gest model

The `gossil::gest_model` module holds the Gest domain model. It has one record
type for each Gest entity, and the primitive values that those records use. The
storage adapters and the `issue` commands will share these types. Storage and
commands for these records remain planned.

Gest is a task and artifact tracker. Gossil ports it onto Fossil mechanisms. The
module is a copy of the `src/store/model` folder of
[Gest at commit `eaf821b`](https://github.com/rahuldave/gest/tree/eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8/src/store/model),
under the MIT licence of Gest. The licence text is in `src/gest_model/gest-LICENSE.txt`.
In the rest of this page, the pin means that commit.

## Records and primitives

Each submodule keeps the upstream name of its record, `Model`, and of its inputs
`New`, `Patch` and `Filter` where upstream has them:

- **Records.** The submodules are `artifact`, `author`, `entity_tag`, `iteration`,
  `note`, `project`, `relationship`, `tag` and `task`. Each record has private
  fields and a getter for each field, with the name and return type of upstream.
- **Primitives.** The `primitives` submodule holds `Id`, `TaskStatus`,
  `IterationStatus`, `AuthorType`, `EntityType`, `RelationshipType` and `Priority`.
  Each keeps its upstream variants, `Display` text, parser, serde spelling and methods,
  such as `is_terminal` and `inverse`.

An `Id` is 16 random bytes, written as 32 letters from `k` to `z`. `Id::new`
takes the bytes from the random source of the operating system. The constructors
`Author::new`, `Tag::new`, `Project::new` and `entity_tag::Model::new` read the
system clock, as upstream does.

## Build a model from a validated record

Gest decodes each record from a row of its own SQL tables. Gossil has no such
tables. Instead, a converter in a `gossil::gest_*_values` module validates the
decoded fields of a record. Then `From` moves the validated record into its model.
The conversion cannot fail, because the converter has checked each field.

```rust
use gossil::gest_model::{primitives::TaskStatus, task};
use gossil::gest_task_values::{TaskValueInputs, task_from_values};
use serde_json::json;

let record = task_from_values(TaskValueInputs {
    gest_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
    project_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
    title: json!("Review"),
    priority: json!(255),
    status: json!("IN_PROGRESS"),
    description: json!(""),
    assigned_to: json!(null),
    metadata: json!({}),
    resolved_at: json!(null),
    created_at: json!("2026-11-18T07:00:00-05:00"),
    updated_at: json!("2026-11-18T12:00:00Z"),
})
.unwrap();
let model = task::Model::from(record);
assert_eq!(model.status(), TaskStatus::InProgress);
assert_eq!(serde_json::to_value(&model).unwrap()["status"], "inprogress");
```

The task, artifact, iteration, author, tag, note, relationship and entity tag
records each have this conversion. The project record has none, because its
canonical fields leave out the root. The root is a local checkout binding, and
Gossil never publishes it. A caller that holds the root builds the project with
`Project::from_synced_parts`.

## The converters and the model parse the same text

The converters and the model accept the same text, because the converters use the
parsers of the primitives. An identifier passes a converter only when `Id` parses
it. A task status, an iteration status, an author type, an entity type and a
relationship type pass only when the parser of their primitive accepts them. The
converters keep their own error messages, which name the field and do not repeat
the input. The validated converter records hold the primitives, and the port did
not change the converter output.

The `Display` text and the serde spelling differ for one value. `Display` writes
the in-progress task status as `in-progress`, which is the stored spelling. Serde
writes it as `inprogress`, which is the spelling of the Gest API. The parser
accepts both, and also `in_progress`, in any case.

## Differences from upstream

The port keeps the upstream types, names and behavior, with these exceptions:

- **No row decoding.** The `TryFrom<libsql::Row>` implementations are not here.
  The `From` conversions above take their place.
- **No local bookkeeping models.** The `project_workspace` and `transaction`
  models are not here. They hold local workspace registrations and the local
  undo history, which the storage tasks own.
- **Fallible identifier decoding.** Upstream decodes an `Id` from serde through
  `From<String>`, which panics on invalid text. Here, invalid text is a decoding
  error. `From<String>` and `From<&str>` still panic on invalid text, as upstream
  does, so use `str::parse` for text from outside.
- **Getters by hand.** Upstream derives the getters with the `getset` crate.
  Here they are written out, with the same names and return types.
- **No new clock feature.** `chrono` has no `clock` feature in Gossil. The
  constructors read `std::time::SystemTime`, as `chrono::Utc::now` does.
- **Random bytes from `getrandom`.** Upstream uses `rand`.

## Evidence and scope

The tests hold the 77 inherited Gest tests of the model files at the pin. Most of
them are in `tests/gest_model.rs`. Four identifier tests use private items, so they
stay next to the code in `src/gest_model/primitives/id.rs`. The bodies of the tests
are the upstream bodies. Only the imports change, and the project test reads the
clock without `chrono::Utc::now`. One inherited test checks the excluded
`project_workspace` model, and it is not ported.

The source map `tests/fixtures/gest_model/source-map.json` names the port of each
inherited test, with the test ID, assertion IDs and fixture hash from the Gest
test inventory. It also cites each ported Gest file with its hash at the pin. A
test compares those hashes with the shared table in
`tests/support/gest_source_map.rs`. It fails when a copy of the map has a stale or
malformed entry. Another test checks that each model test that a converter map
lists as a later obligation is a test that the model map ports.

Other new tests check the serde spellings and the constructors that read the
clock. They also check that a bad identifier gives an error and not a panic, and that
each validated record converts into its model.

The module defines no repository traits, storage, commands or web routes. The
model inputs `New`, `Patch` and `Filter` are plain data, and no code reads them yet.

## Recommendations

Build each model from a validated converter record, and parse outside text with
`str::parse` rather than `From<String>`. Next, port the Gest repository traits on
top of these types.
