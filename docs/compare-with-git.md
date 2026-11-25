# Compare Gossil with Git

Gossil is designed to keep agent files, typed issues and their connections to
code in one coordinated history alongside Git. A historical view selects an
exact file head, complete issue and discussion boundaries, and versioned links
between issues and code. Git remains the source-code authority.

This page compares that intended design with Git's available behavior.
The source CLI initializes empty repositories without a checkout, registers or
reopens one native checkout and offers three delta diagnostics. It also supports
[explicit add and status](cli.md#add-explicit-paths-and-inspect-pending-additions)
without recording file content. Status distinguishes pending and recorded paths
and shows their actual scope.

The Rust library has typed converters for Gest tasks, projects, authors, artifacts,
iterations, tags, notes, relationships, entity tags and iteration tasks. It also has a
task artifact emitter and native storage primitives. It stores and reads private task and project
tickets, as the [issue ticket guide](issue-ticket.md) says.

[Native checkout open](cli.md#open-a-native-checkout) registers or reopens an
existing directory and publishes its lookup locator. The library can record
the complete private tracked tree. The source [scope and commit commands](cli.md#select-scopes-and-record-private-files)
expose that operation with explicit selections and metadata.

The [`issue` group](cli.md#issue-commands) recognises the names of the Gest commands. It runs
[`gos i init`](cli.md#create-the-gest-project-of-a-checkout), which creates the Gest project of an
open checkout. It also runs [`gos i task create`](cli.md#create-a-task), which adds a private task
to that project, and [`gos i task list` and `gos i task show`](cli.md#list-the-tasks-of-the-project),
which read its tasks. The other issue commands remain planned.
See the [current limits](development-plan.md) and
[CLI reference](cli.md) before trying a command.

## What each history records

In Git, the working tree contains the files you edit. The index is the staging
area for the next commit. Normally, a commit records the index as a file tree,
with parents and a message. You can stage selected changes before committing;
a commit need not include every edit on disk.
See the [Git tutorial](https://git-scm.com/docs/gittutorial) and
[commit reference](https://git-scm.com/docs/git-commit).

Git can version task documents, plans and evidence in that same tree.
Branches and commit history support many ways to organize the work.
Git notes can attach information to an object without changing that object;
notes have their own references and history.
See [Git notes](https://git-scm.com/docs/git-notes).

| Need | Git | Intended Gossil design |
|---|---|---|
| Record selected files | Commits retain trees selected through the index or commit options | Native file check-ins retain explicitly tracked agent files |
| Record a task | Version a task document or use a chosen tracker and integration | Typed project/task records have defined fields, identities and reconstruction rules |
| Select an earlier review state | Name commits and any separately required task or notes history | Select the file head plus complete exact boundaries for issues, links and discussions |
| Connect an issue to code | Adopt conventions or tools for references and reverse lookup | Version typed links to registered Git repositories and support lookup in both directions |
| Coordinate related changes | A commit can include code and task documents in one tree | One local database transaction coordinates selected Gossil files, records, scope and checkout heads |
| Keep private agent material | Choose a repository layout and sharing policy | Keep private and shared material within one repository, with an explicit internal boundary |

A standalone tracker beside Git or Fossil can also retain detailed history.
The extra requirement is to coordinate the exact states and dependencies that
belong to a review. Gossil defines that coordination as part of its repository
model, rather than leaving it to a convention between independent tools.
Fossil already has immutable ticket artifacts and file history.
The [repository guide](working-with-repositories.md#review-files-and-issues-together)
explains Gossil's specified additions.

## Recover the agent's decision

Suppose an agent revises `plan.md`, marks its task ready for review, and saves
a private trace of a failed approach. The implementation is in a separate Git
repository. In the planned Gossil workflow:

1. Review the plan diff, task changes and permitted evidence together.
2. Request the integrated operation with the expected file and issue revisions.
3. Record the Gossil artifacts, derived records, scope, local command history and checkout heads in one database transaction.
4. Retain the permitted file head and the complete issue boundaries for later review.

A file head identifies a file check-in. An issue boundary identifies its scope,
complete contributing artifact set and projection version. Artifacts are immutable
stored records; the projection version names the rules that reconstruct the
issue's values. Contributors include losing assignments even when a current screen displays
other values. Notes, links and discussions need their required
dependencies too.

If the agent later changes only the task's status, the file head stays the same.
The earlier issue boundary still selects the state reviewed with the plan.
Record-only changes create no file check-in. Later arrivals do not silently
change that selected historical set, subject to permission and byte availability.
A file hash alone cannot recover this context.

The code link names a registered Git repository, full commit, path and blob,
with a versioned selector when it names a range. The blob identifies file bytes.
From the issue, a reviewer can reach the pinned code; from that code version,
the reviewer can find the issue. A known filename or current checkout path
helps locate material, but neither supplies its repository identity or historical
version. Local paths can change without changing the registered repository's
identity. The design introduces no global file identity.

Uncommitted code needs a captured snapshot. An authorized capture can retain
bytes if the external Git checkout later disappears; an object reference alone
does not guarantee availability. The link descriptor encoding remains unfinished.
See the [repository guide](working-with-repositories.md#review-files-and-issues-together)
for the planned coordination and its limits.

The integrated transaction commits changes to the Gossil database. An external Git commit
remains a separate operation. Checkout files are written after database commit
through recoverable work, so filesystem recovery can still be necessary.
Ordinary issue edits do not automatically batch unrelated work.

## Select files and sharing separately

The planned Gossil checkout root can sit above several Git repositories.
Select `AGENTS.md` and chosen agent files inside those repositories explicitly;
initialization does not track every descendant. Gossil's selection is independent
of the nested Git indices and must preserve their branches and commits.
The [repository guide](working-with-repositories.md#choose-the-checkout-root-and-tracked-files)
shows how to choose this layout.

Git ignore rules help omit untracked files; they do not affect files already
tracked. Sparse checkout controls which tracked paths appear in the working
tree. Neither mechanism establishes a private/shared boundary for stored history.
See [gitignore](https://git-scm.com/docs/gitignore) and
[sparse checkout](https://git-scm.com/docs/git-sparse-checkout).

Git transfer can select references through refspecs, which map source references
to destinations. This selects history to fetch, not a confidentiality policy for
individual paths. Access rules depend on the chosen repository and service
configuration. See [Git fetch](https://git-scm.com/docs/git-fetch).

Gossil's chosen design keeps private and shared history in one local repository.
Shared history must be usable without private-only dependencies. Team replication,
sync and autosync remain planned; this design does not promise arbitrary partial
sharing within the shared portion today.

This page follows the nossaifd writing guide, including principles credited to
Rosalind Reid, Shreya Shankar and Oskar Austegard.

## Current private recording

The [private recorder](native-storage.md#private-recording-library) saves a
snapshot of every tracked file. Add enrolls paths rather than staging their bytes.
The recorder reads those paths again, so it includes edits made without re-adding.
Each path needs an explicit immutable private scope choice before recording.

This operation creates owner-local file history and leaves the shared empty root
unchanged. Shared recording and synchronization remain unsupported. The
[qualification record](development-plan.md#private-recording-qualification) separates
the checked library checkpoint from the CLI checkpoint still awaiting final gates.
The installed tracking release, version 0.5.0, includes neither scope selection
nor private recording.

The source library also reads [complete recorded ancestry](native-storage.md#read-recorded-checkout-ancestry)
through `recorded_timeline`. Its rows preserve full native hashes and exact
metadata bytes, current first and shared root last. The source
[timeline and log commands](cli.md#read-complete-recorded-ancestry) print those
rows as compact NDJSON with explicit selectors and an existing matching locator.
This bounded reader does not establish general Fossil timeline compatibility.

## Prepared glob utility

The [glob source](glob.md) matches bounded Fossil pattern lists and constructs
SQL expressions. It adds no ignore rules, filesystem traversal or wildcard
selection to add or status. Its [focused source checkpoint](glob.md#current-source-status)
passed with the selected 0.10.0 engine. Complete frozen-tree and delivery
qualification are separate.

## Recommendations

Keep Git for source work today. Next, try the [delta tutorial](getting-started.md)
and use the repository guide to select agent files.
For a future integrated review, retain the exact file head, complete issue
boundaries and versioned code links together.
