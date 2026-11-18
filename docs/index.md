---
title: Gossil development
---

Gossil is planned as a Rust library and application for versioned agentic
project state, combining Fossil version control with Gest task management.
Planned records include tasks, shared issues and private agent traces.
Git remains the source-code authority in the first release.

This checkout contains a bounded Fossil delta decoder, its
`gossil vcs test-delta-apply` diagnostic command and a runner for inherited Fossil Tcl tests.
Gest integration, libsql storage and the remaining Fossil mechanisms are planned work.

- [Delta decoder](delta-decoder.md) explains the library, diagnostic command,
  preserved fixtures and compatibility limits.
- [Inherited-test runner](conformance.md) explains prerequisites, configuration,
  evidence and acceptance limits.
- [Development plan](development-plan.md) gives the delivery order, ownership
  and local checkpoint workflow.

The product specification remains at
`~/Projects/AIProjectInfra/docs/gossil/README.md`. Reference experiment files
are described by `evidence/baselines/README.md` at the repository root.
The complete source and test mappings are described in
`evidence/inventories/README.md`, including the current Fossil family corrections
and complete pinned Git identities. The parser correction and its focused
regression evidence are recorded in `evidence/review-correction/checkpoint.json`.

These archives are outside this documentation site. Open their paths from
the root of the local checkout. Those historical results
remain separate from runner controls and future Gossil compatibility.

The local commands require Rust with Cargo, Git, Just and uv on `PATH`.
See the runner guide for Tcl, Fossil and fresh-clone setup limits.
Use `just docs-check` to check site links and API documentation, and
`just docs-build` to render the site. `just docs` serves a local preview.
Automatic publication is disabled for this checkout.

## Recommendations

Use the decoder guide for bounded delta application and the runner guide for
inherited suites. Next, implement the encoder and `test-delta` diagnostics,
then qualify `delta1`, the unchanged inherited Fossil delta test suite.
