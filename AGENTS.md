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
- Source directories: `src/` contains the inherited-test runner and bounded delta decoder; `tests/` contains their controls and portable fixtures.
- Local guides: `docs/conformance.md`, `docs/delta-decoder.md` and `docs/development-plan.md`.
- Product specification: `~/Projects/AIProjectInfra/docs/gossil/README.md` and its linked pages.
- Workflow playbook: `.agents/skills/gtw/references/gest_codex_workflow.md`.

The executable `gossil-conformance` is development infrastructure. It runs
explicit reference or candidate configurations, preserves evidence and rejects
failed or incomplete suites. It implements no Gossil engine behavior.
The library also implements bounded delta application, exposed by
`gossil vcs test-delta-apply ORIGIN DELTA TARGET`. The encoder, remaining source
port, Gest integration and libsql core remain planned work.

## Commands and evidence

Use `just conformance CONFIG` for one suite and `just conformance-self-test`
for runner controls. Use the installed Rust checks through Just, including
`lint`, `typecheck`, `test`, `integration`, `coverage-check`, `docstring-check`
and `api-check`. `just docs-check` checks the site and API docs;
`just docs-build` renders the site. Do not weaken checks to obtain a pass.

The runner checkpoint is `evidence/runner-checkpoint-2026-11-17.json`.
It records executed macOS checks, not a final gate pass or Linux qualification.
The baseline archive is `evidence/baselines/`. Keep failed, blocked, skipped
and disabled-feature results distinct from passes. Generic skips mean
`not-run`; a capability observation must support `feature-disabled`.

Use `jlog run tclsh` for local Tcl 8.6.18 and `jlog run fossil` for the
Fossil 2.29 reference binary at `7e8ecf0ce4`. Tcl uses shared Homebrew storage
under the user's temporary exception. The Fossil build omits TLS and FUSE.
Ignored `.jlog` tools are local prerequisites, not automatic clone setup.
Worktrees may share them through `.worktreelink`.

## Ownership and delivery

The initial bootstrap had no HEAD and used `gest/xnuqympy-bootstrap`.
The decoder topic is `gest/mpxpvmkq-delta-decoder`, based on
`e603376aac1977ca6ba40973d1ae6eca75e8fdf3`. The coordinator
`delta_implementation` owns implementation writes. Documentation and review
agents work only within their assigned scopes and preserve other writers' files.
Follow the documented order of fresh agent checks before committing.
No empty commit replaces verification.

`just install` uses package clones in `~/Projects`, including from worktrees.
`SKILL_PACKAGES_DIR` can override that location. Distinguish package sources,
installed skills, hook files and actual executions when reporting results.
Keep reusable workflow changes in their owning packages.

No remote or release workflow has been selected. Keep delivery local.
Publishing and GitHub issue promotion remain deferred for that reason.

## Recommendations

Use the decoder guide for bounded application and the runner guide for explicit
suite configurations. Next, implement the encoder and `test-delta` diagnostics,
then qualify the unchanged `delta1` suite.
