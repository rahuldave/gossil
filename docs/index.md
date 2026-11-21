---
title: Gossil
---

Gossil aims to keep agent work, issues and file history together alongside Git.
Fossil is a version-control system. This build exposes a Rust library for Fossil's delta bytes, artifact parsing and
local storage. The `gossil` command creates, applies and checks deltas between
files without a version-control checkout. It also initializes an empty
repository through [`vcs init`](cli.md#initialize-a-repository), without creating a checkout.
The source [`vcs open` command](cli.md#open-a-native-checkout) registers or reopens
one native checkout and publishes its lookup locator without selecting files.

The library also selects explicit path scopes and records a complete private
tracked tree while the shared root stays empty. Scope assigns a tracked path
to shared or owner-local private history. Shared recording remains planned.

Start with a [file round trip](getting-started.md), browse the
[CLI reference](cli.md), or use the [Rust API reference](api/index.qmd).
Use [working with repositories](working-with-repositories.md) to plan agent
files alongside Git. It explains checkouts, stored history and the intended
private and shared workflows.

For an already installed development preview, try
the [disposable file-history walkthrough](working-with-repositories.md#try-the-installed-development-preview).
The preview is unmerged.

Read [compare Gossil with Git](compare-with-git.md) for the planned coordination
of file history, exact issue state and versioned links to code.
The [examples](examples.md) show library calls for deltas, compressed bytes,
hashes and token escaping.

Gest is a task and artifact tracker. The converters in the module table use its task, project and author record model.

## Choose a library module

| Task | Guide |
|---|---|
| Create a delta from two byte slices | [Delta encoder](delta-encoder.md) |
| Apply a delta with a size limit | [Delta decoder](delta-decoder.md) |
| Compress bytes or compute a content hash | [Byte primitives](blob-primitives.md) |
| Compute a legacy artifact checksum | [Artifact checksums](artifact-checksum.md) |
| Escape bytes in Fossil tokens | [Token codec](token-codec.md) |
| Parse structural artifact records | [Artifact parser](artifact-parser.md) |
| Compare normalized byte lines | [Line diff](diff.md) |
| Own a local database transaction | [Local storage](local-storage.md) |
| Initialize a root, register a directory, select tracked scopes or record private files | [Native storage](native-storage.md) |
| Validate decoded task snapshots in memory | [Gest task values](gest-task-values.md) |
| Emit bounded task creation artifact bytes | [Task tickets](task-ticket.md) |
| Validate decoded project snapshots in memory | [Gest project values](gest-project-values.md) |
| Validate decoded author snapshots in memory | [Gest author values](gest-author-values.md) |

The [concepts guide](concepts.md) explains how these modules fit together.
Gossil is under development. General version-control commands, Gest task
management, a server and a user interface remain planned.
See [development](development-plan.md) for current limits and contribution steps.

## Tracking and private recording

The source [add and status commands](cli.md#add-explicit-paths-and-inspect-pending-additions)
enroll exact paths and list pending or recorded paths with their stored scope.
Add alone chooses neither shared nor private scope and stores no file bytes.

The [private recording library](native-storage.md#private-recording-library)
requires explicit private scope for every tracked path and captures all current
bytes and executable modes. Scope selection and recording currently require the library API.
[Development status](development-plan.md#private-recording-qualification) separates
this source contract from released and installed versions.

## Recommendations

Try the file round trip first. Next, choose a module guide and use the generated
API reference for its exact types, methods and errors.
