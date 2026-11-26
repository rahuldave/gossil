"""Check the build and CI settings that keep Gossil's cached and parallel test runs correct.

    python3 scripts/check-ci-config.py [ROOT]

It prints each problem. It exits with 1 when it finds a problem, and with 0 when it finds none. A file that it
cannot read is a problem too. It only reads files, and it uses neither cargo nor the network. The checks accept only
the plain forms that Gossil uses, so that a form they cannot judge is a problem and not a pass:

- .config/nextest.toml has exactly one override, of the profile default, which is the profile that the integration
  step runs. The override holds only filter, test-group and threads-required = "num-test-threads". nextest takes
  each setting from the first override that matches, so an override placed before it would win. The filter is plain
  names, test(NAME) or binary(NAME), joined by | or the word or, and it names each term of ALONE, the tests that
  must run alone.
- Each test(NAME) names a function with the attribute #[test] or #[tokio::test] in src/ or tests/, and each
  binary(NAME) names a test target, as Cargo.toml names it. The scan drops // comments and /* */ comments, which
  nest in Rust. nextest refuses a binary() that matches no test binary, but it accepts a test() that matches no
  test, with no message. So a renamed test would drop out of the override, and nextest would say nothing.
- Each install script that a workflow (*.yml or *.yaml) fetches from raw.githubusercontent.com, in any letter case,
  is fetched over https at a full commit hash. Each time the host appears, the workflow writes out the whole URL, as
  https://raw.githubusercontent.com/OWNER/REPO/HASH/. So a URL that takes its ref from a variable or an expression
  is a problem.
- Each step is under a job line that is a plain name at an indent of two spaces, below the line jobs:. Each item
  of a list is - KEY: or - VALUE, with one space after the dash, so a step in another form is a problem.
- Each line that names actions/cache, in any letter case, is uses: actions/cache@vN, actions/cache/restore@vN or
  actions/cache/save@vN, with no quotes and no comment. In each job with a cache step, the step with the id rust
  comes before the cache steps, has no if: and no continue-on-error:, and writes the image of the runner to its
  outputs. The line that writes it comes right after the line that fails the step when the runner gives no ImageOS.
- Each cache step has a line that holds only with:, and exactly one line key:. No line of it has a comment. Each key
  holds the image output of the step rust, and so does each line of a restore-keys block. The save step may reuse
  the primary key of an earlier restore step with the id rust-cache in its job instead.
- release.yml has a step that restores the cache, and no defaults: or working-directory: line. After each restore,
  and before the next restore or step that runs just in its job, there is a step with only the keys name and run.
  The command of that step only removes the coverage stamp. A step runs just when the word just appears anywhere in
  a run line.

A test that a Rust string literal writes out counts as a test, because the scan does not parse Rust strings.
"""

import re
import sys
import tomllib
from pathlib import Path

# A plain name in a filter: test(NAME) or binary(NAME). The match must cover the whole term.
TERM = re.compile(r"\b(test|binary)\(\s*([A-Za-z0-9_]+)\s*\)")
# The start of each term of the kinds that the script checks, so that the script finds a term that TERM does not cover.
TERM_START = re.compile(r"\b(test|binary)\(")
# A raw GitHub URL, in any case and over http or https, and the ref in it, the third segment of the path.
RAW = re.compile(r"raw\.githubusercontent\.com", re.IGNORECASE)
# The only form of such a URL that the check accepts: https, an owner, a repository and a full commit hash.
PINNED = re.compile(r"https://raw\.githubusercontent\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+/[0-9a-f]{40}/", re.IGNORECASE)
STAMP = "rm -f target/llvm-cov-target/jnprofile-rust-build.txt"
# The terms that must run alone, because the tests of the binary conformance start programs with short timeouts.
ALONE = {("test", "plan_cli_and_evidence"), ("binary", "conformance")}
# The keys that the one override may hold.
OVERRIDE_KEYS = {"filter", "test-group", "threads-required"}
# The line of the step with the id rust that writes the image of the runner to its outputs.
IMAGE_OUTPUT = 'echo "image=$ImageOS" >> "$GITHUB_OUTPUT"'
# The line right before it, which fails the step when the runner gives no ImageOS.
IMAGE_GUARD = ('[[ "${ImageOS:-}" =~ ^[a-z]+[0-9]+$ ]] || { echo "::error::The runner gives no ImageOS, so the cache has '
               'no key."; exit 1; }')
# A line that starts an item of a list: a step is "- KEY:" with one space, and a plain value is "- VALUE".
ITEM = re.compile(r"\s*-(\s|$)")
PLAIN_ITEM = re.compile(r"\s*- ([A-Za-z][A-Za-z0-9_-]*:(\s.*)?|[^\s{\[:#&*!|>'\"][^:]*)")
# The output of the image in a key, as a whole expression.
IMAGE = re.compile(r"\$\{\{\s*steps\.rust\.outputs\.image\s*\}\}")
PRIMARY_KEY = "key: ${{ steps.rust-cache.outputs.cache-primary-key }}"
# A cache step, written as Gossil writes it, with no quotes and no comment.
CACHE = re.compile(r"uses: actions/cache(/restore|/save)?@v[0-9]+")
RESTORE = re.compile(r"uses: actions/cache(/restore)?@v[0-9]+")
# The word just anywhere in a line of a run, quoted, after a path or after an operator of the shell.
JUST = re.compile(r"(?<![A-Za-z0-9_.-])just(?![A-Za-z0-9_.-])")


def test_targets(root):
    """The names of the test targets: each [[test]] name of Cargo.toml, and the stem of each tests/NAME.rs that no
    [[test]] names by its path, unless autotests = false."""
    manifest = root / "Cargo.toml"
    config = tomllib.loads(manifest.read_text()) if manifest.is_file() else {}
    targets = [target for target in config.get("test") or [] if isinstance(target, dict)]
    claimed = {str(Path(target["path"])) for target in targets if isinstance(target.get("path"), str)}
    names = {target["name"] for target in targets if isinstance(target.get("name"), str)}
    if (config.get("package") or {}).get("autotests", True) is not False:
        names |= {path.stem for path in (root / "tests").glob("*.rs")
                  if str(path.relative_to(root)) not in claimed}
    return names


def without_comments(text):
    """TEXT without its // comments and its /* */ comments, which nest in Rust. Strings are not parsed."""
    out, depth, i = [], 0, 0
    while i < len(text):
        pair = text[i:i + 2]
        if depth == 0 and pair == "//":
            end = text.find("\n", i)
            i = len(text) if end < 0 else end
        elif pair == "/*":
            depth, i = depth + 1, i + 2
        elif depth and pair == "*/":
            depth, i = depth - 1, i + 2
        else:
            if depth == 0 or text[i] == "\n":
                out.append(text[i])
            i += 1
    return "".join(out)


def tests(root):
    """The names of the functions with the attribute #[test] or #[tokio::test] in each .rs file under src/ or tests/."""
    names = set()
    for folder in ("src", "tests"):
        for path in (root / folder).rglob("*.rs"):
            marked = False
            text = without_comments(path.read_text(errors="replace"))
            for line in text.splitlines():
                line = line.strip()
                if not line or line.startswith("//"):
                    continue
                if re.match(r"#\[(tokio::)?test\b", line):
                    marked = True
                    continue
                if line.startswith("#["):
                    continue
                found = re.search(r"\bfn\s+([A-Za-z0-9_]+)\s*[<(]", line)
                if marked and found:
                    names.add(found[1])
                marked = False
    return names


def terms(text):
    """The (kind, name) of each plain term of a filter, and the text of each term that is not plain."""
    plain, other = [], []
    for start in TERM_START.finditer(text):
        match = TERM.match(text, start.start())
        if match:
            plain.append((match[1], match[2]))
        else:
            end = text.find(")", start.start())
            other.append(text[start.start():end + 1 if end >= 0 else len(text)])
    return plain, other


def joined_by_or(text):
    """Whether the filter TEXT is plain terms joined only by | or or, so that it names exactly those tests."""
    rest = TERM.sub("", text)
    return re.fullmatch(r"(\s*(\||\bor\b)\s*)*\s*", rest) is not None


def nextest_problems(root):
    """The problems of .config/nextest.toml: other overrides, other keys, unknown or unchecked names."""
    config = tomllib.loads((root / ".config" / "nextest.toml").read_text())
    name = ".config/nextest.toml"
    found = [(profile, override) for profile, table in (config.get("profile") or {}).items()
             for override in table.get("overrides") or []]
    if len(found) != 1 or found[0][0] != "default":
        return [f"{name}: it needs exactly one override, of the profile default, and it has "
                f"{', '.join(profile for profile, _ in found) or 'none'}"]
    override = found[0][1]
    problems = []
    for key in sorted(set(override) - OVERRIDE_KEYS):
        problems.append(f"{name}: the override holds {key}, and it may hold only {', '.join(sorted(OVERRIDE_KEYS))}")
    if override.get("threads-required") != "num-test-threads":
        problems.append(f'{name}: the override needs threads-required = "num-test-threads"')
    text = override.get("filter")
    text = text if isinstance(text, str) else ""
    plain, other = terms(text)
    for term in other:
        problems.append(f"{name}: {term} is not a plain name, so its test cannot be checked")
    if not other and not joined_by_or(text):
        problems.append(f"{name}: {text!r} joins its terms with more than | or or")
    known = {"test": tests(root), "binary": test_targets(root)}
    for kind, term in plain:
        if term not in known[kind]:
            problems.append(f"{name}: {kind}({term}) names no test of the project")
    for kind, term in sorted(ALONE - set(plain)):
        problems.append(f"{name}: the override does not name {kind}({term}), which must run alone")
    return problems


# The job of a step whose job line is not a plain name at an indent of two spaces.
JOB_FORM = "(a job line in another form)"


def steps(text):
    """The steps of a workflow in their order, each as (job, lines), with the comment lines left out.

    Each line keeps its indent, and the dash that starts a step becomes a space, so that the keys of a step line up.
    """
    found, job, in_jobs = [], None, False
    for line in text.splitlines():
        if line.lstrip().startswith("#") or not line.strip():
            continue
        if not line.startswith(" "):
            in_jobs, job = line.rstrip() == "jobs:", None
            continue
        top = re.match(r"^  ([A-Za-z0-9_-]+):\s*$", line)
        if in_jobs and indent(line) == 2:
            job = top[1] if top else JOB_FORM
        elif re.match(r"\s*- [A-Za-z][A-Za-z0-9_-]*:", line):
            found.append((job, [line.replace("- ", "  ", 1).rstrip()]))
        elif found:
            found[-1][1].append(line.rstrip())
    return found


def stripped(lines):
    """The LINES of a step without their indent."""
    return [line.strip() for line in lines]


def indent(line):
    return len(line) - len(line.lstrip())


def key_problems(name, lines):
    """The problems of the keys of one cache step: another form, a key or a line of restore-keys with no image."""
    plain = stripped(lines)
    problems = []
    if "with:" not in plain:
        problems.append(f"{name}: a cache step needs a line with: alone, so that its keys are checked")
    if sum(line.startswith("key:") for line in plain) != 1:
        problems.append(f"{name}: a cache step needs exactly one line key:")
    block = None
    for line in lines:
        text = line.strip()
        if block is not None and indent(line) > block:
            if not IMAGE.search(text):
                problems.append(f"{name}: the restore key {text} holds no image of the runner")
            continue
        block = None
        if " #" in text:
            problems.append(f"{name}: the line {text} of a cache step has a comment, so its value cannot be checked")
        if text.startswith("restore-keys:"):
            if text.removeprefix("restore-keys:").strip() != "|":
                problems.append(f"{name}: restore-keys must be a block, restore-keys: |, so that each line is checked")
            block = indent(line)
        elif text.startswith("key:"):
            if text != PRIMARY_KEY and not IMAGE.search(text):
                problems.append(f"{name}: the cache key {text} holds no image of the runner")
    return problems


def stamp_problems(order):
    """The problems of release.yml: a restore with no plain step that removes the stamp before the next just."""
    restores = [i for i, (_, lines) in enumerate(order) if any(RESTORE.match(line) for line in stripped(lines))]
    if not restores:
        return ["release.yml: no step restores the cache, so the check of the coverage stamp has nothing to check"]
    problems = []
    for restore in restores:
        job = order[restore][0]
        for other, lines in order[restore + 1:]:
            if other != job:
                break
            plain = stripped(lines)
            if plain[-1:] == [f"run: {STAMP}"] and len(plain) <= 2 and all(re.match(r"(name|run):", line) for line in plain):
                break
            if any(RESTORE.match(line) for line in plain) or any(JUST.search(line.removeprefix("run:")) for line in plain):
                problems.append(f"release.yml: the restore of the job {job} has no step of only a name and run: that "
                                f"removes the coverage stamp before the next restore or just")
                break
    return problems


def guarded(lines):
    """Whether the stripped LINES of a step write the image right after the line that fails on no ImageOS."""
    return any(lines[i] == IMAGE_GUARD and lines[i + 1] == IMAGE_OUTPUT for i in range(len(lines) - 1))


def workflow_problems(root):
    """The problems of the workflows: unpinned scripts, cache steps in other forms, keys with no image, and a kept
    coverage stamp."""
    problems = []
    folder = root / ".github" / "workflows"
    if not (folder / "release.yml").is_file():
        problems.append("release.yml is missing")
    for path in sorted([*folder.glob("*.yml"), *folder.glob("*.yaml")]):
        text = path.read_text()
        code = [line for line in text.splitlines() if not line.lstrip().startswith("#")]
        joined = "\n".join(code)
        for found in RAW.finditer(joined):
            start = max(joined.rfind(c, 0, found.start()) for c in " \t\n\"'=(") + 1
            if not PINNED.match(joined, start) or start + len("https://") != found.start():
                url = joined[start:].split()[0] if joined[start:].split() else ""
                problems.append(f"{path.name}: a script is fetched at {url}, not over https at a full commit hash")
        for line in code:
            if ITEM.match(line) and not PLAIN_ITEM.fullmatch(line.rstrip()):
                problems.append(f"{path.name}: {line.strip()} is a list item in a form that is not - KEY: or - VALUE")
            if "actions/cache" in line.lower() and not CACHE.fullmatch(line.strip().removeprefix("- ")):
                problems.append(f"{path.name}: {line.strip()} names actions/cache in a form that is not "
                                f"uses: actions/cache[/restore|/save]@vN")
        order = steps(text)
        for job, lines in order:
            if job in (None, JOB_FORM):
                problems.append(f"{path.name}: the step {stripped(lines)[0]} is not under a job line of a plain name "
                                f"at an indent of two spaces")
        for index, (job, lines) in enumerate(order):
            if not any(CACHE.fullmatch(line) for line in stripped(lines)):
                continue
            problems += key_problems(path.name, lines)
            if PRIMARY_KEY in stripped(lines):
                source = [stripped(lines) for other, lines in order[:index] if other == job
                          and "id: rust-cache" in stripped(lines)]
                if not any(any(RESTORE.fullmatch(line) for line in lines) for lines in source):
                    problems.append(f"{path.name}: the key {PRIMARY_KEY} is not of an earlier restore step with the id "
                                    f"rust-cache in the job {job}")
            rust = [stripped(lines) for other, lines in order[:index] if other == job and "id: rust" in stripped(lines)]
            if not any(guarded(lines) and not any(re.match(r"(if|continue-on-error):", line) for line in lines)
                       for lines in rust):
                problems.append(f"{path.name}: no step rust with no if: and no continue-on-error: writes "
                                f"{IMAGE_OUTPUT} before the cache step of the job {job}")
        if path.name == "release.yml":
            for line in code:
                if re.match(r"\s*(defaults|working-directory):", line):
                    problems.append(f"release.yml: {line.strip()} could move the step that removes the coverage stamp "
                                    f"to another folder")
            problems += stamp_problems(order)
    return problems


def main(argv):
    root = Path(argv[1] if len(argv) > 1 else ".")
    try:
        problems = nextest_problems(root) + workflow_problems(root)
    except (OSError, tomllib.TOMLDecodeError) as error:
        print(f"check-ci-config: {error}", file=sys.stderr)
        return 1
    for problem in problems:
        print(f"check-ci-config: {problem}", file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
