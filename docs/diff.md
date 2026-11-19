# Compare normalized byte lines

The `diff` module compares two byte slices and returns an edit script in line
counts. Use `line_diff(from, to, options, limits)` when a caller needs Fossil's
line matching and edit ordering without a rendered patch. Bytes need not be valid
UTF-8. The module opens no files or databases.

Normalize the input before calling this API. Fossil's full `text_diff` wrapper
uses `blob_to_utf8_no_bom` to remove a byte order mark or convert UTF-16. This
module does neither. It also supplies no token diff, regular expression filter
or text renderer. Its results describe the supplied bytes, after the caller's
chosen conversion.

## Read the edit script

`line_diff` returns `Result<Vec<Edit>, DiffError>`. Each `Edit` has three counts:

| Field | Lines consumed |
|---|---|
| `copy` | Common lines from both inputs. |
| `delete` | Lines from FROM only. |
| `insert` | Lines from TO only. |

Read each triple in that order. Advance FROM by `copy + delete` and TO by
`copy + insert`. The vector
ends with the native zero triple, whose three counts are zero. Inserted content
comes from TO. The script holds counts and omits line bytes.

LF separates lines and is excluded from comparison. An unterminated final line
still counts as a line. Adding or removing its final LF alone produces no line
change. An empty input has zero lines.

## Select comparison options

`LineDiffOptions::default()` selects exact comparison, native readability
optimization and no other option. Choose these fields for each call:

| Field | Behavior |
|---|---|
| `whitespace: Whitespace::Exact` | Compare all bytes within a line. |
| `whitespace: Whitespace::IgnoreEol` | Ignore trailing Fossil whitespace bytes. |
| `whitespace: Whitespace::IgnoreAll` | Ignore Fossil whitespace throughout a line. |
| `strip_cr` | Remove a trailing CR from each line before comparison. |
| `invert` | Exchange FROM and TO before computing edits. |
| `optimize` | Apply native readability shifts to insertions and deletions. |
| `not_too_big` | Reject more than 10,000 inserted plus deleted lines. |

Fossil whitespace here means bytes 8, 9, 10, 12, 13 and 32. It is not Unicode
whitespace. `IgnoreEol` and `IgnoreAll` return the error `DiffError::WhitespaceOnly` when the
script has one copy-only triple followed by the zero triple. They can also return
scripts that contain edits.

## Bound input and work

`DiffLimits::default()` supplies finite bounds. Applications can choose smaller
bounds for their inputs:

| Field | Default and meaning |
|---|---|
| `max_input_bytes` | 16 MiB combined input bytes. |
| `max_lines` | 200,000 combined input lines. |
| `max_edits` | 200,001 triples, including the final zero triple. |
| `max_work` | 100,000,000 charged inspections, comparisons and search iterations. |

The implementation also caps combined bytes at `i32::MAX` and combined lines at
1,000,000, even when caller limits are larger. Work is a charged algorithm budget.
It does not measure each processor instruction or total heap bytes. Input bounds
are checked before line-table allocation, and reservations can fail.

`DiffError::Binary` reports an embedded NUL or a line longer than 32,767 bytes.
`WhitespaceOnly` reports the rejection described above for the whitespace modes. `TooManyChanges`
reports the enabled change-count rejection. `Limit` reports a caller bound or fixed cap,
checked arithmetic failure or allocation failure. A failure returns no partial
script.

## Evidence and remaining work

The algorithm derives from Fossil `src/diff.c` at revision
`b3a1c8efb2731dffbd5cc38c58ac349746de6da6`. The source correspondence record is
`evidence/diff/source-map.json`. The raw fixture at
`tests/fixtures/diff/native-raw.json` records reference identity and cases.
The module retains its upstream copyright and 2-Clause BSD attribution.

Bounded ordinary qualification passed all eight focused test groups and 143
native comparisons. The qualified `src/diff.rs` SHA256 is
`0d7ce1bacd07ff2b922821ccfdb35e837f741a026dcce1e956917789c8bfa843`.
The exact freeze and receipt are retained in
`/tmp/gossil-fossil-diff-qualification-20261119/corrected-freeze.json` and
`/tmp/gossil-fossil-diff-qualification-20261119/focused-receipt.json`.
An independent source review covered that frozen implementation. Execution-pending
notes in the earlier source map are historical. The qualification receipt records
the later candidate run.

The raw fixtures do not establish full `diff.test` inheritance or show that the
normalization, token, rendering and filtering wrappers are complete. Normal coverage
and full project gates remain pending. Internal hashes for line matching follow
the inspected native signed-char and byte-order behavior. High-byte inputs on
other architectures, including Linux arm64, still need platform qualification. No cross-platform
native parity is claimed by the macOS run.

## Recommendations

Normalize bytes in the caller, choose finite limits and read the terminating
zero triple explicitly. Next, complete normal coverage and full project gates
for this module, then qualify platform-sensitive hashing before broader use.
