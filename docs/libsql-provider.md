# Local native date provider

Gossil uses `libsql = 0.9.30` with `default-features = false` and `core` only.
A relative Cargo patch selects the complete published `libsql-ffi` source in
`vendor/libsql-ffi`. One native engine serves the existing local Rust API.
Cargo uses the upstream build script without changes or source downloads.

## Source identity and bounds

The adopted `sqlite3.c` has SHA256
`1c99ebdc5a6fccdecb30c7702f7f6b07fa8258eea7d4216d878863cb3b7deefd`.
SQLite version and source-ID queries still report the base SQLite 3.45.1.
Use the source hash and `evidence/libsql-provider/provenance.json` to identify
this provider.

The patch adapts SQLite 3.55.0 date arithmetic from `extsrc/sqlite3.c` in
Fossil revision `7e8ecf0ce4a3c05aa39de7ffe2828d23e334efe446ef2387e09f205ef00b2227`.
It corrects fractional seconds, hour 24 rollover and negative Gregorian centuries.
The private tests call SQL `julianday(TEXT)`, which uses the upstream decimal
conversion helpers inside the same engine. The existing internal `sqlite3AtoF` routine, its length and encoding conversion
API, and its unrelated callers remain unchanged.

The complete registry archive supplies licenses, notices and optional sources.
Only `bundled/src/sqlite3.c` differs. The Cargo extraction marker `.cargo-ok`
is absent from the archive and is omitted. MultipleCiphers sources remain
unchanged; enabling optional engine features requires separate qualification.

## Reproduce and verify

Run `just libsql-vendor-check` to check every vendor path, length and SHA256,
plus frozen provenance, fixtures and maintenance artifacts. Missing or extra
files, symlinks and unreadable or inconsistent manifests fail the check.
`just libsql-vendor-test` runs twelve controls for tampering, staged bytes,
regeneration, literal whitespace and upstream reader preservation.
Both checks require Python 3 and the repository files. The test controls also
require Git and create disposable indexes. The hook adds `--index` to the
verifier and requires the reviewed artifacts to match staged bytes.

Both checks are required by `typecheck` and `ci`; CI also invokes them directly.

Regeneration is a maintenance operation. Supply the original registry
`bundled/src/sqlite3.c` and the pinned Fossil `extsrc/sqlite3.c` as local files:

```sh
python3 scripts/regenerate-libsql.py ORIGINAL NATIVE NEW_OUTPUT
```

The recipe checks both input hashes, the patch hash, all eight hunk counts and
the final output hash. It creates a new output file and never runs during Cargo
builds. Source URLs, revisions and input hashes are in the provenance record.

## Hook ownership

The project owns the `local` and
`https://github.com/pre-commit/pre-commit-hooks` units in `.pre-commit-config.yaml`.
The original commit gate checks recorded evidence. Its message hook adds the
commit trailer, and the pre-push hook runs `just ci`. Those entries remain unchanged.
Future upstream hook updates need deliberate reconciliation in these units.

The 1000 KB size hook excludes exactly these four upstream paths under
`vendor/libsql-ffi/`:

- `bundled/src/sqlite3.c`
- `bundled/SQLite3MultipleCiphers/src/sqlite3.c`
- `bundled/SQLite3MultipleCiphers/src/shell.c`
- `bundled/SQLite3MultipleCiphers/src/sqlite3patched.c`

Three of the four currently exceed the limit.
An always-run replacement verifies the complete inventory at pre-commit and
pre-merge, including the index bytes and agreement with working files.
All other size checks, syntax checks and commit gates remain in place.
A changed validator or manifest still needs ordinary review.

## Literal byte preservation

`.gitattributes` preserves inherited whitespace in the complete hash-verified
vendor tree. It also preserves `adapted.patch`, `fossil-configure.stdout`
and `product-red.stdout` under `evidence/libsql-provider`.
These three files retain literal upstream or observed
bytes. The complete source and evidence inventory still checks their hashes.

The staged whitespace audit enumerated every affected file before choosing
these paths. Authored Rust, docs, scripts and configuration retain ordinary
whitespace checks. A disposable indexed control confirms that literal paths
pass while ordinary files and neighboring path names fail.

## Preserved upstream reader pages

The project checker map `.agents/nossaifd-checkers.json` names exactly three
upstream pages. They are the MultipleCiphers `readme.md` and `src/filelist.md`,
and the bundled PCRE2 `README.md`. The adapter
`scripts/check-preserved-libsql-prose.py` verifies their original archive hashes
and the complete source and provenance inventory. It rejects a changed map,
an unlisted page, missing files, altered bytes and symlinks.

The always-run source verifier also checks the exact map.

These pages are readable third-party prose. The original MultipleCiphers README
still has four hard style findings under the normal checker. Its verbatim
checker output is retained in `evidence/libsql-provider/upstream-style-original.txt`.
This policy checks source preservation instead of rewriting upstream prose.
Authored project pages continue to use the installed normal style checker.

The map and adapter are project-owned files outside the installer's managed
files. A disposable official `just install --defaults` refresh checks retention.

## Executed evidence and limits

The private storage regression covers 489 fixed tokens and 30 numeric threshold
inputs. It compares SQL storage classes, NULL and exact binary64 values, plus
unchanged numeric conversion callers. The original provider fails the known
fractional-second case; the adopted provider passes the retained corpus.

Fossil D cards carry artifact dates, and E cards carry event dates.
These SQL tests do not execute an inherited or Gossil parser for those cards.
A shared-clock test must compare native Fossil and Gossil against the same
injected current time, or explicit time bounds, before claiming runtime parity.
Runtime clock comparisons, date modifiers, formatting, alternate build flags
and the platform matrix remain unqualified. A test-only build of the modern
Fossil shell against the older libSQL core failed on unavailable APIs.
That compile failure establishes no date behavior.

## Recommendations

Keep the pinned core-only provider and verify its full source inventory.
Next, test the actual artifact parser and shared-clock behavior before claiming
D/E compatibility or expanding the supported engine features.
