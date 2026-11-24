//! Read Git commits, refs and repository settings beside Git, and never write to Git.
//!
//! This module is the only place in Gossil that uses the gix crates. It opens one
//! Git worktree and reads what binding needs, by GIT-01 of the Git access contract.
//! Binding is the step that ties a Gossil change to a Git commit. The module reads
//! the Git directory and the common directory, `HEAD` and a branch tip. It also
//! reads a commit with its ordered parents, the object format and three configuration keys.
//!
//! The reader reads raw objects. It opens the object store with `gix_odb::at_opts`,
//! with no replacement table and without `multi-pack-index`. So `refs/replace` never
//! changes a result. It recomputes the hash of each commit that it reads, and it returns
//! an error for a mismatch.
//!
//! Each read has a finite limit from [`Limits`](crate::git_reader::Limits).
//! The reader itself reads `HEAD`, each ref, a `.git` file, `commondir`, each
//! configuration file and each `alternates` file. It reads each one without blocking
//! and within its limit. It parses a loose ref, `HEAD` and `packed-refs` itself, as Git
//! does, and gives the bytes of the configuration and of a `.git` file to the gix parsers.
//!
//! The reader refuses what the first release does not support, by GIT-04. It refuses
//! a shallow clone, a reftable repository, an unknown object format, an unknown
//! version-1 extension and a bare repository. It reads `core.bare` and
//! `core.worktree` from the files that Git reads for the worktree. So a linked
//! worktree of a bare repository opens, except when `extensions.worktreeConfig` makes the
//! common `core.bare` apply to it. The reader makes these checks each time it opens a
//! worktree.
//!
//! The lint `just lint` bans the write paths of the gix crates (GIT-03).
//! `tests/git_reader.rs` checks that a run of every call leaves the Git
//! directories unchanged. `tests/git_write_lint.rs` checks that no other source
//! file names a gix path.
//!
//! # Recommendations
//! Open a worktree for each registration, each locate and each bind, because a repository
//! can change its format between two runs. Keep every new gix call in this module.
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// `Limits` sets the finite bound of each read. The type has no unbounded default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum bytes of each configuration file, `config` or `config.worktree`.
    pub config_bytes: u64,
    /// Maximum bytes of `HEAD`, each loose ref, a `.git` file and `commondir`.
    pub small_file_bytes: u64,
    /// Maximum bytes of the `packed-refs` file.
    pub packed_refs_bytes: u64,
    /// Maximum number of refs that one lookup reads, `HEAD` included. Git's `SYMREF_MAXDEPTH` is five.
    pub symbolic_depth: usize,
    /// Maximum bytes of one commit object, after decompression.
    pub commit_bytes: u64,
    /// Maximum number of parents of one commit.
    pub parents: usize,
    /// Maximum bytes of each `objects/info/alternates` file.
    pub alternates_bytes: u64,
    /// Maximum number of alternate object directories, over all nested `alternates` files.
    pub alternates: usize,
}

impl Limits {
    /// `STANDARD` holds finite limits that fit ordinary repositories. They are 1 MiB of
    /// configuration, 4 KiB small files, 256 MiB of packed refs, depth 5 and 16 MiB commits.
    /// They also allow 1024 parents, and 64 KiB of `alternates` with 16 alternate directories.
    pub const STANDARD: Limits = Limits {
        config_bytes: 1 << 20,
        small_file_bytes: 4096,
        packed_refs_bytes: 256 << 20,
        symbolic_depth: 5,
        commit_bytes: 16 << 20,
        parents: 1024,
        alternates_bytes: 64 << 10,
        alternates: 16,
    };
}

/// The hash algorithm of a Git repository, from `extensions.objectFormat`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ObjectFormat {
    /// SHA-1 with 20-byte IDs, the default when the key is absent.
    Sha1,
    /// SHA-256 with 32-byte IDs.
    Sha256,
}

impl ObjectFormat {
    /// Return the name that Git uses for the format in configuration.
    ///
    /// # Returns
    /// `sha1` or `sha256`.
    pub fn name(self) -> &'static str {
        match self {
            ObjectFormat::Sha1 => "sha1",
            ObjectFormat::Sha256 => "sha256",
        }
    }

    /// Return the number of bytes in one full object ID of this format.
    ///
    /// # Returns
    /// 20 for SHA-1, 32 for SHA-256.
    pub fn id_bytes(self) -> usize {
        match self {
            ObjectFormat::Sha1 => 20,
            ObjectFormat::Sha256 => 32,
        }
    }

    /// Return the gix hash kind of this format.
    ///
    /// # Returns
    /// The matching `gix_hash::Kind`.
    fn kind(self) -> gix_hash::Kind {
        match self {
            ObjectFormat::Sha1 => gix_hash::Kind::Sha1,
            ObjectFormat::Sha256 => gix_hash::Kind::Sha256,
        }
    }
}

/// A full, immutable Git object ID with its hash algorithm.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GitObjectId {
    /// Hash algorithm of the ID.
    format: ObjectFormat,
    /// Digest bytes, whose length is always `format.id_bytes()`.
    bytes: Vec<u8>,
}

impl GitObjectId {
    /// Parse a full hexadecimal object ID of `format`, and refuse an abbreviated ID.
    ///
    /// # Arguments
    /// * `format` - Hash algorithm that the ID must have.
    /// * `hex` - Exactly 40 or 64 hexadecimal digits, by `format`, in upper or lower case.
    ///
    /// # Returns
    /// The parsed ID.
    ///
    /// # Errors
    /// Returns [`GitReadError::InvalidObjectId`] for a wrong length or a non-hex digit.
    ///
    /// # Examples
    /// ```
    /// use gossil::git_reader::{GitObjectId, GitReadError, ObjectFormat};
    ///
    /// let hex = "4B825DC642CB6EB9A060E54BF8D69288FBEE4904";
    /// let id = GitObjectId::from_hex(ObjectFormat::Sha1, hex).unwrap();
    /// assert_eq!(id.as_bytes().len(), 20);
    /// assert_eq!(id.to_string(), hex.to_ascii_lowercase());
    ///
    /// // An abbreviated ID, or a SHA-1 ID read as SHA-256, is refused.
    /// let short = GitObjectId::from_hex(ObjectFormat::Sha1, "4b825dc");
    /// assert!(matches!(short, Err(GitReadError::InvalidObjectId { .. })));
    /// let other = GitObjectId::from_hex(ObjectFormat::Sha256, hex);
    /// assert!(matches!(other, Err(GitReadError::InvalidObjectId { .. })));
    /// ```
    pub fn from_hex(format: ObjectFormat, hex: &str) -> Result<Self, GitReadError> {
        let invalid = || GitReadError::InvalidObjectId {
            text: hex.to_string(),
            format,
        };
        if hex.len() != format.id_bytes() * 2 {
            return Err(invalid());
        }
        let id = gix_hash::ObjectId::from_hex(hex.as_bytes()).map_err(|_| invalid())?;
        Ok(Self::from_gix(format, &id))
    }

    /// Return the hash algorithm of the ID.
    ///
    /// # Returns
    /// The object format.
    pub fn format(&self) -> ObjectFormat {
        self.format
    }

    /// Return the digest bytes of the ID.
    ///
    /// # Returns
    /// 20 or 32 bytes, by the format.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Return the ID as lowercase hexadecimal digits, as Git prints it.
    ///
    /// # Returns
    /// 40 or 64 digits, by the format.
    pub fn to_hex(&self) -> String {
        self.bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// Copy a gix ID that a reader of `format` produced.
    ///
    /// # Arguments
    /// * `format` - Format of the repository that produced `id`.
    /// * `id` - Digest that gix parsed or read with the hash kind of `format`.
    ///
    /// # Returns
    /// The ID in the type of this module.
    fn from_gix(format: ObjectFormat, id: &gix_hash::oid) -> Self {
        Self {
            format,
            bytes: id.as_bytes().to_vec(),
        }
    }

    /// Convert the ID to the gix type, for a lookup.
    ///
    /// # Returns
    /// The owned gix ID with the same bytes.
    ///
    /// # Errors
    /// Returns [`GitReadError::InvalidObjectId`] if gix refuses the digest length.
    fn to_gix(&self) -> Result<gix_hash::ObjectId, GitReadError> {
        gix_hash::ObjectId::try_from(self.bytes.as_slice()).map_err(|_| {
            GitReadError::InvalidObjectId {
                text: self.to_hex(),
                format: self.format,
            }
        })
    }
}

impl fmt::Display for GitObjectId {
    /// Write the ID as lowercase hexadecimal digits.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// The type of a Git object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    /// A commit object.
    Commit,
    /// A tree object.
    Tree,
    /// A blob object.
    Blob,
    /// An annotated tag object.
    Tag,
}

impl ObjectKind {
    /// Return the name that Git uses for the type.
    ///
    /// # Returns
    /// `commit`, `tree`, `blob` or `tag`.
    pub fn name(self) -> &'static str {
        match self {
            ObjectKind::Commit => "commit",
            ObjectKind::Tree => "tree",
            ObjectKind::Blob => "blob",
            ObjectKind::Tag => "tag",
        }
    }

    /// Convert a gix object kind.
    ///
    /// # Arguments
    /// * `kind` - Type that the object store reported.
    ///
    /// # Returns
    /// The matching type of this module.
    fn from_gix(kind: gix_object::Kind) -> Self {
        match kind {
            gix_object::Kind::Commit => ObjectKind::Commit,
            gix_object::Kind::Tree => ObjectKind::Tree,
            gix_object::Kind::Blob => ObjectKind::Blob,
            gix_object::Kind::Tag => ObjectKind::Tag,
        }
    }
}

/// The state of `HEAD` in one worktree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Head {
    /// `HEAD` names a ref, normally a branch. `tip` is `None` for an unborn branch.
    Branch {
        /// Full name of the last ref of the symbolic chain, such as `refs/heads/main`, which is
        /// the branch that `git symbolic-ref HEAD` names.
        name: Vec<u8>,
        /// Object that the ref resolves to, or `None` when the ref does not exist yet.
        tip: Option<GitObjectId>,
    },
    /// `HEAD` holds an object ID directly.
    Detached(GitObjectId),
}

/// A commit object that the reader checked, with its raw parents in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitCommit {
    /// ID of the commit, which equals its recomputed hash.
    id: GitObjectId,
    /// Parents in the order of the `parent` lines of the raw object.
    parents: Vec<GitObjectId>,
}

impl GitCommit {
    /// Return the ID of the commit, whose recomputed hash the reader checked.
    ///
    /// # Returns
    /// The full commit ID.
    pub fn id(&self) -> &GitObjectId {
        &self.id
    }

    /// Return the raw parents of the commit, in the order of its `parent` lines.
    ///
    /// # Returns
    /// The parent IDs, or an empty list for a root commit.
    pub fn parents(&self) -> &[GitObjectId] {
        &self.parents
    }
}

/// The underlying error of a failed read, kept as the source of a [`GitReadError`].
///
/// Two causes compare equal when their messages are equal, so the error type can derive `PartialEq`.
#[derive(Clone, Debug)]
pub struct Cause(Arc<dyn std::error::Error + Send + Sync + 'static>);

impl Cause {
    /// Keep `error` as a cause.
    ///
    /// # Arguments
    /// * `error` - Error that the operating system or gix returned.
    ///
    /// # Returns
    /// The shared cause.
    fn new(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Cause(Arc::new(error))
    }

    /// Make a cause from a message of this module.
    ///
    /// # Arguments
    /// * `text` - Message that names the problem.
    ///
    /// # Returns
    /// The shared cause.
    fn text(text: &str) -> Self {
        Cause(Arc::from(Box::<dyn std::error::Error + Send + Sync>::from(
            text,
        )))
    }

    /// Return the underlying error.
    ///
    /// # Returns
    /// The error that the cause holds.
    pub fn error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
        self.0.as_ref()
    }
}

impl fmt::Display for Cause {
    /// Write the message of the underlying error.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl PartialEq for Cause {
    /// Compare two causes by their messages.
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

impl Eq for Cause {}

/// The three configuration keys of the repository's own files, after the support checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryConfig {
    /// `extensions.objectFormat`, or SHA-1 when the key is absent.
    pub object_format: ObjectFormat,
    /// Raw `extensions.refStorage`. An opened repository has `None` or `files`.
    pub ref_storage: Option<Vec<u8>>,
    /// Raw `core.hooksPath` of `config`, or of `config.worktree` when that file applies.
    pub hooks_path: Option<Vec<u8>>,
    /// True when a read file has an `include` or `includeIf` section, which the reader does not follow.
    pub unread_includes: bool,
}

/// `GitReadError` names each reason why the reader refuses or fails, with what the user must know.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitReadError {
    /// A file system call failed.
    Io {
        /// Path of the failed call.
        path: PathBuf,
        /// Underlying error, also given by `source()`.
        cause: Cause,
    },
    /// The folder has no `.git` entry and is not a Git directory.
    NotAWorktree {
        /// Folder that was opened.
        path: PathBuf,
    },
    /// The `.git` entry does not lead to a valid Git directory.
    NotARepository {
        /// The `.git` entry.
        path: PathBuf,
        /// Underlying error, also given by `source()`.
        cause: Cause,
    },
    /// The folder is a bare repository, which is not a location (SRC-09).
    Bare {
        /// Git directory of the bare repository.
        path: PathBuf,
    },
    /// The repository is a shallow clone, which the first release refuses (P10).
    Shallow {
        /// Common Git directory that holds the file `shallow`.
        common_dir: PathBuf,
    },
    /// The repository uses the reftable ref backend, which the first release refuses.
    Reftable {
        /// Common Git directory of the repository.
        common_dir: PathBuf,
    },
    /// `extensions.objectFormat` names a format other than `sha1` or `sha256`.
    UnsupportedObjectFormat {
        /// Raw value of the key.
        value: Vec<u8>,
    },
    /// `extensions.refStorage` names a backend other than `files` or `reftable`.
    UnsupportedRefStorage {
        /// Raw value of the key.
        value: Vec<u8>,
    },
    /// `core.repositoryformatversion` is not 0 or 1, or is 0 with a version-1 extension.
    UnsupportedRepositoryVersion {
        /// Value of the key.
        version: i64,
    },
    /// `core.worktree` names a folder other than the opened one, so Git's worktree is elsewhere.
    ForeignWorktree {
        /// Folder that was opened.
        worktree: PathBuf,
        /// Worktree that `core.worktree` names, joined to the Git directory.
        configured: PathBuf,
    },
    /// Version 1 names an extension that the reader does not understand, which Git also refuses.
    UnsupportedExtension {
        /// Name of the key below `extensions`.
        name: String,
    },
    /// A configuration file or value could not be parsed.
    Config {
        /// File that was read.
        path: PathBuf,
        /// Underlying error, also given by `source()`.
        cause: Cause,
    },
    /// A file or object is larger than its limit.
    TooLarge {
        /// What was read, such as a path or an object ID.
        what: String,
        /// Size that was found.
        size: u64,
        /// Limit that applied.
        limit: u64,
    },
    /// An entry that must be a regular file is a FIFO, a device, a socket, a folder, or a
    /// symbolic link that the reader does not follow.
    NotAFile {
        /// Path of the entry.
        path: PathBuf,
        /// What the entry is, such as `FIFO` or `character device`.
        kind: &'static str,
    },
    /// An `objects/info/alternates` file names an alternate that the reader does not use, or the
    /// alternates changed after the open.
    Alternate {
        /// The `alternates` file.
        path: PathBuf,
        /// What is wrong with the alternate.
        cause: Cause,
    },
    /// A text is not a full object ID of the format.
    InvalidObjectId {
        /// Text that was given.
        text: String,
        /// Format that the ID needed.
        format: ObjectFormat,
    },
    /// An object ID has a format other than the format of the repository.
    FormatMismatch {
        /// ID that was given.
        id: GitObjectId,
        /// Format of the repository.
        repository: ObjectFormat,
    },
    /// A branch name does not make a valid full ref name.
    InvalidBranchName {
        /// Name that was given.
        name: String,
    },
    /// A ref could not be read or parsed.
    Ref {
        /// Full name of the ref.
        name: Vec<u8>,
        /// Underlying error, also given by `source()`.
        cause: Cause,
    },
    /// A chain of symbolic refs is deeper than the limit.
    SymbolicDepth {
        /// Full name where the lookup started.
        name: Vec<u8>,
        /// Limit that applied.
        limit: usize,
    },
    /// The object store failed to read an object.
    ObjectRead {
        /// ID of the object.
        id: GitObjectId,
        /// Underlying error, also given by `source()`.
        cause: Cause,
    },
    /// No object with the ID is present, not even in an alternate (COMP-T06).
    ObjectMissing {
        /// ID of the object.
        id: GitObjectId,
    },
    /// The object exists, but it is not a commit.
    NotACommit {
        /// ID of the object.
        id: GitObjectId,
        /// Its actual type.
        kind: ObjectKind,
    },
    /// The recomputed hash of the object differs from its ID.
    HashMismatch {
        /// ID that was requested.
        id: GitObjectId,
        /// Hash of the bytes that were read.
        computed: GitObjectId,
    },
    /// The commit body could not be parsed up to its parents.
    MalformedCommit {
        /// ID of the commit.
        id: GitObjectId,
        /// Underlying error, also given by `source()`.
        cause: Cause,
    },
    /// The commit has more parents than the limit.
    TooManyParents {
        /// ID of the commit.
        id: GitObjectId,
        /// Limit that applied.
        limit: usize,
    },
}

impl fmt::Display for GitReadError {
    /// Write a message that names the reason, and the remedy when one exists.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lossy = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
        match self {
            Self::Io { path, cause } => write!(f, "cannot read {}: {cause}", path.display()),
            Self::NotAWorktree { path } => write!(f, "{} is not a Git worktree", path.display()),
            Self::NotARepository { path, cause } => {
                write!(
                    f,
                    "{} does not lead to a Git repository: {cause}",
                    path.display()
                )
            }
            Self::Bare { path } => write!(
                f,
                "{} is a bare Git repository, which is not a location; name a worktree",
                path.display()
            ),
            Self::Shallow { common_dir } => write!(
                f,
                "{} is a shallow clone; Gossil does not support shallow clones in the first release",
                common_dir.display()
            ),
            Self::Reftable { common_dir } => write!(
                f,
                "{} uses the reftable ref backend; Gossil supports only the files backend for now",
                common_dir.display()
            ),
            Self::UnsupportedObjectFormat { value } => {
                write!(f, "unsupported extensions.objectFormat {:?}", lossy(value))
            }
            Self::UnsupportedRefStorage { value } => {
                write!(f, "unsupported extensions.refStorage {:?}", lossy(value))
            }
            Self::UnsupportedRepositoryVersion { version } => {
                write!(f, "unsupported core.repositoryformatversion {version}")
            }
            Self::ForeignWorktree {
                worktree,
                configured,
            } => write!(
                f,
                "core.worktree of {} names {}; open the worktree that Git uses",
                worktree.display(),
                configured.display()
            ),
            Self::NotAFile { path, kind } => write!(
                f,
                "{} is a {kind}, not a regular file; Gossil reads only regular files in a Git directory",
                path.display()
            ),
            Self::Alternate { path, cause } => {
                write!(f, "cannot use an alternate of {}: {cause}", path.display())
            }
            Self::UnsupportedExtension { name } => {
                write!(f, "unsupported repository extension extensions.{name}")
            }
            Self::Config { path, cause } => {
                write!(
                    f,
                    "cannot parse the configuration {}: {cause}",
                    path.display()
                )
            }
            Self::TooLarge { what, size, limit } => {
                write!(
                    f,
                    "{what} has {size} bytes or items, over the limit of {limit}"
                )
            }
            Self::InvalidObjectId { text, format } => {
                write!(f, "{text:?} is not a full {} object ID", format.name())
            }
            Self::FormatMismatch { id, repository } => write!(
                f,
                "{id} is a {} ID, but the repository uses {}",
                id.format.name(),
                repository.name()
            ),
            Self::InvalidBranchName { name } => write!(f, "{name:?} is not a valid branch name"),
            Self::Ref { name, cause } => write!(f, "cannot read ref {}: {cause}", lossy(name)),
            Self::SymbolicDepth { name, limit } => {
                write!(
                    f,
                    "ref {} has more than {limit} symbolic links",
                    lossy(name)
                )
            }
            Self::ObjectRead { id, cause } => write!(f, "cannot read object {id}: {cause}"),
            Self::ObjectMissing { id } => write!(f, "object {id} is not in the repository"),
            Self::NotACommit { id, kind } => {
                write!(f, "object {id} is a {}, not a commit", kind.name())
            }
            Self::HashMismatch { id, computed } => {
                write!(
                    f,
                    "object {id} has a hash mismatch: its bytes hash to {computed}"
                )
            }
            Self::MalformedCommit { id, cause } => write!(f, "commit {id} is malformed: {cause}"),
            Self::TooManyParents { id, limit } => {
                write!(f, "commit {id} has more than {limit} parents")
            }
        }
    }
}

impl std::error::Error for GitReadError {
    /// Return the underlying error of a failed read, when one exists.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { cause, .. }
            | Self::NotARepository { cause, .. }
            | Self::Config { cause, .. }
            | Self::Ref { cause, .. }
            | Self::ObjectRead { cause, .. }
            | Self::Alternate { cause, .. }
            | Self::MalformedCommit { cause, .. } => Some(cause.error()),
            _ => None,
        }
    }
}

/// Make an I/O error value for `path`.
///
/// # Arguments
/// * `path` - Path of the failed call.
/// * `error` - Error that the call returned.
///
/// # Returns
/// The typed error.
fn io_error(path: &Path, error: std::io::Error) -> GitReadError {
    GitReadError::Io {
        path: path.to_path_buf(),
        cause: Cause::new(error),
    }
}

/// Report whether `path` exists, without following a final symbolic link.
///
/// # Arguments
/// * `path` - Path to inspect.
///
/// # Returns
/// Whether an entry exists.
///
/// # Errors
/// Returns [`GitReadError::Io`] for an error other than a missing entry.
fn exists(path: &Path) -> Result<bool, GitReadError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(path, error)),
    }
}

/// What [`read_entry`] found at a path.
#[derive(Debug, PartialEq, Eq)]
enum Entry {
    /// No entry exists.
    Missing,
    /// The ref is missing, with no look in `packed-refs`, as Git reads it.
    ///
    /// Either a folder on the path is a file, so no entry can exist, or, with
    /// [`Links::RefOrFollow`], the entry is a symbolic link to a folder.
    Blocked,
    /// The entry is a folder, or, except with [`Links::RefOrFollow`], a symbolic link to one.
    Folder,
    /// The entry is a symbolic link whose target starts with `refs/`, which Git reads as a
    /// symbolic ref to that name.
    Link(Vec<u8>),
    /// The bytes of a regular file, at most the limit.
    File(Vec<u8>),
}

/// How [`read_entry`] treats a symbolic link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Links {
    /// Follow the link. Its target must be a regular file or a folder.
    Follow,
    /// A link to `refs/...` is a symbolic ref, as Git reads a loose ref. Other links are followed.
    ///
    /// A followed link to a folder gives [`Entry::Blocked`], because Git reads it as a missing ref.
    RefOrFollow,
    /// A link to `refs/...` is a symbolic ref. Any other link is refused, as Git refuses it for `HEAD`.
    ///
    /// The open also uses `O_NOFOLLOW`, so a link that replaces the file after the check is not followed.
    RefOnly,
}

/// Name the kind of a file that is not a regular file or a folder.
///
/// # Arguments
/// * `kind` - File type of the opened entry.
///
/// # Returns
/// A name such as `FIFO`.
fn special_kind(kind: fs::FileType) -> &'static str {
    use std::os::unix::fs::FileTypeExt;
    match () {
        () if kind.is_fifo() => "FIFO",
        () if kind.is_char_device() => "character device",
        () if kind.is_block_device() => "block device",
        () if kind.is_socket() => "socket",
        () => "special file",
    }
}

/// Read one entry of a Git directory within `limit`, without blocking on a special file.
///
/// The open uses `O_NONBLOCK`, so a FIFO cannot stall it. With [`Links::RefOnly`] it also uses
/// `O_NOFOLLOW`, so a link that replaces the file after the check is not followed. The type check uses the opened
/// handle, so a swap after a check cannot pass. The read takes at most `limit + 1` bytes, so a file
/// that grows during the read is refused, not read without a bound.
///
/// # Arguments
/// * `path` - Entry to read.
/// * `limit` - Largest size in bytes.
/// * `links` - How to treat a symbolic link.
///
/// # Returns
/// What the entry is, with the bytes of a regular file.
///
/// # Errors
/// Returns [`GitReadError::NotAFile`] for a FIFO, a device, a socket, a dangling link or a refused link,
/// [`GitReadError::TooLarge`] for more bytes than `limit`, or [`GitReadError::Io`].
fn read_entry(path: &Path, limit: u64, links: Links) -> Result<Entry, GitReadError> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Entry::Missing),
        Err(error) if error.kind() == std::io::ErrorKind::NotADirectory => {
            return Ok(Entry::Blocked);
        }
        Err(error) => return Err(io_error(path, error)),
    };
    let link = meta.file_type().is_symlink();
    if link && links != Links::Follow {
        let target = fs::read_link(path).map_err(|error| io_error(path, error))?;
        let target = target.as_os_str().as_bytes();
        if target.starts_with(b"refs/") {
            return Ok(Entry::Link(target.to_vec()));
        }
        if links == Links::RefOnly {
            return Err(GitReadError::NotAFile {
                path: path.to_path_buf(),
                kind: "symbolic link outside refs/",
            });
        }
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(match links {
            // A link that replaced the file after the check is not followed.
            Links::RefOnly => libc::O_NONBLOCK | libc::O_NOFOLLOW,
            _ => libc::O_NONBLOCK,
        })
        .open(path);
    let file = match file {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(GitReadError::NotAFile {
                path: path.to_path_buf(),
                kind: "dangling symbolic link",
            });
        }
        Err(error) => return Err(io_error(path, error)),
    };
    let meta = file.metadata().map_err(|error| io_error(path, error))?;
    if meta.is_dir() {
        // Git reads a loose ref that is a link to a folder as missing, with no look in
        // `packed-refs`.
        return Ok(match link && links == Links::RefOrFollow {
            true => Entry::Blocked,
            false => Entry::Folder,
        });
    }
    if !meta.is_file() {
        return Err(GitReadError::NotAFile {
            path: path.to_path_buf(),
            kind: special_kind(meta.file_type()),
        });
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| io_error(path, error))?;
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size > limit {
        return Err(GitReadError::TooLarge {
            what: path.display().to_string(),
            size: size.max(meta.len()),
            limit,
        });
    }
    Ok(Entry::File(bytes))
}

/// Read an entry that must be a regular file, or absent.
///
/// # Arguments
/// * `path` - File to read.
/// * `limit` - Largest size in bytes.
///
/// # Returns
/// The bytes, or `None` when no entry exists.
///
/// # Errors
/// Returns [`GitReadError::NotAFile`] for a folder or a special file, and the errors of [`read_entry`].
fn read_file(path: &Path, limit: u64) -> Result<Option<Vec<u8>>, GitReadError> {
    match read_entry(path, limit, Links::Follow)? {
        Entry::Missing | Entry::Blocked => Ok(None),
        Entry::File(bytes) => Ok(Some(bytes)),
        Entry::Folder | Entry::Link(_) => Err(GitReadError::NotAFile {
            path: path.to_path_buf(),
            kind: "folder",
        }),
    }
}

/// Read and parse one configuration file without includes, within the limit.
///
/// # Arguments
/// * `path` - Configuration file.
/// * `source` - Level of the file, for the gix metadata.
/// * `limit` - Largest size in bytes.
///
/// # Returns
/// The parsed file, or `None` when the file does not exist.
///
/// # Errors
/// Returns the errors of [`read_file`], or [`GitReadError::Config`].
fn load_config(
    path: &Path,
    source: gix_config::Source,
    limit: u64,
) -> Result<Option<gix_config::File>, GitReadError> {
    let Some(bytes) = read_file(path, limit)? else {
        return Ok(None);
    };
    let meta = gix_config::file::Metadata::from(source);
    gix_config::File::from_bytes_no_includes(&bytes, meta, Default::default())
        .map(Some)
        .map_err(|error| config_error(path, error))
}

/// Deepest level of a nested `alternates` file that Git reads, from `link_alt_odb_entries` of Git.
///
/// The top file is at level 0, so Git reads six files and ignores a deeper one.
const ALTERNATE_DEPTH: usize = 5;

/// Check the alternate object directories below `objects_dir` before gix reads them.
///
/// Each `info/alternates` file must be a regular file within `alternates_bytes`, and each
/// alternate must be a folder. An alternate in a file deeper than level [`ALTERNATE_DEPTH`] is
/// refused, and at most `alternates` alternates may exist in all.
///
/// The check splits lines as gix does, with no trim. It refuses three cases that gix would
/// resolve otherwise than the check or than Git. They are a quoted path, a relative path in a
/// nested file, and a folder that appears twice, as in a cycle, a repeated line or a folder that
/// names itself.
///
/// # Arguments
/// * `objects_dir` - The `objects` folder of the common directory.
/// * `limits` - Read limits.
///
/// # Returns
/// The canonical object folders, `objects_dir` first.
///
/// # Errors
/// Returns [`GitReadError::Alternate`], or the errors of [`read_file`].
fn check_alternates(objects_dir: &Path, limits: &Limits) -> Result<Vec<PathBuf>, GitReadError> {
    let mut seen = vec![canonical(objects_dir)?];
    let mut stack = vec![(seen[0].clone(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let path = dir.join("info").join("alternates");
        let Some(bytes) = read_file(&path, limits.alternates_bytes)? else {
            continue;
        };
        let refuse = |text: String| GitReadError::Alternate {
            path: path.clone(),
            cause: Cause::text(&text),
        };
        for line in bytes.split(|byte| *byte == b'\n') {
            if line.is_empty() || line.starts_with(b"#") {
                continue;
            }
            if line.starts_with(b"\"") {
                return Err(refuse("a quoted alternate path is not supported".into()));
            }
            if depth > ALTERNATE_DEPTH {
                return Err(refuse(format!(
                    "alternates nest deeper than {ALTERNATE_DEPTH}"
                )));
            }
            if seen.len() > limits.alternates {
                return Err(refuse(format!(
                    "more than {} alternates",
                    limits.alternates
                )));
            }
            // Git resolves a relative path against the folder of its `alternates` file, but gix
            // against the top `objects` folder, so the two agree only at the top.
            if depth > 0 && !line.starts_with(b"/") {
                return Err(refuse(
                    "a relative path in a nested alternates file is not supported".into(),
                ));
            }
            let alternate = dir.join(gix_path_of(line));
            let alternate = match fs::metadata(&alternate) {
                Ok(meta) if meta.is_dir() => canonical(&alternate)?,
                _ => {
                    return Err(refuse(format!("{} is not a folder", alternate.display())));
                }
            };
            // gix refuses a cycle, so the reader refuses a folder that it has seen.
            if seen.contains(&alternate) {
                return Err(refuse(format!(
                    "{} appears twice, as in a cycle",
                    alternate.display()
                )));
            }
            seen.push(alternate.clone());
            stack.push((alternate, depth + 1));
        }
    }
    Ok(seen)
}

/// Make a configuration error value for `path`.
///
/// # Arguments
/// * `path` - File that was read.
/// * `error` - Error that gix returned.
///
/// # Returns
/// The typed error.
fn config_error(
    path: &Path,
    error: impl std::error::Error + Send + Sync + 'static,
) -> GitReadError {
    GitReadError::Config {
        path: path.to_path_buf(),
        cause: Cause::new(error),
    }
}

/// Report whether a configuration file has an include section.
///
/// # Arguments
/// * `file` - Parsed configuration, or `None` for an absent file.
///
/// # Returns
/// True when an `include` or `includeIf` section exists.
fn has_includes(file: Option<&gix_config::File>) -> bool {
    file.is_some_and(|file| {
        file.sections_by_name("include").is_some() || file.sections_by_name("includeIf").is_some()
    })
}

/// The extensions that Git knows in format versions 0 and 1, in lower case, as in Git's `setup.c`.
const EXTENSIONS_V0: [&str; 4] = ["noop", "preciousobjects", "partialclone", "worktreeconfig"];

/// The extensions that Git knows only in format version 1, in lower case. Git refuses each of them
/// in version 0.
const EXTENSIONS_V1: [&str; 6] = [
    "noop-v1",
    "objectformat",
    "compatobjectformat",
    "refstorage",
    "relativeworktrees",
    "submodulepathconfig",
];

/// The version-1 extensions that Git knows but the reader does not support.
const UNSUPPORTED_EXTENSIONS: [&str; 1] = ["compatobjectformat"];

/// The extensions that Git refuses when they have no value, such as a bare `objectFormat` line.
const VALUED_EXTENSIONS: [&str; 4] = [
    "partialclone",
    "objectformat",
    "compatobjectformat",
    "refstorage",
];

/// Check the keys below `extensions` as Git's `setup.c` does.
///
/// A key in a subsection, such as `[extensions "x"] noop`, is unknown to Git. In version 0 Git
/// ignores an unknown key and refuses a key of [`EXTENSIONS_V1`]. In version 1 Git refuses an
/// unknown key, and the reader also refuses each key of [`UNSUPPORTED_EXTENSIONS`].
///
/// # Arguments
/// * `file` - Parsed `config` of the common directory.
/// * `path` - Path of that file, for errors.
/// * `version` - Value of `core.repositoryformatversion`, 0 or 1.
///
/// # Returns
/// Success when Git and the reader both accept the extensions.
///
/// # Errors
/// Returns [`GitReadError::Config`] for a key of [`VALUED_EXTENSIONS`] without a value,
/// [`GitReadError::UnsupportedRepositoryVersion`] for a version-1 key in version 0, and
/// [`GitReadError::UnsupportedExtension`] for an unknown or unsupported key in version 1.
fn check_extensions(
    file: &gix_config::File,
    path: &Path,
    version: i64,
) -> Result<(), GitReadError> {
    for section in file.sections_by_name("extensions").into_iter().flatten() {
        let subsection = section
            .header()
            .subsection_name()
            .map(|name| name.to_string());
        for name in section.value_names() {
            let key = name.to_ascii_lowercase();
            if subsection.is_none()
                && VALUED_EXTENSIONS.contains(&key.as_str())
                && section
                    .value_implicit(&name)
                    .is_some_and(|value| value.is_none())
            {
                return Err(GitReadError::Config {
                    path: path.to_path_buf(),
                    cause: Cause::text(&format!("extensions.{key} has no value")),
                });
            }
            let (known_v0, known_v1) = match subsection {
                Some(_) => (false, false),
                None => (
                    EXTENSIONS_V0.contains(&key.as_str()),
                    EXTENSIONS_V1.contains(&key.as_str()),
                ),
            };
            if version == 0 && known_v1 {
                return Err(GitReadError::UnsupportedRepositoryVersion { version });
            }
            let unsupported = UNSUPPORTED_EXTENSIONS.contains(&key.as_str());
            if version == 1 && (!(known_v0 || known_v1) || unsupported) {
                let name = match &subsection {
                    Some(subsection) => format!("{subsection}.{name}"),
                    None => name,
                };
                return Err(GitReadError::UnsupportedExtension { name });
            }
        }
    }
    Ok(())
}

/// Read the repository configuration and apply the support checks of GIT-04.
///
/// # Arguments
/// * `common_dir` - Common Git directory, which holds `config`.
/// * `git_dir` - Git directory of the worktree, which may hold `config.worktree`.
/// * `limits` - Read limits.
///
/// # Returns
/// The configuration keys, and the layout keys of [`Layout`].
///
/// # Errors
/// Returns the refusal for reftable, an unknown format or backend, an unknown
/// version, the errors of [`check_extensions`] and the errors of [`load_config`].
fn read_config(
    common_dir: &Path,
    git_dir: &Path,
    limits: &Limits,
) -> Result<(RepositoryConfig, Layout), GitReadError> {
    let path = common_dir.join("config");
    let local = load_config(&path, gix_config::Source::Local, limits.config_bytes)?;
    let empty = gix_config::File::default();
    let file = local.as_ref().unwrap_or(&empty);
    let version = file
        .integer("core.repositoryformatversion")
        .map_err(|error| config_error(&path, error))?
        .unwrap_or(0);
    let format_value = file.string("extensions.objectFormat").map(|v| v.to_vec());
    let ref_storage = file.string("extensions.refStorage").map(|v| v.to_vec());
    if ref_storage.as_deref() == Some(b"reftable") {
        return Err(GitReadError::Reftable {
            common_dir: common_dir.to_path_buf(),
        });
    }
    if !(0..=1).contains(&version) {
        return Err(GitReadError::UnsupportedRepositoryVersion { version });
    }
    check_extensions(file, &path, version)?;
    if let Some(value) = ref_storage
        .as_ref()
        .filter(|value| value.as_slice() != b"files")
    {
        return Err(GitReadError::UnsupportedRefStorage {
            value: value.clone(),
        });
    }
    let object_format = match format_value.as_deref() {
        None | Some(b"sha1") => ObjectFormat::Sha1,
        Some(b"sha256") => ObjectFormat::Sha256,
        Some(value) => {
            return Err(GitReadError::UnsupportedObjectFormat {
                value: value.to_vec(),
            });
        }
    };
    let mut layout = Layout {
        common_bare: file
            .boolean("core.bare")
            .map_err(|error| config_error(&path, error))?
            .unwrap_or(false),
        common_worktree: file.string("core.worktree").map(|v| v.to_vec()),
        own_bare: None,
        own_worktree: None,
        worktree_config: false,
    };
    let worktree_config = file
        .boolean("extensions.worktreeConfig")
        .map_err(|error| config_error(&path, error))?
        .unwrap_or(false);
    let mut hooks_path = file.string("core.hooksPath").map(|v| v.to_vec());
    let mut unread_includes = has_includes(local.as_ref());
    layout.worktree_config = worktree_config;
    if worktree_config {
        let path = git_dir.join("config.worktree");
        let worktree = load_config(&path, gix_config::Source::Worktree, limits.config_bytes)?;
        if let Some(value) = worktree
            .as_ref()
            .and_then(|file| file.string("core.hooksPath"))
        {
            hooks_path = Some(value.to_vec());
        }
        if let Some(file) = worktree.as_ref() {
            layout.own_bare = file
                .boolean("core.bare")
                .map_err(|error| config_error(&path, error))?;
            layout.own_worktree = file.string("core.worktree").map(|v| v.to_vec());
        }
        unread_includes |= has_includes(worktree.as_ref());
    }
    let config = RepositoryConfig {
        object_format,
        ref_storage,
        hooks_path,
        unread_includes,
    };
    Ok((config, layout))
}

/// The keys that decide where the worktree of a repository is, from its configuration files.
///
/// Without `extensions.worktreeConfig`, Git reads `core.bare` and `core.worktree` of the common
/// `config` only for a main worktree. With it, the common values apply to each worktree, and
/// `config.worktree` of a worktree overrides them, as `git rev-parse` shows.
struct Layout {
    /// `core.bare` of the common `config`, false when absent.
    common_bare: bool,
    /// Raw `core.worktree` of the common `config`.
    common_worktree: Option<Vec<u8>>,
    /// `core.bare` of `config.worktree` of this worktree.
    own_bare: Option<bool>,
    /// Raw `core.worktree` of `config.worktree` of this worktree.
    own_worktree: Option<Vec<u8>>,
    /// `extensions.worktreeConfig`, which makes the common values apply to each worktree.
    worktree_config: bool,
}

/// Make the canonical absolute form of `path`.
///
/// # Arguments
/// * `path` - Existing path.
///
/// # Returns
/// The canonical path.
///
/// # Errors
/// Returns [`GitReadError::Io`] when the path cannot be resolved.
fn canonical(path: &Path) -> Result<PathBuf, GitReadError> {
    fs::canonicalize(path).map_err(|error| io_error(path, error))
}

/// One opened Git worktree, after the support checks, with a read-only object store.
pub struct GitRepository {
    /// Canonical top folder of the worktree.
    worktree: PathBuf,
    /// Canonical Git directory of the worktree.
    git_dir: PathBuf,
    /// Canonical common Git directory, which equals `git_dir` in the main worktree.
    common_dir: PathBuf,
    /// Configuration keys after the support checks.
    config: RepositoryConfig,
    /// Read limits of every later call.
    limits: Limits,
    /// Object store over loose objects, packs and alternates, with no replacements.
    objects: gix_odb::Handle,
    /// Canonical object folders, the repository's own folder first and then each alternate.
    object_dirs: Vec<PathBuf>,
}

impl fmt::Debug for GitRepository {
    /// Write the paths and the configuration of the opened worktree.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitRepository")
            .field("worktree", &self.worktree)
            .field("git_dir", &self.git_dir)
            .field("common_dir", &self.common_dir)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl GitRepository {
    /// Open the Git worktree whose top folder is `worktree`, and check that the first release supports it.
    ///
    /// The folder must hold the `.git` entry itself. The reader searches no parent
    /// folder and reads no Git environment variable.
    ///
    /// # Arguments
    /// * `worktree` - Top folder of a main or linked worktree.
    /// * `limits` - Bounds of this call and of every later call.
    ///
    /// # Returns
    /// The opened repository.
    ///
    /// # Errors
    /// Returns one of these errors:
    /// * [`GitReadError::NotAWorktree`] for a folder without a `.git` entry.
    /// * [`GitReadError::NotARepository`] for a `.git` entry that leads to no valid Git directory,
    ///   a `.git` file whose path ends in whitespace, or a `.git` file that names no folder.
    /// * [`GitReadError::Shallow`], [`GitReadError::Reftable`], [`GitReadError::Bare`],
    ///   [`GitReadError::ForeignWorktree`] or [`GitReadError::UnsupportedExtension`].
    /// * [`GitReadError::TooLarge`] for a `HEAD`, a `.git` file or `commondir` over
    ///   `small_file_bytes`, for a configuration file over `config_bytes`, or for an
    ///   `alternates` file over `alternates_bytes`.
    /// * [`GitReadError::NotAFile`] for one of those entries that is a FIFO, a device,
    ///   a socket, a dangling or refused symbolic link, or a folder other than `.git`.
    /// * [`GitReadError::Ref`] for a `HEAD` that cannot be parsed.
    /// * [`GitReadError::NotARepository`] also for a `HEAD` that names a ref outside `refs/`,
    ///   or a `commondir` that is empty after its line ends.
    /// * [`GitReadError::Alternate`] for alternates over the count or the depth, an
    ///   alternate that is not a folder, a quoted path, a relative path in a nested
    ///   `alternates` file, or a folder that appears twice.
    /// * [`GitReadError::Config`] for a configuration that cannot be parsed.
    /// * An error for an unsupported format or version, or for a read error.
    ///
    /// # Examples
    /// ```
    /// use gossil::git_reader::{GitReadError, GitRepository, Head, Limits};
    /// use std::path::Path;
    ///
    /// /// Return the parents of the commit at `HEAD`, or `None` for an unborn branch.
    /// fn head_parents(worktree: &Path) -> Result<Option<Vec<String>>, GitReadError> {
    ///     let repo = GitRepository::open(worktree, Limits::STANDARD)?;
    ///     let tip = match repo.head()? {
    ///         Head::Branch { tip, .. } => tip,
    ///         Head::Detached(id) => Some(id),
    ///     };
    ///     let Some(tip) = tip else { return Ok(None) };
    ///     let commit = repo.read_commit(&tip)?;
    ///     Ok(Some(commit.parents().iter().map(|id| id.to_hex()).collect()))
    /// }
    ///
    /// // A folder without a `.git` entry is not a worktree. The open searches no parent folder.
    /// let folder = tempfile::tempdir().unwrap();
    /// let error = head_parents(folder.path()).unwrap_err();
    /// assert!(matches!(error, GitReadError::NotAWorktree { .. }));
    /// ```
    pub fn open(worktree: &Path, limits: Limits) -> Result<Self, GitReadError> {
        let worktree = canonical(worktree)?;
        let dot_git = worktree.join(".git");
        if !exists(&dot_git)? {
            let bare = looks_like_git_dir(&worktree, &limits)?
                && worktree.file_name().is_some_and(|name| name != ".git");
            return Err(match bare {
                true => GitReadError::Bare { path: worktree },
                false => GitReadError::NotAWorktree { path: worktree },
            });
        }
        let not_repository = |cause: Cause| GitReadError::NotARepository {
            path: dot_git.clone(),
            cause,
        };
        let git_dir = match read_entry(&dot_git, limits.small_file_bytes, Links::Follow)? {
            Entry::File(bytes) => {
                // Git strips only the line end of the path, but gix strips all trailing
                // whitespace, so the two would name different folders.
                let end = bytes
                    .iter()
                    .rposition(|byte| !matches!(byte, b'\n' | b'\r'))
                    .map_or(0, |last| last + 1);
                if gix_ref::bstr::ByteSlice::trim_end(&bytes[..end]).len() != end {
                    return Err(not_repository(Cause::text(
                        "a gitdir path that ends in whitespace is not supported",
                    )));
                }
                let path = gix_discover::parse::gitdir(&bytes)
                    .map_err(|error| not_repository(Cause::new(error)))?;
                worktree.join(path)
            }
            _ => dot_git.clone(),
        };
        let git_dir = canonical(&git_dir).map_err(|e| not_repository(Cause::new(e)))?;
        if !git_dir.is_dir() {
            return Err(not_repository(Cause::text(
                "the Git directory is not a folder",
            )));
        }
        let commondir = git_dir.join("commondir");
        let common_dir = match read_file(&commondir, limits.small_file_bytes)? {
            Some(bytes) => {
                let end = bytes
                    .iter()
                    .rposition(|byte| !matches!(byte, b'\n' | b'\r'))
                    .map_or(0, |last| last + 1);
                let relative = &bytes[..end];
                if relative.is_empty() {
                    return Err(not_repository(Cause::text("commondir is empty")));
                }
                git_dir.join(gix_path_of(relative))
            }
            None => git_dir.clone(),
        };
        let common_dir = canonical(&common_dir).map_err(|e| not_repository(Cause::new(e)))?;
        let (config, layout) = read_config(&common_dir, &git_dir, &limits)?;
        if exists(&common_dir.join("shallow"))? {
            return Err(GitReadError::Shallow { common_dir });
        }
        let hash = config.object_format.kind();
        match read_head(&git_dir, &limits, hash)? {
            Some(gix_ref::Target::Symbolic(name)) if !name.as_bstr().starts_with(b"refs/") => {
                return Err(not_repository(Cause::text(&format!(
                    "HEAD names {}, which is not below refs/",
                    name.as_bstr()
                ))));
            }
            Some(_) => {}
            None => return Err(not_repository(Cause::text("the Git directory has no HEAD"))),
        }
        for folder in ["objects", "refs"] {
            if !common_dir.join(folder).is_dir() {
                return Err(not_repository(Cause::text(&format!(
                    "the common directory has no folder {folder}"
                ))));
            }
        }
        // Without `extensions.worktreeConfig`, the common `core.bare` and `core.worktree` describe
        // the main worktree only, so a linked worktree of a bare repository is not bare.
        let common = git_dir == common_dir || layout.worktree_config;
        let bare = layout.own_bare.unwrap_or(common && layout.common_bare);
        if bare {
            return Err(GitReadError::Bare { path: git_dir });
        }
        // `core.worktree` names the worktree, relative to the Git directory, so it must lead to the
        // opened folder.
        let core_worktree = layout
            .own_worktree
            .or(layout.common_worktree.filter(|_| common));
        if let Some(value) = core_worktree {
            let configured = git_dir.join(gix_path_of(&value));
            if fs::canonicalize(&configured).ok().as_ref() != Some(&worktree) {
                return Err(GitReadError::ForeignWorktree {
                    worktree,
                    configured,
                });
            }
        }
        let objects_dir = common_dir.join("objects");
        let object_dirs = check_alternates(&objects_dir, &limits)?;
        let objects =
            // gix opens each pack that `multi-pack-index` names with a blocking open and no type
            // check, so the reader turns the index off and gix reads only regular `.idx` files.
            gix_odb::at_opts(
                &objects_dir,
                hash,
                None,
                gix_odb::store::init::Options {
                    use_multi_pack_index: false,
                    // gix resolves relative alternates against this folder, and reads the
                    // current folder of the process when it is unset, which fails for a
                    // removed folder. The reader refuses each relative path that this changes.
                    current_dir: Some(common_dir.clone()),
                    ..Default::default()
                },
            )
            .map_err(|error| io_error(&objects_dir, error))?;
        Ok(Self {
            worktree,
            git_dir,
            common_dir,
            config,
            limits,
            objects,
            object_dirs,
        })
    }

    /// Return the canonical top folder of the worktree.
    ///
    /// # Returns
    /// The folder that holds `.git`.
    pub fn worktree(&self) -> &Path {
        &self.worktree
    }

    /// Return the canonical Git directory of the worktree, which holds its `HEAD`.
    ///
    /// # Returns
    /// `.git` of a main worktree, or `.git/worktrees/<name>` of the main repository for a linked one.
    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    /// Return the canonical common Git directory, which holds the objects, the shared refs and `config`.
    ///
    /// # Returns
    /// The common directory, which equals [`Self::git_dir`] in a main worktree.
    pub fn common_dir(&self) -> &Path {
        &self.common_dir
    }

    /// Report whether the worktree is a linked worktree.
    ///
    /// # Returns
    /// True when the Git directory differs from the common directory.
    pub fn is_linked_worktree(&self) -> bool {
        self.git_dir != self.common_dir
    }

    /// Return the object format of the repository.
    ///
    /// # Returns
    /// SHA-1 or SHA-256.
    pub fn object_format(&self) -> ObjectFormat {
        self.config.object_format
    }

    /// Return the configuration keys that the open read.
    ///
    /// # Returns
    /// The keys of the repository's own configuration files.
    pub fn config(&self) -> &RepositoryConfig {
        &self.config
    }

    /// Read `HEAD` of this worktree and resolve the ref that it names.
    ///
    /// # Returns
    /// The branch and its tip, or the detached object ID.
    ///
    /// # Errors
    /// Returns [`GitReadError::Ref`] for a `HEAD` that is missing, cannot be parsed or names a
    /// ref outside `refs/`, and [`GitReadError::TooLarge`], [`GitReadError::NotAFile`] or
    /// [`GitReadError::SymbolicDepth`].
    pub fn head(&self) -> Result<Head, GitReadError> {
        let hash = self.config.object_format.kind();
        let Some(target) = read_head(&self.git_dir, &self.limits, hash)? else {
            return Err(GitReadError::Ref {
                name: b"HEAD".to_vec(),
                cause: Cause::text("HEAD does not exist"),
            });
        };
        match target {
            gix_ref::Target::Object(id) => Ok(Head::Detached(self.id(&id))),
            gix_ref::Target::Symbolic(name) if !name.as_bstr().starts_with(b"refs/") => {
                Err(GitReadError::Ref {
                    name: b"HEAD".to_vec(),
                    cause: Cause::text("HEAD names a ref that is not below refs/"),
                })
            }
            gix_ref::Target::Symbolic(name) => {
                // Git counts the read of HEAD against its depth of symbolic refs.
                // Git names the last ref of the chain as the branch, as `git symbolic-ref HEAD`.
                let (last, tip) =
                    self.resolve(&name, self.limits.symbolic_depth.saturating_sub(1))?;
                Ok(Head::Branch {
                    name: last.as_bstr().to_vec(),
                    tip,
                })
            }
        }
    }

    /// Read the tip of the branch `refs/heads/<name>`, from a loose ref or `packed-refs`.
    ///
    /// # Arguments
    /// * `name` - Short branch name, such as `main`.
    ///
    /// # Returns
    /// The object ID of the tip, or `None` when the branch does not exist.
    ///
    /// # Errors
    /// Returns [`GitReadError::InvalidBranchName`], [`GitReadError::Ref`],
    /// [`GitReadError::TooLarge`], [`GitReadError::NotAFile`] or [`GitReadError::SymbolicDepth`].
    pub fn branch_tip(&self, name: &str) -> Result<Option<GitObjectId>, GitReadError> {
        let invalid = || GitReadError::InvalidBranchName {
            name: name.to_string(),
        };
        if name.is_empty() {
            return Err(invalid());
        }
        let full =
            gix_ref::FullName::try_from(format!("refs/heads/{name}")).map_err(|_| invalid())?;
        Ok(self.resolve(&full, self.limits.symbolic_depth)?.1)
    }

    /// Follow a ref through symbolic refs to an object ID, within the depth limit.
    ///
    /// # Arguments
    /// * `start` - Full name of the first ref.
    /// * `reads` - Most refs to read, the first one included.
    ///
    /// # Returns
    /// The name of the last ref of the chain, and its object ID, or `None` when that ref does
    /// not exist.
    ///
    /// # Errors
    /// Returns [`GitReadError::Ref`], [`GitReadError::TooLarge`], [`GitReadError::NotAFile`] or
    /// [`GitReadError::SymbolicDepth`].
    fn resolve(
        &self,
        start: &gix_ref::FullName,
        reads: usize,
    ) -> Result<(gix_ref::FullName, Option<GitObjectId>), GitReadError> {
        let mut name = start.clone();
        for _ in 0..reads {
            match self.find_ref(&name)? {
                None => return Ok((name, None)),
                Some(gix_ref::Target::Object(id)) => return Ok((name, Some(self.id(&id)))),
                Some(gix_ref::Target::Symbolic(next)) => name = next,
            }
        }
        Err(GitReadError::SymbolicDepth {
            name: start.as_bstr().to_vec(),
            limit: self.limits.symbolic_depth,
        })
    }

    /// Read one ref by its full name, from its loose file or from `packed-refs`.
    ///
    /// The reader reads the loose file of a ref that is private to a worktree, such as
    /// `refs/bisect/...`, from the Git directory. It reads the loose file of every other ref from
    /// the common directory, also for a name with no gix category, such as `foo/bar`. Without a
    /// loose file, it reads the shared `packed-refs` for a name below `refs/`, as Git does. A real
    /// folder at the path of a loose ref means that the ref is not loose. A path that crosses a
    /// file, or a loose ref that is a symbolic link to a folder, means that the ref is missing,
    /// with no look in `packed-refs`.
    ///
    /// # Arguments
    /// * `name` - Full name of the ref.
    ///
    /// # Returns
    /// The target of the ref, or `None` when it does not exist.
    ///
    /// # Errors
    /// Returns [`GitReadError::Ref`] for a ref that cannot be parsed or that names another
    /// worktree, and the errors of [`read_entry`].
    fn find_ref(&self, name: &gix_ref::FullName) -> Result<Option<gix_ref::Target>, GitReadError> {
        use gix_ref::Category;
        let bytes = name.as_bstr().to_vec();
        let shared = match name.category() {
            Some(
                Category::MainPseudoRef
                | Category::MainRef
                | Category::LinkedPseudoRef { .. }
                | Category::LinkedRef { .. },
            ) => {
                return Err(GitReadError::Ref {
                    name: bytes,
                    cause: Cause::text(
                        "the ref names another worktree, which the reader does not read",
                    ),
                });
            }
            Some(
                Category::PseudoRef
                | Category::Bisect
                | Category::Rewritten
                | Category::WorktreePrivate,
            ) => false,
            // Git keeps every other name in the common directory.
            _ => true,
        };
        let dir = if shared {
            &self.common_dir
        } else {
            &self.git_dir
        };
        let path = dir.join(gix_path_of(&bytes));
        let hash = self.config.object_format.kind();
        match read_entry(&path, self.limits.small_file_bytes, Links::RefOrFollow)? {
            Entry::Link(target) => {
                let target = gix_ref::FullName::try_from(gix_ref::bstr::BStr::new(&target))
                    .map_err(|error| ref_error(&bytes, error))?;
                return Ok(Some(gix_ref::Target::Symbolic(target)));
            }
            Entry::File(contents) => {
                return parse_loose(&bytes, &contents, hash).map(Some);
            }
            // Git reads a ref whose path crosses a file, or a loose ref that is a link to a folder,
            // as missing, with no look in `packed-refs`.
            Entry::Blocked => return Ok(None),
            Entry::Missing | Entry::Folder => {}
        }
        // `packed-refs` holds only names below `refs/`, and gix would try other names below
        // `refs/heads` and the like, which Git does not.
        if !bytes.starts_with(b"refs/") {
            return Ok(None);
        }
        // Git falls back to the shared `packed-refs` for a private ref too.
        let packed = self.common_dir.join("packed-refs");
        let Some(contents) = read_file(&packed, self.limits.packed_refs_bytes)? else {
            return Ok(None);
        };
        find_packed(&bytes, &contents, hash)
    }

    /// Check the `alternates` files again, and refuse a change since the open.
    ///
    /// gix reads `alternates` again at its first lookup and after each miss, without a bound, so
    /// the reader checks the files first. A change since the open, such as a FIFO or a new
    /// alternate, is refused.
    ///
    /// # Returns
    /// Success when the object folders are those of the open.
    ///
    /// # Errors
    /// Returns [`GitReadError::Alternate`] for a change, and the errors of the alternates check.
    fn check_object_dirs(&self) -> Result<(), GitReadError> {
        let objects_dir = self.common_dir.join("objects");
        if check_alternates(&objects_dir, &self.limits)? != self.object_dirs {
            return Err(GitReadError::Alternate {
                path: objects_dir.join("info").join("alternates"),
                cause: Cause::text("the alternates changed after the open"),
            });
        }
        Ok(())
    }

    /// Refuse a loose file of `id` that is not a regular file, before gix opens it.
    ///
    /// gix opens a loose object without `O_NONBLOCK`, so a FIFO there would block it.
    ///
    /// # Arguments
    /// * `id` - Full ID in the format of the repository.
    ///
    /// # Returns
    /// Success when each loose file of `id` is absent or a regular file.
    ///
    /// # Errors
    /// Returns [`GitReadError::NotAFile`] or [`GitReadError::Io`].
    fn check_loose(&self, id: &GitObjectId) -> Result<(), GitReadError> {
        let hex = id.to_hex();
        for dir in &self.object_dirs {
            let path = dir.join(&hex[..2]).join(&hex[2..]);
            match read_entry(&path, 0, Links::Follow) {
                Ok(_) | Err(GitReadError::TooLarge { .. }) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// Read the type of the object `id` from its header.
    ///
    /// # Arguments
    /// * `id` - Full ID in the format of the repository.
    ///
    /// # Returns
    /// The type of the object.
    ///
    /// # Errors
    /// Returns [`GitReadError::FormatMismatch`], [`GitReadError::ObjectMissing`],
    /// [`GitReadError::ObjectRead`], or [`GitReadError::NotAFile`] for a loose file of the object
    /// that is a FIFO, a device or a socket. Returns [`GitReadError::Alternate`] when the
    /// `alternates` files changed after the open, and [`GitReadError::NotAFile`] or
    /// [`GitReadError::TooLarge`] when an `alternates` file is now a special file or too large.
    pub fn object_kind(&self, id: &GitObjectId) -> Result<ObjectKind, GitReadError> {
        Ok(ObjectKind::from_gix(self.header(id)?.0))
    }

    /// Read the header of the object `id`.
    ///
    /// # Arguments
    /// * `id` - Full ID in the format of the repository.
    ///
    /// # Returns
    /// The gix type, the size and the gix ID of the object.
    ///
    /// # Errors
    /// Returns [`GitReadError::FormatMismatch`], [`GitReadError::ObjectMissing`],
    /// [`GitReadError::ObjectRead`], or [`GitReadError::NotAFile`] for a loose file of the object
    /// that is a FIFO, a device or a socket. Returns [`GitReadError::Alternate`] when the
    /// `alternates` files changed after the open, and [`GitReadError::NotAFile`] or
    /// [`GitReadError::TooLarge`] when an `alternates` file is now a special file or too large.
    fn header(
        &self,
        id: &GitObjectId,
    ) -> Result<(gix_object::Kind, u64, gix_hash::ObjectId), GitReadError> {
        use gix_object::FindHeader as _;
        if id.format != self.config.object_format {
            return Err(GitReadError::FormatMismatch {
                id: id.clone(),
                repository: self.config.object_format,
            });
        }
        self.check_object_dirs()?;
        self.check_loose(id)?;
        let oid = id.to_gix()?;
        let header = self
            .objects
            .try_header(&oid)
            .map_err(|error| object_error(id, error))?;
        let header = header.ok_or_else(|| GitReadError::ObjectMissing { id: id.clone() })?;
        Ok((header.kind, header.size, oid))
    }

    /// Read the commit `id`, check its type and its hash, and return its raw parents in order.
    ///
    /// # Arguments
    /// * `id` - Full ID in the format of the repository.
    ///
    /// # Returns
    /// The checked commit.
    ///
    /// # Errors
    /// Returns [`GitReadError::NotACommit`], [`GitReadError::HashMismatch`],
    /// [`GitReadError::TooLarge`], [`GitReadError::TooManyParents`],
    /// [`GitReadError::MalformedCommit`] and the errors of [`Self::object_kind`].
    pub fn read_commit(&self, id: &GitObjectId) -> Result<GitCommit, GitReadError> {
        use gix_object::Find as _;
        let (kind, size, oid) = self.header(id)?;
        let not_commit = |kind| GitReadError::NotACommit {
            id: id.clone(),
            kind: ObjectKind::from_gix(kind),
        };
        if kind != gix_object::Kind::Commit {
            return Err(not_commit(kind));
        }
        let limit = self.limits.commit_bytes;
        let too_large = |size| GitReadError::TooLarge {
            what: format!("commit {id}"),
            size,
            limit,
        };
        if size > limit {
            return Err(too_large(size));
        }
        let mut buffer = Vec::new();
        let data = self
            .objects
            .try_find(&oid, &mut buffer)
            .map_err(|error| object_error(id, error))?;
        let data = data.ok_or_else(|| GitReadError::ObjectMissing { id: id.clone() })?;
        let commit = data.kind == gix_object::Kind::Commit;
        commit.then_some(()).ok_or_else(|| not_commit(data.kind))?;
        let hash = self.config.object_format.kind();
        let computed = gix_object::compute_hash(hash, data.kind, data.data)
            .map_err(|error| object_error(id, error))?;
        if computed != oid {
            return Err(GitReadError::HashMismatch {
                id: id.clone(),
                computed: self.id(&computed),
            });
        }
        let parents = self.parents(id, data.data)?;
        Ok(GitCommit {
            id: id.clone(),
            parents,
        })
    }

    /// Parse the tree line and the parent lines of a commit body.
    ///
    /// # Arguments
    /// * `id` - ID of the commit, for errors.
    /// * `body` - Raw commit bytes after the object header.
    ///
    /// # Returns
    /// The parents in the order of the body.
    ///
    /// # Errors
    /// Returns [`GitReadError::MalformedCommit`] or [`GitReadError::TooManyParents`].
    fn parents(&self, id: &GitObjectId, body: &[u8]) -> Result<Vec<GitObjectId>, GitReadError> {
        let malformed = |text: &str| GitReadError::MalformedCommit {
            id: id.clone(),
            cause: Cause::text(text),
        };
        // As `parse_commit_buffer` of Git: a tree line, then each parent line, and nothing else.
        let digits = self.config.object_format.kind().len_in_hex();
        let line = |rest: &[u8], prefix: &[u8]| -> Option<Result<gix_hash::ObjectId, ()>> {
            let rest = rest.strip_prefix(prefix)?;
            let (hex, end) = rest.split_at_checked(digits).ok_or(()).ok()?;
            Some(match end.first() {
                Some(b'\n') => gix_hash::ObjectId::from_hex(hex).map_err(|_| ()),
                _ => Err(()),
            })
        };
        let entry = prefix_len(b"tree ", digits);
        match line(body, b"tree ") {
            Some(Ok(_)) => {}
            _ => return Err(malformed("the commit has no valid tree line")),
        }
        let mut rest = &body[entry..];
        let mut parents = Vec::new();
        while let Some(parent) = line(rest, b"parent ") {
            let parent = parent.map_err(|()| malformed("the commit has a bad parent line"))?;
            if parents.len() == self.limits.parents {
                return Err(GitReadError::TooManyParents {
                    id: id.clone(),
                    limit: self.limits.parents,
                });
            }
            parents.push(self.id(&parent));
            rest = &rest[prefix_len(b"parent ", digits)..];
        }
        // A short line that starts like a parent line is bad too.
        if rest.starts_with(b"parent ") {
            return Err(malformed("the commit has a bad parent line"));
        }
        // Git needs a byte after the tree line and after the last parent line.
        if rest.is_empty() {
            return Err(malformed("the commit ends after its tree or parent lines"));
        }
        Ok(parents)
    }

    /// Copy a gix ID that this repository produced.
    ///
    /// # Arguments
    /// * `id` - ID in the format of the repository.
    ///
    /// # Returns
    /// The ID in the type of this module.
    fn id(&self, id: &gix_hash::oid) -> GitObjectId {
        GitObjectId::from_gix(self.config.object_format, id)
    }
}

/// Return the length of a commit line of `prefix`, an ID of `digits` and a line end.
///
/// # Arguments
/// * `prefix` - Start of the line, such as `parent `.
/// * `digits` - Hex digits of an ID.
///
/// # Returns
/// The number of bytes.
fn prefix_len(prefix: &[u8], digits: usize) -> usize {
    prefix.len() + digits + 1
}

/// Report whether `byte` is whitespace for Git's `isspace`, which is a space, a tab, a line feed
/// or a carriage return, but not a form feed or a vertical tab.
///
/// # Arguments
/// * `byte` - Byte to test.
///
/// # Returns
/// True for whitespace.
fn git_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

/// Remove Git's whitespace from both ends of `bytes`.
///
/// # Arguments
/// * `bytes` - Bytes to trim.
///
/// # Returns
/// The trimmed bytes.
fn git_trim(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !git_space(*byte))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !git_space(*byte))
        .map_or(start, |last| last + 1);
    &bytes[start..end]
}

/// Find a ref by its exact name in the bytes of `packed-refs`, as Git reads the file.
///
/// The reader parses the file itself, because the parser of gix ends a name at a carriage return
/// and refuses the whole file for one bad name. Git keeps the carriage return in the name and
/// skips the bad entry. Each record is a hex object ID of the format, a space and a name up to the
/// line feed. An optional header line starts with `#`, and a peeled line starts with `^`. A name
/// that Git calls broken never equals a valid name, so it is never found.
///
/// # Arguments
/// * `name` - Full name of the ref.
/// * `contents` - Bytes of `packed-refs`.
/// * `hash` - Object format of the repository.
///
/// # Returns
/// The object ID of the ref, or `None` when the file holds no record of that name.
///
/// # Errors
/// Returns [`GitReadError::Ref`] for a file that Git refuses, which has a record line that is not
/// a hex ID and a space, or a last line with no line feed.
fn find_packed(
    name: &[u8],
    contents: &[u8],
    hash: gix_hash::Kind,
) -> Result<Option<gix_ref::Target>, GitReadError> {
    let broken = |what: &'static str| GitReadError::Ref {
        name: name.to_vec(),
        cause: Cause::text(what),
    };
    if contents.is_empty() {
        return Ok(None);
    }
    let Some(body) = contents.strip_suffix(b"\n") else {
        return Err(broken("packed-refs does not end with a line feed"));
    };
    let digits = hash.len_in_hex();
    let mut found = None;
    for (index, line) in body.split(|byte| *byte == b'\n').enumerate() {
        if (index == 0 && line.starts_with(b"#")) || line.starts_with(b"^") {
            continue;
        }
        if line.len() <= digits || line[digits] != b' ' {
            return Err(broken("packed-refs has a line that is not a record"));
        }
        if found.is_none() && &line[digits + 1..] == name {
            let id = gix_hash::ObjectId::from_hex(&line[..digits])
                .map_err(|error| ref_error(name, error))?;
            found = Some(gix_ref::Target::Object(id));
        } else if !line[..digits].iter().all(u8::is_ascii_hexdigit) {
            return Err(broken("packed-refs has a record without a hex object ID"));
        }
    }
    Ok(found)
}

/// Parse the contents of a loose ref, as `parse_loose_ref_contents` of Git parses them.
///
/// A symbolic ref is `ref:`, any whitespace and the target name, with trailing whitespace
/// removed. A direct ref is a full hex ID, followed by the end or by whitespace.
///
/// # Arguments
/// * `name` - Full name of the ref, for the error.
/// * `contents` - Bytes of the loose file.
/// * `hash` - Object format of the repository.
///
/// # Returns
/// The target of the ref.
///
/// # Errors
/// Returns [`GitReadError::Ref`] for contents that Git calls broken.
fn parse_loose(
    name: &[u8],
    contents: &[u8],
    hash: gix_hash::Kind,
) -> Result<gix_ref::Target, GitReadError> {
    if let Some(target) = contents.strip_prefix(b"ref:") {
        let target = gix_ref::FullName::try_from(gix_ref::bstr::BStr::new(git_trim(target)))
            .map_err(|error| ref_error(name, error))?;
        return Ok(gix_ref::Target::Symbolic(target));
    }
    let digits = hash.len_in_hex();
    let broken = || GitReadError::Ref {
        name: name.to_vec(),
        cause: Cause::text("the ref holds no full object ID"),
    };
    let (hex, rest) = contents.split_at_checked(digits).ok_or_else(broken)?;
    if rest.first().is_some_and(|byte| !git_space(*byte)) {
        return Err(broken());
    }
    gix_hash::ObjectId::from_hex(hex)
        .map(gix_ref::Target::Object)
        .map_err(|error| ref_error(name, error))
}

/// Read `HEAD` of a Git directory, as Git reads it.
///
/// A symbolic link to `refs/...` is a symbolic ref to that name, and any other link is refused.
///
/// # Arguments
/// * `git_dir` - Git directory that holds `HEAD`.
/// * `limits` - Read limits.
/// * `hash` - Object format of the repository.
///
/// # Returns
/// The target of `HEAD`, or `None` when it does not exist.
///
/// # Errors
/// Returns [`GitReadError::Ref`] for contents that cannot be parsed, [`GitReadError::NotAFile`]
/// for a folder, and the errors of [`read_entry`].
fn read_head(
    git_dir: &Path,
    limits: &Limits,
    hash: gix_hash::Kind,
) -> Result<Option<gix_ref::Target>, GitReadError> {
    let path = git_dir.join("HEAD");
    match read_entry(&path, limits.small_file_bytes, Links::RefOnly)? {
        Entry::Missing | Entry::Blocked => Ok(None),
        Entry::Folder => Err(GitReadError::NotAFile {
            path,
            kind: "folder",
        }),
        Entry::Link(target) => gix_ref::FullName::try_from(gix_ref::bstr::BStr::new(&target))
            .map(|name| Some(gix_ref::Target::Symbolic(name)))
            .map_err(|error| ref_error(b"HEAD", error)),
        Entry::File(contents) => parse_loose(b"HEAD", &contents, hash).map(Some),
    }
}

/// Report whether a folder is a Git directory, which has a valid `HEAD`, `objects` and `refs`.
///
/// `HEAD` may hold a SHA-1 or a SHA-256 ID, so a bare SHA-256 repository with a detached `HEAD`
/// is a Git directory too.
///
/// # Arguments
/// * `folder` - Folder to inspect.
/// * `limits` - Read limits.
///
/// # Returns
/// True for a Git directory.
///
/// # Errors
/// Returns the errors of [`read_entry`] for a `HEAD` that is too large or a special file.
fn looks_like_git_dir(folder: &Path, limits: &Limits) -> Result<bool, GitReadError> {
    // A `HEAD` that does not parse as SHA-1 is read again as SHA-256.
    let head = match read_head(folder, limits, gix_hash::Kind::Sha1) {
        Err(GitReadError::Ref { .. }) => read_head(folder, limits, gix_hash::Kind::Sha256),
        other => other,
    };
    let head = match head {
        Ok(head) => head,
        Err(GitReadError::Ref { .. } | GitReadError::NotAFile { kind: "folder", .. }) => None,
        Err(error) => return Err(error),
    };
    Ok(head.is_some() && folder.join("objects").is_dir() && folder.join("refs").is_dir())
}

/// Turn the bytes of a full ref name or a relative path into a path.
///
/// # Arguments
/// * `name` - Full ref name, which gix validated, or a relative path from a Git file.
///
/// # Returns
/// The path.
fn gix_path_of(name: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(name))
}

/// Make a ref error value.
///
/// # Arguments
/// * `name` - Full ref name.
/// * `error` - Error that gix returned.
///
/// # Returns
/// The typed error.
fn ref_error(name: &[u8], error: impl std::error::Error + Send + Sync + 'static) -> GitReadError {
    GitReadError::Ref {
        name: name.to_vec(),
        cause: Cause::new(error),
    }
}

/// Make an object read error value.
///
/// # Arguments
/// * `id` - ID of the object.
/// * `error` - Error that gix returned.
///
/// # Returns
/// The typed error.
fn object_error(
    id: &GitObjectId,
    error: impl std::error::Error + Send + Sync + 'static,
) -> GitReadError {
    GitReadError::ObjectRead {
        id: id.clone(),
        cause: Cause::new(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every error message names its reason, so a user can act on it.
    #[test]
    fn every_error_message_names_its_reason() {
        let id = GitObjectId::from_hex(ObjectFormat::Sha1, &"ab".repeat(20)).unwrap();
        let path = PathBuf::from("/repo/.git");
        let cases = [
            (
                io_error(&path, std::io::Error::other("denied")),
                "cannot read /repo/.git: denied",
            ),
            (
                GitReadError::NotAWorktree { path: path.clone() },
                "is not a Git worktree",
            ),
            (
                GitReadError::NotARepository {
                    path: path.clone(),
                    cause: Cause::text("no HEAD"),
                },
                "does not lead to a Git repository: no HEAD",
            ),
            (
                GitReadError::Bare { path: path.clone() },
                "bare Git repository",
            ),
            (
                GitReadError::Shallow {
                    common_dir: path.clone(),
                },
                "shallow clone",
            ),
            (
                GitReadError::Reftable {
                    common_dir: path.clone(),
                },
                "reftable",
            ),
            (
                GitReadError::UnsupportedObjectFormat {
                    value: b"sha512".to_vec(),
                },
                "\"sha512\"",
            ),
            (
                GitReadError::UnsupportedRefStorage {
                    value: b"other".to_vec(),
                },
                "refStorage \"other\"",
            ),
            (
                GitReadError::UnsupportedRepositoryVersion { version: 2 },
                "repositoryformatversion 2",
            ),
            (
                GitReadError::ForeignWorktree {
                    worktree: PathBuf::from("/a"),
                    configured: PathBuf::from("/b"),
                },
                "core.worktree of /a names /b",
            ),
            (
                GitReadError::NotAFile {
                    path: PathBuf::from("/a/HEAD"),
                    kind: "FIFO",
                },
                "/a/HEAD is a FIFO, not a regular file",
            ),
            (
                GitReadError::Alternate {
                    path: PathBuf::from("/a/alternates"),
                    cause: Cause::text("more than 1 alternates"),
                },
                "cannot use an alternate of /a/alternates: more than 1",
            ),
            (
                GitReadError::UnsupportedExtension {
                    name: "frobnicate".into(),
                },
                "extension extensions.frobnicate",
            ),
            (
                config_error(&path, std::io::Error::other("bad")),
                "cannot parse the configuration /repo/.git: bad",
            ),
            (
                GitReadError::TooLarge {
                    what: "x".into(),
                    size: 3,
                    limit: 2,
                },
                "over the limit of 2",
            ),
            (
                GitReadError::InvalidObjectId {
                    text: "zz".into(),
                    format: ObjectFormat::Sha256,
                },
                "not a full sha256 object ID",
            ),
            (
                GitReadError::FormatMismatch {
                    id: id.clone(),
                    repository: ObjectFormat::Sha256,
                },
                "is a sha1 ID, but the repository uses sha256",
            ),
            (
                GitReadError::InvalidBranchName { name: "a b".into() },
                "not a valid branch name",
            ),
            (
                ref_error(b"refs/heads/x", std::io::Error::other("broken")),
                "cannot read ref refs/heads/x: broken",
            ),
            (
                GitReadError::SymbolicDepth {
                    name: b"HEAD".to_vec(),
                    limit: 5,
                },
                "more than 5 symbolic",
            ),
            (
                object_error(&id, std::io::Error::other("pack")),
                "cannot read object abab",
            ),
            (
                GitReadError::ObjectMissing { id: id.clone() },
                "is not in the repository",
            ),
            (
                GitReadError::NotACommit {
                    id: id.clone(),
                    kind: ObjectKind::Blob,
                },
                "is a blob, not a commit",
            ),
            (
                GitReadError::HashMismatch {
                    id: id.clone(),
                    computed: id.clone(),
                },
                "hash mismatch",
            ),
            (
                GitReadError::MalformedCommit {
                    id: id.clone(),
                    cause: Cause::text("x"),
                },
                "is malformed: x",
            ),
            (
                GitReadError::TooManyParents {
                    id: id.clone(),
                    limit: 2,
                },
                "more than 2 parents",
            ),
        ];
        for (error, expected) in cases {
            let text = error.to_string();
            assert!(text.contains(expected), "{text:?} lacks {expected:?}");
        }
    }

    /// Each error with an underlying cause returns it from `source()`, and the others return none.
    #[test]
    fn causes_are_sources_and_compare_by_message() {
        let id = GitObjectId::from_hex(ObjectFormat::Sha1, &"ab".repeat(20)).unwrap();
        let io = io_error(Path::new("/x"), std::io::Error::other("denied"));
        let source = std::error::Error::source(&io).unwrap();
        assert_eq!(source.to_string(), "denied");
        assert!(source.downcast_ref::<std::io::Error>().is_some());
        assert_eq!(
            io,
            io_error(Path::new("/x"), std::io::Error::other("denied"))
        );
        assert_ne!(
            io,
            io_error(Path::new("/x"), std::io::Error::other("other"))
        );
        for error in [
            config_error(Path::new("/x"), std::io::Error::other("a")),
            ref_error(b"HEAD", std::io::Error::other("a")),
            object_error(&id, std::io::Error::other("a")),
        ] {
            assert_eq!(std::error::Error::source(&error).unwrap().to_string(), "a");
        }
        let missing = GitReadError::ObjectMissing { id };
        assert!(std::error::Error::source(&missing).is_none());
    }

    /// IDs keep their format and print as Git prints them, and the commit kind has its name.
    #[test]
    fn ids_and_kinds_round_trip() {
        let id = GitObjectId::from_hex(ObjectFormat::Sha256, &"0F".repeat(32)).unwrap();
        assert_eq!(id.to_string(), "0f".repeat(32));
        assert_eq!(id.as_bytes(), [0x0f; 32]);
        assert_eq!(id.to_gix().unwrap().as_slice(), [0x0f; 32]);
        assert!(GitObjectId::from_hex(ObjectFormat::Sha1, &"g".repeat(40)).is_err());
        let broken = GitObjectId {
            format: ObjectFormat::Sha1,
            bytes: vec![1, 2, 3],
        };
        assert!(matches!(
            broken.to_gix(),
            Err(GitReadError::InvalidObjectId { .. })
        ));
        assert_eq!(ObjectKind::Commit.name(), "commit");
        assert_eq!(ObjectFormat::Sha256.name(), "sha256");
        assert_eq!(
            ObjectKind::from_gix(gix_object::Kind::Commit),
            ObjectKind::Commit
        );
    }

    /// A file system error other than a missing entry is a typed I/O error, not a pass.
    #[test]
    fn unreadable_entries_are_io_errors() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("locked");
        fs::create_dir(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let inside = locked.join("shallow");
        let checks = (exists(&inside), read_entry(&inside, 1, Links::Follow));
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            matches!(checks.0, Err(GitReadError::Io { .. })),
            "{:?}",
            checks.0
        );
        assert!(
            matches!(checks.1, Err(GitReadError::Io { .. })),
            "{:?}",
            checks.1
        );
        assert_eq!(exists(&dir.path().join("absent")), Ok(false));
    }

    /// The reader takes at most the limit of bytes, and it opens no FIFO, device or refused link.
    #[test]
    fn read_entry_is_bounded_and_reads_only_regular_files() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let path = |name: &str| dir.path().join(name);
        fs::write(path("file"), b"0123456789").unwrap();
        assert_eq!(
            read_entry(&path("file"), 10, Links::Follow),
            Ok(Entry::File(b"0123456789".to_vec()))
        );
        assert!(matches!(
            read_entry(&path("file"), 9, Links::Follow),
            Err(GitReadError::TooLarge {
                size: 10,
                limit: 9,
                ..
            })
        ));
        assert_eq!(
            read_entry(&path("absent"), 9, Links::Follow),
            Ok(Entry::Missing)
        );
        // A sparse file of 1 TiB is refused after at most ten bytes, with its true size.
        let sparse = fs::File::create(path("sparse")).unwrap();
        sparse.set_len(1 << 40).unwrap();
        assert!(matches!(
            read_entry(&path("sparse"), 9, Links::Follow),
            Err(GitReadError::TooLarge {
                size: 1099511627776,
                limit: 9,
                ..
            })
        ));
        assert_eq!(read_entry(dir.path(), 9, Links::Follow), Ok(Entry::Folder));
        assert_eq!(read_file(&path("absent"), 9), Ok(None));
        assert!(matches!(
            read_file(dir.path(), 9),
            Err(GitReadError::NotAFile { kind: "folder", .. })
        ));

        symlink("/dev/zero", path("zero")).unwrap();
        for links in [Links::Follow, Links::RefOrFollow] {
            assert!(matches!(
                read_entry(&path("zero"), 9, links),
                Err(GitReadError::NotAFile {
                    kind: "character device",
                    ..
                })
            ));
        }
        assert!(matches!(
            read_entry(&path("zero"), 9, Links::RefOnly),
            Err(GitReadError::NotAFile {
                kind: "symbolic link outside refs/",
                ..
            })
        ));
        let fifo = std::ffi::CString::new(path("fifo").to_str().unwrap()).unwrap();
        // SAFETY: `fifo` is a valid C string that lives across the call.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert!(matches!(
            read_entry(&path("fifo"), 9, Links::Follow),
            Err(GitReadError::NotAFile { kind: "FIFO", .. })
        ));
        symlink("refs/heads/main", path("head")).unwrap();
        assert_eq!(
            read_entry(&path("head"), 9, Links::RefOnly),
            Ok(Entry::Link(b"refs/heads/main".to_vec()))
        );
        assert!(matches!(
            read_entry(&path("head"), 9, Links::Follow),
            Err(GitReadError::NotAFile {
                kind: "dangling symbolic link",
                ..
            })
        ));
        let socket = std::os::unix::net::UnixListener::bind(path("socket")).unwrap();
        // The open of a socket fails, or its handle is not a regular file: either way no read.
        assert!(matches!(
            read_entry(&path("socket"), 9, Links::Follow),
            Err(GitReadError::NotAFile { .. } | GitReadError::Io { .. })
        ));
        drop(socket);
    }

    /// Each special file type has its own name, and any other type is a special file.
    #[test]
    fn special_kind_names_each_file_type() {
        let dir = tempfile::tempdir().unwrap();
        let kind = |path: &Path| special_kind(fs::symlink_metadata(path).unwrap().file_type());
        let socket_path = dir.path().join("socket");
        let socket = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        assert_eq!(kind(&socket_path), "socket");
        drop(socket);
        assert_eq!(kind(Path::new("/dev/null")), "character device");
        // A block device exists on most hosts, but not in every container.
        for device in ["/dev/disk0", "/dev/sda", "/dev/vda", "/dev/loop0"] {
            if fs::symlink_metadata(device).is_ok_and(|meta| {
                std::os::unix::fs::FileTypeExt::is_block_device(&meta.file_type())
            }) {
                assert_eq!(kind(Path::new(device)), "block device");
            }
        }
        let link = dir.path().join("link");
        std::os::unix::fs::symlink("target", &link).unwrap();
        assert_eq!(kind(&link), "special file");
    }

    /// An empty `packed-refs` holds no ref, and a record of another ref without a hex ID is refused.
    #[test]
    fn find_packed_reads_an_empty_file_and_refuses_a_record_without_hex() {
        let sha1 = gix_hash::Kind::Sha1;
        assert_eq!(find_packed(b"refs/heads/main", b"", sha1), Ok(None));
        let other = format!("{} refs/heads/other\n", "z".repeat(40));
        let error = find_packed(b"refs/heads/main", other.as_bytes(), sha1).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("packed-refs has a record without a hex object ID"),
            "{error:?}"
        );
    }
}
