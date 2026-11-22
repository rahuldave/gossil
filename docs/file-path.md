# Lexical file paths

The `gossil::file_path` module simplifies filename bytes by the rules of
Fossil's `file_simplify_name` function. Lexical simplification changes only the
bytes of the name. It does not read the filesystem, resolve symlinks or make a
name absolute. The native C source has one branch for Unix and one for Windows
and Cygwin. The module ports both source branches, and the caller selects one
of them on every host.

## Simplify a name

`simplify_name(input, keep_trailing_slash, limits)` uses the Unix branch and
returns `Result<Vec<u8>, PathError>`. Give the complete filename as a byte
slice, which does not have to be UTF-8. An empty input gives an empty output,
and an input that contains a NUL byte is an error.

```rust
use gossil::file_path::{Limits, simplify_name};

let limits = Limits { max_input_bytes: 128, max_output_bytes: 128 };
let path = simplify_name(b"a/b/../c/", false, limits).unwrap();
assert_eq!(path, b"a/c");
```

`simplify_name_for_source(input, keep_trailing_slash, limits, platform)` takes
a `SourcePlatform` and returns `Result<SimplifiedName, PathError>`.

| Source choice | Behavior |
|---|---|
| `SourcePlatform::Unix` | Backslashes and extended prefixes stay ordinary bytes |
| `SourcePlatform::WindowsCygwin` | Backslashes become slashes, and the `//?/` extended prefix is removed |

The source choice does not depend on the host operating system. A choice of
`WindowsCygwin` does not show that Gossil runs correctly on Windows or Cygwin.

## Read the result of a source branch

A `SimplifiedName` has two views of its bytes:

- **`reported_span()`** borrows the bytes that the native return value counts.
- **`visible_bytes()`** borrows the bytes before the first NUL, as a C string shows them.

The two views differ for one input. For the Windows/Cygwin input `//?/`, native
C copies the terminator of the input to byte zero but reports a length of one.
So the reported span is `[0]`, and the visible name is empty.

## Set the bounds

`Limits` has the fields `max_input_bytes` and `max_output_bytes`. The input
must fit its limit. The module reserves one buffer of the input length plus
one sentinel byte, the C terminator, before it copies the input. The output limit counts the
reported span, and the span can hold an early NUL. So the input `//?/` needs an
output limit of at least one with `WindowsCygwin`, but its visible name is
empty.

| Error | Condition |
|---|---|
| `InputLimit` | The input exceeds its limit, or the buffer exceeds addressable memory |
| `OutputLimit` | The reported span exceeds its limit |
| `EmbeddedNul` | The input contains NUL |
| `TerminalCancellation` | A relative name cancels to before the start of the buffer |
| `Allocation` | The buffer reservation fails, and the error keeps the original cause |

## The order of the lexical rules

The module follows the loops of the native function in their order:

1. With `WindowsCygwin`, it converts each backslash to a slash, then handles an initial `//?/`.
2. If `keep_trailing_slash` is false, it trims the trailing slashes.
3. It collapses duplicate slashes, but keeps an initial double slash.
4. It removes leading `./`, internal `/./` and each `/component/..` that can cancel.

For example, `//a` stays `//a`, but `//` becomes `/` when `keep_trailing_slash` is false.
Unresolved `..` components stay, even above an absolute root. A nonempty name
that simplifies to nothing becomes `/`.

The trailing slash option does not keep every trailing slash. With the option
true, `a///` becomes `a/`, but `a///./` becomes `a`. The second result comes
from the collapse loop. The loop shortens the logical length but leaves the old
bytes after it. A later `/./` test can read those bytes, and the port keeps
this behavior.

With `WindowsCygwin`, the module compares the three bytes after `//?/` with
`UNC`, and it ignores ASCII case, as SQLite's `sqlite3_strnicmp` does. A match
gives an initial slash and skips `UNC`. So `//?/UNC/a/b` becomes `//a/b`.
Without a match, the byte after the prefix moves to the start, and `//?/a:/a/b`
becomes `a:/a/b`. The port does not check drives, the names after the `UNC` prefix or
whether a name is valid.

## When a relative name cancels past its start

A relative name such as `a/..` cancels its last component back past the start
of the name. The case is a relative terminal cancellation. Native `backup_dir`
then leaves the output cursor at minus one, so native C writes the final
terminator one byte before the buffer. The write is undefined behavior, and
native Fossil has no defined result for these names.

The port returns `PathError::TerminalCancellation` in place of a result.
Examples are `a/..`, `.../..` and `a/../` with the trailing slash option false.
The error is a safe policy of the port, and not a native result.

## Provenance and tests

The port follows Fossil check-in `7e8ecf0ce4`, the same check-in as the
reference binary of the project. The source is `src/file.c`, lines 1172 to 1180
for `backup_dir` and lines 1195 to 1260 for `file_simplify_name`. The test
`test/file1.test` and its BSD license text are copied unchanged into
`tests/fixtures/file-path/`.

The tests in `tests/file_path.rs` check these groups of cases:

| Group | Source |
|---|---|
| 25 cases of `simplify-name` 100 to 107 | `file1.test` |
| 6 cases of `simplify-name` 108 and 109 | `file1.test`, which runs them only on Windows |
| 14 Unix cases and 18 Windows/Cygwin cases | adapter plans for a C copy of the native source |
| Lexical controls, bounds and errors | the Rust tests |

An earlier run compiled the C copy on a macOS host, once for each branch. Each
build passed all of its plan cases. The records of that run are not in the
repository.

The `test-simplify-name` command of the Fossil 2.29 binary gives the same output
as the fixtures for all 25 inherited cases. It does the same for the Unix adapter
cases with the option false. The command has no trailing slash option, so it
cannot check the cases with the option true.

Relative-name, relative-tree-name and absolute-tree-name operations are not
part of the module, because they need a working directory and a repository root.
The Windows and Cygwin branch has not run on a Windows or Cygwin host.

## Recommendations

Select the source branch when a name needs platform conversion. Set byte
limits, and keep reported spans apart from visible names. Apply the policy for
the filesystem and for containment in a separate step. Next, qualify the
Windows/Cygwin branch on a native host before code depends on it.
