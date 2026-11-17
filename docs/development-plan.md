# Gossil development plan

Gossil is planned as a Rust library and application for versioned agentic
project state. It combines Fossil version-control mechanisms with Gest task
management, commands and an application programming interface (API).
Axum is the Rust web framework for the embedded server and dashboard.
The planned storage core uses libsql, a SQLite database implementation.
Git owns source code, and the current executable is only inherited-test infrastructure.

The design and acceptance requirements are in the separate local specification
at `~/Projects/AIProjectInfra/docs/gossil/README.md`. Its linked acceptance
page defines the source inventory and the test families required beyond
inherited Fossil and Gest tests.

Development work is tracked in this project's local Gest task database.
The product parent task is `xyzzowpv`, and iteration `usytswsv` groups its
implementation tasks. A leaf is a task with no child tasks.
The completed environment task `xnuqympy` records bootstrap work.
Parent tasks stay open until their complete scope passes.

With Gest installed and the local task database available, run
`gest task show xyzzowpv` from the repository root to inspect the parent.
Use `gest task show ID` for any task ID in the table below.
These task records are local workflow data, separate from the product specification.

## Execution order

| Phase | Parent | Required result |
| --- | --- | --- |
| 1 | `zlwzpnkx` | Working runner, a recorded decision for every source/test surface, complete Fossil and Gest baselines |
| 2 | `tkqnmpkv` | libsql transactions, hashes, compression, delta, artifacts, cross-links and rebuild |
| 3 | `rtkpowvn` | Tracking, manifests, branches, diff, merge, update, undo and stash |
| 4 | `ukqklzmm` | Records, rendering, scripting, administration, alerts, chat and platform mechanisms |
| 5 | `nuszyluo` | Fossil clone, pull, push, xfer, autosync and failure recovery |
| 6 | `uvswrttq` | Gest core adaptation, CLI/API parity, server, dashboard and Git browsing |
| 7 | `nmtxoskp` | Specified private/shared records, Git links, discussions, annotations and packages |
| 8 | `zzqxwppt` | Every inherited contract, build configuration and extra test family specified by the acceptance page |

The inventory must cover all Fossil and Gest surfaces. For each source and
test surface, record whether it is ported, adapted or awaiting work, with its
corresponding tests. An absent UI screen
never removes an engine mechanism. Assertions and fixtures remain unchanged
unless an explicit requirement has a recorded replacement mapping.

The repository archive `evidence/inventories/README.md` describes the mapped
984 Fossil files and 544 Gest entries, with declared test and API sets.
Read the archived Fossil mappings with the current family corrections and
pinned Git identities described there. The archive preserves original artifacts
and hashes. Its identity and mapping checks leave semantic coverage, required
configurations and storage encodings open. Historical generation scripts require their original inputs.

## Checkpoints and ownership

At this bootstrap checkpoint, the checkout has no HEAD. Its primary branch is
`gest/xnuqympy-bootstrap`. The branch `main` is the intended later integration target.

The coordinating agent assigns one writer to the initial runner. That writer
preserves the staged bootstrap.
Those staged files contain the repository's initial workflow and tool setup,
which must not be overwritten or silently dropped by runner work.
No empty commit or coverage exclusion may replace a genuine checkpoint.

Every production change starts with focused tests and checks of the callers
and tests that depend on the changed behavior. Fresh GPT-6.1 Sol agents
review changes independently and challenge assumptions.
The installed guide `.agents/skills/gtw/references/gest_codex_workflow.md`
and root `AGENTS.md` describe the full process in the checkout.

An agentic target writes a task packet for a fresh agent. The agent returns
a result packet, which the parent validates and records. A record identifies
the reviewed file tree and the packet's nonce, a unique identifier.
It applies to that content, so edits can require a new check. Run the Just commands in this order.
`just update-docs`, `just prose`, `just review`, `just test-review`, then
`just check` on the final tree.

`just test-parent`, a dependency of `test-review`, runs added or changed tests
on the parent commit. The first checkout has no parent, so that step is
skipped. The skip does not show that the tests failed on earlier behavior.

After the verified initial commit, independent writers may use physical
worktrees. Each needs an owner, absolute path, topic branch and explicit base.
Create a worktree after the initial commit with
`just worktree-new NAME BASE`, using an existing branch or commit for
`BASE`. The recipe reads `.worktreelink` and links the existing ignored `.jlog`
folder from the primary checkout into the new worktree.
For an existing worktree, `just worktree-sync` applies those links.
The link does not install tools or share source edits.

Dependent branches remain sequential until their lower checkpoint exists.

Delivery stays local. No remote, push, pull request or publication is selected.
Keep task records and review evidence local until a delivery destination is selected. Local commits
still require complete verification and independent review.

## Unsettled extension contracts

The specification above lists storage, Gest, sync and UI contract pages that
have not yet been written. Exact
issue revisions, Git-link encodings, annotation descriptors and shared/private
replication rules need byte fixtures before durable extension writes begin.
Task `qvqyvqlt` tracks those missing contracts. Inspect it with
`gest task show qvqyvqlt` in the local Gest project. Do not invent storage or wire formats.

The private/shared contract must settle dependency closure, transaction identity,
authorization, declassification and identical-byte visibility. Shared history
must restore without private objects. Unsupported required extensions must
fail before an operation reports complete success.

## Recommendations

Keep the full Gest task hierarchy and deliver small reviewed checkpoints.
Next, verify and review the runner controls, resolve the inventory's named
acceptance gaps and characterize the required reference baselines for phase 1.
Preserve failures and disabled-feature results as classified evidence.
Begin phase 2 primitives against preserved reference tests after that checkpoint.
