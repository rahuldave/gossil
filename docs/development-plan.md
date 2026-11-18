# Development and contributing

Gossil implements byte primitives, structural artifact parsing and bounded local
storage. The `gossil` executable implements three delta diagnostics.
The generated [Rust API](api/index.qmd) describes the public library surface.

The standalone [Gest task values converter](gest-task-values.md) validates eleven decoded fields.
The [Gest project values converter](gest-project-values.md) validates four canonical project fields.
Both converters operate in memory. Storage integration, raw field admission and the full Gest API remain future work.

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
The accepted bounded `delta1` run does not qualify storage or the full engine.
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

`just setup` fetches Cargo dependencies. `just install` refreshes the saved tool
selection from those companion packages. Neither command acquires the pinned
Fossil source or local Tcl and Fossil binaries.
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

`just test` and `just integration` use the installed coverage workflow.
Keep failed, blocked, skipped and disabled-feature observations distinct from
passes. Installed tool copies and hook files do not prove actual executions.
The coordinator owns final ordered gates and reviewed delivery to `main`.

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

The root Cargo patch selects the complete `libsql-ffi` 0.9.30 archive under
`vendor/libsql-ffi`. Its adapted date conversion includes `<stdint.h>` itself
because it uses `UINT64_C`. The upstream build script does not define
`HAVE_STDINT_H` or `HAVE_INTTYPES_H` for the core build.

Use these checks when changing the provider:

```sh
just libsql-vendor-check
just libsql-vendor-test
just libsql-provider-compile
just libsql-provider-dates
```

The compile regression builds the complete adapted C source with the core feature
flags and both optional header defines unset. On Linux it also reconstructs the
exact previous adapted source and requires its integer initializers to fail.
macOS skips that negative control because its transitive headers supply the macro.
The date recipe runs the retained finite date tests through the bundled Rust provider
without coverage instrumentation.

The existing required PR job `check / ci` runs these compile and semantic checks
on Ubuntu 24.04, then builds the site and Rust API with `just docs-build`.
A successful local macOS run does not establish a Linux pass or a Pages deployment.

For regeneration, supply the pinned original registry source and native Fossil
source to `scripts/regenerate-libsql.py ORIGINAL NATIVE NEW_OUTPUT`.
The script verifies both input hashes, all eight patch hunks and the output hash.
Keep canonical baselines unchanged. Review the source diff before updating the
adapted patch, source identity and affected manifest receipts together.
The provenance record is `evidence/libsql-provider/provenance.json`.

Five frozen task-value captures retain literal whitespace through exact `.gitattributes` paths.
They are `green.log`, `guide.diff` and `source.diff` under `evidence/gest-task-values-cause-fix`,
plus `focused-test.log` and `source-edge-observation.log` under `evidence/gest-task-values`.
These paths preserve command output and patch context without changing the captured bytes.
Ordinary authored files, suffixed paths and neighboring names retain whitespace checks.

## Evidence archive

The repository keeps exact experiments in
[the evidence directory](https://github.com/rahuldave/gossil/tree/main/evidence).
The complete earlier guide narratives remain in
[the documentation history](https://github.com/rahuldave/gossil/tree/main/docs/_history),
which is excluded from rendering, navigation and search.
These archives preserve source identities, failures and qualifications.
The local companion specification is `~/Projects/AIProjectInfra/docs/gossil/README.md`.
It is not required to use the current CLI or library.

## Recommendations

Use module guides and the generated API for library work. Next, run the relevant
Just checks and get independent review before a pull request to `main`.
