---
title: Gossil development
---

Gossil is planned as a Rust library and application for versioned agentic
project state. It combines Fossil version control with Gest task management.
Fossil is a distributed version-control system. Gest tracks issues and tasks.
Planned records include tasks, shared issues and private agent traces.
Git remains the source-code authority in the first release.

This checkout contains a bounded Fossil delta encoder and decoder, their
`gossil vcs` diagnostics and a runner for inherited Fossil Tcl tests.
It also composes byte primitives, legacy artifact checksums, token byte escaping
and owned local libSQL transactions. Gest integration, domain storage and the remaining Fossil mechanisms
remain planned work.

- [Delta encoder](delta-encoder.md) explains creation, round-trip diagnostics
  and the finite compatibility evidence.
- [Delta decoder](delta-decoder.md) explains the library, diagnostic command,
  preserved fixtures and compatibility limits.
- [Inherited-test runner](conformance.md) explains prerequisites, configuration,
  evidence and acceptance limits.
- [Byte digests and packed blobs](blob-primitives.md) describes SHA1DC safe
  hashes, thirteen SHA3 widths, decoding limits and native interoperability.
- [Artifact byte checksums](artifact-checksum.md) explains bounded MD5 bytes,
  owned incremental computations and finite reference checks.
- [Token byte escaping](token-codec.md) describes bounded byte and C-string
  behavior, fallible allocation and the finite native observations.
- [Local storage](local-storage.md) describes database lifecycle, exclusive
  transactions and the private fixture that composes storage with byte primitives.
- [libSQL source import boundary](libsql-import-boundary.md) records exact upstream
  classifications and the controls required before importing the provider.
- [Native date provider](libsql-provider.md) describes the pinned source,
  integrity checks and limits of the private date regression.
- [Development plan](development-plan.md) gives the delivery order, ownership
  and local checkpoint workflow.

The product specification is in a local companion checkout at
`~/Projects/AIProjectInfra/docs/gossil/README.md`. Reference experiment files
are described by `evidence/baselines/README.md` at the repository root.
The complete source and test mappings are described in
`evidence/inventories/README.md`, including corrections to groups of related Fossil mechanisms and Git
identities for every pinned path. `evidence/review-correction/checkpoint.json`
records tests of rejection of malformed and duplicate result summaries.

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
inherited suites. Next, follow the [review workflow](development-plan.md#checkpoints-and-ownership)
for the library's delta, byte, token and storage modules together.
Retain the recorded qualification of `delta1`, the unchanged inherited Fossil delta test suite.
