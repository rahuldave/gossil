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

# Load emitted Gest tickets into a scratch repository of the pinned Fossil reference; exit 5 means not-run.
gest-ticket-interop:
    #!/usr/bin/env bash
    set -euo pipefail
    fossil="${GOSSIL_FOSSIL_REFERENCE:-{{justfile_directory()}}/.jlog/bin/fossil}"
    if [ ! -x "$fossil" ]; then
        echo "not-run: the pinned Fossil reference is absent at $fossil; run just setup" >&2
        exit 5
    fi
    GOSSIL_FOSSIL_REFERENCE="$fossil" just test --test gest_ticket stock_fossil_reads_emitted_gest_tickets -- --ignored --exact

# Check a new repository, and one with check-ins and an issue, with the pinned Fossil reference: test-integrity, rebuild of a copy, timeline and ticket show, and a stock Fossil repository that verify and open refuse without changing a byte; exit 5 means not-run.
catalog-interop:
    #!/usr/bin/env bash
    set -euo pipefail
    fossil="${GOSSIL_FOSSIL_REFERENCE:-{{justfile_directory()}}/.jlog/bin/fossil}"
    if [ ! -x "$fossil" ]; then
        echo "not-run: the pinned Fossil reference is absent at $fossil; run just setup" >&2
        exit 5
    fi
    GOSSIL_FOSSIL_REFERENCE="$fossil" just test --test catalog_line stock_fossil_checks_a_new_catalog6_repository -- --ignored --exact
    GOSSIL_FOSSIL_REFERENCE="$fossil" just test --test issue_ticket stock_fossil_reads_check_ins_and_issues_together -- --ignored --exact

# Check stored private issue tickets, alone, beside check-ins and after gos i init, with the pinned Fossil reference: test-integrity, ticket show, timeline, and the rows of a ticket rebuild; exit 5 means not-run.
issue-ticket-interop:
    #!/usr/bin/env bash
    set -euo pipefail
    fossil="${GOSSIL_FOSSIL_REFERENCE:-{{justfile_directory()}}/.jlog/bin/fossil}"
    if [ ! -x "$fossil" ]; then
        echo "not-run: the pinned Fossil reference is absent at $fossil; run just setup" >&2
        exit 5
    fi
    GOSSIL_FOSSIL_REFERENCE="$fossil" just test --test issue_ticket stock_fossil_reads -- --ignored
    GOSSIL_FOSSIL_REFERENCE="$fossil" just test --test issue_init_cli stock_fossil_reads -- --ignored

# Run the demo of docs/issues.md, then read its repository with the pinned Fossil reference: test-integrity, timeline, ticket show, a refused rebuilt copy, and a push to a clone after which verify, commit and task create still pass; exit 5 means not-run.
issue-demo:
    #!/usr/bin/env bash
    set -euo pipefail
    fossil="${GOSSIL_FOSSIL_REFERENCE:-{{justfile_directory()}}/.jlog/bin/fossil}"
    if [ ! -x "$fossil" ]; then
        echo "not-run: the pinned Fossil reference is absent at $fossil; run just setup" >&2
        exit 5
    fi
    GOSSIL_FOSSIL_REFERENCE="$fossil" just test --test issue_demo stock_fossil_reads_the_demo_repository -- --ignored --exact

# Store 5,000 tasks in a release build and print the time of create, list, show, status, commit and verify at 100, 1,000 and 5,000 tasks.
issue-capacity:
    cargo test --release --test issue_capacity five_thousand_tasks_fit_the_cli_ceilings -- --ignored --exact --nocapture

# Compare the JSON of task list and task show with a reference gest for the same two tasks; exit 5 means not-run.
issue-read-oracle:
    #!/usr/bin/env bash
    set -euo pipefail
    gest="${GOSSIL_GEST_REFERENCE:-$(command -v gest || true)}"
    if [ -z "$gest" ] || [ ! -x "$gest" ]; then
        echo "not-run: no gest executable; set GOSSIL_GEST_REFERENCE or put gest on PATH" >&2
        exit 5
    fi
    GOSSIL_GEST_REFERENCE="$gest" cargo test --lib --locked -- --ignored --exact product::issue_read::tests::gest_reference_json_matches

# Respell the Gest ticket creation fixtures from their hand-written cards.
gest-ticket-reference:
    jlog run python3 tests/fixtures/gest-ticket/generate.py

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

# Verify the immutable wrapper source and unchanged sys/provider composition.
libsql-wrapper-check:
    jlog run python3 scripts/verify-libsql-wrapper.py

# Reject wrapper revision, feature, sys checksum and provider-manifest drift.
libsql-wrapper-test:
    jlog run python3 -m unittest discover -s tests -p 'test_libsql_wrapper.py' -v

# Record the actual resolved provider graph for composition audits.
libsql-wrapper-metadata:
    mkdir -p target
    jlog run cargo metadata --locked --all-features --format-version 1 > target/libsql-wrapper-metadata.json

# Retain the Rust profile type check after the mandatory vendor identity checks and the rustdoc check with warnings denied.
typecheck *args: libsql-vendor-check libsql-vendor-test libsql-wrapper-check libsql-wrapper-test rustdoc-check
    just --justfile {{quote(justfile())}} rust typecheck "$@"
    jlog step typecheck

# Run clippy on every target and fail on each warning; forbid the Git write paths of clippy.toml (GIT-03). Extra arguments go to cargo: just lint -p gossil
lint *args:
    jlog run cargo clippy --workspace --all-targets --all-features --locked "$@" -- -D warnings -F clippy::disallowed_methods -F clippy::disallowed_types
    jlog step lint

# Count the packages that the six gix crates of the Git reader add, from cargo tree on this host.
git-reader-packages:
    jlog run uv run --no-project --quiet python scripts/git-reader-packages.py

# Required CI verifies vendor bytes as well as the existing commit evidence gate.
ci: libsql-vendor-check libsql-vendor-test libsql-wrapper-check libsql-wrapper-test ci-gate

# Run the current product CLI without coverage instrumentation.
gossil *args:
    jlog run cargo run --locked --bin gossil -- "$@"

# Compile the complete adapted C provider without optional integer-header defines.
libsql-provider-compile:
    jlog run python3 -m unittest discover -s tests -p 'test_libsql_provider_compile.py' -v

# Run the finite date semantic controls through the actual bundled Rust provider.
libsql-provider-dates:
    jlog run cargo test --locked --lib storage::date_tests

# Build the rustdoc of the library and binaries, private items too, and fail on each warning, such as a broken intra-doc link.
# The step typecheck depends on it, so just check runs it without a project copy of the docs-check of the docs role.
rustdoc-check:
    RUSTDOCFLAGS="-D warnings" jlog run cargo doc --locked --all-features --no-deps --document-private-items
