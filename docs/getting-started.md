# Get started

Use Gossil to create a delta between two files and restore the target exactly.
A delta encodes target bytes as new bytes and copies from an origin file.
This exercise needs no repository and does not change Git history.

Gossil is intended to keep agent files, issues and their history alongside
Git. Its command line initializes and opens repositories, tracks explicit files,
selects their scopes, records private history and has three delta diagnostics.
The Rust library also supports artifact parsing, local storage and task, project
and author conversion. Try the delta commands below. Read
[working with repositories](working-with-repositories.md) for the intended
file and issue workflows.

## Prepare a source checkout

Install Git, Rust with Cargo, Just, uv and the
[`jlog` command recorder](https://github.com/rahuldave/jlog#readme).
These tools must be available on `PATH`. Rust builds also need the native
compiler and platform development tools used by Cargo dependencies.
Linux setup also requires Make, zlib development headers and CA certificates
to build the pinned Tcl and Fossil references. On other hosts, supply the
[reference prerequisites](conformance.md#local-prerequisites) before setup.
The commands below use a POSIX shell, such as the default shell on macOS or Linux.

```sh
git clone https://github.com/rahuldave/gossil.git
cd gossil
just setup
```

`just setup` supplies the Linux references or checks the existing tools on other
hosts, then fetches the Cargo dependencies.
`just gossil` then builds and runs the source executable through `jlog` and
Cargo. You do not need a globally installed `gossil` executable.
The first build can take longer than later calls.

The example builds from source. Keep the checkout open in
this shell for the following commands. The
[contributor setup](development-plan.md#contributor-setup) is for people who
also need the optional workflow packages and verification guidance.

## Create a delta

Make a fresh scratch folder so that the example cannot overwrite earlier work.
Keep the quotation marks around paths so that paths with spaces work too.

```sh
example_dir=$(mktemp -d /tmp/gossil-example.XXXXXX)
printf 'before\n' > "$example_dir/origin"
printf 'after\n' > "$example_dir/target"
just gossil vcs test-delta-create "$example_dir/origin" "$example_dir/target" "$example_dir/change.delta"
```

The arguments are the original file, the desired file, and the delta to write.
Success writes `change.delta` and prints no command output. Cargo can print
build messages on stderr. A delta need not be smaller than its target,
especially for small files like these.

## Restore and check the result

Apply the delta to the same origin, writing a separate restored file:

```sh
just gossil vcs test-delta-apply "$example_dir/origin" "$example_dir/change.delta" "$example_dir/restored"
cmp "$example_dir/target" "$example_dir/restored"
cat "$example_dir/restored"
```

`cmp` prints nothing and exits with status 0 when the files match.
`cat` prints `after`. The original and target files remain available.
To check that Gossil can create and apply deltas in both directions, run:

```sh
just gossil vcs test-delta "$example_dir/origin" "$example_dir/target"
```

Success prints `ok`. This command checks the round trip in memory. It does not
save another delta or create a check-in. You can repeat these commands with
text or binary files. A delta describes bytes, so keep its matching origin
when you need to restore the target later.

## Use the short group name

`v` is the short name for `vcs`:

```sh
just gossil v test-delta "$example_dir/origin" "$example_dir/target"
```

The source also builds a `gos` executable with the same diagnostics. See
[short executable names](cli.md#short-names) for its source invocation.
The same aliases support [native checkout open](cli.md#open-a-native-checkout).
Issue commands, shared recording and sync remain planned.

## Handle failures and choose the next step

A diagnostic exits with status 1 and writes an error to stderr when it fails.
Check the input paths and argument order first. If applying a delta fails,
check that you kept the correct origin and an intact delta. The
[CLI reference](cli.md#exit-status-and-errors) lists the exact messages.
An unknown command exits with status 2. Run `just gossil --help` to list
the commands, or add `--help` after a command to print its usage.

Successful create and apply operations replace an existing output file.
Use a fresh output path until you have checked the result. Validation failures
preserve the output, but a write failure can leave it truncated.

The [repository guide](working-with-repositories.md) explains how a file delta
differs from a check-in and how Gossil is intended to work beside Git.
For application code, use the [library examples](examples.md) and
[generated Rust API](api/index.qmd). The application manifest must select
Gossil's bundled libSQL provider as shown in those examples. The provider implements
the Fossil date arithmetic required by artifact parsing and repository validation.

## Create an empty repository

Choose a destination that does not exist:

```sh
just gossil vcs init --admin-user alice example.gossil
```

The receipt gives the project and server identifiers and the initial administrator
password. Save the password privately. This command creates the empty root without
a checkout or tracked files. It reads no preference file. See the
[initialization reference](cli.md#initialize-a-repository) for date, hash and login options.
An already installed mini-VCS preview has a separate
[command contract](working-with-repositories.md#try-the-installed-development-preview);
these commands build the current source.

## Register an existing directory

After the initialization above, create a directory and register it from source:

```sh
mkdir example-work
just gossil vcs open --repository example.gossil --checkout work --root example-work
```

Open registers the initial empty root and writes a local lookup file,
`example-work/.gossil/checkout.json`. Commands use that file to find and verify the checkout.
It selects no files and requires no issue, project or author initialization.
Configuration, permissions and existing markers can refuse the operation.
Read [native checkout open](cli.md#open-a-native-checkout) before retrying a
failure, because database commit and locator publication have distinct outcomes.

## Track explicit paths

Use [add and status](cli.md#add-explicit-paths-and-inspect-pending-additions)
to enroll exact file paths. Add stores tracking metadata without changing working
files or storing file content. Status lists every tracked path with its pending
or recorded state and actual scope. New paths have unselected scope.

The [scope and commit commands](cli.md#select-scopes-and-record-private-files)
select immutable scopes and record the complete private tracked tree. The
[development guide](development-plan.md#private-recording-qualification) records the
qualification boundary between source, release and installation.

## Record private files from source

Start in the Gossil source repository root after setup. This example runs `gos` through Cargo
and keeps its database outside the working root. It uses absolute paths, so
later commands can run inside the registered checkout.
Existing Gest configuration must satisfy the [open contract](cli.md#configuration-before-mutation).

```sh
recording_source=$(pwd -P)
recording_trial=$(mktemp -d /tmp/gossil-recording.XXXXXX)
recording_gos() {
  cargo run --locked --manifest-path "$recording_source/Cargo.toml" --bin gos -- "$@"
}
mkdir "$recording_trial/work"
recording_gos v init --admin-user alice --date-override '2026-11-20 00:00:00' "$recording_trial/history.gossil"
recording_gos v open --repository "$recording_trial/history.gossil" --checkout work --root "$recording_trial/work"
cd "$recording_trial/work"
printf 'first plan\n' > plan.md
printf 'first review\n' > review.md
recording_gos v add --repository "$recording_trial/history.gossil" --checkout work --path plan.md --path review.md
recording_gos v scope --repository "$recording_trial/history.gossil" --checkout work --scope private --path plan.md --path review.md
recording_gos v commit --repository "$recording_trial/history.gossil" --checkout work --scope private --all-tracked --user alice --comment 'Save notes' --date-override '2026-11-21T12:00:00'
printf 'revised plan\n' > plan.md
recording_gos v commit --repository "$recording_trial/history.gossil" --checkout work --scope private --all-tracked --user alice --comment 'Revise plan' --date-override '2026-11-22T12:00:00'
recording_gos v open --repository "$recording_trial/history.gossil" --checkout work
recording_gos v status --repository "$recording_trial/history.gossil" --checkout work
recording_gos v commit --repository "$recording_trial/history.gossil" --checkout work --scope private --all-tracked --user alice --comment 'Check unchanged tree' --date-override '2026-11-23T12:00:00'
```

Scope prints `Selected scopes: 2`. Each changed commit prints
`Recorded private check-in: HASH`, where `HASH` identifies that check-in.
The final commit prints `No changes`. Each output ends with a newline.
Status lists both paths as `R` with private scope. It does not report live edits.

The second commit captures the edit without re-adding the file. Reopen preserves
the files and private history; the shared head still names the empty root.
These dates follow the deterministic root and each previous parent. Use later
valid dates for subsequent commits, including no-change checks. Comments must
not contain `[`, even when the tree is unchanged.

A nonzero exit after an output failure can leave a durable commit. Retry the
same binding with unchanged files and valid later metadata; a previously recorded
tree yields `No changes`. See the [failure contract](cli.md#grammar-limits-and-failures).
This walkthrough describes current source and does not install a default executable.

## Inspect the recorded identity

From the source walkthrough checkout above, use its `recording_gos` helper:

```sh
recording_gos vcs info --repository "$recording_trial/history.gossil" --checkout work
```

The command prints the supplied repository path and canonical checkout root as
JSON strings, then the current checkout, shared head and private head.
Before a private recording, the private head is `none` and checkout equals the
shared head. After a private recording, checkout equals the private combined head.
Edits without a commit do not change these recorded IDs. See the
[exact info contract](cli.md#inspect-current-recorded-checkout-identity) for output and failure rules.

## Read the recorded ancestry

From the source walkthrough checkout, run:

```sh
recording_gos vcs timeline --repository "$recording_trial/history.gossil" --checkout work
recording_gos v log --checkout work --repository "$recording_trial/history.gossil"
```

Both commands print the same three compact JSON rows for the revised private
check-in, the first private check-in and the shared empty root, in that order. Each row ends
with LF and contains `hash`, `date`, `actor`, `comment` and `private`, in that
order. The no-change retry added no row. Hashes are complete, and the original
metadata is preserved with standard JSON escaping. The root alone has
`private: false`.

The commands need a complete matching locator. They do not read working-file
contents, select revisions or accept filters. See the
[ancestry command contract](cli.md#read-complete-recorded-ancestry) for bounds
and failures. This walkthrough builds source; it supplies no installation evidence.

## Inspect a glob list from source

The diagnostic accepts literal pattern lists and candidates without a
repository. The [2026-11-21 focused checkpoint](glob.md#current-source-status)
built and exercised the source interface. Its full qualification limits are
separate from this example.

Return to the Gossil source root before running this command.

```sh
just gossil vcs test-glob '*.rs,src/*' src/lib.rs notes.md
```

The shell quotes keep the pattern list in one argument. The diagnostic prints
SQL, parsed patterns and both match indices. It selects no tracked files.
Read the [CLI grammar](cli.md#inspect-a-fossil-glob-list) and
[glob contract](glob.md) before using it. The default installed development
preview is a separate build; this source example does not update it.

## Recommendations

Use disposable copies for the source private recording walkthrough. Next,
inspect status, edit a tracked copy and commit again with a later explicit date.
