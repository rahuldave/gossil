# Working with repositories

Use this guide to plan which agent files to track, how to review changes,
and how to connect tasks to exact file versions. Plan the history around a task,
its plan and the evidence behind a decision.
Git remains the source-code authority. Gossil is designed to give the
surrounding agent work its own history and sharing rules.

Most of this page describes planned repository workflows. The source CLI supports
[empty repository initialization and three file-delta diagnostics](cli.md).
Initialization creates no checkout and selects no files. The Rust library supports
[checkout-free initialization and bound private content](native-storage.md).
That storage foundation does not yet provide ordinary file check-ins or issue
management. The Fossil references teach the inherited workflow; their commands
are not Gossil aliases.

## Try the installed development preview

A separately installed development preview supports a small local file workflow.
This unmerged preview is not a release, and full qualification is incomplete.
Use a disposable folder and copies of files, and keep Git
as the source authority.

These commands require that preview's installed `gos` or `gossil` executable.
The source CLI shares the `init` spelling, but does not support the preview's
checkout or file-history commands. The generated Rust API on this site does not
describe the preview's additional storage methods.

Start with an empty scratch checkout. Keep the repository database outside it:

```sh
trial=$(mktemp -d /tmp/gossil-preview.XXXXXX)
mkdir -p "$trial/work/app/.agent" "$trial/work/service/.agent"
gos vcs init "$trial/history.gossil"
gos vcs open --repository "$trial/history.gossil" --checkout source --root "$trial/work"
printf 'first plan\n' > "$trial/work/app/.agent/plan.md"
printf 'first review\n' > "$trial/work/service/.agent/review.md"
gos vcs add --repository "$trial/history.gossil" --checkout source --path app/.agent/plan.md service/.agent/review.md
gos vcs status --repository "$trial/history.gossil" --checkout source --json
gos vcs commit --repository "$trial/history.gossil" --checkout source -m "Record agent files"
```

The folders can sit below sibling Git repositories. Gossil tracks the explicitly
named paths independently of Git's index. Here, `add` chooses paths for tracking;
it is not Git-style content staging for each later commit.

Edit a tracked file, inspect the change and commit again without re-adding it:

```sh
printf 'revised plan\n' > "$trial/work/app/.agent/plan.md"
gos vcs status --repository "$trial/history.gossil" --checkout source --json
gos vcs diff --repository "$trial/history.gossil" --checkout source --path app/.agent/plan.md
gossil vcs commit --repository "$trial/history.gossil" --checkout source -m "Revise the plan"
gos vcs timeline --repository "$trial/history.gossil" --checkout source --limit 10 --json
gossil vcs log --repository "$trial/history.gossil" --checkout source --limit 10 --json
gos vcs open --repository "$trial/history.gossil" --checkout source
```

A recording commit includes changed tracked paths; these examples do not request
a partial commit. `log` is the alias for `timeline`. Reopening an existing
checkout omits `--root`; it does not restore or replace working files.
No issue, project or author initialization is required for this file workflow.

The installed preview passed an isolated smoke test covering the initial empty
check-in, two later commits and reopen. This result does not establish release
readiness, branching, sync, automatic checkout discovery or full Fossil parity.
Issue commands and the server remain unavailable in the preview.

## Review files and issues together

In the planned workflow, an agent revises a tracked plan, updates its task and
retains a private trace. Review the file diff and task changes together before
requesting the integrated operation. The operation checks expected revisions
and commits artifacts, derived records, scope, local command history and Gossil
checkout heads together. Record-only work creates no file check-in.

The shared head selects shared files. An optional private combined head contains
those files with the same bytes and attributes, plus selected private files.
Each checkout stores its own head pair. Historical review chooses a permitted
file head and the exact contributing artifact sets for issues and discussions.

Two task-only edits can share one file head, so a check-in hash does not select
an issue revision. Complete references also retain required notes, link targets
and discussion dependencies. Later arrivals do not rewrite that historical set.

Typed issue-to-code links support navigation in both directions across registered
Git repositories. They pin commits, blobs and any selected ranges or changes;
current filenames and moving branch names cannot supply that guarantee.
Required captures preserve permitted target bytes when external code is absent.
A descriptor is a structured record naming the versioned target and any
selected range. Its transferable encoding remains unfinished; there is no
exchange format for applications to implement from this guide yet.

Fossil already has immutable ticket history and file check-ins. Gossil's added
contract coordinates exact record boundaries, shared/private file heads and
scoped links. Gest contributes the task workflow; one owned transaction
strengthens its multi-statement storage helpers. This coordinated operation is
planned and is not provided by the current delta diagnostics.

The transaction governs the Gossil database. Git commits remain separate source
operations, and checkout files are materialized after commit with recovery
bookkeeping. A failure then can require checkout recovery while the database
history remains committed. Ordinary commands do not automatically combine
unrelated file and issue work. See [library concepts](concepts.md#files-and-issues-in-one-operation)
for the shared model and [compare Gossil with Git](compare-with-git.md) for the
plan/task example and current availability.

## Separate stored history from the files you edit

A repository holds recorded history. A checkout is the folder of files you
edit at a selected point in that history. A check-in records a file tree,
its parent versions and a comment. A file you edit in the checkout has not
become a new check-in until you commit it. Fossil's
[concepts guide](https://fossil-scm.org/home/doc/trunk/www/concepts.wiki)
explains this separation.

An artifact is immutable content identified by a hash. A manifest records
which file paths and content hashes belong to a check-in. A delta encodes
bytes for storage or transfer. It is not itself a user-visible history entry,
so the [delta tutorial](getting-started.md) creates no repository history.

For Gossil, plan one local repository database with both private and shared
material. A team replica receives the shared portion. You do not need a
separate personal repository to keep private agent notes.
Team replication remains planned.

## Choose the checkout root and tracked files

Choose a root above all the agent files you want in one history. That root
may contain several Git repositories. For example, the intended selection
could look like this:

```text
workspace/                 Gossil checkout root
  AGENTS.md                selected by Gossil
  app/                     Git repository
    src/main.rs            selected by Git
    .agent/plan.md         selected by Gossil
  service/                 another Git repository
    .agent/review.md       selected by Gossil
```

The two systems choose files independently. A Git ignore rule for `.agent/`
does not select those files for Gossil, and Git's index does not define
Gossil's tracked set. The intended tracking model
also permits a Gossil root inside a Git checkout. Choose the root to fit
the paths you need. Gossil operations must preserve the nested Git indices,
branches and commits.

Initialization is intended to select no files automatically. Select files
explicitly, then inspect the pending selection before making a check-in.
Keep these states distinct:

| File state | Meaning |
|---|---|
| Tracked | Selected for version control; edits can enter later check-ins |
| Untracked | Present on disk but outside the selected file history |
| Ignored | Matches a rule that helps omit unmanaged paths from discovery or recursive addition |
| Missing | A tracked path is absent on disk; inspect whether removal was intended |

An ignore rule is a selection aid, not permission control. Fossil treats
explicit paths differently from recursive addition. Recursive addition normally
omits dotfiles, while explicitly named ignored files can require confirmation
or force. See its [add reference](https://fossil-scm.org/home/help?cmd=add).

Gossil's planned default ignore pattern is `.git,*/.git,.git/*,*/.git/*`.
It excludes Git metadata, including `.git` directories and Git worktree `.git` files. It must still be possible to see
what was selected. The preset does not protect private content by itself.

## Review a change before recording it

The intended daily file workflow follows Fossil:

1. Inspect status to identify edited, added, missing, removed and renamed paths.
2. Read the diff to inspect the actual changes in tracked files.
3. Add new paths, record intentional removals, and record renames.
4. Review status and diff again, then commit one coherent change with a comment.
5. Inspect history to find the resulting check-in and its parent versions.

In Fossil, [status](https://fossil-scm.org/home/help?cmd=status) describes pending
changes, while [diff](https://fossil-scm.org/home/help?cmd=diff) compares content.
Inspect untracked files separately. A quiet diff of tracked files does not
prove that every intended file was added. A
[commit](https://fossil-scm.org/home/help?cmd=commit) records the pending changes,
unless a file subset was requested. The
[timeline](https://fossil-scm.org/home/help?cmd=timeline) helps locate earlier work.

Removing a path from future versions does not erase its older history.
Also distinguish recording a removal or rename from changing the file on disk.
Fossil's [remove](https://fossil-scm.org/home/help?cmd=rm) and
[move](https://fossil-scm.org/home/help?cmd=mv) commands have options and settings
that control disk effects. Check those effects before using Fossil itself.
Gossil will need its own command reference when these operations ship.

## Use branches and review merges

A branch names a line of check-ins. A fork occurs when work produces competing
tips on the same branch. For a plan experiment, a named branch lets you keep
the experiment separate while retaining its starting point. See Fossil's
[branching guide](https://fossil-scm.org/home/doc/trunk/www/branching.wiki).

Merging combines changes in the checkout. Inspect the result, resolve conflicts
and test the affected work before committing the merge. Fossil's
[merge reference](https://fossil-scm.org/home/help?cmd=merge) makes that separate
commit step explicit. Downloading another check-in and merging it into your
files are different decisions. Gossil's branches and merge operations remain
planned; use your existing Git workflow for source-code work now.

## Distinguish sharing history from updating a checkout

Fossil gives four useful names to repository transfer operations:

| Operation | Purpose in Fossil |
|---|---|
| Clone | Create a local repository from an existing one |
| Pull | Receive remote history into the local repository |
| Push | Send local history to another repository |
| Sync | Exchange history in both directions |

A clone and an open checkout are distinct operations, even where Fossil offers
to do both together. A pull receives history. Updating the checkout changes
the files you work on. See the
[clone reference](https://fossil-scm.org/home/help?cmd=clone) and
[sharing section](https://fossil-scm.org/home/doc/trunk/www/quickstart.wiki).

Fossil's [autosync setting](https://fossil-scm.org/home/help?cmd=autosync)
can arrange transfers around commands such as commit and update. Know which
remote and mode you are using before assuming a local action was shared.
Gossil plans to retain replication and autosync workflows, but none of these
transfer operations is currently a Gossil command.

Gossil adds an explicit private/shared boundary. The planned local write
can record a shared issue change and private notes in one database transaction.
Either both changes commit or both roll back.

Shared history
must remain usable without private dependencies. To publish shared material,
it is not enough to omit some paths during transfer. The planned synchronization
rules define that boundary. Neither an ignore pattern nor Fossil's private-transfer
options establish that Gossil contract.

## Connect tasks to exact file versions

A task describes the work, and a file check-in records selected files. Updating
an issue does not have to create a new file check-in. Fossil stores ticket
changes as separate artifacts, as its
[ticket guide](https://fossil-scm.org/home/doc/trunk/www/tickets.wiki) explains.
Gest is a task and artifact tracker. Gossil adapts this history model to
Gest's tasks, projects and related records.

When you record why a change was made, name the exact version. The planned
Gossil file reference contains a check-in, relative path and content hash.
A Git file reference names its registered repository, full commit, path and
blob. A moving branch name alone cannot identify the version an agent read.
Uncommitted content needs a captured snapshot if the reference must survive
later edits.

Exact version references are part of the planned issue and
annotation model. A Git object reference also does not guarantee that a
recipient has its bytes.

The current [task](gest-task-values.md) and [project](gest-project-values.md)
converters validate decoded records in memory. They do not save an issue or
create a file reference. Their generated API links give the callable surface.

Notebook support remains planned. A notebook is a selected directory and its
tracked descendants. It is intended to use selected native file history. SQLAR packages, which are SQLite archives of files, are for interchange
rather than a second history system.

## Restore files and undo operations

Before discarding work, inspect the affected paths and keep any unsaved
material you still need. Restoring files, backing out a recorded change and
undoing a task command have different effects.

In Fossil, [revert](https://fossil-scm.org/home/help?cmd=revert) restores files
to the checkout baseline or a requested revision. With no file argument it
acts on all files. [Undo](https://fossil-scm.org/home/help?cmd=undo) can reverse
certain recent working-tree operations, including update, merge and revert.
It has one level; commit and checkout clear it. Its dry-run option shows
what it would restore. It is not a general backup or an undo for every command.

Gossil's intended undo for Gest command history is separate from undo for
operations on checkout files. General recovery commands are not available in the current
CLI. For today's delta diagnostics, preserve the inputs and write to a new
output path. Compare the restored bytes before replacing a file.

## Recommendations

Keep Git as the source authority and plan a small explicit selection of agent
files for Gossil. Next, try the current delta tutorial, or use disposable files
for the installed preview above. Use this workflow guide to choose roots and
scope before checkout and file-history commands ship in the source CLI. For integrated
review, retain the file head and complete issue boundaries together.
