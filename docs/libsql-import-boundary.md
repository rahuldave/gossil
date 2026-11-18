# libSQL source import boundary

This prerequisite classifies four unchanged Rust files from libsql-ffi 0.9.30
before its source import. It contains no vendor source or executable guard.
The classifications require the reviewed import controls below.
They do not establish product runtime acceptance.

## Exact source and classifications

The published archive has SHA256
`0be1da6f123ceb2cd23f469883415cab9ee963286a85d61e22afb8b12e15e681`
and crate revision `0653c5788d77ef16a97c56ff3e9fdc11717a72d9`.
The complete comparison accounts for all 272 archive paths and all 272 candidate
paths. There are no missing or extra paths. The Cargo extraction marker
`.cargo-ok` is in neither inventory.

| Exact path under `vendor/libsql-ffi/` | Classification | Reason |
|---|---|---|
| `build.rs` | `coverage-exclude.txt` | Handwritten, reachable upstream build logic, preserved unchanged. |
| `src/lib.rs` | `coverage-exclude.txt` | Handwritten, reachable upstream FFI helpers, preserved unchanged. |
| `bundled/bindings/bindgen.rs` | `generated.txt` | rust-bindgen 0.66.1 generated core bindings, preserved unchanged. |
| `bundled/bindings/session_bindgen.rs` | `generated.txt` | rust-bindgen 0.66.1 generated session bindings, preserved unchanged. |

The two handwritten files are neither generated nor untestable. The exception
substitutes complete pinned-source verification for new first-party line coverage.
It retains caller-facing behavior tests and applies only to the exact bytes
reviewed here. Changed Rust bytes and neighboring files need ordinary review
and coverage decisions. These lists contain no folder or wildcard entries.

Only `bundled/src/sqlite3.c` differs from the archive. Its reviewed adaptation
has SHA256 `1c99ebdc5a6fccdecb30c7702f7f6b07fa8258eea7d4216d878863cb3b7deefd`.
That C adaptation receives no exemption from these four entries.
The bindings are copied into `OUT_DIR` during upstream builds; session bindings
require the optional session feature. Core-only tests do not qualify that feature.

## Observed evidence

Open `evidence/libsql-import-boundary/` from the repository root.
`archive-proof.json` records every archive and candidate length and SHA256,
including the four identical Rust files and the one C adaptation.
`candidate-files.json` ties all 445 changed paths to staged blob identities
and current bytes of candidate tree `2f96dcc546aae747e3a7c1dec424a2cebf7d3c87`,
based on `de790aeab99012034f4b0bc9f9074181c935cd49`.
The saved controls and manifest have the same byte identities as that candidate.

`dependency-impact.json` records the AST inspection of policy consumers.
`matcher-controls.json` records the existing matcher accepting exactly the four
paths and rejecting neighboring paths. Coverage exclusions affect coverage only;
generated entries also affect generated-source documentation and API checks.
The matcher reads lists from `HEAD`, so this prerequisite must be committed
before the import. A same-commit list change cannot exempt its source.

The retained independent product and test reviews accepted the bounded candidate.
Their receipts include twelve maintenance controls and seventeen storage, date
and composition tests. Date evidence covers 489 main tokens and 30 threshold
tokens; it does not establish unrestricted SQL or artifact-parser parity.
`prior-final-check.txt` retains the real coverage failure for these four files.
It remains a failed product gate. Earlier broad test selection and missing Tcl
setup are recorded separately from the actual fractional-date behavioral red.

The owner policy is committed at
`b0c4a946ca17a7e401e3f3851f70ca3a179b3c22`.
`owner-policy-checkpoint.json` records 186 owner tests, including one CI-only
skip, and the accepted independent review. The official installation used
that clean committed source and passed `just install --check`.
`official-install-receipt.json` and `installed-correspondence.json` retain
the installation record and all fifteen matching Rust source files.
The existing owner package-manifest limitation remains recorded; no
package-maker success is claimed.

## Required import controls

The import must bring its complete manifest, source verifier, regeneration tool,
reader-policy checker and maintenance tests with the frozen reviewed source.
The verifier must reject missing, extra, changed, linked or nonregular files,
invalid manifests, wrong provenance and staged/worktree disagreement.
The saved candidate runs the full verifier before type checking and in CI.
Its always-run pre-commit and pre-merge guard uses `--index` with
`pass_filenames=false`. Both verifier modes passed fresh source checks here.

This prerequisite does not install that guard because the vendor tree is absent.
If the reviewed guard does not arrive with the import, the import fails policy.
Reverify the complete controls at import gates after the official committed-source
profile installation. The recorded installation matches the committed owner
source, with no algorithm change to the existing matcher. Next, get independent
review of the complete product tree.
No final product gate or new native Fossil probe ran for this prerequisite.

Writing follows nossaifd guidance, including Rosalind Reid's communication
principles, Shreya Shankar's plain-writing guidance and Oskar Austegard's declaude
revision method. Independent prose revision remains part of the final gate.

## Recommendations

Prefer these exact, source-backed entries with the complete reviewed import
controls. Next, review and commit this prerequisite with the installed owner profile,
then import the frozen source and rerun the product gates.
Keep optional features, clocks, artifact parsing and platform qualification
outside the finite acceptance recorded here.
