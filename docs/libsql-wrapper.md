# Exact libsql TEXT retrieval

Gossil pins a correction of the libsql 0.10.0 Rust wrapper at
[`c02db34ae8a7e91ca7ab8faee18fe41515626940`](https://github.com/rahuldave/gossil-libsql/commit/c02db34ae8a7e91ca7ab8faee18fe41515626940).
The published wrapper truncates TEXT at an embedded NUL and can panic on
malformed UTF-8. The correction preserves complete valid text and returns
existing errors for invalid text and column indices.
[Issue 36](https://github.com/rahuldave/gossil/issues/36) tracks integration
and the bugfix release.

## Behavior and source boundary

Owned values are copied immediately from the existing length-aware view.
TEXT uses strict UTF-8 decoding. Public `get_str` accepts only TEXT and returns
`InvalidColumnType` for nontext or malformed text. Invalid indices return
`InvalidColumnIndex`; column names return `None` for those indices.
Generic wrong-type `get<T>` conversions retain their upstream behavior.

The companion repository preserves the complete published crate and its MIT
license at authenticated baseline
`243281163e4a39303322385d5f9537736277026e`. Its correction changes the local
retrieval paths, makes four existing lifetimes explicit and applies normal
formatting. Public Row signatures and error variants stay unchanged.
The existing statement/current-row lifetime boundary stays in place.

Gossil retains the registry `libsql-sys` 0.10.0 identity and the complete
existing `libsql-ffi` patch. The wrapper correction changes no engine or C ABI.
`evidence/libsql-wrapper/provenance.json` records the exact source identities,
archive and patch digests. The original temporary path-patch packet is retained
as evidence; it is not a build input.

## Checks and composition

Run these checks from the Gossil repository root:

```sh
just libsql-wrapper-check
just libsql-wrapper-test
just check
```

The wrapper checks reject revision, feature, registry-sys and provider drift.
They run before the normal type check and in CI. Gossil's installed strict
lint and coverage gates remain unchanged.

The companion has an explicit upstream-baseline lint contract. It retains raw
strict failures and admits only exact diagnostic/code matches at unchanged
source origins. New diagnostics and compiler failures fail that contract.
It uses fresh profiles for each executed test target and has no coverage
exclusions. See [the pinned qualification receipt](https://github.com/rahuldave/gossil-libsql/blob/c02db34ae8a7e91ca7ab8faee18fe41515626940/evidence/qualification/summary.json)
for its finite core-only evidence.

`just libsql-wrapper-metadata` writes the resolved dependency graph to
`target/libsql-wrapper-metadata.json`. It runs Cargo metadata with the locked
dependencies and all features. The graph records source resolution, not runtime behavior.

A consuming workspace must select the same full Git revision in its own root
patch table, as shown in the [application examples](examples.md). Cargo ignores
patch tables in dependencies. Verify the actual
resolved source and provider path when composing Gossil with another package.

`tests/libsql_row_text.rs` exercises the public erased `libsql::Row` API.
It covers NUL positions, Unicode, malformed UTF-8, storage types, invalid
indices, recovery after errors and owned values after rows are dropped.
SQLite `typeof` and `hex` queries check stored bytes before retrieval checks.
These controls do not qualify other libsql features or platforms.

The retained consuming replays use the same full Git revision. The VCS
registration matrix passed all 21 corruption cases with no accepted corruption,
provider panic or application or working-file mutation. Four unchanged Gest
fidelity tests also passed. Their receipts are under
`evidence/libsql-wrapper/consuming-replays/`. These focused runs do not qualify
the complete VCS or Gest feature sets.

The wrapper correction was integrated for Gossil 0.4.1 and retained in the
released 0.5.0 tracking baseline. This checkout prepares a separate private
recording library checkpoint. The [development guide](development-plan.md)
distinguishes that preparation from delivered tracking. The focused controls
here do not establish a complete private-recording qualification or installation.

Writing follows nossaifd guidance, including Rosalind Reid's communication
principles, Shreya Shankar's plain-writing guidance and Oskar Austegard's
revision method.

## Recommendations

Prefer the full reviewed Git revision over a moving branch.
Run the composition checks and public Row controls before changing the pin.
