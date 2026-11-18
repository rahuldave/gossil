# Gossil development plan

Gossil is planned as a Rust library and application for versioned agentic
project state. It combines Fossil version-control mechanisms with Gest task
management, commands and an application programming interface (API).
Axum is the Rust web framework for the embedded server and dashboard.
The storage boundary uses libsql, a SQLite database implementation.
Git owns source code.

The checkout implements bounded delta creation and application, their
`gossil vcs` diagnostics, and the inherited-test runner.
It also composes [byte primitives](blob-primitives.md), [token escaping](token-codec.md)
and [local transactions](local-storage.md), with no domain schema or public SQL.
The [encoder](delta-encoder.md) and [decoder](delta-decoder.md) guides give
the current scope and limits.

The planned public command domains are `issue` and `vcs`. The planned `ui`
command starts an embedded Gest/Axum server and opens a browser, while `serve`
runs the server without a browser. The implemented diagnostics are
`vcs test-delta-apply`, `vcs test-delta-create` and `vcs test-delta`.
A public wiki or forum product is outside the design, but
inherited Fossil mechanisms and tests remain internal compatibility obligations.

The design and acceptance requirements are in the separate local specification
at `~/Projects/AIProjectInfra/docs/gossil/README.md`. Its linked acceptance
page defines the source inventory and the test families required beyond
inherited Fossil and Gest tests.

Development work is tracked in this project's local Gest task database.
The commands below require an installed Gest tool and that local database.
A fresh clone does not supply those task records. Follow the installed
`.agents/skills/gtw/references/gest_codex_workflow.md` for the project workflow.

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

Cross-links connect stored artifacts to the records derived from them.
The `/xfer` endpoint carries Fossil synchronization traffic.

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

The composed core is the library's delta, byte, token and local storage modules
and their private composition test. Its topic starts at accepted encoder
`6ea4c22aea10b49a759f26645166193597b165de` and imports accepted blob, token
and database changes. These imports do not establish ancestry of their source
commits. Independent review and a final combined gate precede the integration
checkpoint. The following bootstrap notes describe earlier setup.

Every production change starts with focused tests and checks of the callers
and tests that depend on the changed behavior. Fresh GPT-6.1 Sol agents
review changes independently and challenge assumptions.
The installed guide `.agents/skills/gtw/references/gest_codex_workflow.md`
and root `AGENTS.md` describe the full process in the checkout.
Follow that workflow guide for packet dispatch and result validation before
using the commands below.

An agentic target is a Just command that writes a task packet for a fresh agent. The agent returns
a result packet, which the parent validates and records. A record identifies
the reviewed file tree and the packet's nonce, a unique identifier.
It applies to that content, so edits can require a new check.
Use the installed workflow guide above for packet dispatch, result validation
and recording; printing a packet alone does not complete its check.

The following commands create and check those packets as part of the review workflow.
Run the Just commands in this order.
`just update-docs`, `just prose`, `just review`, `just test-review`, then
`just check` on the final tree.
The first four targets emit packets for fresh
agents. Dispatch each packet, validate the returned result, then run
`just record TARGET RESULT_FILE` before starting the next gate.
Replace the capitalized arguments with the target name and saved result path.

A worktree is a separate checkout that shares Git history with the primary
checkout, the original checkout from which it was created.

Independent writers use physical worktrees. Each needs an owner, absolute path, topic branch and explicit base.
Create a worktree with
`just worktree-new NAME BASE`, using an existing branch or commit for
`BASE`.
The recipe reads `.worktreelink` and links the existing ignored `.jlog`
folder from the primary checkout into the new worktree.
For an existing worktree, `just worktree-sync` applies those links.
The link does not install tools or share source edits.

For dependent branches, finish the prerequisite checkpoint before starting work
that requires it.

Delivery stays local. No remote, push, pull request or publication is selected.
Keep task records and review evidence local until a delivery destination is selected. Local commits
still require complete verification and independent review.

### Historical bootstrap context

The historical bootstrap checkpoint had no HEAD and used the primary branch
`gest/xnuqympy-bootstrap`. The branch `main` was the intended later integration target.

The initial runner work preserved the staged bootstrap, including the repository's
workflow and tool setup. Those instructions describe the initial checkpoint,
not the current commit or branch state.
No empty commit or coverage exclusion may replace a genuine checkpoint.

`just test-parent`, a dependency of `test-review`, runs added or changed tests
on the parent commit. The first checkout had no parent, so that step was
skipped. The skip does not show that the tests failed on earlier behavior.

## Decoder checkpoint and next integration

The pure decoder validates commands before it reserves output storage, then
replays checked slices. Its command reads both inputs and decodes before opening
the target. The preserved fixtures cover bounded application and command framing.
The five original C reference cases marked `not-run` keep that status.
The [decoder evidence section](delta-decoder.md#evidence-and-limits) describes
those observations and the portable fixtures.

The encoder was implemented after this decoder checkpoint. Its [guide](delta-encoder.md)
records the implementation and inherited `delta1` evidence. The candidate
uses an explicit wrapper to map Fossil diagnostics into `gossil vcs`.
The inherited assertions and original corpus remain unchanged.

## Unsettled extension contracts

The linked specification lists storage, Gest, sync and UI contract pages that
have not yet been written. Exact issue revisions, Git-link encodings, annotation descriptors and shared/private
replication rules need byte fixtures before durable extension writes begin.
Task `qvqyvqlt` tracks those missing contracts. Inspect it with
`gest task show qvqyvqlt` in the local Gest project. Do not invent storage or wire formats.

These rules remain unsettled.

The private/shared contract must settle dependency closure, transaction identity,
authorization, declassification and identical-byte visibility.

Dependency closure includes every object that a record requires and determines
which related objects must travel together.
Transaction identity determines how a transaction is identified across replicas.
Authorization controls access, while declassification permits private data to
become shared under explicit rules. Identical-byte visibility determines access when private and
shared objects contain the same bytes. Shared history
must restore without private objects. Unsupported required extensions must
fail before an operation reports complete success.

## Recommendations

Keep the full Gest task hierarchy and deliver small reviewed checkpoints.
Next, follow [Checkpoints and ownership](#checkpoints-and-ownership) to review
the composed core. Preserve the unchanged `delta1` qualification evidence.
Resolve the inventory's contract and configuration gaps while preserving
failures and disabled-feature results as classified evidence.
