# Get started

Use Gossil to create a delta between two files and restore the target exactly.
A delta encodes target bytes as new bytes and copies from an origin file.
This exercise needs no repository and does not change Git history.

Gossil is intended to keep agent files, issues and their history alongside
Git. Its command line initializes and opens repositories, tracks explicit files
and has three delta diagnostics. The Rust library also supports private file
recording, artifact parsing, local storage and task, project and author conversion. Try the delta commands below. Read
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
Issue, file-recording and sync commands remain planned.

## Handle failures and choose the next step

A diagnostic exits with status 1 and writes an error to stderr when it fails.
Check the input paths and argument order first. If applying a delta fails,
check that you kept the correct origin and an intact delta. The
[CLI reference](cli.md#exit-status-and-errors) lists the exact messages.

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
`ROOT/.gossil/checkout.json`, so commands can find and verify the checkout.
It selects no files and requires no issue, project or author initialization.
Configuration, permissions and existing markers can refuse the operation.
Read [native checkout open](cli.md#open-a-native-checkout) before retrying a
failure, because database commit and locator publication have distinct outcomes.

## Track explicit paths

Use [add and status](cli.md#add-explicit-paths-and-inspect-pending-additions)
to enroll exact file paths. Add stores tracking metadata without changing working
files or storing file content. Status lists every tracked path with its pending
or recorded state and actual scope. New paths have unselected scope.

For application code, the [private recording library](native-storage.md#private-recording-library)
selects explicit immutable scopes and records the complete private tracked tree.
That API is separate from the current command set. Scope and commit commands
remain deferred; the [development guide](development-plan.md#private-recording-qualification)
records the qualification boundary.

## Recommendations

Use the scratch-file round trip first. Next, repeat it with copies of your
own files and use distinct output paths before integrating the library.
