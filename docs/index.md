---
title: Gossil development
---

Gossil is planned as a Rust library and application for versioned agentic
project state, combining Fossil version control with Gest task management.
Planned records include tasks, shared issues and private agent traces.
Git remains the source-code authority in the first release.

This checkout currently contains a Rust runner for inherited Fossil Tcl tests.
The engine port, Gest integration and libsql storage remain planned work.

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

To run tests, start with the runner guide and keep executed controls separate
from inherited compatibility. To continue development, complete the independent
reviews and final combined check described in the development plan.
