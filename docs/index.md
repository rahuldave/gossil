---
title: Gossil
---

Gossil aims to keep agent work, issues and file history together alongside Git.
Fossil is a version-control system. This build exposes a Rust library for its delta bytes, artifact parsing and
local storage. The `gossil` command creates, applies and checks deltas between
files without a version-control checkout.

Start with a [file round trip](getting-started.md), browse the
[CLI reference](cli.md), or use the [Rust API reference](api/index.qmd).
Use [working with repositories](working-with-repositories.md) to plan agent
files alongside Git. It explains checkouts, stored history and the intended
private and shared workflows.
Read [compare Gossil with Git](compare-with-git.md) for the planned coordination
of file history, exact issue state and versioned links to code.
The [examples](examples.md) show library calls for deltas, compressed bytes,
hashes and token escaping.

Gest is a task and artifact tracker. The converters below use its task and project record model.

## Choose a library module

| Task | Guide |
|---|---|
| Create a delta from two byte slices | [Delta encoder](delta-encoder.md) |
| Apply a delta with a size limit | [Delta decoder](delta-decoder.md) |
| Compress bytes or compute a content hash | [Byte primitives](blob-primitives.md) |
| Compute a legacy artifact checksum | [Artifact checksums](artifact-checksum.md) |
| Escape bytes in Fossil tokens | [Token codec](token-codec.md) |
| Parse structural artifact records | [Artifact parser](artifact-parser.md) |
| Own a local database transaction | [Local storage](local-storage.md) |
| Initialize an empty root or store private bytes | [Native storage](native-storage.md) |
| Validate decoded task snapshots in memory | [Gest task values](gest-task-values.md) |
| Emit bounded task creation artifact bytes | [Task tickets](task-ticket.md) |
| Validate decoded project snapshots in memory | [Gest project values](gest-project-values.md) |

The [concepts guide](concepts.md) explains how these modules fit together.
Gossil is under development. General version-control commands, Gest task
management, a server and a user interface remain planned.
See [development](development-plan.md) for current limits and contribution steps.

## Recommendations

Try the file round trip first. Next, choose a module guide and use the generated
API reference for its exact types, methods and errors.
