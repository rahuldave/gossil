# Gossil development

Gossil is planned as a Rust library and application for versioned agentic
project state, combining Fossil version control with Gest task management.
Planned records include tasks, shared issues and private agent traces.
Git remains the source-code authority in the first release.

This checkout includes a Fossil delta encoder and decoder with explicit input
and output size limits. They are exposed in
`gossil::delta`, with create, apply and two-way test diagnostics under `gossil vcs`.
It also includes `gossil-conformance`, a Rust runner for inherited Fossil Tcl
tests against an explicit reference or candidate program. Gest integration,
libsql storage and the remaining Fossil mechanisms are planned work.

## Start here

Read [the delta encoder guide](docs/delta-encoder.md) for creation and two-way
round trips, and [the delta decoder guide](docs/delta-decoder.md) for the library,
diagnostic command and tested compatibility limits.
The [runner guide](docs/conformance.md) covers prerequisites, configuration,
exit statuses and evidence limits. The [development plan](docs/development-plan.md)
gives the delivery order and local review workflow. The product specification
remains at `~/Projects/AIProjectInfra/docs/gossil/README.md`.

The product specification is a local companion checkout, rather than a page
supplied by this repository.

The commands below require Rust with Cargo, Git, Just and uv on `PATH`.
Just runs the recipes in `Justfile`, and uv runs the installed Python tools.
Tcl and Fossil are also required for suite execution. The
[runner prerequisites](docs/conformance.md#local-prerequisites) explain what
this checkout supplies and what a fresh clone still needs.

From the repository root, run the commands that match your task. Replace
`ORIGIN`, `DELTA` and `TARGET` with file paths before trying the decoder.
`just setup` fetches Cargo dependencies, while `just install --check` checks
the saved workflow-tool installation. The remaining
commands run the decoder, runner checks, a configured suite or documentation checks.

```sh
just setup
just install --check
cargo run --locked --bin gossil -- vcs test-delta-apply ORIGIN DELTA TARGET
just conformance-self-test
just conformance /absolute/path/to/CONFIG.json
just docs-check
just docs-build
```

`conformance-self-test` checks runner controls, including expected failures.
`conformance` executes one configured suite and returns nonzero if acceptance
fails. Reference and candidate runs remain separate, with no fallback.
The [runner guide](docs/conformance.md) includes a complete configuration example.

## Verification status

The decoder tests replay the preserved bounded native apply observations and
command output. Its [guide](docs/delta-decoder.md#evidence-and-limits) separates
the original 64 reference cases, the 27 supplemental observations and
the portable fixture checks. The encoder observations and qualification of the inherited `delta1` suite
are recorded separately in [the encoder guide](docs/delta-encoder.md).

The encoder guide records acceptance of all 22,804 assertions from `delta1`,
the inherited Fossil delta suite, against the exact frozen runner and candidate.
The run qualifies bounded delta behavior. Full-engine and database qualification remain open.

The [source and test inventories](evidence/inventories/README.md) map all 984
Fossil files and 544 Gest entries, with declared test and API sets.
An overlay corrects the inventory metadata for fourteen Fossil contract
families, each a group of related mechanisms. Companion records give Git modes,
kinds and blob identities for all 984 Fossil paths.
Identity and mapping checks do not establish semantic coverage or accepted
compatibility. Configuration and storage contract gaps remain open; the
[inventory guide](evidence/inventories/README.md) describes the pending work.

The executed controls ran on macOS. Linux qualification and clean-clone CI
remain pending. The historical
[runner checkpoint](evidence/runner-checkpoint-2026-11-17.json) preserves failed
cleanup experiments and the later runner controls, with their limits.

The [reference baseline archive](evidence/baselines/README.md) preserves
Fossil and Gest experiment results, including failures, timeouts and disabled
features. Passing runner controls do not establish inherited compatibility.
A generic skipped result means `not-run`. The status `feature-disabled`
requires evidence for the missing capability.

### Earlier runner evidence

The historical [runner checkpoint](evidence/runner-checkpoint-2026-11-17.json) records
three unit and six integration test functions, lint, docstring and API checks.
Coverage checked every executable source line because there was no parent
commit from which to select changed lines. No coverage exemptions were used.
The checkpoint predates final independent documentation, prose and review
records. It does not claim a final `just check` pass.

The [review correction](evidence/review-correction/checkpoint.json) separately
records rejection of malformed numeric summaries and duplicate summaries.
Its focused regression tests failed before the fix and passed after it.
The later strict coverage run passed three unit and eight integration test
functions. That evidence is not a final combined gate for the current tree.

## Native tools and installed workflow

This checkout exposes Tcl 8.6.18 and Fossil 2.29 at revision `7e8ecf0ce4`
through `.jlog/bin`. Use `jlog run tclsh` and `jlog run fossil version`.
The user temporarily permitted this Tcl installation to use shared Homebrew
storage rather than keeping every native dependency inside the checkout.
The local Fossil build omits TLS and FUSE. The ignored `.jlog` files are not
supplied or automatically acquired by a fresh clone.

The Rust profile supplies check recipes. The Gest workflow skills describe
task tracking and review, while prose tools check documentation.
Hook files connect those checks to supported agent and Git events.

Their package sources are `jnterfaces`, `jnprofile-rust`,
`agent_gest_git_skills` and `nossaifd` under `~/Projects`. `SKILL_PACKAGES_DIR` can select
another location. The checked-in `.agents/stack.json` holds the saved
profile and tool selection. `just install` refreshes that selection from the
package sources. The installed checks include `tools-check` for tool versions
and `release-check` for the configured release policy.

`just setup` fetches Cargo dependencies. Neither `just setup` nor `just install`
acquires the pinned Fossil source or the local Tcl and Fossil binaries.
Installed copies and hook files do not prove live hook execution or a complete
gate pass. Keep installation receipts separate from executed check records.

Delivery stays local. No remote, push, pull request or publication workflow
has been selected. Automatic docs publication is disabled.

## Recommendations

Use the bounded decoder for characterized delta application, and retain the
explicit runner configurations for inherited suites. Next, review the encoder
and the recorded `delta1` qualification evidence, which this documentation
revision leaves unchanged.
Keep native acquisition and the remaining reference configurations as open work.
