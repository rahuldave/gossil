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

The CLI and library also select explicit path scopes and record a complete private
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

Gest is a task and artifact tracker. The converters in the module table use its record model for tasks, projects, authors, artifacts, iterations, tags, notes, relationships, entity tags and iteration tasks.

Use [`vcs info`](cli.md#inspect-current-recorded-checkout-identity) to show the
current recorded checkout and its shared and private heads. Explicit repository
and checkout selections and a complete locator are required. The source
[`vcs timeline` and `vcs log` commands](cli.md#read-complete-recorded-ancestry)
print complete recorded ancestry as compact NDJSON. Historical revision selection
remains unsupported. The library can read
[complete recorded ancestry](native-storage.md#read-recorded-checkout-ancestry)
through `recorded_timeline`, with native metadata bytes and the shared root last.

## Choose a library module

| Task | Guide |
|---|---|
| Create a delta from two byte slices | [Delta encoder](delta-encoder.md) |
| Apply a delta with a size limit | [Delta decoder](delta-decoder.md) |
| Compress bytes or compute a content hash | [Byte primitives](blob-primitives.md) |
| Compute a legacy artifact checksum | [Artifact checksums](artifact-checksum.md) |
| Escape bytes in Fossil tokens | [Token codec](token-codec.md) |
| Match bounded Fossil pattern lists or construct SQL expressions | [Glob matching](glob.md) |
| Simplify filename bytes by Fossil's lexical rules | [Lexical file paths](file-path.md) |
| Find a shortest path or common ancestor in a check-in graph | [Check-in graph paths](history.md) |
| Parse structural artifact records | [Artifact parser](artifact-parser.md) |
| Compare normalized byte lines | [Line diff](diff.md) |
| Own a local database transaction | [Local storage](local-storage.md) |
| Initialize a root, register a directory, record private files or read recorded ancestry | [Native storage](native-storage.md) |
| Decode strict local JSON records and charge queries before their rows | [Strict local records](local-records.md) |
| Validate decoded task snapshots in memory | [Gest task values](gest-task-values.md) |
| Emit bounded task creation artifact bytes | [Task tickets](task-ticket.md) |
| Validate decoded project snapshots in memory | [Gest project values](gest-project-values.md) |
| Validate decoded author snapshots in memory | [Gest author values](gest-author-values.md) |
| Validate decoded artifact snapshots in memory | [Gest artifact values](gest-artifact-values.md) |
| Validate decoded iteration snapshots in memory | [Gest iteration values](gest-iteration-values.md) |
| Validate decoded tag snapshots in memory | [Gest tag values](gest-tag-values.md) |
| Validate decoded note snapshots in memory | [Gest note values](gest-note-values.md) |
| Validate decoded relationship snapshots in memory | [Gest relationship values](gest-relationship-values.md) |
| Validate decoded entity tag snapshots in memory | [Gest entity tag values](gest-entity-tag-values.md) |
| Validate decoded iteration task snapshots in memory | [Gest iteration task values](gest-iteration-task-values.md) |

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
bytes and executable modes. The source [scope and commit commands](cli.md#select-scopes-and-record-private-files)
require explicit repository, checkout, scope and recording metadata. Try the
[private recording walkthrough](getting-started.md#record-private-files-from-source).
[Development status](development-plan.md#private-recording-qualification) separates
this source contract from released and installed versions.

## Glob source checkpoint

The [glob module](glob.md) and [`vcs test-glob` diagnostic](cli.md#inspect-a-fossil-glob-list)
operate without storage. The [2026-11-21 focused checkpoint](glob.md#current-source-status)
passed with the selected libSQL 0.10.0 engine. It does not establish complete
frozen-tree gates or release qualification. Historical 0.9.30 results remain
separate. File tracking continues to use exact paths.

## Recommendations

Try the file round trip first. Next, choose a module guide and use the generated
API reference for its exact types, methods and errors.
