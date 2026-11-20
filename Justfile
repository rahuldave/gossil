# gossil: every task is a recipe here. `just` lists them.
# Started by jnterfaces. A recipe below overrides a recipe of just install with the same name.
# just install adds the recipes of each tool: just install lint adds a tool, and just uninstall lint takes it out.
import? '.agents/install.just'

# Keep one package directory for the primary checkout and its worktrees.
export SKILL_PACKAGES_DIR := env("SKILL_PACKAGES_DIR", env("HOME") / "Projects")

# One run ID for this just call and each just call inside it, for the run log of jlog. Keep it in this file only
export JLOG_RUN := env("JLOG_RUN", uuid())

# Each word after a recipe's name reaches its commands whole, as "$1", "$2" and so on; "$@" is all of them
set positional-arguments

# A recipe of this file with the name of a recipe of just install, such as lint, takes its place. A step of just
# check, such as lint, ends with the line: jlog step lint
set allow-duplicate-recipes

[private]
default:
    @just --list --unsorted --list-submodules

# Supply pinned native test references, then fetch the Rust dependencies.
setup: _reference-tools rust::setup

[private]
_reference-tools:
    jlog run python3 scripts/setup-reference-tools.py

[private]
_reference-tools-tests:
    jlog run python3 tests/test_reference_tools.py -v

# Keep native setup security controls in the generic test contract.
test *args: _reference-tools-tests
    just --justfile {{quote(justfile())}} rust test "$@"
    jlog step test

# Execute one explicit reference or candidate configuration; failed suites exit nonzero.
conformance CONFIG:
    cargo run --locked --bin gossil-conformance -- "$1"

# Verify runner controls, including deliberate failures, timeouts and isolated child cleanup.
conformance-self-test: integration

# Generate a complete delta1 invocation plan using the selected Tcl file ordering.
delta1-plan SOURCE OUTPUT:
    jlog run uv run --no-project --quiet python scripts/delta1-plan.py "$1" "$2" --tcl "$(pwd)/.jlog/bin/tclsh" --revision b3a1c8efb2731dffbd5cc38c58ac349746de6da6
# Resolve dependencies for the composed library.
blob-lock:
    jlog run cargo generate-lockfile

# Regenerate the finite pinned Fossil blob and digest reference corpus.
blob-reference:
    jlog run python3 tests/fixtures/blob/generate.py

# Check Rust-packed streams with the explicit locally installed Fossil reference.
blob-interop:
    GOSSIL_FOSSIL_REFERENCE="{{justfile_directory()}}/.jlog/bin/fossil" just test --test blob_primitives rust_packed_bytes_decode_with_native_fossil -- --ignored

# Inspect Rust dependers of the new byte primitive contracts.
blob-impact:
    jlog run ast-grep run --lang rust --pattern 'pack($$$)' --json=compact src tests
    jlog run ast-grep run --lang rust --pattern 'pack_two($$$)' --json=compact src tests
    jlog run ast-grep run --lang rust --pattern 'unpack($$$)' --json=compact src tests
    jlog run ast-grep run --lang rust --pattern 'sha1($$$)' --json=compact src tests || test "$?" -eq 1
    jlog run ast-grep run --lang rust --pattern 'sha3($$$)' --json=compact src tests || test "$?" -eq 1
    # Rust macro token trees also need literal searches for calls inside assertions.
    jlog run rg -n 'pack\(|pack_two\(|unpack\(|sha1\(|sha3\(' src tests/blob_primitives.rs

# Run stable Rust examples and compile-fail ownership contracts without coverage instrumentation.
doctests:
    jlog run cargo test --locked --doc

# Exercise real delta, packed bytes and hashes through private local transactions.
core-composition:
    just test --lib storage::composition_tests

# Verify the complete reviewed source tree and provenance before every required type check.
libsql-vendor-check:
    jlog run python3 scripts/verify-libsql-vendor.py

# Exercise vendor tampering, index consistency and maintenance regeneration controls.
libsql-vendor-test:
    jlog run python3 -m unittest discover -s tests -p 'test_libsql_vendor.py' -v

# Retain the Rust profile type check after the mandatory vendor identity check.
typecheck *args: libsql-vendor-check libsql-vendor-test
    just --justfile {{quote(justfile())}} rust typecheck "$@"
    jlog step typecheck

# Required CI verifies vendor bytes as well as the existing commit evidence gate.
ci: libsql-vendor-check libsql-vendor-test ci-gate

# Run the current product CLI without coverage instrumentation.
gossil *args:
    jlog run cargo run --locked --bin gossil -- "$@"

# Compile the complete adapted C provider without optional integer-header defines.
libsql-provider-compile:
    jlog run python3 -m unittest discover -s tests -p 'test_libsql_provider_compile.py' -v

# Run the finite date semantic controls through the actual bundled Rust provider.
libsql-provider-dates:
    jlog run cargo test --locked --lib storage::date_tests
