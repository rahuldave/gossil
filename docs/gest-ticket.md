# Emit artifact and project creation tickets

The `gest_ticket` module turns a validated Gest artifact or project record into
complete, uncompressed bytes in Fossil's ticket artifact format. Gest is the
issue tracker that Gossil ports. Each Gest record becomes an ordinary Fossil
ticket, so stock Fossil can store, rebuild and show it.

A Gest artifact is a document record, for example a spec or a plan. It is not
a Fossil blob. A Gest project groups the tasks, artifacts and iterations of one
piece of work. Planned work also uses the artifact ticket. A logical change is
a stable name for future work on an issue. The planned-changes contract
CHANGE-01 uses an artifact ticket to carry each logical change.

Use the library to inspect or test these creation artifacts in memory. The
[issue ticket writer](issue-ticket.md) stores a project creation privately in a
native repository. Separate work remains for the attachment that declares a logical
change, the storage of artifact tickets and the issue commands other than `gos i init` and
`gos i task create`.

## Use the library

Validate the decoded fields first, with `artifact_from_values` or
`project_from_values`. The [artifact values guide](gest-artifact-values.md) and
the [project values guide](gest-project-values.md) show that step. Then emit
the artifact with an explicit byte limit:

```rust
use gossil::gest_artifact_values::{ArtifactValueInputs, artifact_from_values};
use gossil::gest_ticket::artifact_creation_bytes;
use gossil::native_date::NativeDate;
use serde_json::json;

let artifact = artifact_from_values(ArtifactValueInputs {
    gest_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
    project_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
    title: json!("Plan"),
    body: json!("Complete body"),
    metadata: json!({}),
    archived_at: json!(null),
    created_at: json!("2026-11-18T12:00:00Z"),
    updated_at: json!("2026-11-18T12:00:00Z"),
})
.unwrap();
let date = NativeDate::parse(b"2026-11-18T12:00:00Z").unwrap();
let ticket_id = b"0123456789abcdef0123456789abcdef01234567";
let bytes = artifact_creation_bytes(&artifact, ticket_id, b"native actor", date, 65_536).unwrap();
assert!(bytes.len() <= 65_536);
```

`project_creation_bytes` takes a `ProjectValues` record and the same four other
arguments. In an application, handle `TicketError` and do not unwrap the
result.

## Artifact cards

A ticket artifact is a sequence of newline-terminated records called cards.
Each emitted artifact has a D card, the sorted J cards, a K card, a U card and
a final Z card:

- **D** holds the creation date in the [native date grammar](task-ticket.md#date-domain).
- **J** assigns one named field to its encoded value.
- **K** holds the ticket identity, exactly forty lowercase ASCII hexadecimal bytes.
- **U** holds the actor bytes, escaped with the [token codec](token-codec.md).
- **Z** holds the lowercase MD5 checksum of all the bytes before it.

The caller supplies K independently of the record's `gest_id`. A Gest ID has
32 lowercase letters from `k` through `z` and stays a J field. The emitter
neither generates identities nor derives one identity from the other.

## Fields of each kind

The J value of each model field is the compact `serde_json` encoding of the converter output,
followed by native token escaping. The converter output is the value of
`artifact_to_values` or `project_to_values`. Strings keep their JSON quotes,
metadata keeps its JSON value, and null values stay present. Timestamps use
the UTC form `+00:00`.

| Kind | Model fields | J cards |
|---|---|---|
| artifact | `archived_at`, `body`, `created_at`, `gest_id`, `metadata`, `project_id`, `title`, `updated_at` | 11 |
| project | `archived_at`, `created_at`, `gest_id`, `updated_at` | 7 |

Each artifact also has the three fixed fields of the task ticket.
`gossil_deleted` is JSON `false`. `gossil_kind` is the raw kind name, `artifact`
or `project`, and `gossil_schema` is raw `1`. The J cards are sorted by name.

The field names are the Gest table columns, with `id` renamed to `gest_id`. The
artifact body keeps its exact text. A non-null `archived_at` does not mark a
record as deleted. The project's local checkout root is not a canonical field,
so a project ticket does not include the path of the local checkout.

## One shared emitter

The task, artifact and project emitters call one crate-private function. Each
kind gives only its sorted list of fields and its kind name. So the three kinds
share the card syntax, the size plan, the limit checks and the
checksum. The [task ticket guide](task-ticket.md) describes the same rules for
tasks. It also describes `max_output`.

The shared emitter does not change the task bytes, and two task fixtures hold
the expected bytes. The first fixture was spelled by hand before the shared
emitter existed, and the second fills every task field.

`max_output` bounds the complete encoded artifact. An exact-size limit
succeeds, and a smaller limit returns `TicketError::Size`. The emitter counts
the encoded size of the borrowed title, body and metadata before it copies
them. A test checks that a 256 KiB body over the limit causes no allocation of
4096 bytes or more.

This module defines `TicketError`, and `task_ticket` re-exports it. Its `Size`
message is `ticket output limit`, because all three kinds return the error.

## Tests and the Fossil check

`tests/gest_ticket.rs` covers both kinds:

- **Golden bytes.** Each kind matches a fixture that
  `tests/fixtures/gest-ticket/generate.py` spells by hand. The same script
  also writes both task fixtures of `tests/task_ticket.rs`.
- **Round trips.** Each record survives the structural parser, then the
  converter again, for row GEST-T01 of the Gest contract.
- **Field mapping.** The J names equal the canonical field names in the
  committed converter source maps. Each J value equals the JSON bytes of the
  converter output.
- **Limits.** An oversized title, body or metadata value fails before a large
  allocation.

`just gest-ticket-interop` runs the stock Fossil check. It loads one artifact,
one project and one task ticket into a scratch repository of the pinned Fossil
2.29 at `7e8ecf0ce4`. The repository has one ticket column for each Gest field. After a full
rebuild, the test checks these results:

- `test-integrity --parse` finds no error.
- `ticket show` returns each J value byte for byte.
- `ticket history` lists each field change.
- `artifact` returns the emitted bytes.

If the Fossil reference binary is absent, the recipe prints `not-run` and exits with status
5. A `not-run` result is not a pass. The test never calls `fossil ticket add`,
because that command can crash in Fossil 2.29 at tkt.c:1862.

## Caller responsibilities

Check raw field presence, duplicate keys and input byte limits before you
construct a record. Choose the ticket identity, actor, D instant and output
limit independently of the model fields.

The emitter reads no clock, generates no identity and writes no database. It
does not replay history, check references or decide who may write a record. A
future repository operation must enforce these rules, including any rule that
artifact dates increase.

This page follows the nossaifd writing guide, including principles credited to
Rosalind Reid, Shreya Shankar and Oskar Austegard.

## Recommendations

Validate a record with its converter, then call `artifact_creation_bytes` or
`project_creation_bytes` with an explicit output limit. Run
`just gest-ticket-interop` after a change to the emitter, and report `not-run`
apart from a pass. Store a project with the [issue ticket writer](issue-ticket.md).
The storage of artifact tickets and the declaration of a logical change remain planned.
