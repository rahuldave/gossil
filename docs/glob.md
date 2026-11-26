# Bounded Fossil glob port

The glob module ports list parsing, matching and SQL expression construction
from Fossil `src/glob.c`. It calls `sqlite3_strglob` from the selected libSQL
0.10.0 engine. The direct `libsql-ffi` dependency requests that exact version
with default features disabled. The existing Cargo patch selects
the accepted [vendored provider](libsql-wrapper.md) for this dependency too.

`glob_create` stores reusable patterns. Its `glob_match` method returns the
first matching pattern's one-based index, or zero. `glob_multi_match` scans
once, using Fossil's 100-byte temporary pattern buffer and allocating for
longer patterns. Rust owns the buffers instead of calling `glob_free`.

The list grammar uses commas and ASCII whitespace (bytes 9 through 13 and
space). Single and double quotes retain commas and whitespace in a pattern.
An unmatched opening quote consumes the rest of the list. Only a quote at
the start of a term opens quoted parsing. There is no escape syntax.

Empty quoted terms remain real empty patterns. Absent, empty and delimiter-only lists
have no patterns.

Lists, candidates and SQL value expressions end at the first
NUL byte, as in C.

`glob_expr` produces SQL bytes and doubles apostrophes in patterns. The value
argument must be a trusted SQL expression. The function does not quote or
sanitize that argument. Use a trusted column expression or parameter expression
such as `?1`. Bind an untrusted candidate separately through a caller-owned SQL
interface. The function returns bytes and executes no SQL.

After a closing quote, SQL construction skips one more byte, as the source
does. For example, `'a'bc` gives matching patterns `a` and `bc`, but SQL terms
`a` and `c`. Do not normalize this distinction. SQL output joins terms with
` OR ` inside parentheses. A list with no patterns returns `0`. A quoted
empty pattern produces a real `GLOB ''` term.

Callers supply finite `Limits` for list bytes, candidate bytes, pattern count
and expression bytes. Reusable matching retains its candidate limit. Byte
limits apply to the prefix before the first NUL. Finding that prefix scans
until the first NUL or the end of the original slice. Reservations are fallible and
capacity additions are checked. Typed errors retain allocation failures.

The one-shot matcher returns as soon as a pattern matches. It need not inspect
later terms against the pattern-count budget. SQLite owns wildcard syntax,
Unicode character handling and matching recursion. These byte budgets bound
accepted prefixes and allocations, but they do not bound SQLite matching CPU time
or stack use. SQL execution can
impose further engine limits on patterns and expressions.

Matching follows SQLite GLOB rules. It is case sensitive, `*` can cross `/`
and match dot-prefixed names, and backslashes are literal bytes rather than
escapes. Character classes include ranges such as `[a-c]` and negation such as
`[^a]`. The selected-engine controls include `?` matching `é`, invalid UTF-8
bytes and an unmatched `[` that matches nothing.

Both `gossil` and `gos` accept `vcs` or its alias `v`.
`gossil vcs test-glob PATTERN STRING ...` prints the SQL expression, parsed
patterns and the indices returned by `glob_match` and `glob_multi_match`. The adapter uses fixed finite budgets of
1 MiB per list or candidate, 65,536 patterns and 16 MiB per expression.
It rejects setting names beginning with `@`. Repository settings remain a
follow-up. JSON rendering also remains a follow-up.

Unix arguments retain their exact bytes. On other platforms, the adapter
accepts Unicode arguments as UTF-8 and rejects arguments that cannot be
represented as Unicode. It does not replace invalid characters. The historical qualification used Unix targets only. Non-Unix conversion has
source review, without cross-compilation or runtime qualification.

## Current source status

This adaptation preserves the shared product entry and both thin executables.
It changes no storage, tracking, traversal, settings or selection behavior.
The 2026-11-21 focused checkpoint built both product binaries with the selected
libSQL 0.10.0 engine. It passed 3 CLI tests, 9 glob integration tests and 5 filtered
library units (4 glob units plus an existing global-state control). All 20
unchanged upstream Tcl assertions passed against the Rust diagnostic. The graph
comparison retained all 126 nonroot packages and their complete edges/features,
with one `libsql-ffi` 0.10.0 provider.

The tested tree was `a5583c3c6a73dda1a75ad1dd4c168efc06655c40`, on actual
parent `4250b873`. It included a prospective checked-commit prerequisite rather
than accepted prerequisite ancestry. [PR 47](https://github.com/rahuldave/gossil/pull/47)
supplies typed commit and cleanup causes. Strict Rust 1.100 lint stopped at that
prerequisite's `commit_checked` large-error lint. The owner's local lint fix is
synchronized in the source successor, but this checkpoint does not qualify it.

At the checkpoint, `Cargo.toml` set the version of the Gossil crate to 0.9.0,
which was later released as `v0.9.0`. The current version of the crate is
0.17.0, and version 0.16.0 was released as `v0.16.0`. The `libsql-ffi`
provider is at 0.10.0, and its version is separate
from the version of the crate. Complete gates on the final frozen tree,
accepted-parent checks, independent review, release and exact merged installation
proof remain separate from the focused results.

## Historical evidence and limits

`evidence/glob/upstream-cases.hex` contains all 20 `glob.test` assertion
families, 100 through 119. A Tcl evaluation extracted arguments and expected
output from the pinned source without executing Fossil. The cases record source
expectations rather than newly observed native outcomes. Rust tests compare the
adapter output with each original expectation, including CR/LF normalization.

Boundary tests cover empty lists and patterns, all ASCII delimiters,
unmatched and adjacent quotes, apostrophe escaping and first-match indices.
They also cover NUL boundaries, 99/100-byte buffers, invalid UTF-8, Unicode,
character classes, case sensitivity, slash and dotfile matching, and budgets. `evidence/glob/red.log.gz` records a compiled behavioral failure before adapter
integration. `evidence/glob/green.log.gz` records the focused passing run.

Historical dependency reports record package and feature
identity for the earlier 0.9.30 composition. They do not establish the resolved
provider or enabled features of this adaptation.
`run-upstream.tcl` also runs the unchanged upstream test file against the Rust
adapter. Its log records all 20 assertions passing.

An instrumented unit-test probe received SIGKILL before tests ran. Its cause
remains unresolved. The separate focused instrumented integration run passed
all six tests. Those early results did not establish a full coverage gate pass.

The later historical composed run in `evidence/glob-provider-integration/` passes the
coverage gate with all 293 changed executable lines covered. All 20 unchanged
upstream Tcl assertions pass against the Rust diagnostic.
Other tests cover ordinary API and conversion behavior, invalid arity,
settings rejection and errors from closed stdout. Engine checks retain the
selected vendored source and unchanged feature set.

The accepted timeline baseline is `4250b873`, released as `v0.8.0`. The focused glob
checkpoint above does not establish final integration acceptance. Confirm the
accepted checked-commit prerequisite and qualify the complete resulting tree
before delivery.
The composed results are observations of the Rust candidate, not a fresh
native Fossil run.
`historical-log-packaging.json` maps the two compressed historical raw logs
to their original paths and records their lossless gzip packaging.

The source is copyright 2011 D. Richard Hipp, and the tests are copyright 2013
D. Richard Hipp, both under the 2-Clause BSD license retained in
`evidence/glob/fossil-license.txt`. This port does not implement file tracking,
directory traversal, path boundaries or persistent file selection.

## Recommendations

Use the bounded library functions for list matching with the selected engine.
Next, confirm the accepted revision of [PR 47](https://github.com/rahuldave/gossil/pull/47)
and qualify glob on that parent. Then run the complete frozen-tree gates and
verify the exact delivered build.
