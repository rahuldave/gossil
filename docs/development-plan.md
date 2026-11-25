# Development and contributing

Gossil implements byte primitives, structural artifact parsing and bounded local
storage. The `gossil` executable initializes empty repositories, opens native
checkouts, adds explicit paths, lists tracked states and implements three delta
diagnostics. The CLI calls the library to select immutable path scopes and record the complete
private tracked tree. The prepared [glob source](glob.md) adds bounded list
matching and a repository-free diagnostic, with a focused source checkpoint
described below.
The generated [Rust API](api/index.qmd) describes the public library surface.

The standalone [Gest task values converter](gest-task-values.md) validates eleven decoded fields.
The [Gest project values converter](gest-project-values.md) validates four canonical project fields.
The [Gest author values converter](gest-author-values.md) validates six canonical author fields.
The [Gest artifact values converter](gest-artifact-values.md) validates eight canonical artifact fields.
The [Gest iteration values converter](gest-iteration-values.md) validates nine canonical iteration fields.
The [Gest tag values converter](gest-tag-values.md) validates four canonical tag fields.

The [Gest note values converter](gest-note-values.md) validates seven canonical note fields.
The [Gest relationship values converter](gest-relationship-values.md) validates eight canonical relationship fields.
The [Gest entity tag values converter](gest-entity-tag-values.md) validates four canonical entity tag fields.
The [Gest iteration task values converter](gest-iteration-task-values.md) validates four canonical iteration task fields.

All ten converters operate in memory and leave storage integration, raw field admission and the full Gest API to future work.

The [Gest model](gest-model.md) is a copy of the Gest record types and their primitive values.
A record that a converter has validated converts into its model record through the `From` trait.

The [task ticket emitter](task-ticket.md) serializes validated task snapshots with
caller-supplied native identity, actor and date. The [Gest ticket emitter](gest-ticket.md)
serializes validated artifact and project snapshots in the same way, and the three kinds share one emitter.

The internal modules for [strict local records](local-records.md) decode local
JSON records and charge queries against fixed limits. The metered content methods
read and write content through the metered queries, and they admit only the newest
catalog. The [issue ticket writer](issue-ticket.md) is the first production caller
of the metered queries and of those methods.

## Current scope

| Available in source | Planned |
|---|---|
| Delta create, apply and two-way file checks | General version-control CLI |
| Bounded glob API and `test-glob` diagnostic; focused checkpoint | Settings lookup, wildcard tracking and directory traversal |
| Compression, hashes, token escaping, artifact checksums and [lexical filename simplification](file-path.md) | Repository traversal and changes to persisted scope choices |
| [Shortest paths and common ancestors](history.md) over caller-supplied check-in rows | Persisted history graphs, bisect, annotation and file history |
| Read-only [Git reader](git-reader.md) library for a worktree, its refs, commit parents and three configuration keys | Bind command, Git hooks and the source registry |
| Eight structural artifact types with explicit limits | General crosslinks, migration, rebuild and filesystem recovery |
| Native initialization/open, tracking, scope/commit CLI and library; catalog1 private content | Shared recording, sync and artifact publication |
| Explicit inherited-test runner configurations | Gest task CLI, server and user interface |
| Library [writer and reader](issue-ticket.md) of private task and project tickets; [`issue init`](cli.md#create-the-gest-project-of-a-checkout) | Shared tickets, ticket changes and the other `issue` commands |

Initialization creates catalog6 without a checkout. The library can register
one existing directory and authenticate its registration by ID or exact root.
Opening a repository admits exactly zero or one registration and the supported tracking, scope
and private recording family, with private issue tickets written before, between or after check-ins.
Bound catalog1 creation retains its checkout and private content operations. Catalog6
generic private content writes remain unsupported; its separate recorder writes
complete private file trees.
Registration changes the database only.

The source CLI combines registration with bounded locator
publication and recovery of the exact registration. Working-file materialization
remains future work. See [native checkout open](cli.md#open-a-native-checkout).

An empty root is a check-in with no files. Private content operations do not
extend that public graph. Build this version from source. The implemented modules do not establish
full-engine compatibility. Git remains the source-code authority for this project.

Historical results qualify only their recorded source, inputs and environment.
The archived `delta1` suite tests delta operations with explicit bounds.
It does not qualify storage or the full Fossil engine.
Linux qualification and clean-clone CI for the current private recording
checkpoint remain separate work.

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

`just docs-check` checks the site and the API pages. `just typecheck` runs
`just rustdoc-check` before its own type check. `just rustdoc-check` builds the
rustdoc with private items and fails on each warning. So `just typecheck`, and
`just check` with it, fails on a broken intra-doc link. A broken intra-doc link is
a link in a doc comment that rustdoc cannot resolve to an item.

`just test` first runs nine focused Python installer controls, then runs the Rust
unit tests through the installed coverage workflow. `just integration` uses that
Rust coverage workflow. Its reports do not measure Python installer coverage.
Keep failed, blocked, skipped and disabled-feature observations distinct from
passes. Installed tool copies and hook files do not prove actual executions.
Complete the required checks and independent review before delivery to `main`.

## Documentation build

Documentation uses smalldocs 0.5.13 and the installed Atelier extension.
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
`libsql-sys` to 0.10.0. The Rust wrapper uses the full Git revision in the
[TEXT retrieval guide](libsql-wrapper.md); `libsql-sys` retains its registry identity.
The [wrapper correction](libsql-wrapper.md) preserves complete valid TEXT
without changing the bundled provider. Its adapted date conversion includes `<stdint.h>` itself
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

All ten converters of Gest values share the private field validators of
`gest_value_validation`. The validators parse identifiers and entity types with the
primitives of the Gest model, and the converters parse each status and type the
same way. Changes must preserve public conversion behavior and the original
parser errors returned through the public error accessor. Before the converters
plan the size of the output, they allocate only the short identifier, status and
timestamp strings. The title, description and metadata of a task remain borrowed.

The `gest_value_validation` module also defines the borrowed canonical values
that the task, artifact and project ticket emitters serialize. The task emitter
writes the status form that `gest_task_values` defines. It writes the identifiers
and the UTC timestamps in the forms that `canonical_id` and `canonical_timestamp`
define in `gest_value_validation`.

Historical task-value source mappings and run accounting are preserved in
[the task value evidence archive](https://github.com/rahuldave/gossil/blob/main/docs/_history/task-value-evidence.md).

## Native repository CLI

The source routes `gossil` and `gos`, `vcs` and `v`, and `init` and `new`
to the checkout-free native initializer. It accepts administrator, date and SHA1
options and uses SQLite randomness in a private memory database. Login fallback
uses environment values without a preference database. Existing delta diagnostics
retain their dispatcher and argument contracts.

Before dispatch, the product entry answers help requests with the text of the
private `product::help` module, and rejects unknown groups and commands with
status 2. The
[help section](cli.md#get-help) gives the forms, and `tests/cli_help.rs` holds
the CLI controls.

The `issue` group, or `i`, routes each Gest command by the issue table of
`product::help`. The parser in `product::issue` checks the command names
and their aliases. It runs `init`, `task create`, `task list` and `task show`, and
refuses each other recognised command with status 69. The
[issue section](cli.md#issue-commands) gives the forms and errors, and
`tests/issue_cli.rs` holds the controls.

`product::issue_init` runs [`issue init`](cli.md#create-the-gest-project-of-a-checkout), and
`tests/issue_init_cli.rs` holds its controls. `product::issue_task` runs
[`issue task create`](cli.md#create-a-task), and `tests/issue_task_cli.rs` holds
its controls. The handlers of `task list` and `task show` are in
`product::issue_read`, and `tests/issue_read_cli.rs` holds the controls of the
two commands. The [issue read guide](issue-read.md) gives their output.

`tests/issue_demo.rs` runs the walk-through of the [issue guide](issues.md) on the
real `gos` binary. Its ignored test reads the demo repository with stock Fossil
and checks that Gossil refuses a copy after `fossil rebuild`. `just issue-demo`
runs that part with the pinned Fossil 2.29.

The module `vcs_verify` runs [`vcs verify`](cli.md#verify-a-repository). It runs the full proof of a
repository, or the proof of one issue, and changes nothing. The proofs are
`LocalOwnerRepository::verify` and `verify_issue` in `storage::repository::verification`.
`tests/vcs_verify_cli.rs` holds the controls of the command.

The focused CLI controls are in `tests/vcs_init_cli.rs`; parser, environment,
password and receipt controls are in the private `vcs_init` module. Source edits
and authored tests do not establish a passing qualification run. Versioned
executable and matching documentation delivery have their own qualification checks.

The source `vcs open` adapter adds first registration, reopen and bounded locator
recovery without project initialization. Its configuration comes from the
invocation directory, independently of an explicit root. The
[CLI contract](cli.md#open-a-native-checkout) covers selection, conflicts and
publication phases. Scope selection and private recording are available through
the library and the separate CLI checkpoint described below.

Locator/parser/publication controls are in `src/checkout_locator.rs`, configuration
controls in `src/checkout_config.rs`, and executable controls in
`tests/vcs_open_cli.rs`. Generic lifecycle commit-failure composition remains
distinct from an injected native-registration COMMIT failure. Source controls
and focused passes do not establish full coverage, Linux qualification or delivery.

## Delivered tracking baseline

The accepted baseline is [PR 38](https://github.com/rahuldave/gossil/pull/38),
merged at `fa841b1a914c17dfbd8378bae4ef9ab7e26fb5cc` and tagged `v0.5.0`.
It includes the accepted [libsql wrapper correction](libsql-wrapper.md).
That tracking release passed its final source checks, independent review,
Linux CI and source-release workflow. Its isolated installation passed both
`gos v` and `gossil vcs` smoke tests. All 41 live HTML files matched the
merged-source documentation build. The default preview executables stayed unchanged.

Version 0.5.0 enrolls explicit pending additions without choosing their scope.
It does not include this checkpoint's private recording API or new status format.
A source tag, installed checkpoint and default command are separate delivery facts.

## Private recording qualification

The current branch adds explicit immutable path scopes, complete private file
recording, and status that reports each tracked path's state and actual scope.
Its first checkpoint exposes recording through the library. The second adds
`scope` and `commit` to both executables and both command-group aliases.

The committed library checkpoint is
`fca8c89f8c3d42a0a1fa7c7589042a4c1cc324d4`. Its normal full qualification passed
207 unit tests, 232 integration tests and 100 percent changed-line coverage over
5880 lines. Its API check documented all 533 items. These results qualify that
library checkpoint, not the later CLI tree.

The focused CLI run passed three parser unit tests and two fresh-process
integration tests under guard `bbc05d10-b495-4d2e-ae33-dfbc255ccffd`.
Its unit selection filtered out 207 other tests. Both executable aliases recorded
first and repeated private commits, including an edit without re-add. A later
noon commit with closed stdout returned failure after durable native rows changed;
reopen and an unchanged retry preserved that history.
The same run checked explicit grammar, scopes and locator refusals.

The API/site check documented all 538 API items before the guide updates and
version change. That result is not the final documentation gate. Full CLI checks,
coverage and fresh independent gate records still remain. Draft PR 39 is on hold
for this second coherent commit, final version and gates. No release, installation
or publication is claimed for private recording.

An earlier prepared library tree passed 203 library tests and 228 integration
tests, but failed its coverage check. Those results remain historical and are
superseded for library qualification by the committed checkpoint above.

The source is based on the [accepted tracking merge](#delivered-tracking-baseline).
The recorder retains the original parent-directory descriptors to check that
captured paths still have the same ancestry, charging the same operation budget.
Native controls are in `tests/native_path_scope.rs`, `tests/native_recording.rs`
and the private storage modules. CLI controls are in `src/vcs_recording.rs`
and `tests/vcs_recording_cli.rs`.

The native controls cover the complete expected view, immutable scopes,
exact all-tracked requests, current bytes and modes, no-change behavior, rollback,
strict history projections and shared work limits. Source controls alone do not
establish execution. The qualification results above identify the checks that ran.

## Recorded ancestry API qualification

The current source adds `LocalOwnerRepository::recorded_timeline` and
`RecordedCheckin` for complete supported catalog6 checkout ancestry. The checkpoint
covers the library, and it ran on catalog4, before catalog6 replaced it.

A separate source CLI checkpoint adds `timeline` and `log`.
The timeline delivery baseline is accepted at `4250b873` and released as
`v0.8.0`. The checkpoint results below retain their original, narrower scope.
They do not qualify subsequent source changes.

The guarded focused run passed eight public API tests, 116 repository unit tests
and 69 existing native integration tests. Strict lint also passed. The unit
selection filtered out 102 tests. These results do not establish a final full
gate pass, changed-line coverage, release, installation or publication.

The native comparison has 34 owned assertions and zero inherited assertions.
It is a bounded native ancestor and manifest comparison, not execution of the
full Fossil suite. The independent source review found no source blocker and
confirmed unchanged old admission checks and accounting by exact body comparison.
That review is not the final independent gate review.

The reader preserves native bytes and full hashes, shares one content budget
and parser ledger through admission and projection, and withholds rows until
checked completion. Existing admission behavior remains unchanged. Its
[storage contract](native-storage.md#read-recorded-checkout-ancestry) describes
complete results, refusals and the absence of working-file content reads.
The committed API checkpoint is `9a6da5aa7efe9246a46f30ff2e736113725a3107`.
Its full qualification passed 220 unit tests and 246 integration tests, and the
API check documented all 551 items. Those results qualify the API checkpoint
and do not establish a full pass for the later CLI tree.

## Recorded ancestry CLI qualification

The source `src/vcs_timeline.rs` adapter exposes complete recorded ancestry
through both executable names, both group names and `timeline` or `log`.
[The CLI reference](cli.md#read-complete-recorded-ancestry) defines explicit
selectors, existing-locator admission, exact NDJSON and separate opening, API
and serialization allowances. This checkpoint implements no filters, truncation,
working-file content reads or locator publication.

The guarded focused run passed seven process tests, five adapter unit tests,
three product unit tests and 19 existing CLI tests. Strict lint passed under
Rust 1.100. These focused results do not establish a full CLI gate pass. Complete documentation, prose,
coverage and fresh independent gate review remain necessary before delivery.

Controls cover literal root and private ancestry, all eight aliases, strict
selectors and help, original metadata escaping, cumulative serialization bounds,
invalid UTF-8, corruption and locator refusals. Stdout failure retains its
original cause and can leave partial output. Logical database and working-tree
inventories are compared; no physical database byte identity is claimed.

The native comparison remains 34 owned assertions and zero inherited assertions.
No broader Fossil parity is claimed. `cat` remains separate issue 18 work;
issue 17 remains open. Neither issue is completed by these ancestry commands.
The later accepted timeline delivery at `4250b873` completed the `v0.8.0`
release. That delivery is separate from the prepared glob source below.

## Glob source checkpoint and pending qualification

The prepared `glob` API and shared `test-glob` adapter port Fossil list parsing,
matching and SQL construction. See the [module guide](glob.md) and
[CLI grammar](cli.md#inspect-a-fossil-glob-list). The adaptation uses the existing
selected libSQL 0.10.0 engine, with an exact direct `libsql-ffi` dependency and
default features disabled. The 2026-11-21 focused graph comparison retained all
126 nonroot packages and complete edges/features, with one `libsql-ffi` 0.10.0
provider.

The 37 files under `evidence/glob/` and `evidence/glob-provider-integration/`
retain the donor's exact historical bytes. The earlier execution used 0.9.30.
Its 20 unchanged `glob.test` families, 100 through 119, and coverage results
remain historical observations. They do not qualify the present source.

The focused checkpoint built both product binaries and passed 3 CLI tests,
9 glob integration tests, 5 filtered library units (4 glob plus one existing
global-state control) and all 20 unchanged Tcl assertions. Controls cover all
four aliases, exact output and errors, raw arguments, first-match and NUL
boundaries, budgets and generated SQL through the selected engine. Non-Unix
argument conversion has source review only, with no cross-compilation or runtime
qualification.

At the checkpoint, `Cargo.toml` set the version of the Gossil crate to 0.9.0,
which was later released as `v0.9.0`. The current version of the crate is
0.17.0, and version 0.16.0 was released as `v0.16.0`. The `libsql-ffi`
provider is at 0.10.0, and its version is separate
from the version of the crate.

The focused tree
`a5583c3c6a73dda1a75ad1dd4c168efc06655c40` used a prospective checked-commit
prerequisite while actual parent remained `4250b873`.
[PR 47](https://github.com/rahuldave/gossil/pull/47) supplies typed commit and
cleanup causes. Strict Rust 1.100 lint
stopped at the prerequisite's `commit_checked` large-error lint. The owner's
local fix is synchronized, without a fresh qualification claim for that source
successor.

Confirm PR 47's accepted revision before final gates.
Then qualify the complete resulting tree, including the regression suite,
independent prose and review, API/site checks and the frozen-tree Just gates.
The focused checkpoint does not establish those complete gates.

Release, isolated installation of the exact merged binaries and complete
published HTML parity checks remain later delivery work. Preserve the default
installed previews until their separate replacement is authorized. Glob adds
no settings lookup, traversal, persistent selection or wildcard add/status.

## Recommendations

Use module guides and the generated API for library work. Next, run the relevant
Just checks and get independent review before a pull request to `main`.
