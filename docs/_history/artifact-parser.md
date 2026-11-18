# Artifact parser

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

The owned-byte limit counts retained
input, token allocations and list entries. Allocations use fallible reservation.

A resource failure is separate from malformed input, checksum mismatch and
a date-provider failure.

Implement `DateProvider` on the operation's selected SQLite/libSQL owner.
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
The source qualification plan is an analysis of that source, not an executed
native experiment.

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
Windows behavior has not been executed in this checkpoint.

## Evidence and limits

`tests/artifact_parser.rs` replays all 93 preserved fixture acceptance decisions.
The initial eight-type assertion uses `manifest-valid` and `forum-root`.
The fixtures named `manifest` and `forum` are rejected controls. Fixture origins,
hashes and original capture channels remain in
`evidence/parser/fixture-provenance.json`.

The raw fixture bytes came from the preserved characterization archive.
Some are checksum-generated controls, and some are extracted native artifacts.
An accepted native diagnostic result does not change a fixture's origin.
Text-only diagnostic channels remain text-only evidence. No new native process
or malformed-input probe was run for this parser implementation.

The grammar replay uses a constant date adapter. Separate tests call the actual
selected libSQL engine from accepted provider checkpoint
`fea8dcdd3979a30bde80460c39bc299ce286afa3`. They pass all 519 preserved
inputs through each of the actual D and E card paths. Each card admits 470
inputs and rejects 49 under lexical and positive-Julian-day rules.
The expectations preserve exact binary64 values without input substitution.

`evidence/parser-provider-integration/report.json` records these runs, all 24
parser integration tests and three unit tests, and 100% coverage of 703 changed
executable lines. The parser runs are selected-provider Rust observations, not new native
Fossil observations or a final ordered gate pass. Synthetic controls cover
resource limits, decoded fields, raw NUL consumers, W arithmetic and repeated
cards. The module's matrix test enumerates all 208 allowed-card edges, 29
required-card edges and 25 optional-card edges.

These checks do not establish full Fossil conformance. Parsing does not fetch
referenced artifacts, crosslink records, initialize repositories, check closure,
verify signatures, run sync or expose arbitrary database access.

This page follows the nossaifd writing guide, including principles credited to
Rosalind Reid, Shreya Shankar and Oskar Austegard.

## Recommendations

Use this parser with explicit operation limits and the same libSQL owner as
the surrounding transaction. Next, complete the ordered independent gates
and final check for this parser composition through the
[review workflow](development-plan.md#checkpoints-and-ownership). The
[native storage boundary](native-storage.md) already uses this parser for the
empty root. Next, extend that boundary to the first private file child with
real parent and file links under the accepted storage contract.
