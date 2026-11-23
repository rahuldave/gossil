# Written by just install. Do not edit: the next just install writes it again.
# The recipes of the profiles and roles of this project. .agents/install.just imports this file.

# Each line of a recipe runs through jlog, which adds it to the run log in .agents/log
set shell := ["jlog", "sh", "-cu"]

mod rust 'rust.just'
import? 'roles/agent_gest_git_skills.just'
import? 'roles/nossaifd.just'
import? '../skills/docs/docs.just'

# just api-check for each language that has it: rust
api-check:
    just --justfile {{quote(justfile())}} rust api-check

# just api-prose-check for each language that has it: rust
[positional-arguments]
api-prose-check *args:
    just --justfile {{quote(justfile())}} rust api-prose-check "$@"

# just build for each language that has it: rust
[positional-arguments]
build *args:
    just --justfile {{quote(justfile())}} rust build "$@"

# just coverage-check for each language that has it: rust
coverage-check:
    just --justfile {{quote(justfile())}} rust coverage-check
    jlog step coverage-check

# just docstring-check for each language that has it: rust
docstring-check:
    just --justfile {{quote(justfile())}} rust docstring-check
    jlog step docstring-check

# just fmt for each language that has it: rust
[positional-arguments]
fmt *args:
    just --justfile {{quote(justfile())}} rust fmt "$@"

# just fmt-check for each language that has it: rust
fmt-check:
    just --justfile {{quote(justfile())}} rust fmt-check
    jlog step fmt-check

# just integration for each language that has it: rust
[positional-arguments]
integration *args:
    just --justfile {{quote(justfile())}} rust integration "$@"
    jlog step integration

# just lint for each language that has it: rust
[positional-arguments]
lint *args:
    just --justfile {{quote(justfile())}} rust lint "$@"
    jlog step lint

# just quick for each language that has it: rust
[positional-arguments]
quick *args:
    just --justfile {{quote(justfile())}} rust quick "$@"

# just test-paths for each language that has it: rust
[positional-arguments]
test-paths *args:
    just --justfile {{quote(justfile())}} rust test-paths "$@"

# just version for each language that has it: rust
version:
    just --justfile {{quote(justfile())}} rust version

# Every installed check of each language, then the check of git for whitespace errors
verify:
    just --justfile {{quote(justfile())}} rust verify
    git diff --check

# Check each program that .agents/stack.json records: it is there, and at its version floor
tools-check:
    uv run --no-project --quiet --no-python-downloads --python '>=3.9' python .agents/tools_check.py
    jlog step tools-check

# Find whitespace errors in the changes against the last commit
diff-check:
    git diff --check "$(git rev-parse -q --verify HEAD || git hash-object -t tree /dev/null)"
    jlog step diff-check

# Warn at once when jlog pass cannot record a pass: the worktree has unstaged changes or untracked files
[private]
worktree-warning:
    @git status --porcelain=v1 -z --untracked-files=all 2>/dev/null | tr '\n\000' '?\n' | awk '$0 == "" { next } skip { skip = 0; next } length($0) < 4 { next } { x = substr($0, 1, 1); y = substr($0, 2, 1) } x ~ /[RC]/ || y ~ /[RC]/ { skip = 1 } y != " " { n++; if (n <= 10) shown = shown (n > 1 ? ", " : "") substr($0, 4) } END { if (n) printf "check: warning: this run cannot record a pass, because the worktree has %d %s: %s%s. The steps still run. Stage or commit the changes to get a pass.\n", n, (n == 1 ? "unstaged change or untracked file" : "unstaged changes or untracked files"), shown, (n > 10 ? ", and more" : "") }' >&2 || true

# Everything a commit must pass: each step, then the pass of jlog for the worktree
check: worktree-warning tools-check release-check ci-gate fmt-check lint typecheck test integration coverage-check docstring-check docs-check prose-check diff-check
    jlog pass
