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

# Execute one explicit reference or candidate configuration; failed suites exit nonzero.
conformance CONFIG:
    cargo run --locked --bin gossil-conformance -- "$1"

# Verify runner controls, including deliberate failures, timeouts and isolated child cleanup.
conformance-self-test: integration
