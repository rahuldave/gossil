# Git reader

The `gossil::git_reader` module reads a Git repository beside Git, and it never
writes to Git. Binding is the step that ties a Gossil change to a Git commit.
GIT-01 of the Git access contract lists the reads that binding needs, and the module
does each of them. It finds the Git directory of a worktree, reads `HEAD` and a branch
tip, and reads a commit with its ordered parents. It also reads the object format and
three keys of the configuration. The section
[The guards against writes](#the-guards-against-writes) gives the checks that keep
the module from writing.

The module is a library only. No command uses it yet. The bind command, the
hooks and the storage of the source registry are separate work.

## Open a worktree

`GitRepository::open(worktree, limits)` opens one worktree. A worktree is a
folder with checked-out files and a `.git` entry. The folder must hold the
`.git` entry itself. The reader searches no parent folder, and it reads no Git
environment variable, such as `GIT_DIR`.

| Call | Result |
|---|---|
| `GitRepository::open(path, Limits::STANDARD)` | The opened repository, after the support checks below. |
| `repo.worktree()` | The canonical top folder of the worktree, which holds `.git`. |
| `repo.git_dir()` | The Git directory of the worktree, which holds its `HEAD`. |
| `repo.common_dir()` | The common Git directory, which holds the objects, the shared refs and `config`. |
| `repo.is_linked_worktree()` | True when the two directories differ. |
| `repo.object_format()` | `ObjectFormat::Sha1` or `ObjectFormat::Sha256`. |
| `repo.config()` | The three configuration keys, as `RepositoryConfig`. |

A main worktree has `.git` as a folder, and its Git directory is the common
directory. A linked worktree, which `git worktree add` makes, has a `.git` file.
The file names a folder below `.git/worktrees` of the main repository, and
that folder names the common directory in its file `commondir`. The reader
drops only the line ends, `\n` and `\r`, at the end of `commondir`. So a space
stays a part of the path.

## Supported repositories

`GitRepository::open` checks the repository against the table below at each call, because
`git refs migrate` can change a repository between two runs. It returns an
error for an unsupported repository before any other read, and the error names
the reason.

| Repository | Result of the open |
|---|---|
| SHA-1 or SHA-256, files ref backend, loose or packed objects, alternates, linked worktrees | Opened |
| A `multi-pack-index` | Opened; the reader reads the packs without the index |
| Folder without a `.git` entry, which is not a bare repository | `GitReadError::NotAWorktree` |
| `.git` entry that does not lead to a valid Git directory, a `.git` file that names a file and not a folder, or a `HEAD` that names a ref outside `refs/` | `GitReadError::NotARepository` |
| `.git` file whose path ends in whitespace, such as from `git init --separate-git-dir "gd "` | `GitReadError::NotARepository`, because gix would strip the space and open another folder |
| `HEAD` that Git calls broken, such as a short ID or an ID with data after it | `GitReadError::Ref` |
| `.git`, `HEAD`, `config`, `commondir` or `alternates` that is a FIFO, a device, a socket, a dangling link or a refused link | `GitReadError::NotAFile` |
| `objects/info/alternates` over its limits, an alternate that is not a folder, or an alternate that gix reads otherwise than Git, as [Limits](#limits) says | `GitReadError::Alternate`, or `TooLarge` for the size |
| Shallow clone, or a linked worktree of one | `GitReadError::Shallow`, because the common directory holds the file `shallow` |
| `extensions.refStorage=reftable` | `GitReadError::Reftable` |
| Bare repository of either format, also with a detached `HEAD`, or a worktree for which `core.bare` is true | `GitReadError::Bare`, because a bare repository is not a location of the source registry |
| Linked worktree of a bare repository, without `extensions.worktreeConfig` | Opened, because Git does not treat the linked worktree as bare |
| Worktree whose `core.worktree` names another folder | `GitReadError::ForeignWorktree`, because Git's worktree is that other folder |
| Unknown object format, unknown ref backend | `UnsupportedObjectFormat` or `UnsupportedRefStorage` |
| `core.repositoryformatversion` other than 0 or 1 | `UnsupportedRepositoryVersion` |
| Version 0 with an extension that Git knows only in version 1 | `UnsupportedRepositoryVersion`, as in Git |
| Version 1 with an extension that Git does not know, or with `extensions.compatObjectFormat` | `UnsupportedExtension` |
| `extensions.objectFormat`, `refStorage`, `partialClone` or `compatObjectFormat` without a value | `GitReadError::Config`, as in Git |

The reader checks the keys below `extensions` as Git's `setup.c` does, in upper
or lower case:

- Git knows `noop`, `preciousObjects`, `partialClone` and `worktreeConfig` in
  both versions.
- Git knows `noop-v1`, `objectFormat`, `compatObjectFormat`, `refStorage`,
  `relativeWorktrees` and `submodulePathConfig` only in version 1, and it stops
  with an error on them in version 0.
- In version 0, Git ignores an unknown key, and so does the reader.
- In version 1, Git stops with an error on an unknown key. A key in a subsection, such as
  `[extensions "x"] noop`, is unknown.
- The reader also returns `UnsupportedExtension` for `compatObjectFormat`, which it does not support.

A partial clone opens. A commit that is not present gives
`GitReadError::ObjectMissing`.

`core.worktree` names the worktree, relative to the Git directory. The open
returns an error for a value that does not lead to the opened folder. The reader
takes `core.bare` and `core.worktree` from the files that Git reads, as
`git rev-parse` shows:

- `config.worktree` of the worktree, when `extensions.worktreeConfig` is true,
  wins over the common `config`.
- The common `config` applies to a main worktree. It applies to a linked worktree
  only when `extensions.worktreeConfig` is true.
- Without `extensions.worktreeConfig`, Git ignores `config.worktree`, and so does
  the reader.

## Read refs and commits

| Call | Result |
|---|---|
| `repo.head()` | `Head::Branch { name, tip }`, where `name` is the last ref of the chain, as `git symbolic-ref HEAD` gives it, with `tip: None` for an unborn branch, or `Head::Detached(id)`; `GitReadError::Ref` when `HEAD` names a ref outside `refs/`. Each call reads `HEAD` again, so a `HEAD` removed after the open gives `GitReadError::Ref` |
| `repo.branch_tip("main")` | The tip of `refs/heads/main` from a loose ref or `packed-refs`, or `None` when the branch does not exist |
| `repo.object_kind(&id)` | The type of the object: commit, tree, blob or tag |
| `repo.read_commit(&id)` | A `GitCommit`, whose `id()` and `parents()` give the ID and the raw parents in the order of the object |

The reader parses a loose ref and `HEAD` as `parse_loose_ref_contents` of Git
does. After `ref:`, it drops the whitespace before and after the name. A direct
ref holds a full ID, and the end of the file or whitespace must follow it. So
`<id>junk` and a short ID give `GitReadError::Ref`. Whitespace is a space, a tab,
a line feed or a carriage return, as for `isspace` of Git. A form feed and a
vertical tab are not whitespace.

`GitObjectId::from_hex(format, hex)` parses a full ID of 40 or 64 hexadecimal
digits. It returns an error for an abbreviated ID. An ID whose format differs from the format
of the repository gives `GitReadError::FormatMismatch`.

A branch name must make a valid full ref name, `refs/heads/<name>`. A name such as
`bad..name` gives `GitReadError::InvalidBranchName`. A lookup follows symbolic
refs, such as `refs/heads/alias` that names `refs/heads/main`. It reads at most
`Limits::symbolic_depth` refs, five by default, as Git's `SYMREF_MAXDEPTH` does.
The read of `HEAD` counts as one of them. So a chain of five symbolic refs is too
deep, and `HEAD` can lead through at most three symbolic branches.

A ref below `refs/worktree`, `refs/bisect` or `refs/rewritten`, and a pseudo ref
such as `FOO`, is private to a worktree. The reader reads its loose file from the
Git directory of the worktree. Every other loose ref comes from the common
directory, also a name that Git puts in no category, such as `foo/bar`. Without a
loose file, a ref below `refs/` comes from the shared `packed-refs`, as in Git. A
ref outside `refs/` without a loose file does not exist. A name that points into
another worktree, such as `main-worktree/refs/heads/main`, gives
`GitReadError::Ref`.

A lookup in `packed-refs` takes the exact name, so a
symbolic ref to `FOO` never finds `refs/heads/FOO`. A ref whose path crosses a
file, such as `refs/heads/main/sub` beside the file `refs/heads/main`, does not
exist, as in Git. Nor does a loose ref that is a link to a folder. For both, the
reader looks in no `packed-refs`, as Git does. A real folder at the path of a ref
means only that the ref is not loose, so the reader then looks in `packed-refs`.

The reader parses `packed-refs` itself, as Git does. A carriage return stays in
the name, so a file with CRLF line ends finds no branch. An entry with a bad name
never matches, and the other entries still resolve. A last line with no line feed,
or a line that is not a record, gives `GitReadError::Ref`. Git refuses such a
file too. The parser of `gix-ref` differs on each of these points.

`read_commit` returns an error for an object that is not a commit, and the error
names the actual type of the object. It recomputes the hash of the bytes that it
read, and it returns `GitReadError::HashMismatch` when the hash differs. The loose object reader of gix does not check the
hash, so the module checks it.

The reader reads the parents as `parse_commit_buffer` of Git does. It reads the
tree line and each parent line, and it reads nothing after them. So a bad author
line does not stop the read. An ID in these lines may have upper-case hex digits,
as in Git.

A body that ends right after its tree line or its
last parent line gives `GitReadError::MalformedCommit`, as in Git. A short parent
line at the end of the body also gives that error, but Git drops such a line. So
the reader never gives a commit fewer parents than its body names.

Each failed read keeps its underlying error as a `Cause`, and
`std::error::Error::source` returns it. The error of the operating system keeps
its kind, and the error of gix keeps its type. Only the reader makes a
`GitCommit`, because its fields are private. So a `GitCommit` is always a commit
that the reader checked.

## Replacement objects do not apply

The reader opens the object store with `gix_odb::at_opts`, with no table of
replacements. So `refs/replace` and `git replace --graft` never change a result.
It also ignores the file `info/grafts`, which Git still applies.
A commit gives the parents that `git --no-replace-objects cat-file commit` shows,
and not those that `git cat-file commit` shows.

The reader also opens the store without `multi-pack-index`. gix would open each
pack that the index names without a check, so a FIFO there could block it. Without
the index, gix reads only the regular `.idx` files of the pack folder.

The reader gives gix the common directory as the folder against which a relative
path resolves. Without it, gix reads the current folder of the process, and the
open fails when that folder has been removed.

## Configuration keys

`RepositoryConfig` holds the three keys of GIT-01. The reader reads them from
the repository's own files, `config` in the common directory and, when
`extensions.worktreeConfig` is true, `config.worktree` in the Git directory. It
reads no global or system configuration, and it follows no `include.path`.

| Field | Meaning |
|---|---|
| `object_format` | `extensions.objectFormat`, or SHA-1 when the key is absent |
| `ref_storage` | The raw `extensions.refStorage`; an opened repository has `None` or `files` |
| `hooks_path` | The raw `core.hooksPath`; the value of `config.worktree` wins when that file applies |
| `unread_includes` | True when a file has an `include` or `includeIf` section, whose target the reader did not read |

GIT-05 says that `hooks install` fails when `core.hooksPath` is set at any level. The
reader gives only the repository levels. The hooks work must also check the
global and system levels, and treat `unread_includes` as a possible setting.

## Limits

`Limits` bounds each read, and the type has no unbounded default.
`Limits::STANDARD` gives the values below. A read that passes a limit gives
`GitReadError::TooLarge`, `SymbolicDepth`, `TooManyParents` or `Alternate`. A
branch name that names a folder of refs gives `None`, and not `TooLarge`.

The reader itself reads `HEAD`, each loose ref, `packed-refs`, a `.git` file,
`commondir`, each configuration file and each `alternates` file. gix parses
the configuration and the `gitdir:` line of a `.git` file. The reader parses a
loose ref, `HEAD` and `packed-refs` itself, as [Read refs and commits](#read-refs-and-commits) says. Each read works
in this way:

- The open uses `O_NONBLOCK`, so a FIFO cannot stop it.
- The reader checks the type on the opened file. It reads a regular file only.
  A FIFO, a device, a socket or a dangling link gives `GitReadError::NotAFile`.
- A folder where the reader needs a file also gives `NotAFile`. A folder at the
  path of a loose ref means only that the ref is not loose.
- The read takes at most the limit plus one byte. So a file that grows during the
  read gives `TooLarge`, and the reader never reads more than the limit.
- A symbolic link to `refs/...` is a symbolic ref, as Git reads it. Any other link
  at `HEAD` gives `NotAFile`, because Git also stops with an error on such a link.
  The open of `HEAD` uses `O_NOFOLLOW`, so a link that replaces `HEAD` after the
  check is not followed.
  The reader follows any other link at a ref. A target that is a regular file
  is read, and a target that is a folder makes the ref missing.

The open also walks the `alternates` files before gix reads them, and it splits
the lines as gix does. Each file counts against `alternates_bytes`, and all
alternates together count against `alternates`. Git reads six nested files, at
levels 0 to 5, and ignores a deeper one. The reader reads the same six files, and
it returns `GitReadError::Alternate` for an alternate in a deeper file.

Each alternate must be a folder. A line keeps its spaces, as in Git
and gix. The reader returns `GitReadError::Alternate` for three cases that gix
reads otherwise than Git:

- A quoted alternate path.
- A relative path in a nested `alternates` file. Git resolves it against the
  folder of that file, but gix resolves it against the top `objects` folder.
- A folder that appears twice, such as a cycle, a repeated line or a folder that
  names itself. Git skips the repeat, but gix stops with an error on a cycle.

gix reads the checked files once more when it opens the object store, at its
first lookup and after each miss. So before each object read, the reader checks
the `alternates` files again. A change since the open, such as a FIFO or a new
alternate, gives `GitReadError::Alternate`, `NotAFile` or `TooLarge`. The reader then checks
each loose file of the object in the same way. A FIFO there gives `NotAFile`
and does not block the read. A change between a check and the read of gix can
still pass.

| Field | Bound | Standard |
|---|---|---|
| `config_bytes` | Each configuration file | 1 MiB |
| `small_file_bytes` | `HEAD`, each loose ref, a `.git` file and `commondir` | 4 KiB |
| `packed_refs_bytes` | The file `packed-refs` | 256 MiB |
| `symbolic_depth` | Refs read in one lookup, `HEAD` included | 5 |
| `commit_bytes` | One commit object, from its header, before the read | 16 MiB |
| `parents` | Parents of one commit | 1024 |
| `alternates_bytes` | Each `objects/info/alternates` file | 64 KiB |
| `alternates` | Alternate object folders, over all nested `alternates` files | 16 |

## The guards against writes

`gix-ref` and `gix-odb` can also write, and Rust itself cannot forbid a call to
their write methods at compile time. So the module uses three guards, by GIT-03:

- **One module.** All use of gix sits in `src/git_reader.rs`. Its public types
  are its own, so a caller of the module needs no gix type.
- **The lint.** `clippy.toml` lists the write paths in `disallowed-methods` and
  `disallowed-types`. The `lint` recipe of the Justfile passes
  `-F clippy::disallowed_methods -F clippy::disallowed_types`. The flag `-F`
  sets each lint to forbid, so a local `#[allow]` cannot lift it.
- **The test of unchanged directories.** The test `git_t06` hashes the common
  directory, the worktree Git directory and the alternate object directory. It
  runs every call of the module, and then it hashes them again. The hash takes
  the bytes, the mode, the inode and the modified time of each entry. A folder
  gets a new modified time when an entry is made or removed in it. So a lock file
  that a writer makes and removes again changes the hash.

The reader makes no ref store of `gix-ref`. It reads and parses each loose ref,
`HEAD` and `packed-refs` itself, so the ref writers of `gix-ref` have no value
that they could start from.

The list bans each write method of `gix_object::Write`, `gix_odb::sink` and
`gix_odb::Sink`, and the ref transaction of `gix-ref`. It also bans the locks of
`gix-lock`, the persist of `gix-tempfile`, the multi-pack index writers and the
writers of the configuration. It bans the serializers of Git formats too, such
as `gix_object::WriteTo::write_to` and the reflog line of `gix-ref`.

`gix-odb` builds `gix-pack` without the feature `streaming-input`. So the pack
writers and the index writer do not exist in this build. Their entries in
`clippy.toml` have `allow-invalid = true`, so that a later change of features
cannot add them unbanned. `gix-ref` keeps its transaction of packed refs private, and only the
banned file transaction reaches it.

The test `git_t07` in `tests/git_write_lint.rs` writes a small crate with one
call of each banned path. It runs clippy on the crate with the flags of the
recipe and the real `clippy.toml`. Each call must fail the lint, and a plain read
must pass. Each entry with `allow-invalid` must name a writer that does not
compile in this build. The test also requires `-F`, and not `-D`, for both lints.

The test `gix_is_used_only_in_the_reader_module` checks the first guard. It
fails when a source file other than `src/git_reader.rs` names a gix path, such as
`gix_odb::at`. A probe text checks that the scan finds each form of the path.

The test `every_gix_writer_is_banned_or_explained` reads the sources of each
`gix-*` package of `Cargo.lock`. It finds each public function whose name says
that it may write, such as `write*`, `commit`, `persist*`, `create*`, `remove*`
and `set_*`. Each one found must match an entry of a table of the test, by its
package, its file and its name:

- `BANNED_SITES` gives the path that `clippy.toml` bans for the function.
- `EXPLAINED_SITES` and `EXPLAINED_METHODS` give the reason why the function
  cannot write to Git, such as a setter of a parsed configuration in memory.

The `gossil` crate can name only its six gix dependencies and the packages that they
re-export, such as `gix_pack` through `gix_odb::pack`. The test computes that
set from the sources. A free function of another package, such as
`gix_fs::symlink::create`, needs a path that `gossil` cannot write, so it needs no
entry. A method needs only a value of its type, so each method needs an entry in
every package. Each table entry must also name a function that the scan finds.
A trait method, such as `WriteTo::write_to`, is not a `pub fn`, so its ban is
listed by hand.

The scan matches names, so it is a best-effort guard and cannot be complete. The
guarantees are the three guards above:

- All use of gix sits in one module.
- The disallowed lists of `clippy.toml` ban the write paths.
- `git_t06` checks that the Git directories keep their bytes and metadata after a
  run of every call of the API.

## Dependencies

The module uses six gix crates, each pinned to an exact version, with the
features `sha1` and `sha256` and no default features.

| Crate | Version | Role |
|---|---|---|
| `gix-discover` | 0.57.1 | Parse the `gitdir:` line of a `.git` file |
| `gix-config` | 0.62.1 | Parse the configuration files |
| `gix-ref` | 0.69.1 | Check ref names and sort them into categories |
| `gix-odb` | 0.87.0 | Read loose and packed objects, with alternates |
| `gix-object` | 0.66.1 | Parse commits and recompute their hash |
| `gix-hash` | 0.28.1 | Object IDs |

The crates build on Rust 1.95, the toolchain of the development host, and they need Rust
1.88 or later. `Cargo.lock` grows from 127 to 188 packages, so the crates add 61
packages for all targets. Some of them build only on Windows or on embedded
targets. On macOS, `just git-reader-packages` counts 53 packages that only the
six crates bring into the normal build. `Cargo.lock` already had six of the 53
for other uses, which are `fastrand`, `getrandom`, `hashbrown`, `rustix`,
`tempfile` and `zlib-rs`. The hash features bring second major versions of
`sha2`, `digest`, `block-buffer`, `crypto-common` and `cpufeatures`.

The tests in `tests/git_reader_packages.rs` run the script of
`just git-reader-packages` and check its count. They also check that the script
fails when no package is added.

## Tests

The tests in `tests/git_reader.rs` build each repository with `git` in a
temporary scratch folder, with no inherited Git variables. They compare each parent
list with `git --no-replace-objects cat-file commit`. Each test needs `git` 2.46
or later. Without it, a test fails with the status `not-run`, so a missing tool
never counts as a pass.

| Test | Case |
|---|---|
| `git_t01` | Packed objects, loose objects, `packed-refs` and an alternate |
| `git_t02` | A linked worktree, its Git directory, the common directory and its `HEAD` |
| `git_t03` | A commit that `git replace --graft` replaces |
| `git_t04` | A SHA-256 repository |
| `git_t05` | A reftable repository, and a repository that `git refs migrate` changes |
| `git_t06` | Every call leaves the Git directories unchanged |
| `git_t07`, in `tests/git_write_lint.rs` | Each banned write path fails the lint |
| `gix_is_used_only_in_the_reader_module`, in `tests/git_write_lint.rs` | No source file other than `src/git_reader.rs` names a gix path |
| `git_t09` | A loose commit with a valid zlib stream and changed bytes |
| `shallow_clone_and_its_worktree_are_refused` | A shallow clone and its linked worktree |
| `missing_commits_bare_repositories_and_plain_folders_are_refused` | A missing commit, a bare repository and a folder without `.git` |
| `other_object_kinds_are_not_commits` | A tree, a blob and an annotated tag, which `read_commit` refuses |
| `head_and_branch_states` | An unborn branch, a detached `HEAD`, and chains of symbolic refs that it compares with `git rev-parse` |
| `repository_configuration_keys` | The three keys, with `config.worktree` and include sections |
| `limits_bound_every_read` | Each limit stops the read that would pass it, and a read at the limit passes |
| `malformed_commit_body_is_refused` | A commit with a correct hash and an empty body, no tree line, or a bad parent line |
| `linked_worktree_of_a_bare_repository_opens` | A linked worktree of a bare repository |
| `unknown_version_one_extensions_are_refused` | An unknown extension of version 1 |
| `branch_folder_is_absent_not_too_large` | A branch name that is a folder of 200 branches |
| `tag_named_like_a_branch_is_not_the_branch` | A tag named `refs/heads/ghost` |
| `broken_linked_worktrees_are_refused` | A `.git` file or `commondir` over the limit, or a `commondir` that names no folder |
| `extensions_follow_the_rules_of_git` | Keys below `extensions` in both versions, which it compares with `git rev-parse` |
| `core_worktree_must_name_the_opened_folder` | `core.worktree` in `config` and `config.worktree`, and in a linked worktree |
| `undecodable_object_is_a_read_error` | A loose object that is not a zlib stream |
| `linked_worktree_config_is_read` | `config.worktree` of a linked worktree, with an include, a parse error and a size limit |
| `include_if_marks_unread_includes` | An `includeIf` section |
| `worktree_config_sets_bare_and_worktree` | `core.bare` and `core.worktree` in `config.worktree` and the common `config`, which it compares with `git rev-parse` |
| `worktree_config_is_ignored_without_the_extension` | A `config.worktree` without `extensions.worktreeConfig` |
| `oversize_head_is_refused_before_it_is_read` | A `HEAD` over the limit, with and without a `.git` entry |
| `special_head_entries_are_refused_at_once` | `HEAD` as a link to `/dev/zero`, a FIFO or a link outside `refs/`, within ten seconds; a link to `refs/heads/main` opens |
| `special_entries_never_block_a_read` | `config`, `commondir`, `packed-refs`, a loose ref and `alternates` as a FIFO or a device, within ten seconds |
| `alternates_are_bounded` | The size, the count and the depth of `alternates`, a missing alternate and a quoted path |
| `alternates_resolve_as_git_and_gix_do` | A relative alternate, a line with a space, a nested relative path, a cycle, a repeat and a folder that names itself |
| `special_loose_objects_never_block_a_read` | A loose object that is a FIFO, within ten seconds |
| `special_loose_objects_in_alternates_never_block_a_read` | A loose object in an alternate that is a FIFO, within ten seconds |
| `loose_ref_contents_parse_as_in_git` | Loose refs and `HEAD` with spaces after `ref:`, data after the ID, or a short ID, which it compares with `git rev-parse` |
| `folder_with_a_head_folder_is_not_a_worktree` | A folder without `.git` whose `HEAD` is a folder |
| `folder_with_a_garbage_head_is_not_a_worktree` | A folder without `.git`, with `objects` and `refs`, whose `HEAD` is not a ref |
| `git_directory_without_objects_or_refs_is_refused` | A Git directory without its `objects` or its `refs` folder |
| `commondir_of_only_a_line_end_is_refused` | A linked worktree whose `commondir` holds only a line end |
| `head_removed_after_the_open_is_a_ref_error` | A `HEAD` removed after the open |
| `alternates_changed_after_the_open_are_refused` | `alternates` that becomes a FIFO or names a new alternate after the open, within ten seconds |
| `unusual_ref_names_resolve_as_in_git` | A symbolic ref to `FOO` beside a packed `refs/heads/FOO`, and a ref path that crosses a file |
| `file_at_a_loose_folder_does_not_stop_a_packed_read` | A file at the fan-out folder of a packed commit |
| `commit_parents_parse_as_in_git` | Commits with no author, a bad author, upper-case digits, a short parent line, or a body that ends after its tree or parent line, which it compares with `git rev-list --parents` |
| `loose_ref_links_to_folders_are_missing` | Loose refs that are links to folders, and a real folder at the path of a packed ref, which it compares with `git rev-parse` |
| `multi_pack_index_never_blocks_a_read` | A FIFO at a pack that `multi-pack-index` names, within ten seconds |
| `gitdir_paths_are_read_as_git_reads_them` | A `.git` file whose path ends in a space beside a folder without the space, and one that names a file |
| `head_names_the_branch_at_the_end_of_the_chain` | `HEAD` through `refs/heads/alias` to `main` and to an unborn branch, which it compares with `git symbolic-ref HEAD` |
| `packed_refs_parse_as_in_git` | `packed-refs` with CRLF line ends, a bad name with no header, no last line feed and a line that is not a record, which it compares with `git rev-parse` |
| `open_works_in_a_removed_current_folder` | An open in a child process whose current folder was removed |
| `commit_limit_bounds_packed_and_deltified_commits` | `commit_bytes` at the size of a whole packed commit and of a deltified one |
| `bare_sha256_and_commondir_spaces_are_read_as_git_reads_them` | A bare SHA-256 repository with a detached `HEAD`, and `commondir` with a space or a line end |
| `worktree_private_refs_are_read_from_the_worktree` | `refs/worktree/x` in two worktrees and in `packed-refs`, `foo/bar` in both directories, and a ref into `main-worktree/` |
| `linked_loose_refs_are_read_as_git_reads_them` | A loose ref that is a link to `refs/heads/main`, and one that is a link to a file elsewhere |
| `subfolders_of_a_worktree_are_not_worktrees` | A subfolder of a worktree, its `.git` folder and `.git/refs` |
| `environment_and_global_configuration_are_ignored` | A child process with `GIT_DIR`, `GIT_CONFIG_COUNT` and a global `core.hooksPath` |

The unit test `read_entry_is_bounded_and_reads_only_regular_files` checks the
bounded read with a file, a folder, `/dev/zero`, a FIFO, a socket and links.
The unit test `unreadable_entries_are_io_errors` makes a folder unreadable with
mode `000`. A process that runs as root can still read the folder, so that test
fails as root.

## Known differences from Git and follow-ups

The reader refuses some layouts that Git reads:

- an `alternates` file that names a folder twice
- alternates more than 5 levels deep
- a short `parent` line in a commit
- a loose ref of the form `<hex>\0junk`
- a symbolic link with a target that is not a valid ref name
- a `gitdir:` path that ends in whitespace other than the line end

Other differences:

- The reader ignores `info/grafts`, which Git still applies.
- gix skips a pack whose `.idx` is a symbolic link.
- The reader finds no name outside `refs/` in `packed-refs`.
- Git binary-searches a `packed-refs` file whose header says `sorted`, even when
  its entries are out of order. The reader scans every line, so the two can give
  different answers for such a file.
- Some malformed layouts give `GitReadError::Io` where `NotARepository` would fit
  better.
- The code runs on Unix only.

Known risks:

- A pack file cut short while gix maps it can end the process with `SIGBUS`.
- Object reads have no limit on the memory that gix allocates for them.
- The reader reads `packed-refs` again at each step of a symbolic chain.
- A file can change between a check by the reader and the later read by gix.

Follow-ups:

- Give the doc comments of `src/git_reader.rs` a prose pass.
- Add tests for behavior that is already correct, from the lists of the test
  reviewers.

## Recommendations

Open the worktree again for each registration, each locate and each bind. Keep each new gix
call in `src/git_reader.rs`. Add each new write path of gix to `clippy.toml`, with
a line of `tests/git_write_lint.rs`.
