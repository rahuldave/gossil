# Artifact parser

[API: `parse_artifact`](api/artifact.qmd#parse_artifact) and
[`parse_artifact_with_usage`](api/artifact.qmd#parse_artifact_with_usage) give the exact contracts.

The `artifact` module parses all eight Fossil structural artifact types with
explicit resource limits. The types are manifest, cluster, control, wiki, ticket,
attachment, technote and forum. A card is a record in the artifact byte format.
The parser takes bytes and returns an owned result with
read-only access to parsed fields. It keeps the original input, including any
signature envelope. It does not require valid UTF-8.

## Call the parser

Call `parse_artifact(input, limits, dates).await` with a `ParseLimits` value
chosen for the calling operation. Set each field explicitly, including
`input_bytes`, `cards`, `owned_bytes` and `entries`. All four fields have type
`usize`. The byte fields count bytes, and the other fields count cards and list
entries. There is no default policy.

The owned-byte limit counts logical charges for retained input, encoded tokens,
copies and list entries. Allocations use fallible reservation.

A resource failure is separate from malformed input, checksum mismatch and
a date-provider failure.

## Track usage across calls

For several artifacts in one operation, call
`parse_artifact_with_usage(input, limits, dates).await`. It returns the same
parsed result with a `ParseUsage` value on success. The original
`parse_artifact` function delegates to this implementation and returns only the
parsed result.

`ParseUsage` reports charges against each limit. `input_bytes` counts the full
raw input, including a recognized signature envelope. `cards` counts nonzero
card markers consumed by the structural loop, including Z and repeated empty
P cards. A trailing zero marker consumes its delimiter without adding a card.
Body newlines and envelope lines do not add cards.

`owned_bytes` reports all charged bytes, including retained raw input, copies,
encoded token lengths charged before decoding, and the size of inserted list
entries. The counters report logical charges against `ParseLimits`; they do not measure every
physical Rust allocator byte. This charge can exceed the decoded output length. It also includes
allocations for tokens later discarded, such as reserved file paths.
`entries` counts list insertions; empty P cards add none.

Subtract successful usage from the operation's remaining limits before the
next call. Each call checks those remaining limits before its charged
allocations. Usage is returned only on success. A caller that needs an atomic
operation must stop on any error and discard its operation state.

The parser does not maintain the aggregate budget or remember earlier failures. The
[API example](api/artifact.qmd#parse_artifact_with_usage) parses a cluster and
subtracts all four successful charges from a caller-owned budget. The private recording library borrows one parser work ledger across its
[complete repository operation](native-storage.md#private-recording-library).
Other callers remain responsible for their own aggregate accounting.

## Supply dates and read results

Implement `DateProvider` on the operation's selected SQLite/libSQL owner.
The [local storage guide](local-storage.md) explains transaction ownership.

Its asynchronous `julianday` method can borrow a transaction. It receives an
optional raw token already truncated by the parser at its native C-string
boundary, and returns an optional Julian day. A NULL result acts as zero and fails date validation.
Provider execution errors retain their original cause. A constant date adapter
can isolate grammar tests, but it cannot qualify date compatibility.

The result exposes decoded comments and users, manifest files and parents,
tags, ticket fields and cluster members. It also exposes attachment fields,
wiki and forum fields, body bytes and converted dates.
D cards carry artifact dates, and E cards carry event dates. Their raw date tokens remain available.
Callers cannot construct or mutate a validated result through this API.

## Preserved source behavior

The port follows Fossil `manifest.c:467-1153` and its eight allowed/required
card masks at lines 129-138. The pinned Fossil UUID is
`7e8ecf0ce4a3c05aa39de7ffe2828d23e334efe446ef2387e09f205ef00b2227`.
Fossil's source is copyright D. Richard Hipp under the Simplified BSD License.

The parser checks the exact MD5 before decoding a recognized final Z record.
It strips recognized PGP and SSH envelopes for parsing and checksum purposes.
Envelope stripping does not verify a signature's authenticity. The pinned card matrix requires
Z even for manifests. R validates hexadecimal syntax. Repository file checksum
verification belongs to storage.

Tokens stop at space, LF or raw NUL. Each card consumes tokens as its native
branch does. A raw NUL can be an intermediate delimiter in some cards. It is not subject to a blanket acceptance rule. Decoded tokens retain
bytes after an escaped NUL, together with the length visible to C consumers.

A zero card marker ends the native card loop after consuming its space or LF
delimiter. Parsing succeeds only if that cursor reaches the end and the earlier
cards satisfy the type rules. Unread bytes after the marker still fail.

The W scanner uses unsigned 32-bit modular arithmetic and the native
`new < old` overflow branch. It checks the remaining input before copying body
bytes. Decimal suffixes and empty digit sequences follow the source branch.

Manifest file paths use the legacy nonstrict rules. Paths must be nonempty and
have no empty, dot or dot-dot slash-separated components. Unicode and backslash
restrictions are not imposed. Reserved filenames are
suppressed after comparison with the last retained filename.

The parser retains repeated empty P cards, native type inference, tag ordering,
forum postconditions and the historical technote P-before-N exception.
Attachment basenames follow the compiled platform's directory separator rule.
Windows behavior remains unqualified.

## Recommendations

Use the contracts above with explicit resource limits. Next, consult the
[generated Rust API](api/index.qmd) for the full signatures and errors.
