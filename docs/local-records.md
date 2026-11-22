# Strict local records and metered queries

Gossil will keep some of its own records in the local database as JSON, for
example the saved ceilings of a file operation. A later reader of such a record must
refuse malformed input. It must also stop at fixed limits of work, because a
damaged or hostile database can hold a very large or deeply nested record. Three
internal modules of `src/storage/` give the base for such a reader:

- `command_codec.rs` decodes and encodes strict local JSON against one work ledger.
- `operation_limits.rs` joins the ceilings of one file operation and their saved form.
- `metered_query.rs` charges the size of a query result before it reads the rows.

The modules have no public API and no caller yet. The record readers of catalog 5
are the first planned callers. Until they exist, an `expect(dead_code)` attribute
on each module states that no production code calls it. When production code
calls every item of a module, the lint fails on the attribute, so that someone removes it.

The modules follow these rules from CHANGE-06 of the planned changes of the
product specification:

- Reject wrong types, duplicate JSON keys and unknown fields before any local
  effect.
- Charge every limit cumulatively, and do not reset an allowance inside a helper.

## Decode a record with one ledger

A work ledger is a `CodecWork` value. It starts from a `CodecLimits` value with
three ceilings:

| Ceiling | What it counts |
|---|---|
| `bytes` | Serialized JSON bytes, for every document that the ledger reads or writes |
| `nodes` | JSON values and object keys; `{"a":[1]}` counts four |
| `depth` | Nesting of values, with the root at depth one; at most 127 |

`CodecWork::new` refuses a zero ceiling and a depth above 127. The ledger keeps
one remaining count of each kind for the whole operation. Each call subtracts
from it, and no call adds to it. So the second record of an operation can fail
on a limit that it alone does not reach.

`CodecWork::document` charges the bytes of a document first. It then decodes the
JSON and charges one node for each value and each key. It refuses these inputs:

- a byte, node or depth limit that the document exceeds
- a duplicate key in any object, at any depth
- malformed JSON and input after the first document

When the method refuses a document, the ledger keeps the work that the document
spent before the refusal. A refusal from the decoder keeps the error of
`serde_json` as its source.

## Refuse unknown and missing fields

`CodecWork::closed_record` decodes a closed record. A closed record is a JSON
object with exactly the fields that the caller lists. The method first calls
`document`, so it refuses duplicate keys and exhausted limits. Then it refuses
these records:

- a value that is not an object
- a field that the list does not hold
- a listed field that the object does not hold, even an optional one

The last rule follows CHANGE-06, which requires an explicit null for an
optional field. Then the method decodes the object into its Rust type and
refuses a field of the wrong type. The method checks only the top-level fields.
To close a nested object, give its Rust type the attribute
`#[serde(deny_unknown_fields)]`.

## Encode within the ledger

`CodecWork::encode_record` writes the JSON of a record into a buffer that stops
at the remaining bytes. It then decodes the output with `document`, so the output
also pays its node and depth work. Output that does not fit is refused, and
the refusal spends no work.

`CodecWork::admit_serialized` charges the nodes and the depth of a value without
writing any output. A test checks that it charges the same nodes and depth as a
strict decode of the output. It refuses the raw JSON encoding of `serde_json`
and a map key that is not a string, a number or a boolean.

`CodecWork::charge` charges bytes for a buffer that the caller allocates itself.
`remaining_bytes` and `remaining_nodes` report the unused work, so that an owner
can charge an outer ledger with the same amounts.

## Narrow the ceilings to saved ones

A saved record can carry its own ceilings. `CodecWork::constrain` lowers each
ceiling of the ledger to the smaller of the current and the saved value. It
keeps every charge made before. A larger saved ceiling does not replenish
work. If the saved ceilings are invalid, or lower than the work already
spent, the method refuses them and leaves the ledger unchanged. The owner must
then abort its operation.

## Join the ceilings of a file operation

`FileOperationLimits::new` joins the ceilings of one complete file operation:

- the native content `Limits`
- the artifact parser `ParseLimits`
- the `FileSystemLimits` of paths, plan rows and directory entries
- the bytes of a saved recipe, the objects of the file closure and the derived rows

It refuses a zero ceiling and an output ceiling above `i32::MAX`.

`declared` gives all eighteen ceilings as a `DeclaredBudgets` value. That value is
the closed saved form, and `DeclaredBudgets::FIELDS` lists its field names for
`closed_record`. `check_saved` accepts a saved declaration only when each saved
ceiling is valid and at most the ceiling of the current caller. So a saved
declaration can narrow a later operation and never widen it. The check grants
no authority over a journal or a recipe.

`codec` derives the `CodecLimits` of the operation from the recipe bytes and the
parser ceilings. It caps the depth at 127. `root` gives the unchanged
`AdmissionLimits` of the content and the parser.

## Charge a query before its rows

A metered query runs on a `WriteTransaction` and spends a `SqlWork` value. That
value holds the remaining result rows and payload bytes of the operation.
`query_bound_sql_work` takes fixed internal SQL, its bound parameters and the
exact number of result columns, from 1 to 128. It does these steps:

1. Charge one row and 32 bytes for a preflight query.
2. Run the preflight query. It counts the rows of the result and sums the bytes
   of its text and blob cells. Text counts in UTF-8 bytes, not in
   characters, so `é` counts as two bytes.
3. Charge those rows, and 16 bytes for each cell plus the summed bytes.
4. Run the query, and refuse a result whose rows, width or text and blob bytes
   differ from the charge. The check runs after the rows are read.

Each charge subtracts both the rows and the bytes, or neither.
`query_bound_work` also charges the decoded content `Budget` of the operation,
for the bound parameters before the preflight and for the retained cells before
the query.

Each failure latches in the transaction, as every other storage failure does.
Later SQL on that transaction returns the first failure, and rollback returns
it too. In a test, the method refuses a query with one row too few of work, and
one with one byte too few. The test then reopens the database and finds the
rollback complete, and a retry with enough work succeeds.

Do not pass caller-supplied SQL to these methods. The preflight wraps the SQL in
a common table expression, so only fixed internal SQL is safe.

## Tests

The tests live in `tests/support/storage_command_codec.rs`,
`tests/support/storage_operation_limits.rs` and
`tests/support/storage_metered_query.rs`. They run as unit tests of the storage
module, and the metered query tests use a real libSQL file. They check each
limit at the last value that it accepts and at the next value, which it refuses.

## Provenance

The code comes from uncommitted work in the worktree `gest-native-core-integration`.
These parts of that work were left out:

- strict decoding with a one-time ledger, and the charges for scope arrays and scope names
- the unmetered query routes and the query that reads its column count
- the restore of saved limits that only the tests used

The port also follows these rules from the independent review of that work:

- test only behavior
- keep `metered_query` internal
- run `rustfmt` on every line
- do not call `unwrap` on stored data

## Recommendations

Give each operation one `CodecWork`, one `SqlWork` and one content `Budget`, and
pass them to every record and query of the operation. Decode each saved record
with `closed_record`.
