# Development and contributing

Gossil implements byte primitives, structural artifact parsing and bounded local
storage. The `gossil` executable implements three delta diagnostics.
The generated [Rust API](api/index.qmd) describes the public library surface.

The standalone [Gest task values converter](gest-task-values.md) validates eleven decoded fields.
The [Gest project values converter](gest-project-values.md) validates four canonical project fields.
The [Gest author values converter](gest-author-values.md) validates six canonical author fields.
The [task ticket emitter](task-ticket.md) serializes validated task snapshots with
caller-supplied native identity, actor and date. All three converters operate in memory. Storage integration, raw field admission and the full Gest API remain future work.

## Current scope

| Available | Planned |
|---|---|
| Delta create, apply and two-way file checks | General version-control CLI |
| Compression, hashes, token escaping and artifact checksums | Repository traversal and persistent selection commands |
| Eight structural artifact types with explicit limits | General crosslinks, migration, rebuild and filesystem recovery |
| Complete empty-root initialization and bounded private content | File-containing check-ins, sync and publication |
| Explicit inherited-test runner configurations | Gest task CLI, server and user interface |

An empty root is a check-in with no files. Private content operations do not
extend that public graph. Build this version from source. The implemented modules do not establish
full-engine compatibility. Git remains the source-code authority for this project.

Historical results qualify only their recorded source, inputs and environment.
The archived `delta1` suite tests delta operations with explicit bounds.
It does not qualify storage or the full Fossil engine.
Linux qualification and clean-clone CI remain separate work.

## Contributor setup

Install Rust with Cargo, Git, Just, uv and the `jlog` command recorder.
The repository has installed workflow copies under `.agents/`.
Their owning packages are `jnterfaces`, `jnprofile-rust`,
`agent_gest_git_skills` and `nossaifd` under `~/Projects`.
Use `SKILL_PACKAGES_DIR` to select another package location.

```sh
just setup
just install --check
```

`just setup` supplies the pinned Linux Tcl and Fossil references, or checks the
existing tools on other hosts, then fetches Cargo dependencies.
`just install` refreshes the saved tool selection from those companion packages.
The [runner guide](conformance.md#local-prerequisites) describes those prerequisites.

Read `AGENTS.md` before changing the repository. Follow the installed Gest
workflow for task ownership, tag classification, dependency impact and review.
Keep reusable tool changes in their owning packages.

## Check a change

Use the repository's Just entry points:

```sh
just fmt-check
just lint
just typecheck
just test
just integration
just doctests
just docstring-check
just docs-check
just docs-build
```

`just test` first runs nine focused Python installer controls, then runs the Rust
unit tests through the installed coverage workflow. `just integration` uses that
Rust coverage workflow. Its reports do not measure Python installer coverage.
Keep failed, blocked, skipped and disabled-feature observations distinct from
passes. Installed tool copies and hook files do not prove actual executions.
Complete the required checks and independent review before delivery to `main`.

## Documentation build

Documentation uses smalldocs 0.5.12 and the installed Atelier extension.
The theme comes from [smalldocs-atelier](https://github.com/rahuldave/smalldocs-atelier)
at commit `b285e162eed9521114745e34ce3b3d3af5474177`.
Its source receipt, licenses and local fonts ship under `docs/_extensions/atelier/`.

The API reference comes from Rust source comments through rustdoc JSON.
Install the required toolchain before a first documentation build:

```sh
rustup toolchain install nightly-2026-11-12 --profile minimal
just docs-check
just docs-build
just docs
```

Generated `docs/api/` pages and `docs/_site/` output are ignored by Git.
Edit source comments to change API content. The site has generated API navigation
and search, with source links to the published `main` branch.

The main-only Pages workflow retains `just docs-publish` and deploys the rendered
artifact through GitHub Pages. It installs the pinned nightly toolchain before
generation. Local rendering does not establish a successful deployment.

## Maintain the bundled provider

The root Cargo patch selects the complete `libsql-ffi` 0.10.0 archive under
`vendor/libsql-ffi`. The root dependency and lockfile also pin `libsql` and
`libsql-sys` to 0.10.0. Its adapted date conversion includes `<stdint.h>` itself
because it uses `UINT64_C`. The upstream build script does not define
`HAVE_STDINT_H` or `HAVE_INTTYPES_H` for the core build.

Use these checks when changing the provider:

```sh
just libsql-vendor-check
just libsql-vendor-test
just libsql-provider-compile
just libsql-provider-dates
cargo test --locked --test libsql_connection_lifecycle
```

The compile controls build the complete adapted C source and check version 2 of
the write-ahead log (WAL) interface, including `xSavepointForget`. The core build
keeps both optional integer-header defines unset. Linux controls retain the
historical 0.9.30 missing-header failure and test the current integer-header requirement.
macOS skips those two negative controls because its headers supply the macro.
A separate negative control removes `<math.h>` before the date conversion code.

The lifecycle tests check connection destruction, retained handles and database isolation.
They do not establish memory safety through allocator or sanitizer diagnostics.
The date recipe runs the retained finite date tests through the bundled Rust provider
without coverage instrumentation.

The existing required PR job `check / ci` runs these compile and semantic checks
on Ubuntu 24.04, then builds the site and Rust API with `just docs-build`.
A successful local macOS run does not establish a Linux pass or a Pages deployment.

For regeneration, supply the pinned original registry source and native Fossil
source to `scripts/regenerate-libsql.py ORIGINAL NATIVE NEW_OUTPUT`.
The script verifies both input hashes, all seven patch hunks and the output hash.
Keep canonical baselines unchanged. Review the source diff before updating the
adapted patch, source identity and affected manifest receipts together.
The provenance record is `evidence/libsql-provider/provenance.json`.

The seven hunks do not count semantic adaptations. The mapping in
`evidence/libsql-provider/upgrade-0.10/adaptation-map.json` accounts for all eight
old hunks. SQLite 3.47.0 already includes the negative Gregorian century fix.
The signed-year change retains the official floor and timezone control flow.
Earlier provider reports describe 0.9.30 and do not qualify the upgraded provider.

## Evidence archive

The repository keeps exact experiments in
[the evidence directory](https://github.com/rahuldave/gossil/tree/main/evidence).
The complete earlier guide narratives remain in
[the documentation history](https://github.com/rahuldave/gossil/tree/main/docs/_history),
which is excluded from rendering, navigation and search.
These archives preserve source identities, failures and qualifications.
The local companion specification is `~/Projects/AIProjectInfra/docs/gossil/README.md`.
It is not required to use the current CLI or library.

## Shared value validation

Task, project and author conversion share private field validators. The task ticket emitter
borrows the status and UTC timestamp forms defined by `gest_task_values`.
Changes must preserve public conversion behavior and the original parser errors
returned through the public error accessor.
Only timestamp strings with a fixed maximum length are allocated before output
size planning; task strings and metadata remain borrowed.

Historical task-value source mappings and run accounting are preserved in
[the task value evidence archive](https://github.com/rahuldave/gossil/blob/main/docs/_history/task-value-evidence.md).

## Recommendations

Use module guides and the generated API for library work. Next, run the relevant
Just checks and get independent review before a pull request to `main`.
