# Get started

Create and restore a delta between two files with the current `gossil` command.
A delta describes target bytes using literal bytes and copies from an origin.
You do not need a Fossil repository for this example.

## Build from source

Gossil currently builds from source. Install Rust with Cargo, Git, Just and uv.
The repository's Just recipes use `jlog`, a command recorder. Install the pinned
version before running setup:

```sh
cargo install --locked --git https://github.com/rahuldave/jlog --rev 8febdf18387974f38047abbe88b7473bc5f27bb9 jlog
git clone https://github.com/rahuldave/gossil.git
cd gossil
just setup
```

Make sure Cargo's binary folder is on `PATH`. `just setup` fetches the Cargo
dependencies. See [contributor setup](development-plan.md#contributor-setup)
for the optional companion workflow packages.

## Create and restore bytes

Use separate output paths because successful writes replace existing files.
Run these commands from the repository root:

```sh
mkdir -p /tmp/gossil-example
printf 'before\n' > /tmp/gossil-example/origin
printf 'after\n' > /tmp/gossil-example/target
just gossil vcs test-delta-create /tmp/gossil-example/origin /tmp/gossil-example/target /tmp/gossil-example/change.delta
just gossil vcs test-delta-apply /tmp/gossil-example/origin /tmp/gossil-example/change.delta /tmp/gossil-example/restored
cmp /tmp/gossil-example/target /tmp/gossil-example/restored
just gossil vcs test-delta /tmp/gossil-example/origin /tmp/gossil-example/target
```

`cmp` exits with status 0 when the restored file matches the target.
The final command checks deltas in both directions and prints `ok`.
The create and apply commands write no stdout on success. Cargo may print build
messages on stderr.

The [CLI reference](cli.md) lists all current commands, argument order and
failure behavior. For calls within Rust, use the [application setup and library examples](examples.md).
An application must select the bundled libSQL provider in its root Cargo manifest.

## Recommendations

Use distinct files for inputs and outputs while learning the commands. Next,
repeat the example with your own files and read the limits in the delta guides.
