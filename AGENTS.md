<!-- begin lines of tasker, the tasker role, from agent_gest_git_skills at 9225e7b. -->
<!-- Last written to AGENTS.md by `just install --defaults tasker=git --clone agent_gest_git_skills=/Users/rahul/Projects/agent_gest_git_skills.worktrees/codex-release-bootstrap-fix --clean`. The record is parts."AGENTS.md" in .agents/stack.json. -->
Read and follow @.agents/instructions/tasker.md
<!-- end lines of tasker. `just uninstall tasker` removes them. -->
<!-- begin lines of prose, the prose role, from nossaifd at 7ab9e94. -->
<!-- Last written to AGENTS.md by `install_stack.py --target /Users/rahul/Projects/gossil --profile rust --defaults --clean`. The record is parts."AGENTS.md" in .agents/stack.json. -->
Read and follow @.agents/instructions/prose.md
<!-- end lines of prose. `just uninstall prose` removes them. -->

# Agent instructions

## Project context

- Project name: Gossil.
- Source directories: `src/` contains the inherited-test runner, delta and byte
  primitives, token codec, structural artifact parser and local storage boundary. `tests/` contains controls,
  portable fixtures and the private storage composition test.
- Local guides: `docs/conformance.md`, `docs/delta-encoder.md`,
  `docs/delta-decoder.md`, `docs/blob-primitives.md`, `docs/token-codec.md`,
  `docs/local-storage.md`, `docs/native-storage.md`, `docs/artifact-parser.md` and `docs/development-plan.md`.
- Product specification: `~/Projects/AIProjectInfra/docs/gossil/README.md` and its linked pages.
- Workflow playbook: `.agents/skills/gtw/references/gest_codex_workflow.md`.

The executable `gossil-conformance` is development infrastructure. It runs
explicit reference or candidate configurations, preserves evidence and rejects
failed or incomplete suites. It implements no Gossil engine behavior.

The library also implements bounded delta application, exposed by
`gossil vcs test-delta-apply ORIGIN DELTA TARGET`. It creates deltas through `gossil vcs test-delta-create` and checks
both directions through `gossil vcs test-delta`. The local libSQL boundary owns database lifecycle and exclusive transactions.
The CLI also exposes `vcs init` and `vcs new` through `gossil` or `gos`, with
`v` as the group alias. These create an empty repository without a checkout. See
`docs/cli.md` for the separate native `vcs open` contract.

Native local storage supports checkout-free initialization, then one library
checkout registration at the empty root. Reopen admits strict catalog4 with zero
or one registration, explicit tracked scopes and the supported private recording
history. ID and exact-root views authenticate existing registrations;
the library methods publish no locator. The source `vcs open` adapter registers
or reopens that checkout and publishes or reconstructs its owner-local locator.
It requires explicit repository and checkout selection, with `--root` for first
registration. It selects no files.

Bound catalog1 creation and bounded private content operations remain supported.
Generic private content writes refuse catalog4.

The private recording library requires the complete tracked set, immutable
private scopes and all seven expected checkout facts. It captures every working file with one borrowed content
budget and parser ledger, preserves the empty shared root, and admits exact
private history projections. Working files remain unchanged. Existing CLI status
prints A/R and actual scope; new scope and commit commands are deferred. Broader
crosslinks, shared recording, sync and Gest integration remain planned work.

## Commands and evidence

Use `just conformance CONFIG` for one suite and `just conformance-self-test`
for runner controls. Use generic `just setup` for native reference setup and
Cargo acquisition. On Linux it builds the pinned real references; on other
hosts it checks the existing tools without replacing them.

Use the installed Rust checks through Just, including
`lint`, `typecheck`, `test`, `integration`, `coverage-check`, `docstring-check`
and `api-check`. `just docs-check` checks the site and API docs.
`just docs-build` renders the site. Do not weaken checks to get a pass.

The runner checkpoint is `evidence/runner-checkpoint-2026-11-17.json`.
It records executed macOS checks, not a final gate pass or Linux qualification.
The baseline archive is `evidence/baselines/`. Keep failed, blocked, skipped
and disabled-feature results distinct from passes. Generic skips mean
`not-run`; a capability observation must support `feature-disabled`.

Use `jlog run tclsh` for local Tcl 8.6.18 and `jlog run fossil` for the
Fossil 2.29 reference binary at `7e8ecf0ce4`. Tcl uses shared Homebrew storage
under the user's temporary exception. The Fossil build omits TLS and FUSE.
Ignored `.jlog` tools are explicit setup outputs on Linux. Existing tools remain
local prerequisites on other hosts. Worktrees may share them through `.worktreelink`.

## Ownership and delivery

The composed core topic is `gest/ruxnuror-core-integration`, based on accepted
encoder `6ea4c22aea10b49a759f26645166193597b165de`. It imports accepted blob,
token and database changes without claiming their source commits as ancestors.
The coordinator owns Gest commands, final staging, gates and the commit.
Independent workers preserve each other's files and immutable native evidence.
The database exposes lifecycle and admitted native content operations; arbitrary
SQL stays private.

`just install` uses package clones in `~/Projects`, including from worktrees.
`SKILL_PACKAGES_DIR` can override that location. Distinguish package sources,
installed skills, hook files and actual executions when reporting results.
Keep reusable workflow changes in their owning packages.

The public remote is `https://github.com/rahuldave/gossil`. Create reviewed
pull requests to `main` for subsequent delivery. The coordinator owns pushes,
GitHub changes and publication checks. The source-only release workflow checks
pull requests and tags successful pushes to `main`; it publishes no package.

The saved answer `docs-publish=yes` enables `just docs-publish` through
smalldocs 0.5.12. The main-only workflow also uploads the built site and deploys
it through GitHub Pages. Configure Pages to use GitHub Actions. A workflow
file and a successful build do not prove that the public site is live.

## Recommendations

Use the module guides for explicit bounds and lifecycle contracts.
Next, review the composed core and preserve the unchanged `delta1` evidence
and its qualifications. Qualify the library recording checkpoint on its accepted
tracking base, then add the scope and commit command adapters in a separate coherent commit.
