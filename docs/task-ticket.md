# Emit a task creation artifact

The `task_ticket` module turns a validated `TaskValues` snapshot into complete,
uncompressed bytes in Fossil's ticket artifact format. A ticket records work to
track, and its artifact is a sequence of newline-terminated records called cards.
Gossil uses those cards to carry task fields. Here, native means the Fossil
artifact format.

The task emitter shares its card syntax, size plan and checksum with the
artifact and project emitters, which the [Gest ticket guide](gest-ticket.md) describes.

Use the library to inspect or test
task creation artifacts in memory. No current Gossil repository
API consumes these bytes. Raw input admission, storage integration and issue
commands remain separate work.

## Use the library

Validate all eleven decoded fields with `task_from_values` first.
The [task values guide](gest-task-values.md#use-the-library) shows that step.
Given the resulting `task`, emit an artifact with an explicit byte limit:

```rust
use gossil::native_date::NativeDate;
use gossil::task_ticket::task_creation_bytes;

let date = NativeDate::parse(b"2026-11-18T12:00:00.000000001Z").unwrap();
let ticket_id = b"0123456789abcdef0123456789abcdef01234567";
let bytes = task_creation_bytes(&task, ticket_id, b"native actor", date, 65_536).unwrap();
assert!(bytes.len() <= 65_536);
```

Use the [application setup](examples.md#add-gossil-to-an-application) when calling
Gossil from another crate. Handle `DateError` and `TicketError` in applications
rather than unwrapping the results.

A J card assigns a named field to its encoded value. The D card records the
artifact creation date, and the U card records its actor.

The K card contains the native ticket identity, which requires exactly forty
lowercase ASCII hexadecimal bytes. Supply it independently of the task's `gest_id`.
Gest is a task and artifact tracker. A Gest ID has 32 lowercase letters from `k` through `z` and remains a J field.
The emitter neither generates identities nor derives one identity from the other.

The U card contains caller-supplied actor bytes escaped with the Fossil token
codec. These bytes need not be a Gest author ID or UTF-8 text.

## Date domain

`NativeDate` represents a Gregorian UTC instant as signed `i128` nanoseconds
since the Unix epoch. Supported years are 0001 through 9999, inclusive.
`parse` accepts `YYYY-MM-DDTHH:MM:SS[.fraction][Z]`, where a fraction has one
through nine digits. A missing suffix also means UTC.
It rejects offsets, leap seconds, year zero, invalid calendars and excess
fractional digits. It does not normalize other spellings into this grammar.

`from_epoch_nanos` checks the same year range. `epoch_nanos` returns the integer
value without floating-point conversion. Display and the emitted D card use
exactly nine fractional digits and no suffix.
This domain is separate from the more permissive Gest business timestamps in
`TaskValues`. The caller supplies the D card date independently of those timestamps.

## Artifact and limits

The artifact has a D card, fourteen J cards sorted by field name, a K card,
a U card and a final Z checksum card. Each card ends with a newline.

The eleven model fields are `assigned_to`, `created_at`, `description`,
`gest_id`, `metadata`, `priority`, `project_id`, `resolved_at`, `status`,
`title` and `updated_at`. Each uses compact `serde_json` encoding of the values
returned by [`task_to_values`](gest-task-values.md#conversion-contract), followed
by [native token escaping](token-codec.md). Strings retain JSON quotes, priority
is an integer or null, and metadata retains its JSON value. Status and business
timestamps use the normalized forms in the task values contract. Null values
remain present.

The three fixed fields classify this Gossil record. `gossil_deleted` is JSON
`false`, marking the creation as not deleted. `gossil_kind` is raw `task`,
identifying a task record, and `gossil_schema` is raw `1`, identifying its schema
version. These fields define Gossil's task schema; this emitter does not implement
a consumer for that schema. The last two values are literal bytes rather than JSON strings.

This function emits complete creation snapshots. It emits neither field-appending
updates that extend existing field values nor cards that link a parent artifact
to record ancestry. See the
[artifact parser guide](artifact-parser.md) for supported structural card forms.

The Z card holds the lowercase MD5 checksum of all preceding artifact bytes.

`max_output` bounds the complete encoded artifact, including token expansion,
card syntax, checksum and newlines. The effective limit also respects
`isize::MAX`. An exact-size limit succeeds; a smaller limit returns
`TicketError::Size`. The limit is an output bound, rather than a total heap
budget. The emitter counts borrowed model values before retaining serialized
JSON or reserving the output buffer.

Bookkeeping allocations have fixed bounds independent of task string and metadata
sizes. The identifier, status and timestamp strings, the fourteen-field list and
the checksum still allocate.
Even a zero output limit can therefore allocate fixed bookkeeping; the limit does
not cap all memory use.

`TicketError::TicketId` rejects malformed K bytes. `Allocation` retains a
fallible reservation error, `Token` retains a token codec error, and `Json`
retains a serialization error. The generated API documents these variants.
The `gest_ticket` module defines `TicketError`, and `task_ticket` re-exports it.
Its `Size` message is `ticket output limit`, because the task, artifact and
project emitters all return it.

## Caller responsibilities

Check raw field presence, duplicate keys and input byte limits before constructing
`TaskValues`. Choose the native ticket identity, actor, D instant and output
budget independently of the task's model fields.

The emitter performs no clock reads, identity generation or database writes.
It does not replay history, check references to other records or determine who
may write a task. A future repository operation must define and enforce those
rules, including any requirement that artifact dates increase. Emission alone
does not establish Fossil engine compatibility. Repository migration must separately validate and store any existing records.

This page follows the nossaifd writing guide, including principles credited to
Rosalind Reid, Shreya Shankar and Oskar Austegard.

## Recommendations

Use `task_from_values` to validate a snapshot, then call `task_creation_bytes`
with an explicit output limit. Next, inspect the returned bytes or use them in
application tests, and handle emission errors. Repository storage integration
remains planned.
