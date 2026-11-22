# Gest iteration task values

The `gossil::gest_iteration_task_values` module converts all four canonical
`iteration_task` fields into an owned, validated record. Use it for complete
decoded snapshots before storage integration. Iteration membership commands
remain planned.

Gest is a task and artifact tracker. An `iteration_task` row places one task in
one phase of one iteration. A phase is a number that orders the work inside an
iteration. The row is a join record, which links two other records and has no
Gest ID of its own.
Here, canonical means the four `iteration_task` fields in the reviewed field map.
In a source checkout, the map is `tests/fixtures/gest_iteration_task_values/source-map.json`.

`IterationTaskValueInputs` requires an explicit `serde_json::Value` for
`iteration_id`, `task_id`, `phase` and `created_at`.
`iteration_task_from_values` returns an `IterationTaskValues` record with private
fields, and `iteration_task_to_values` returns all four fields as independent
owned values.

Gest has no model file for this row, and the [upstream iteration repository](https://github.com/rahuldave/gest/blob/eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8/src/store/repo/iteration.rs) reads and writes it.
See [library setup](examples.md) for Cargo dependencies.

## Use the library

Before you build the `serde_json::Value` inputs, check the raw JSON for required fields,
unknown fields and duplicate keys. Check duplicate keys before decoding, because decoding merges them.
Then assemble every field for conversion. The converter does not read a clock,
generate an identifier, look up references or execute SQL.

```rust
use gossil::gest_iteration_task_values::{
    IterationTaskValueInputs, iteration_task_from_values, iteration_task_to_values,
};
use serde_json::json;

let fields = IterationTaskValueInputs {
    iteration_id: json!("klmnopqrstuvwxyzklmnopqrstuvwxyz"),
    task_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
    phase: json!(4294967295u32),
    created_at: json!("2026-11-18T07:00:00-05:00"),
};
let record = iteration_task_from_values(fields).unwrap();
let output = iteration_task_to_values(&record);
assert_eq!(output.phase, json!(4294967295u32));
assert_eq!(output.created_at, json!("2026-11-18T12:00:00+00:00"));
assert_eq!(iteration_task_from_values(output).unwrap(), record);
```

## Validation and caller responsibilities

The iteration ID and the task ID must each contain exactly 32 lowercase letters
from `k` through `z`.

The phase must be a JSON integer from 0 through 4294967295, the range of an
unsigned 32-bit value. A negative integer, a larger integer, a float such as
`1.0` and a string fail. Upstream, Gest stores the phase as an SQL integer and
reads it back with a cast that wraps a value outside that range. The Gossil
contract requires a checked range instead, so such a value fails.

The timestamp must be a string. The shared timestamp validation uses `chrono`
0.4.44. It converts the timestamp to UTC and checks that it can parse the
output again.

The Gossil contract gives each join record an immutable key, the JSON tuple
`[iteration_id,task_id]`. The phase is not part of the key. Each membership also
has its own ticket, called an incarnation. The tuple is stored in the field
`gossil_key` when that ticket is created. At most one incarnation for a tuple may be live.

Gest adds a membership with `INSERT OR IGNORE`, so a repeated add keeps the existing
phase. The converter does not build the key, choose the incarnation or apply
that rule.

`IterationTaskValueError` displays the field and a static reason, and does not repeat the input.
Its `parse_cause()` accessor returns the `chrono` error from the parse of the input
or from the parse of the canonical UTC output. Other failures have no parser cause.

Callers own raw decoding, required-field presence, unknown-field classification,
duplicate keys in the original input and byte budgets. Callers also own the key,
the incarnation, the addition and removal of a membership, and the repeated-add
rule. In addition, they own the check that the iteration and the task exist.
The converter does not implement lifecycle, replay, migration, storage or the
complete Gest API.

## Evidence and scope

The eight converter controls in `tests/gest_iteration_task_values.rs` check
complete output, independent ownership, types, identifiers, the phase range, the
key fields and timestamp boundaries with typed causes. One of them checks that
the source map names each field, and that each control that it names exists in
the file.

These tests cover the converter itself. They do not reproduce the tests of the
upstream iteration repository, which remain later obligations. The source map
records the hash of the test accounting addendum, a review document. The
addendum lists the upstream tests of enums and constructors that later API work
must keep. It lists no
such test for this row, so the source map lists no later obligation of that kind.

## Recommendations

Use the converter after you check the complete raw input. Next, build the key
tuple and keep the existing phase on a repeated add before you store a converted
membership.
