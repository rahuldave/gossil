//! Check the read-only Git reader against fixtures that `git` builds in a scratch folder.
//!
//! Each test needs `git` 2.46 or later. Without it, a test fails with the status
//! `not-run`, so a missing tool never counts as a pass.
use gossil::git_reader::{
    GitObjectId, GitReadError, GitRepository, Head, Limits, ObjectFormat, ObjectKind,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// Stop the test with the distinct status `not-run` when no usable `git` exists.
fn require_git() {
    let output = Command::new("git").arg("--version").output();
    let Ok(output) = output else {
        panic!("not-run: no git executable on PATH; this case is not a pass");
    };
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let version: Vec<u32> = text
        .trim()
        .trim_start_matches("git version ")
        .split('.')
        .take(2)
        .filter_map(|part| part.parse().ok())
        .collect();
    assert!(
        output.status.success() && version.len() == 2 && (version[0], version[1]) >= (2, 46),
        "not-run: git 2.46 or later is required, found {text:?}; this case is not a pass"
    );
}

/// Hold a scratch directory and run `git` there with an isolated environment.
struct Scratch {
    /// Root of the fixture tree, removed on drop.
    dir: TempDir,
}

impl Scratch {
    /// Create an empty scratch tree after the `git` check.
    fn new() -> Self {
        require_git();
        let dir = tempfile::Builder::new()
            .prefix("gossil-gitreader-")
            .tempdir()
            .unwrap();
        fs::create_dir(dir.path().join("home")).unwrap();
        Self { dir }
    }

    /// Return a path below the scratch root.
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// Run `git` in `cwd` with no inherited Git variables and fixed identities, and return stdout.
    fn git(&self, cwd: &Path, args: &[&str]) -> String {
        let output = self.run_git(cwd, args);
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    /// Run `git` like [`Scratch::git`], and return its stdout, or `None` when Git fails.
    fn try_git(&self, cwd: &Path, args: &[&str]) -> Option<String> {
        let output = self.run_git(cwd, args);
        output
            .status
            .success()
            .then(|| String::from_utf8(output.stdout).unwrap())
    }

    /// Run `git` in `cwd` with the fixed environment, and return its whole output.
    fn run_git(&self, cwd: &Path, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", self.path("home"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Fixture Author")
            .env("GIT_AUTHOR_EMAIL", "author@example.invalid")
            .env("GIT_AUTHOR_DATE", "1700000000 +0000")
            .env("GIT_COMMITTER_NAME", "Fixture Committer")
            .env("GIT_COMMITTER_EMAIL", "committer@example.invalid")
            .env("GIT_COMMITTER_DATE", "1700000000 +0000")
            .env("LC_ALL", "C")
            .output()
            .unwrap()
    }

    /// Create a repository with `extra` init options and a `main` branch.
    fn init(&self, name: &str, extra: &[&str]) -> PathBuf {
        let mut args = vec!["init", "-q", "-b", "main"];
        args.extend_from_slice(extra);
        args.push(name);
        self.git(self.dir.path(), &args);
        self.path(name)
    }

    /// Write one file and commit it, and return the full ID of the new commit.
    fn commit(&self, repo: &Path, file: &str, text: &str) -> String {
        fs::write(repo.join(file), text).unwrap();
        self.git(repo, &["add", file]);
        self.git(repo, &["commit", "-q", "-m", text]);
        self.rev(repo, "HEAD")
    }

    /// Return the full object ID that `git rev-parse` gives for `spec`.
    fn rev(&self, repo: &Path, spec: &str) -> String {
        self.git(repo, &["rev-parse", spec]).trim().to_string()
    }

    /// Return the raw parents that `git --no-replace-objects cat-file commit` shows.
    fn raw_parents(&self, repo: &Path, id: &str) -> Vec<String> {
        parents_of(&self.git(repo, &["--no-replace-objects", "cat-file", "commit", id]))
    }

    /// Return every commit that `git rev-list --all` reaches from `repo`.
    fn all_commits(&self, repo: &Path) -> Vec<String> {
        self.git(repo, &["--no-replace-objects", "rev-list", "--all"])
            .lines()
            .map(str::to_string)
            .collect()
    }
}

/// Extract the `parent` lines of a commit body, in order.
fn parents_of(body: &str) -> Vec<String> {
    body.lines()
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.strip_prefix("parent "))
        .map(str::to_string)
        .collect()
}

/// A configuration key, its value and the check of the error that the value must give.
type ConfigCase = (&'static str, &'static str, fn(&GitReadError) -> bool);

/// Supply the documented standard limits.
fn limits() -> Limits {
    Limits::STANDARD
}

/// Parse a full hex ID of `format`.
fn id(format: ObjectFormat, hex: &str) -> GitObjectId {
    GitObjectId::from_hex(format, hex).unwrap()
}

/// Return the hex IDs of the parents that the reader reports for `hex`.
fn reader_parents(repo: &GitRepository, hex: &str) -> Vec<String> {
    let commit = repo
        .read_commit(&id(repo.object_format(), hex))
        .unwrap_or_else(|error| panic!("read {hex}: {error}"));
    assert_eq!(commit.id().to_hex(), hex);
    commit.parents().iter().map(GitObjectId::to_hex).collect()
}

/// Build history with a merge and an octopus merge, so parent order matters.
fn branchy_history(scratch: &Scratch, repo: &Path) -> Vec<String> {
    let first = scratch.commit(repo, "a.txt", "one");
    scratch.git(repo, &["switch", "-q", "-c", "side"]);
    let side = scratch.commit(repo, "side.txt", "side");
    scratch.git(repo, &["switch", "-q", "main"]);
    scratch.commit(repo, "a.txt", "two");
    scratch.git(
        repo,
        &["merge", "-q", "--no-ff", "-m", "merge side", "side"],
    );
    scratch.git(repo, &["branch", "x", &first]);
    scratch.git(repo, &["branch", "y", &side]);
    scratch.git(repo, &["switch", "-q", "x"]);
    scratch.commit(repo, "x.txt", "x");
    scratch.git(repo, &["switch", "-q", "y"]);
    scratch.commit(repo, "y.txt", "y");
    scratch.git(repo, &["switch", "-q", "main"]);
    scratch.git(repo, &["merge", "-q", "-m", "octopus", "x", "y"]);
    scratch.all_commits(repo)
}

/// Return the commits that `git verify-pack -v` shows as deltas in the packs of `repo`.
fn deltified_commits(scratch: &Scratch, repo: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for entry in fs::read_dir(repo.join(".git/objects/pack")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "idx") {
            let listing = scratch.git(repo, &["verify-pack", "-v", path.to_str().unwrap()]);
            for line in listing.lines() {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() == 7 && fields[1] == "commit" {
                    out.push(fields[0].to_string());
                }
            }
        }
    }
    out
}

/// Hash the names, kinds, modes, bytes, inodes and modified times of every entry below each root.
///
/// The modified time of a folder changes when an entry is made or removed in it. So a lock file
/// that a writer makes and removes again changes the hash, and so does a file rewritten with the
/// same bytes.
fn tree_hash(roots: &[&Path]) -> BTreeMap<PathBuf, String> {
    let mut out = BTreeMap::new();
    for root in roots {
        let mut stack = vec![root.to_path_buf()];
        while let Some(path) = stack.pop() {
            let meta = fs::symlink_metadata(&path).unwrap();
            let mut hasher = Sha256::new();
            hasher.update(format!(
                "{:o} {} {}.{}",
                meta.permissions().mode(),
                meta.ino(),
                meta.mtime(),
                meta.mtime_nsec()
            ));
            if meta.is_dir() {
                hasher.update(b"dir");
                for entry in fs::read_dir(&path).unwrap() {
                    stack.push(entry.unwrap().path());
                }
            } else if meta.file_type().is_symlink() {
                hasher.update(fs::read_link(&path).unwrap().as_os_str().as_encoded_bytes());
            } else {
                hasher.update(fs::read(&path).unwrap());
            }
            out.insert(path, format!("{:x}", hasher.finalize()));
        }
    }
    out
}

/// GIT-T01: packed objects, loose objects, `packed-refs` and an alternate give Git's raw parents.
#[test]
fn git_t01_packed_loose_packed_refs_and_alternate() {
    let scratch = Scratch::new();
    let base = scratch.init("base", &[]);
    branchy_history(&scratch, &base);
    let octopus = scratch.rev(&base, "main");
    let long = "a long message that makes similar commits deltify in the pack\n".repeat(60);
    for round in 0..6 {
        fs::write(base.join("a.txt"), format!("round {round}")).unwrap();
        scratch.git(
            &base,
            &["commit", "-q", "-a", "-m", &format!("{long}round {round}")],
        );
    }
    let history = scratch.all_commits(&base);
    scratch.git(
        &base,
        &[
            "repack",
            "-q",
            "-a",
            "-d",
            "-f",
            "--depth=50",
            "--window=50",
        ],
    );
    scratch.git(&base, &["pack-refs", "--all"]);
    let deltas = deltified_commits(&scratch, &base);
    assert!(
        !deltas.is_empty(),
        "the pack holds commits stored as deltas"
    );
    assert!(deltas.iter().all(|hex| history.contains(hex)));
    assert!(!base.join(".git/refs/heads/side").exists());
    assert!(
        fs::read_to_string(base.join(".git/packed-refs"))
            .unwrap()
            .contains("refs/heads/side")
    );

    let opened = GitRepository::open(&base, limits()).unwrap();
    assert_eq!(opened.object_format(), ObjectFormat::Sha1);
    let side_tip = opened.branch_tip("side").unwrap().unwrap();
    assert_eq!(side_tip.to_hex(), scratch.rev(&base, "side"));
    for hex in &history {
        assert_eq!(
            reader_parents(&opened, hex),
            scratch.raw_parents(&base, hex)
        );
    }

    scratch.git(
        scratch.dir.path(),
        &["clone", "-q", "--shared", "base", "clone"],
    );
    let clone = scratch.path("clone");
    assert!(clone.join(".git/objects/info/alternates").is_file());
    let loose = scratch.commit(&clone, "c.txt", "loose");
    assert!(
        clone
            .join(".git/objects")
            .join(&loose[..2])
            .join(&loose[2..])
            .is_file()
    );

    let reader = GitRepository::open(&clone, limits()).unwrap();
    let Head::Branch { name, tip } = reader.head().unwrap() else {
        panic!("HEAD of the clone names a branch");
    };
    assert_eq!(name, b"refs/heads/main");
    assert_eq!(tip.unwrap().to_hex(), loose);
    assert_eq!(reader.branch_tip("main").unwrap().unwrap().to_hex(), loose);
    let all = scratch.all_commits(&clone);
    assert!(all.len() > history.len());
    for hex in &all {
        assert_eq!(
            reader_parents(&reader, hex),
            scratch.raw_parents(&clone, hex)
        );
    }
    assert_eq!(reader_parents(&reader, &octopus).len(), 3);
}

/// GIT-T02: a linked worktree has its own Git directory, the shared common directory and its own `HEAD`.
#[test]
fn git_t02_linked_worktree() {
    let scratch = Scratch::new();
    let main = scratch.init("main", &[]);
    let base = scratch.commit(&main, "a.txt", "one");
    scratch.git(
        &main,
        &["worktree", "add", "-q", "-b", "feature", "../linked"],
    );
    let linked = scratch.path("linked");
    let tip = scratch.commit(&linked, "b.txt", "feature");

    let reader = GitRepository::open(&linked, limits()).unwrap();
    let common = fs::canonicalize(main.join(".git")).unwrap();
    assert!(reader.is_linked_worktree());
    let shown = format!("{reader:?}");
    assert!(
        shown.contains("common_dir") && shown.contains("worktrees/linked"),
        "{shown}"
    );
    assert_eq!(reader.common_dir(), common);
    assert_eq!(reader.git_dir(), common.join("worktrees/linked"));
    assert_eq!(reader.worktree(), fs::canonicalize(&linked).unwrap());
    assert_eq!(
        reader.head().unwrap(),
        Head::Branch {
            name: b"refs/heads/feature".to_vec(),
            tip: Some(id(ObjectFormat::Sha1, &tip)),
        }
    );
    assert_eq!(reader.branch_tip("main").unwrap().unwrap().to_hex(), base);
    assert_eq!(reader_parents(&reader, &tip), vec![base.clone()]);

    let primary = GitRepository::open(&main, limits()).unwrap();
    assert!(!primary.is_linked_worktree());
    assert_eq!(primary.git_dir(), primary.common_dir());
    assert_eq!(
        primary.head().unwrap(),
        Head::Branch {
            name: b"refs/heads/main".to_vec(),
            tip: Some(id(ObjectFormat::Sha1, &base)),
        }
    );
}

/// GIT-T03: a commit that `refs/replace` replaces gives its raw object and raw parents.
#[test]
fn git_t03_replace_refs_are_not_applied() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let first = scratch.commit(&repo, "a.txt", "one");
    let second = scratch.commit(&repo, "a.txt", "two");
    scratch.git(&repo, &["replace", "--graft", &second]);
    let replaced = parents_of(&scratch.git(&repo, &["cat-file", "commit", &second]));
    assert!(
        replaced.is_empty(),
        "the graft hides the parent when Git applies replacements"
    );
    assert_eq!(scratch.raw_parents(&repo, &second), vec![first.clone()]);

    let reader = GitRepository::open(&repo, limits()).unwrap();
    assert_eq!(reader_parents(&reader, &second), vec![first]);
    assert_eq!(
        reader.head().unwrap(),
        Head::Branch {
            name: b"refs/heads/main".to_vec(),
            tip: Some(id(ObjectFormat::Sha1, &second)),
        }
    );
}

/// GIT-T04: a SHA-256 repository gives full 64-digit IDs and Git's parents.
#[test]
fn git_t04_sha256_repository() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &["--object-format=sha256"]);
    let history = branchy_history(&scratch, &repo);
    scratch.git(&repo, &["repack", "-q", "-a", "-d"]);
    scratch.git(&repo, &["pack-refs", "--all"]);
    let loose = scratch.commit(&repo, "z.txt", "loose");

    let reader = GitRepository::open(&repo, limits()).unwrap();
    assert_eq!(reader.object_format(), ObjectFormat::Sha256);
    assert_eq!(reader.config().object_format, ObjectFormat::Sha256);
    let Head::Branch { tip: Some(tip), .. } = reader.head().unwrap() else {
        panic!("HEAD names a born branch");
    };
    assert_eq!(tip.to_hex(), loose);
    assert_eq!(tip.to_hex().len(), 64);
    assert_eq!(tip.format(), ObjectFormat::Sha256);
    // Each branch other than main is only in `packed-refs`, with IDs of 64 digits.
    let names = scratch.git(
        &repo,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    );
    let packed: Vec<&str> = names.lines().filter(|name| *name != "main").collect();
    assert!(!packed.is_empty());
    for name in packed {
        assert!(
            !repo.join(".git/refs/heads").join(name).exists(),
            "{name} is packed"
        );
        let expected = id(
            ObjectFormat::Sha256,
            &scratch.rev(&repo, &format!("refs/heads/{name}")),
        );
        assert_eq!(reader.branch_tip(name), Ok(Some(expected)), "{name}");
    }
    for hex in history.iter().chain([&loose]) {
        assert_eq!(
            reader_parents(&reader, hex),
            scratch.raw_parents(&repo, hex)
        );
    }
    let short = &loose[..40];
    assert!(matches!(
        GitObjectId::from_hex(ObjectFormat::Sha256, short),
        Err(GitReadError::InvalidObjectId { .. })
    ));
    let sha1 = id(ObjectFormat::Sha1, short);
    assert!(matches!(
        reader.read_commit(&sha1),
        Err(GitReadError::FormatMismatch { .. })
    ));
}

/// GIT-T05: a reftable repository, and a repository migrated to reftable, are refused by name.
#[test]
fn git_t05_reftable_is_refused() {
    let scratch = Scratch::new();
    let born = scratch.init("born", &["--ref-format=reftable"]);
    scratch.commit(&born, "a.txt", "one");
    let before = tree_hash(&[&born.join(".git")]);
    let error = GitRepository::open(&born, limits()).unwrap_err();
    assert!(matches!(error, GitReadError::Reftable { .. }), "{error:?}");
    assert!(error.to_string().contains("reftable"), "{error}");
    assert_eq!(tree_hash(&[&born.join(".git")]), before);

    let migrated = scratch.init("migrated", &[]);
    scratch.commit(&migrated, "a.txt", "one");
    GitRepository::open(&migrated, limits()).unwrap();
    scratch.git(&migrated, &["refs", "migrate", "--ref-format=reftable"]);
    let before = tree_hash(&[&migrated.join(".git")]);
    let error = GitRepository::open(&migrated, limits()).unwrap_err();
    assert!(matches!(error, GitReadError::Reftable { .. }), "{error:?}");
    assert!(error.to_string().contains("reftable"), "{error}");
    assert_eq!(tree_hash(&[&migrated.join(".git")]), before);
}

/// GIT-T06 and COMP-T07: a run of every call of the reader leaves every Git directory unchanged.
#[test]
fn git_t06_full_exercise_leaves_git_directories_unchanged() {
    let scratch = Scratch::new();
    let base = scratch.init("base", &[]);
    branchy_history(&scratch, &base);
    scratch.git(&base, &["repack", "-q", "-a", "-d"]);
    scratch.git(
        scratch.dir.path(),
        &["clone", "-q", "--shared", "base", "clone"],
    );
    let clone = scratch.path("clone");
    scratch.commit(&clone, "c.txt", "loose");
    scratch.git(&clone, &["worktree", "add", "-q", "-b", "wt", "../wt"]);
    let worktree = scratch.path("wt");
    scratch.commit(&worktree, "w.txt", "worktree");
    let tree = scratch
        .git(&clone, &["rev-parse", "HEAD^{tree}"])
        .trim()
        .to_string();
    let roots = [base.join(".git"), clone.join(".git"), worktree.join(".git")];
    let root_refs: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
    let before = tree_hash(&root_refs);

    for location in [&clone, &worktree] {
        let reader = GitRepository::open(location, limits()).unwrap();
        reader.head().unwrap();
        for branch in ["main", "side", "x", "y", "wt", "absent"] {
            reader.branch_tip(branch).unwrap();
        }
        assert!(reader.branch_tip("bad..name").is_err());
        for hex in scratch.all_commits(&clone) {
            reader_parents(&reader, &hex);
            assert_eq!(
                reader.object_kind(&id(ObjectFormat::Sha1, &hex)).unwrap(),
                ObjectKind::Commit
            );
        }
        assert!(matches!(
            reader.read_commit(&id(ObjectFormat::Sha1, &tree)),
            Err(GitReadError::NotACommit {
                kind: ObjectKind::Tree,
                ..
            })
        ));
        assert!(
            reader
                .read_commit(&id(ObjectFormat::Sha1, &"0".repeat(40)))
                .is_err()
        );
        let _ = reader.config();
    }
    assert_eq!(tree_hash(&root_refs), before);
}

/// GIT-T09: a loose commit whose zlib stream is valid but whose bytes changed is refused.
#[test]
fn git_t09_corrupt_loose_commit_is_a_hash_mismatch() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    scratch.commit(&repo, "a.txt", "one");
    let target = scratch.commit(&repo, "a.txt", "two");
    let path = repo
        .join(".git/objects")
        .join(&target[..2])
        .join(&target[2..]);
    let mut raw = Vec::new();
    flate2::read::ZlibDecoder::new(fs::File::open(&path).unwrap())
        .read_to_end(&mut raw)
        .unwrap();
    let last = raw.len() - 2;
    assert_eq!(raw[last], b'o', "the message ends with the word two");
    raw[last] = b'x';
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&raw).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(&path, encoder.finish().unwrap()).unwrap();

    let reader = GitRepository::open(&repo, limits()).unwrap();
    let error = reader
        .read_commit(&id(ObjectFormat::Sha1, &target))
        .unwrap_err();
    assert!(
        matches!(error, GitReadError::HashMismatch { .. }),
        "{error:?}"
    );
    assert!(error.to_string().contains("hash mismatch"), "{error}");
}

/// P10: a shallow clone, and a linked worktree of one, are refused before any effect.
#[test]
fn shallow_clone_and_its_worktree_are_refused() {
    let scratch = Scratch::new();
    let base = scratch.init("base", &[]);
    scratch.commit(&base, "a.txt", "one");
    scratch.commit(&base, "a.txt", "two");
    let url = format!("file://{}", base.display());
    scratch.git(
        scratch.dir.path(),
        &["clone", "-q", "--depth", "1", &url, "shallow"],
    );
    let shallow = scratch.path("shallow");
    assert!(shallow.join(".git/shallow").is_file());
    scratch.git(
        &shallow,
        &["worktree", "add", "-q", "-b", "wt", "../shallow-wt"],
    );
    let worktree = scratch.path("shallow-wt");
    let before = tree_hash(&[&shallow.join(".git"), &worktree.join(".git")]);
    for location in [&shallow, &worktree] {
        let error = GitRepository::open(location, limits()).unwrap_err();
        assert!(matches!(error, GitReadError::Shallow { .. }), "{error:?}");
        assert!(error.to_string().contains("shallow clone"), "{error}");
    }
    assert_eq!(
        tree_hash(&[&shallow.join(".git"), &worktree.join(".git")]),
        before
    );
}

/// COMP-T06 and SRC-09: a missing commit, a bare repository and a plain folder are refused.
#[test]
fn missing_commits_bare_repositories_and_plain_folders_are_refused() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let tip = scratch.commit(&repo, "a.txt", "one");
    let reader = GitRepository::open(&repo, limits()).unwrap();
    let missing = id(ObjectFormat::Sha1, &"1".repeat(40));
    assert!(matches!(
        reader.read_commit(&missing),
        Err(GitReadError::ObjectMissing { .. })
    ));
    assert!(matches!(
        reader.object_kind(&missing),
        Err(GitReadError::ObjectMissing { .. })
    ));

    scratch.git(
        scratch.dir.path(),
        &["clone", "-q", "--bare", "repo", "bare.git"],
    );
    let error = GitRepository::open(&scratch.path("bare.git"), limits()).unwrap_err();
    assert!(matches!(error, GitReadError::Bare { .. }), "{error:?}");
    scratch.git(&repo, &["config", "core.bare", "true"]);
    let error = GitRepository::open(&repo, limits()).unwrap_err();
    assert!(matches!(error, GitReadError::Bare { .. }), "{error:?}");
    scratch.git(&repo, &["config", "core.bare", "false"]);

    let plain = scratch.path("plain");
    fs::create_dir(&plain).unwrap();
    let error = GitRepository::open(&plain, limits()).unwrap_err();
    assert!(
        matches!(error, GitReadError::NotAWorktree { .. }),
        "{error:?}"
    );
    let error = GitRepository::open(&scratch.path("absent"), limits()).unwrap_err();
    assert!(matches!(error, GitReadError::Io { .. }), "{error:?}");
    fs::create_dir(plain.join(".git")).unwrap();
    let error = GitRepository::open(&plain, limits()).unwrap_err();
    assert!(
        matches!(error, GitReadError::NotARepository { .. }),
        "{error:?}"
    );
    fs::write(plain.join("file.git"), "gitdir: /nonexistent/place\n").unwrap();
    fs::remove_dir(plain.join(".git")).unwrap();
    fs::rename(plain.join("file.git"), plain.join(".git")).unwrap();
    let error = GitRepository::open(&plain, limits()).unwrap_err();
    assert!(
        matches!(error, GitReadError::NotARepository { .. }),
        "{error:?}"
    );
    assert_eq!(
        reader.head().unwrap(),
        Head::Branch {
            name: b"refs/heads/main".to_vec(),
            tip: Some(id(ObjectFormat::Sha1, &tip)),
        }
    );
}

/// Trees, blobs and annotated tags have their own kinds, and are not commits.
#[test]
fn other_object_kinds_are_not_commits() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    scratch.commit(&repo, "a.txt", "one");
    scratch.git(&repo, &["tag", "-a", "-m", "annotated", "v1"]);
    let reader = GitRepository::open(&repo, limits()).unwrap();
    for (spec, kind) in [
        ("HEAD^{tree}", ObjectKind::Tree),
        ("HEAD:a.txt", ObjectKind::Blob),
        ("v1", ObjectKind::Tag),
    ] {
        let object = id(ObjectFormat::Sha1, &scratch.rev(&repo, spec));
        assert_eq!(reader.object_kind(&object).unwrap(), kind);
        let error = reader.read_commit(&object).unwrap_err();
        assert_eq!(
            error,
            GitReadError::NotACommit {
                id: object.clone(),
                kind
            }
        );
        assert!(error.to_string().contains(kind.name()), "{error}");
    }
}

/// An unborn branch, a detached `HEAD` and a symbolic branch resolve as Git resolves them.
#[test]
fn head_and_branch_states() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let reader = GitRepository::open(&repo, limits()).unwrap();
    assert_eq!(
        reader.head().unwrap(),
        Head::Branch {
            name: b"refs/heads/main".to_vec(),
            tip: None
        }
    );
    assert_eq!(reader.branch_tip("main").unwrap(), None);

    let tip = scratch.commit(&repo, "a.txt", "one");
    scratch.git(
        &repo,
        &["symbolic-ref", "refs/heads/alias", "refs/heads/main"],
    );
    assert_eq!(reader.branch_tip("alias").unwrap().unwrap().to_hex(), tip);
    scratch.git(&repo, &["switch", "-q", "--detach", "HEAD"]);
    assert_eq!(
        reader.head().unwrap(),
        Head::Detached(id(ObjectFormat::Sha1, &tip))
    );

    for bad in ["", "a b", "bad..name", "trailing/", "lock.lock"] {
        let error = reader.branch_tip(bad).unwrap_err();
        assert!(
            matches!(error, GitReadError::InvalidBranchName { .. }),
            "{bad:?}: {error:?}"
        );
    }
    for level in 0..6 {
        let to = if level == 0 {
            "refs/heads/main".to_string()
        } else {
            format!("refs/heads/s{}", level - 1)
        };
        scratch.git(
            &repo,
            &["symbolic-ref", &format!("refs/heads/s{level}"), &to],
        );
    }
    // Git reads at most five refs in one lookup, so s3 resolves and s4 is dangling.
    for level in 0..6 {
        let name = format!("refs/heads/s{level}");
        let git = scratch.try_git(&repo, &["rev-parse", "--verify", "-q", &name]);
        match reader.branch_tip(&format!("s{level}")) {
            Ok(Some(found)) => assert_eq!(Some(format!("{found}\n")), git, "s{level}"),
            Ok(None) => panic!("s{level} exists"),
            Err(error) => {
                assert!(
                    git.is_none(),
                    "Git resolves s{level}, the reader gives {error:?}"
                );
                assert!(
                    matches!(error, GitReadError::SymbolicDepth { .. }),
                    "{error:?}"
                );
            }
        }
    }
    assert!(reader.branch_tip("s3").unwrap().is_some());
    // The read of HEAD counts too, so HEAD -> s2 resolves and HEAD -> s3 does not.
    for level in [2, 3] {
        let name = format!("refs/heads/s{level}");
        scratch.git(&repo, &["symbolic-ref", "HEAD", &name]);
        let git = scratch.try_git(&repo, &["rev-parse", "--verify", "-q", "HEAD"]);
        match reader.head() {
            Ok(Head::Branch {
                tip: Some(found), ..
            }) => {
                assert_eq!(Some(format!("{found}\n")), git, "HEAD -> s{level}")
            }
            other => {
                assert!(
                    git.is_none(),
                    "Git resolves HEAD -> s{level}, the reader gives {other:?}"
                );
                assert!(
                    matches!(other, Err(GitReadError::SymbolicDepth { .. })),
                    "{other:?}"
                );
            }
        }
    }
    assert!(matches!(
        reader.head(),
        Err(GitReadError::SymbolicDepth { .. })
    ));
    let shallow = GitRepository::open(
        &repo,
        Limits {
            symbolic_depth: 0,
            ..limits()
        },
    )
    .unwrap();
    assert!(matches!(
        shallow.branch_tip("main"),
        Err(GitReadError::SymbolicDepth { .. })
    ));
    assert!(matches!(
        shallow.head(),
        Err(GitReadError::SymbolicDepth { .. })
    ));
    fs::write(repo.join(".git/refs/heads/broken"), "not a ref\n").unwrap();
    assert!(matches!(
        reader.branch_tip("broken"),
        Err(GitReadError::Ref { .. })
    ));
    fs::write(repo.join(".git/HEAD"), "garbage\n").unwrap();
    assert!(matches!(reader.head(), Err(GitReadError::Ref { .. })));
}

/// The three configuration keys come from the repository's own files, with worktree configuration.
#[test]
fn repository_configuration_keys() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    scratch.commit(&repo, "a.txt", "one");
    let config = GitRepository::open(&repo, limits())
        .unwrap()
        .config()
        .clone();
    assert_eq!(config.object_format, ObjectFormat::Sha1);
    assert_eq!(config.ref_storage, None);
    assert_eq!(config.hooks_path, None);
    assert!(!config.unread_includes);

    scratch.git(&repo, &["config", "core.hooksPath", ".husky/_"]);
    scratch.git(&repo, &["config", "core.repositoryformatversion", "1"]);
    scratch.git(&repo, &["config", "extensions.refStorage", "files"]);
    scratch.git(&repo, &["config", "include.path", "elsewhere.inc"]);
    let config = GitRepository::open(&repo, limits())
        .unwrap()
        .config()
        .clone();
    assert_eq!(config.hooks_path.as_deref(), Some(&b".husky/_"[..]));
    assert_eq!(config.ref_storage.as_deref(), Some(&b"files"[..]));
    assert!(config.unread_includes);

    scratch.git(&repo, &["config", "extensions.worktreeConfig", "true"]);
    scratch.git(
        &repo,
        &["config", "--worktree", "core.hooksPath", "worktree-hooks"],
    );
    let config = GitRepository::open(&repo, limits())
        .unwrap()
        .config()
        .clone();
    assert_eq!(config.hooks_path.as_deref(), Some(&b"worktree-hooks"[..]));

    let cases: [ConfigCase; 5] = [
        ("extensions.objectFormat", "sha512", |e| {
            matches!(e, GitReadError::UnsupportedObjectFormat { .. })
        }),
        ("extensions.refStorage", "other", |e| {
            matches!(e, GitReadError::UnsupportedRefStorage { .. })
        }),
        ("core.repositoryformatversion", "2", |e| {
            matches!(e, GitReadError::UnsupportedRepositoryVersion { .. })
        }),
        ("core.repositoryformatversion", "zero", |e| {
            matches!(e, GitReadError::Config { .. })
        }),
        ("core.bare", "maybe", |e| {
            matches!(e, GitReadError::Config { .. })
        }),
    ];
    for (index, (key, value, expected)) in cases.into_iter().enumerate() {
        let copy = scratch.path(&format!("copy-{index}"));
        scratch.git(
            scratch.dir.path(),
            &["clone", "-q", "repo", copy.to_str().unwrap()],
        );
        scratch.git(
            &copy,
            &[
                "config",
                "--file",
                ".git/config",
                "core.repositoryformatversion",
                "1",
            ],
        );
        scratch.git(&copy, &["config", "--file", ".git/config", key, value]);
        let error = GitRepository::open(&copy, limits()).unwrap_err();
        assert!(expected(&error), "{key}={value}: {error:?}");
    }
    let version_zero = scratch.path("version-zero");
    scratch.git(
        scratch.dir.path(),
        &["clone", "-q", "repo", version_zero.to_str().unwrap()],
    );
    scratch.git(
        &version_zero,
        &[
            "config",
            "--file",
            ".git/config",
            "core.repositoryformatversion",
            "0",
        ],
    );
    scratch.git(
        &version_zero,
        &[
            "config",
            "--file",
            ".git/config",
            "extensions.objectFormat",
            "sha1",
        ],
    );
    let error = GitRepository::open(&version_zero, limits()).unwrap_err();
    assert!(
        matches!(error, GitReadError::UnsupportedRepositoryVersion { .. }),
        "{error:?}"
    );
    let broken = scratch.path("broken-config");
    scratch.git(
        scratch.dir.path(),
        &["clone", "-q", "repo", broken.to_str().unwrap()],
    );
    fs::write(broken.join(".git/config"), "[core\n").unwrap();
    assert!(matches!(
        GitRepository::open(&broken, limits()),
        Err(GitReadError::Config { .. })
    ));
}

/// Each limit stops the read that would pass it, with a typed error.
#[test]
fn limits_bound_every_read() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let history = branchy_history(&scratch, &repo);
    let octopus = scratch.rev(&repo, "main");
    let config = fs::metadata(repo.join(".git/config")).unwrap().len();
    let error = GitRepository::open(
        &repo,
        Limits {
            config_bytes: config - 1,
            ..limits()
        },
    )
    .unwrap_err();
    assert!(matches!(error, GitReadError::TooLarge { .. }), "{error:?}");
    assert!(error.to_string().contains("limit"), "{error}");

    let head_path = repo.join(".git/HEAD");
    let head_text = fs::read_to_string(&head_path).unwrap();
    let head = head_text.len() as u64;
    let small = Limits {
        small_file_bytes: head - 1,
        ..limits()
    };
    let error = GitRepository::open(&repo, small).unwrap_err();
    assert!(matches!(error, GitReadError::TooLarge { .. }), "{error:?}");
    // A HEAD that grows after the open is refused by the read.
    let reader = GitRepository::open(
        &repo,
        Limits {
            small_file_bytes: head,
            ..limits()
        },
    )
    .unwrap();
    fs::write(&head_path, format!("{head_text} ")).unwrap();
    assert!(matches!(reader.head(), Err(GitReadError::TooLarge { .. })));
    fs::write(&head_path, &head_text).unwrap();
    let main = fs::metadata(repo.join(".git/refs/heads/main"))
        .unwrap()
        .len();
    let small = GitRepository::open(
        &repo,
        Limits {
            small_file_bytes: main - 1,
            ..limits()
        },
    )
    .unwrap();
    assert!(matches!(
        small.branch_tip("main"),
        Err(GitReadError::TooLarge { .. })
    ));

    let reader = GitRepository::open(
        &repo,
        Limits {
            parents: 2,
            ..limits()
        },
    )
    .unwrap();
    let error = reader
        .read_commit(&id(ObjectFormat::Sha1, &octopus))
        .unwrap_err();
    assert!(
        matches!(error, GitReadError::TooManyParents { .. }),
        "{error:?}"
    );
    let count = scratch.raw_parents(&repo, &octopus).len();
    assert_eq!(count, 3);
    let reader = GitRepository::open(
        &repo,
        Limits {
            parents: count,
            ..limits()
        },
    )
    .unwrap();
    assert_eq!(
        reader_parents(&reader, &octopus),
        scratch.raw_parents(&repo, &octopus)
    );
    let size: u64 = scratch
        .git(&repo, &["cat-file", "-s", &history[0]])
        .trim()
        .parse()
        .unwrap();
    let reader = GitRepository::open(
        &repo,
        Limits {
            commit_bytes: size,
            ..limits()
        },
    )
    .unwrap();
    reader_parents(&reader, &history[0]);
    let reader = GitRepository::open(
        &repo,
        Limits {
            commit_bytes: size - 1,
            ..limits()
        },
    )
    .unwrap();
    assert!(matches!(
        reader.read_commit(&id(ObjectFormat::Sha1, &history[0])),
        Err(GitReadError::TooLarge { .. })
    ));
    let reader = GitRepository::open(
        &repo,
        Limits {
            commit_bytes: 100,
            ..limits()
        },
    )
    .unwrap();
    let error = reader
        .read_commit(&id(ObjectFormat::Sha1, &history[0]))
        .unwrap_err();
    assert!(matches!(error, GitReadError::TooLarge { .. }), "{error:?}");

    scratch.git(&repo, &["pack-refs", "--all"]);
    let packed = fs::metadata(repo.join(".git/packed-refs")).unwrap().len();
    let reader = GitRepository::open(
        &repo,
        Limits {
            packed_refs_bytes: packed - 1,
            ..limits()
        },
    )
    .unwrap();
    assert!(matches!(
        reader.branch_tip("side"),
        Err(GitReadError::TooLarge { .. })
    ));
    let reader = GitRepository::open(
        &repo,
        Limits {
            packed_refs_bytes: packed,
            ..limits()
        },
    )
    .unwrap();
    assert_eq!(
        reader.branch_tip("side").unwrap().unwrap().to_hex(),
        scratch.rev(&repo, "side")
    );
}

/// A commit body without a tree line is malformed even when its hash is right.
#[test]
fn malformed_commit_body_is_refused() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    scratch.commit(&repo, "a.txt", "one");
    let tree = scratch.rev(&repo, "HEAD^{tree}");
    let signature = "author A <a@b> 1 +0000\ncommitter A <a@b> 1 +0000\n\n";
    for body in [
        String::new(),
        format!("{signature}no tree\n"),
        format!("tree {tree}\nparent zz\n{signature}bad parent\n"),
    ] {
        let path = repo.join("body.txt");
        fs::write(&path, body).unwrap();
        let hex = scratch
            .git(
                &repo,
                &[
                    "hash-object",
                    "-t",
                    "commit",
                    "--literally",
                    "-w",
                    "body.txt",
                ],
            )
            .trim()
            .to_string();
        let reader = GitRepository::open(&repo, limits()).unwrap();
        let error = reader
            .read_commit(&id(ObjectFormat::Sha1, &hex))
            .unwrap_err();
        assert!(
            matches!(error, GitReadError::MalformedCommit { .. }),
            "{error:?}"
        );
        assert!(
            std::error::Error::source(&error).is_some(),
            "the cause is kept"
        );
    }
}

/// A linked worktree of a bare repository is a location, though the common `core.bare` is true.
#[test]
fn linked_worktree_of_a_bare_repository_opens() {
    let scratch = Scratch::new();
    let source = scratch.init("source", &[]);
    let tip = scratch.commit(&source, "a.txt", "one");
    scratch.git(
        scratch.dir.path(),
        &["clone", "-q", "--bare", "source", "bare.git"],
    );
    let bare = scratch.path("bare.git");
    scratch.git(&bare, &["worktree", "add", "-q", "../bwt", "main"]);
    let reader = GitRepository::open(&scratch.path("bwt"), limits()).unwrap();
    assert!(reader.is_linked_worktree());
    assert_eq!(reader.common_dir(), fs::canonicalize(&bare).unwrap());
    assert_eq!(reader.branch_tip("main").unwrap().unwrap().to_hex(), tip);
    assert!(matches!(
        GitRepository::open(&bare, limits()),
        Err(GitReadError::Bare { .. })
    ));
}

/// An extension of version 1 that Git does not know makes Git refuse the repository, and the reader too.
#[test]
fn unknown_version_one_extensions_are_refused() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    scratch.commit(&repo, "a.txt", "one");
    scratch.git(&repo, &["config", "core.repositoryformatversion", "1"]);
    scratch.git(&repo, &["config", "extensions.worktreeConfig", "true"]);
    scratch.git(&repo, &["config", "extensions.noop", "true"]);
    GitRepository::open(&repo, limits()).unwrap();
    for (index, key) in ["extensions.frobnicate", "extensions.compatObjectFormat"]
        .into_iter()
        .enumerate()
    {
        let copy = format!("copy-{index}");
        scratch.git(scratch.dir.path(), &["clone", "-q", "repo", &copy]);
        let config = format!("{copy}/.git/config");
        scratch.git(
            scratch.dir.path(),
            &[
                "config",
                "--file",
                &config,
                "core.repositoryformatversion",
                "1",
            ],
        );
        scratch.git(
            scratch.dir.path(),
            &["config", "--file", &config, key, "sha256"],
        );
        let error = GitRepository::open(&scratch.path(&copy), limits()).unwrap_err();
        let name = key.trim_start_matches("extensions.");
        assert!(
            matches!(&error, GitReadError::UnsupportedExtension { name: found } if found.eq_ignore_ascii_case(name)),
            "{error:?}"
        );
    }
}

/// A branch name that is a folder of other branches is absent, whatever the size of the folder.
#[test]
fn branch_folder_is_absent_not_too_large() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    scratch.commit(&repo, "a.txt", "one");
    for index in 0..200 {
        scratch.git(&repo, &["branch", &format!("feature/b{index}")]);
    }
    let folder = fs::metadata(repo.join(".git/refs/heads/feature"))
        .unwrap()
        .len();
    let reader = GitRepository::open(
        &repo,
        Limits {
            small_file_bytes: folder - 1,
            ..limits()
        },
    )
    .unwrap();
    assert_eq!(reader.branch_tip("feature").unwrap(), None);
    assert!(reader.branch_tip("feature/b7").unwrap().is_some());
}

/// A ref that gix finds under another full name, such as a tag named `refs/heads/ghost`, is not the branch.
#[test]
fn tag_named_like_a_branch_is_not_the_branch() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    scratch.commit(&repo, "a.txt", "one");
    scratch.git(&repo, &["update-ref", "refs/tags/refs/heads/ghost", "HEAD"]);
    let reader = GitRepository::open(&repo, limits()).unwrap();
    assert_eq!(reader.branch_tip("ghost").unwrap(), None);
}

/// A linked worktree whose `.git` file or `commondir` is too large or names no folder is refused.
#[test]
fn broken_linked_worktrees_are_refused() {
    let scratch = Scratch::new();
    let main = scratch.init("main", &[]);
    scratch.commit(&main, "a.txt", "one");
    scratch.git(
        &main,
        &["worktree", "add", "-q", "-b", "feature", "../linked"],
    );
    let linked = scratch.path("linked");
    let dot_git = fs::metadata(linked.join(".git")).unwrap().len();
    let error = GitRepository::open(
        &linked,
        Limits {
            small_file_bytes: dot_git - 1,
            ..limits()
        },
    )
    .unwrap_err();
    assert!(matches!(error, GitReadError::TooLarge { .. }), "{error:?}");

    // A long `commondir` that names the same folder is the largest small file, so its limit is
    // the one that stops the open.
    let commondir = main.join(".git/worktrees/linked/commondir");
    let head = fs::metadata(main.join(".git/worktrees/linked/HEAD"))
        .unwrap()
        .len();
    fs::write(&commondir, format!("{}../..\n", "./".repeat(100))).unwrap();
    let size = fs::metadata(&commondir).unwrap().len();
    assert!(size > dot_git.max(head) + 1);
    GitRepository::open(&linked, limits()).unwrap();
    let error = GitRepository::open(
        &linked,
        Limits {
            small_file_bytes: size - 1,
            ..limits()
        },
    )
    .unwrap_err();
    assert!(
        matches!(&error, GitReadError::TooLarge { what, .. } if what.ends_with("commondir")),
        "{error:?}"
    );

    fs::write(&commondir, "../../missing-common\n").unwrap();
    let error = GitRepository::open(&linked, limits()).unwrap_err();
    assert!(
        matches!(error, GitReadError::NotARepository { .. }),
        "{error:?}"
    );
    assert!(
        std::error::Error::source(&error).is_some(),
        "the cause is kept"
    );
}

/// Write `.git/config` of a new repository `name` with format `version` and the text `extra`.
fn config_repository(scratch: &Scratch, name: &str, version: u8, extra: &str) -> PathBuf {
    let repo = scratch.init(name, &[]);
    scratch.commit(&repo, "a.txt", "one");
    let core = format!(
        "[core]\n\trepositoryformatversion = {version}\n\tfilemode = true\n\tbare = false\n"
    );
    fs::write(repo.join(".git/config"), format!("{core}{extra}")).unwrap();
    repo
}

/// The reader applies Git's rules for the keys below `extensions`, in both format versions.
#[test]
fn extensions_follow_the_rules_of_git() {
    let scratch = Scratch::new();
    type Expect = fn(&Result<GitRepository, GitReadError>) -> bool;
    let opens: Expect = |result| result.is_ok();
    let version: Expect = |result| {
        matches!(
            result,
            Err(GitReadError::UnsupportedRepositoryVersion { version: 0 })
        )
    };
    let unknown: Expect = |result| matches!(result, Err(GitReadError::UnsupportedExtension { .. }));
    let valueless: Expect = |result| matches!(result, Err(GitReadError::Config { .. }));
    let cases: [(u8, &str, Expect); 13] = [
        (0, "[extensions]\n\tsubmodulePathConfig = true\n", version),
        (1, "[extensions]\n\tsubmodulePathConfig = true\n", opens),
        (0, "[extensions]\n\tfrobnicate = x\n", opens),
        (
            0,
            "[extensions]\n\tworktreeConfig = true\n\tpreciousObjects = true\n",
            opens,
        ),
        (0, "[extensions]\n\trelativeWorktrees = true\n", version),
        (0, "[extensions]\n\trefStorage = files\n", version),
        (0, "[extensions]\n\tobjectFormat = sha1\n", version),
        (0, "[extensions]\n\tnoop-v1 = true\n", version),
        (1, "[extensions \"x\"]\n\tnoop = true\n", unknown),
        (1, "[extensions]\n\tobjectFormat\n", valueless),
        (1, "[extensions]\n\trefStorage\n", valueless),
        (
            1,
            "[extensions]\n\tnoop-v1 = true\n\tpreciousObjects\n\tpartialClone = origin\n",
            opens,
        ),
        (1, "[extensions]\n\trelativeWorktrees = true\n", opens),
    ];
    for (index, (format, extra, expect)) in cases.into_iter().enumerate() {
        let repo = config_repository(&scratch, &format!("ext-{index}"), format, extra);
        let result = GitRepository::open(&repo, limits());
        assert!(expect(&result), "case {index} {extra:?}: {result:?}");
        let git = scratch.try_git(&repo, &["rev-parse", "--git-dir"]);
        assert_eq!(
            git.is_some(),
            result.is_ok(),
            "case {index}: Git and the reader agree"
        );
    }
    let repo = config_repository(
        &scratch,
        "ext-sub",
        1,
        "[extensions \"x\"]\n\tnoop = true\n",
    );
    let error = GitRepository::open(&repo, limits()).unwrap_err();
    assert_eq!(
        error,
        GitReadError::UnsupportedExtension {
            name: "x.noop".into()
        }
    );
}

/// A `core.worktree` that names another folder is refused, and Git ignores it in a linked worktree.
#[test]
fn core_worktree_must_name_the_opened_folder() {
    let scratch = Scratch::new();
    let repo = scratch.init("cw", &[]);
    scratch.commit(&repo, "a.txt", "one");
    let other = scratch.path("cwt");
    fs::create_dir(&other).unwrap();
    let canonical = fs::canonicalize(&other).unwrap();
    scratch.git(
        &repo,
        &["config", "core.worktree", canonical.to_str().unwrap()],
    );
    let top = scratch.git(&repo, &["rev-parse", "--show-toplevel"]);
    assert_eq!(Path::new(top.trim()), canonical, "Git's worktree is cwt");
    let error = GitRepository::open(&repo, limits()).unwrap_err();
    assert!(
        matches!(&error, GitReadError::ForeignWorktree { configured, .. } if configured == &canonical),
        "{error:?}"
    );
    assert!(error.to_string().contains("core.worktree"), "{error}");

    scratch.git(&repo, &["config", "core.worktree", ".."]);
    GitRepository::open(&repo, limits()).unwrap();
    let config = repo.join(".git/config");
    let config = config.to_str().unwrap();
    let root = scratch.dir.path();
    scratch.git(
        root,
        &["config", "--file", config, "core.worktree", "../missing"],
    );
    assert!(matches!(
        GitRepository::open(&repo, limits()),
        Err(GitReadError::ForeignWorktree { .. })
    ));

    let target = canonical.to_str().unwrap();
    scratch.git(root, &["config", "--file", config, "core.worktree", target]);
    let linked = scratch.path("linked");
    let linked_text = linked.to_str().unwrap();
    scratch.git(
        &repo,
        &["worktree", "add", "-q", "-b", "linked", linked_text],
    );
    let top = scratch.git(&linked, &["rev-parse", "--show-toplevel"]);
    assert_eq!(
        Path::new(top.trim()),
        fs::canonicalize(&linked).unwrap(),
        "Git ignores it in a linked worktree"
    );
    GitRepository::open(&linked, limits()).unwrap();

    scratch.git(
        root,
        &["config", "--file", config, "--unset", "core.worktree"],
    );
    scratch.git(
        root,
        &[
            "config",
            "--file",
            config,
            "core.repositoryformatversion",
            "1",
        ],
    );
    scratch.git(
        root,
        &[
            "config",
            "--file",
            config,
            "extensions.worktreeConfig",
            "true",
        ],
    );
    let worktree_config = repo.join(".git/config.worktree");
    let worktree_config = worktree_config.to_str().unwrap();
    scratch.git(
        root,
        &["config", "--file", worktree_config, "core.worktree", target],
    );
    let top = scratch.git(&repo, &["rev-parse", "--show-toplevel"]);
    assert_eq!(
        Path::new(top.trim()),
        canonical,
        "Git reads core.worktree of config.worktree"
    );
    assert!(matches!(
        GitRepository::open(&repo, limits()),
        Err(GitReadError::ForeignWorktree { .. })
    ));
}

/// A corrupt object that cannot be decompressed is a read error with its cause, not a missing object.
#[test]
fn undecodable_object_is_a_read_error() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let hex = scratch.commit(&repo, "a.txt", "one");
    let path = repo.join(".git/objects").join(&hex[..2]).join(&hex[2..]);
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    #[allow(
        clippy::permissions_set_readonly_false,
        reason = "the test makes one loose object of its own scratch repository writable to corrupt it"
    )]
    permissions.set_readonly(false);
    fs::set_permissions(&path, permissions).unwrap();
    fs::write(&path, b"this is not a zlib stream").unwrap();
    let reader = GitRepository::open(&repo, limits()).unwrap();
    let commit = id(ObjectFormat::Sha1, &hex);
    for error in [
        reader.object_kind(&commit).unwrap_err(),
        reader.read_commit(&commit).unwrap_err(),
    ] {
        assert!(
            matches!(error, GitReadError::ObjectRead { .. }),
            "{error:?}"
        );
        assert!(
            std::error::Error::source(&error).is_some(),
            "the cause is kept"
        );
    }
    assert!(
        scratch
            .try_git(&repo, &["--no-replace-objects", "cat-file", "-t", &hex])
            .is_none()
    );
}

/// A linked worktree reads its own `config.worktree`, with its includes, size and parse errors.
#[test]
fn linked_worktree_config_is_read() {
    let scratch = Scratch::new();
    let main = scratch.init("main", &[]);
    scratch.commit(&main, "a.txt", "one");
    scratch.git(&main, &["config", "core.repositoryformatversion", "1"]);
    scratch.git(&main, &["config", "extensions.worktreeConfig", "true"]);
    scratch.git(&main, &["config", "core.hooksPath", "main-hooks"]);
    scratch.git(
        &main,
        &["worktree", "add", "-q", "-b", "linked", "../linked"],
    );
    let linked = scratch.path("linked");
    scratch.git(
        &linked,
        &["config", "--worktree", "core.hooksPath", "linked-hooks"],
    );
    for (location, hooks) in [(&main, "main-hooks"), (&linked, "linked-hooks")] {
        let reader = GitRepository::open(location, limits()).unwrap();
        assert_eq!(
            reader.config().hooks_path.as_deref(),
            Some(hooks.as_bytes())
        );
        assert_eq!(
            scratch.git(location, &["config", "core.hooksPath"]).trim(),
            hooks
        );
        assert!(!reader.config().unread_includes);
    }
    let file = main.join(".git/worktrees/linked/config.worktree");
    let text = fs::read_to_string(&file).unwrap();
    fs::write(&file, format!("{text}[include]\n\tpath = elsewhere\n")).unwrap();
    assert!(
        GitRepository::open(&linked, limits())
            .unwrap()
            .config()
            .unread_includes
    );
    assert!(
        !GitRepository::open(&main, limits())
            .unwrap()
            .config()
            .unread_includes
    );

    fs::write(&file, "[broken\n").unwrap();
    assert!(matches!(
        GitRepository::open(&linked, limits()),
        Err(GitReadError::Config { .. })
    ));
    let config = fs::metadata(main.join(".git/config")).unwrap().len();
    fs::write(&file, format!("{}\n", "#".repeat(config as usize + 10))).unwrap();
    let small = Limits {
        config_bytes: config + 5,
        ..limits()
    };
    GitRepository::open(&main, small).unwrap();
    assert!(matches!(
        GitRepository::open(&linked, small),
        Err(GitReadError::TooLarge { .. })
    ));
}

/// An `includeIf` section, like an `include` section, marks configuration that the reader does not follow.
#[test]
fn include_if_marks_unread_includes() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    scratch.commit(&repo, "a.txt", "one");
    assert!(
        !GitRepository::open(&repo, limits())
            .unwrap()
            .config()
            .unread_includes
    );
    scratch.git(
        &repo,
        &["config", "includeIf.gitdir:/nowhere/.path", "extra"],
    );
    assert!(
        GitRepository::open(&repo, limits())
            .unwrap()
            .config()
            .unread_includes
    );
}

/// `core.bare` and `core.worktree` of `config.worktree` apply to each worktree, main or linked.
#[test]
fn worktree_config_sets_bare_and_worktree() {
    let scratch = Scratch::new();
    let main = scratch.init("main", &[]);
    scratch.commit(&main, "a.txt", "one");
    scratch.git(&main, &["config", "core.repositoryformatversion", "1"]);
    scratch.git(&main, &["config", "extensions.worktreeConfig", "true"]);
    let linked = scratch.path("linked");
    scratch.git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked",
            linked.to_str().unwrap(),
        ],
    );
    let other = scratch.path("other");
    fs::create_dir(&other).unwrap();
    let other = fs::canonicalize(&other).unwrap();
    let main_file = main.join(".git/config.worktree");
    let linked_file = main.join(".git/worktrees/linked/config.worktree");
    let root = scratch.dir.path();
    let set = |file: &Path, key: &str, value: &str| {
        scratch.git(
            root,
            &["config", "--file", file.to_str().unwrap(), key, value],
        );
    };
    let clear = |file: &Path| fs::write(file, "").unwrap();

    for (location, file) in [(&main, &main_file), (&linked, &linked_file)] {
        set(file, "core.bare", "true");
        assert_eq!(
            scratch
                .git(location, &["rev-parse", "--is-bare-repository"])
                .trim(),
            "true"
        );
        let error = GitRepository::open(location, limits()).unwrap_err();
        assert!(matches!(error, GitReadError::Bare { .. }), "{error:?}");
        set(file, "core.bare", "false");
        GitRepository::open(location, limits()).unwrap();
        clear(file);
    }

    set(&linked_file, "core.worktree", other.to_str().unwrap());
    let top = scratch.git(&linked, &["rev-parse", "--show-toplevel"]);
    assert_eq!(
        Path::new(top.trim()),
        other,
        "Git's worktree is the folder that config.worktree names"
    );
    let error = GitRepository::open(&linked, limits()).unwrap_err();
    assert!(
        matches!(&error, GitReadError::ForeignWorktree { configured, .. } if configured == &other),
        "{error:?}"
    );
    set(
        &linked_file,
        "core.worktree",
        fs::canonicalize(&linked).unwrap().to_str().unwrap(),
    );
    GitRepository::open(&linked, limits()).unwrap();
    clear(&linked_file);

    // With `extensions.worktreeConfig`, the common values apply to each worktree, and
    // `config.worktree` overrides them.
    scratch.git(&main, &["config", "core.worktree", other.to_str().unwrap()]);
    let top = scratch.git(&linked, &["rev-parse", "--show-toplevel"]);
    assert_eq!(
        Path::new(top.trim()),
        other,
        "the common core.worktree applies"
    );
    assert!(matches!(
        GitRepository::open(&linked, limits()),
        Err(GitReadError::ForeignWorktree { .. })
    ));
    set(
        &linked_file,
        "core.worktree",
        fs::canonicalize(&linked).unwrap().to_str().unwrap(),
    );
    GitRepository::open(&linked, limits()).unwrap();
    clear(&linked_file);
    scratch.git(
        root,
        &[
            "config",
            "--file",
            main.join(".git/config").to_str().unwrap(),
            "--unset",
            "core.worktree",
        ],
    );

    scratch.git(&main, &["config", "core.bare", "true"]);
    for location in [&main, &linked] {
        assert_eq!(
            scratch
                .git(location, &["rev-parse", "--is-bare-repository"])
                .trim(),
            "true"
        );
        assert!(matches!(
            GitRepository::open(location, limits()),
            Err(GitReadError::Bare { .. })
        ));
    }
    set(&linked_file, "core.bare", "false");
    assert_eq!(
        scratch
            .git(&linked, &["rev-parse", "--is-bare-repository"])
            .trim(),
        "false"
    );
    GitRepository::open(&linked, limits()).unwrap();
}

/// Without `extensions.worktreeConfig`, Git ignores `config.worktree`, and so does the reader.
#[test]
fn worktree_config_is_ignored_without_the_extension() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    scratch.commit(&repo, "a.txt", "one");
    scratch.git(&repo, &["config", "core.hooksPath", "shared-hooks"]);
    fs::write(
        repo.join(".git/config.worktree"),
        "[core]\n\thooksPath = own-hooks\n\tbare = true\n\tworktree = /nowhere\n[include]\n\tpath = x\n",
    )
    .unwrap();
    assert_eq!(
        scratch.git(&repo, &["config", "core.hooksPath"]).trim(),
        "shared-hooks"
    );
    assert_eq!(
        scratch
            .git(&repo, &["rev-parse", "--is-bare-repository"])
            .trim(),
        "false"
    );
    let reader = GitRepository::open(&repo, limits()).unwrap();
    assert_eq!(
        reader.config().hooks_path.as_deref(),
        Some(&b"shared-hooks"[..])
    );
    assert!(!reader.config().unread_includes);
}

/// The open checks the size of `HEAD` before gix reads it, with and without a `.git` entry.
#[test]
fn oversize_head_is_refused_before_it_is_read() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    scratch.commit(&repo, "a.txt", "one");
    let head = repo.join(".git/HEAD");
    let text = fs::read_to_string(&head).unwrap();
    fs::write(&head, format!("{text}{}", " ".repeat(5000))).unwrap();
    let error = GitRepository::open(&repo, limits()).unwrap_err();
    assert!(matches!(error, GitReadError::TooLarge { .. }), "{error:?}");

    scratch.git(scratch.dir.path(), &["init", "-q", "--bare", "bare.git"]);
    let bare = scratch.path("bare.git");
    fs::write(
        bare.join("HEAD"),
        format!("ref: refs/heads/main\n{}", " ".repeat(5000)),
    )
    .unwrap();
    let error = GitRepository::open(&bare, limits()).unwrap_err();
    assert!(matches!(error, GitReadError::TooLarge { .. }), "{error:?}");
}

/// Run `work` on another thread and fail the test when it takes longer than ten seconds.
fn within_seconds<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(work());
    });
    receive
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the read finishes within ten seconds and does not block")
}

/// Make a FIFO at `path`.
fn mkfifo(path: &Path) {
    let name = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
    // SAFETY: `name` is a valid C string that lives across the call.
    assert_eq!(
        unsafe { libc::mkfifo(name.as_ptr(), 0o600) },
        0,
        "mkfifo {}",
        path.display()
    );
}

/// Replace the entry at `path` with the special entry that `make` creates.
fn replace(path: &Path, make: impl FnOnce(&Path)) {
    if fs::symlink_metadata(path).is_ok() {
        fs::remove_file(path).unwrap();
    }
    make(path);
}

/// A label and the function that makes one special entry at a path.
type Maker = (&'static str, fn(&Path));

/// A FIFO, a device or a link out of `refs/` as `HEAD` is refused at once, as Git refuses it.
#[test]
fn special_head_entries_are_refused_at_once() {
    use std::os::unix::fs::symlink;
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let tip = scratch.commit(&repo, "a.txt", "one");
    let head = repo.join(".git/HEAD");
    fs::write(repo.join(".git/outside"), "ref: refs/heads/main\n").unwrap();
    let makers: [Maker; 3] = [
        ("device", |path| symlink("/dev/zero", path).unwrap()),
        ("fifo", mkfifo),
        ("outside", |path| symlink("outside", path).unwrap()),
    ];
    for (label, make) in makers {
        replace(&head, make);
        let opened = repo.clone();
        let result = within_seconds(move || GitRepository::open(&opened, limits()).err());
        assert!(
            matches!(result, Some(GitReadError::NotAFile { .. })),
            "{label}: {result:?}"
        );
        if label != "fifo" {
            assert!(
                scratch.try_git(&repo, &["rev-parse", "HEAD"]).is_none(),
                "Git refuses {label}"
            );
        }
    }
    // A HEAD that changes after the open to name a ref outside refs/ is refused, as Git refuses it.
    replace(&head, |path| {
        fs::write(path, "ref: refs/heads/main\n").unwrap()
    });
    let reader = GitRepository::open(&repo, limits()).unwrap();
    fs::write(&head, "ref: ORIG_HEAD\n").unwrap();
    assert!(
        scratch.try_git(&repo, &["rev-parse", "HEAD"]).is_none(),
        "Git refuses ORIG_HEAD"
    );
    assert!(matches!(reader.head(), Err(GitReadError::Ref { .. })));
    assert!(matches!(
        GitRepository::open(&repo, limits()),
        Err(GitReadError::NotARepository { .. })
    ));
    // A link to a ref below refs/ is a symbolic ref, as Git reads it.
    replace(&head, |path| symlink("refs/heads/main", path).unwrap());
    assert_eq!(scratch.rev(&repo, "HEAD"), tip);
    let reader = GitRepository::open(&repo, limits()).unwrap();
    assert_eq!(
        reader.head().unwrap(),
        Head::Branch {
            name: b"refs/heads/main".to_vec(),
            tip: Some(id(ObjectFormat::Sha1, &tip))
        }
    );
}

/// A FIFO or a device as `config`, `commondir`, `packed-refs`, a loose ref or `alternates` is
/// refused within the time limit, and never read.
#[test]
fn special_entries_never_block_a_read() {
    use std::os::unix::fs::symlink;
    let scratch = Scratch::new();
    let main = scratch.init("main", &[]);
    scratch.commit(&main, "a.txt", "one");
    scratch.git(
        &main,
        &["worktree", "add", "-q", "-b", "linked", "../linked"],
    );
    let linked = scratch.path("linked");
    // `config.worktree` is read only with `extensions.worktreeConfig`.
    scratch.git(&main, &["config", "extensions.worktreeConfig", "true"]);
    let git = main.join(".git");
    let cases = [
        (git.join("config"), main.clone(), "main"),
        (
            git.join("worktrees/linked/commondir"),
            linked.clone(),
            "main",
        ),
        (git.join("packed-refs"), main.clone(), "packed"),
        (git.join("refs/heads/loose"), main.clone(), "loose"),
        (git.join("objects/info/alternates"), main.clone(), "main"),
        (linked.join(".git"), linked.clone(), "main"),
        (
            git.join("worktrees/linked/config.worktree"),
            linked.clone(),
            "main",
        ),
    ];
    for (path, location, branch) in cases {
        let saved = fs::read(&path).ok();
        for device in [false, true] {
            replace(&path, |path| match device {
                true => symlink("/dev/zero", path).unwrap(),
                false => mkfifo(path),
            });
            let location = location.clone();
            let result = within_seconds(move || {
                GitRepository::open(&location, limits())
                    .and_then(|reader| reader.branch_tip(branch))
            });
            assert!(
                matches!(result, Err(GitReadError::NotAFile { .. })),
                "{} as {}: {result:?}",
                path.display(),
                if device { "a device" } else { "a FIFO" }
            );
        }
        fs::remove_file(&path).unwrap();
        if let Some(bytes) = saved {
            fs::write(&path, bytes).unwrap();
        }
    }
    GitRepository::open(&linked, limits()).unwrap();
}

/// `objects/info/alternates` is bounded in size, in count and in depth, and each alternate must
/// be a folder.
#[test]
fn alternates_are_bounded() {
    let scratch = Scratch::new();
    let mut chain = Vec::new();
    let mut tips = Vec::new();
    for index in 0..8 {
        let repo = scratch.init(&format!("r{index}"), &[]);
        tips.push(scratch.commit(&repo, "a.txt", &format!("{index}")));
        chain.push(fs::canonicalize(repo.join(".git/objects")).unwrap());
    }
    let alternates = |from: &Path, to: &[&Path]| {
        let text: String = to
            .iter()
            .map(|path| format!("{}\n", path.display()))
            .collect();
        fs::write(from.join("info/alternates"), text).unwrap();
    };
    let top = scratch.path("r0");
    alternates(&chain[0], &[&chain[1], &chain[2]]);
    GitRepository::open(&top, limits()).unwrap();
    GitRepository::open(
        &top,
        Limits {
            alternates: 2,
            ..limits()
        },
    )
    .unwrap();
    let error = GitRepository::open(
        &top,
        Limits {
            alternates: 1,
            ..limits()
        },
    )
    .unwrap_err();
    assert!(matches!(error, GitReadError::Alternate { .. }), "{error:?}");
    assert!(error.to_string().contains("more than 1"), "{error}");
    let size = fs::metadata(chain[0].join("info/alternates"))
        .unwrap()
        .len();
    let error = GitRepository::open(
        &top,
        Limits {
            alternates_bytes: size - 1,
            ..limits()
        },
    )
    .unwrap_err();
    assert!(matches!(error, GitReadError::TooLarge { .. }), "{error:?}");

    // Git reads six nested alternates files, at levels 0 to 5, and ignores a seventh. The reader
    // refuses the seventh.
    for index in 0..7 {
        alternates(&chain[index], &[&chain[index + 1]]);
    }
    let error = GitRepository::open(&top, limits()).unwrap_err();
    assert!(error.to_string().contains("deeper than 5"), "{error}");
    assert!(
        scratch
            .try_git(&top, &["cat-file", "-t", &tips[7]])
            .is_none(),
        "Git ignores level 6"
    );
    fs::remove_file(chain[6].join("info/alternates")).unwrap();
    assert_eq!(
        scratch.git(&top, &["cat-file", "-t", &tips[6]]).trim(),
        "commit"
    );
    let reader = GitRepository::open(&top, limits()).unwrap();
    reader
        .read_commit(&id(ObjectFormat::Sha1, &tips[6]))
        .unwrap();

    alternates(&chain[0], &[&scratch.path("missing")]);
    let error = GitRepository::open(&top, limits()).unwrap_err();
    assert!(error.to_string().contains("is not a folder"), "{error}");
    assert!(
        std::error::Error::source(&error).is_some(),
        "the cause is kept"
    );
    fs::write(chain[0].join("info/alternates"), "\"quoted\"\n").unwrap();
    assert!(matches!(
        GitRepository::open(&top, limits()),
        Err(GitReadError::Alternate { .. })
    ));
    fs::write(chain[0].join("info/alternates"), "# a comment\n\n").unwrap();
    GitRepository::open(&top, limits()).unwrap();
}

/// The open searches no parent folder, so a subfolder of a worktree and its `.git` folder are not
/// worktrees.
#[test]
fn subfolders_of_a_worktree_are_not_worktrees() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    scratch.commit(&repo, "a.txt", "one");
    fs::create_dir(repo.join("sub")).unwrap();
    for folder in [repo.join("sub"), repo.join(".git"), repo.join(".git/refs")] {
        let error = GitRepository::open(&folder, limits()).unwrap_err();
        assert!(
            matches!(error, GitReadError::NotAWorktree { .. }),
            "{}: {error:?}",
            folder.display()
        );
    }
}

/// The open reads no `GIT_*` variable and no global configuration, which a child process with
/// those settings shows. The parent checks that Git itself reads the global file.
#[test]
fn environment_and_global_configuration_are_ignored() {
    if let Some(repo) = std::env::var_os("GOSSIL_GIT_READER_CHILD") {
        let repo = PathBuf::from(repo);
        let reader = GitRepository::open(&repo, limits()).unwrap();
        assert_eq!(reader.config().hooks_path, None);
        assert_eq!(
            reader.git_dir(),
            fs::canonicalize(repo.join(".git")).unwrap()
        );
        assert_eq!(reader.object_format(), ObjectFormat::Sha1);
        return;
    }
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    scratch.commit(&repo, "a.txt", "one");
    let other = scratch.init("other", &["--object-format=sha256"]);
    scratch.commit(&other, "b.txt", "two");
    let home = scratch.path("child-home");
    fs::create_dir(&home).unwrap();
    fs::write(
        home.join(".gitconfig"),
        "[core]\n\thooksPath = /global-hooks\n",
    )
    .unwrap();
    let git = Command::new("git")
        .args(["config", "core.hooksPath"])
        .current_dir(&repo)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", &home)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&git.stdout).trim(),
        "/global-hooks",
        "Git reads the global file"
    );
    let other_git = other.join(".git");
    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args([
            "--exact",
            "environment_and_global_configuration_are_ignored",
            "--nocapture",
        ])
        .env("GOSSIL_GIT_READER_CHILD", &repo)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &home)
        .env("GIT_DIR", &other_git)
        .env("GIT_COMMON_DIR", &other_git)
        .env("GIT_OBJECT_DIRECTORY", other_git.join("objects"))
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.hooksPath")
        .env("GIT_CONFIG_VALUE_0", "/env-hooks");
    let output = child.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "the child fails: {stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A relative alternate at the top resolves as in Git and gix. A line keeps its spaces, as in
/// Git. A relative path in a nested file, a repeated folder and a cycle are refused, because gix
/// resolves or refuses them otherwise than Git.
#[test]
fn alternates_resolve_as_git_and_gix_do() {
    let scratch = Scratch::new();
    let mut tips = Vec::new();
    for name in ["a", "b", "c"] {
        let repo = scratch.init(name, &[]);
        tips.push(scratch.commit(&repo, "a.txt", name));
    }
    let objects = |name: &str| fs::canonicalize(scratch.path(name).join(".git/objects")).unwrap();
    let set =
        |name: &str, text: &str| fs::write(objects(name).join("info/alternates"), text).unwrap();
    let top = scratch.path("a");
    let refused = |label: &str, text: &str| {
        let error = GitRepository::open(&top, limits()).unwrap_err();
        assert!(
            matches!(error, GitReadError::Alternate { .. }),
            "{label}: {error:?}"
        );
        assert!(error.to_string().contains(text), "{label}: {error}");
    };

    set("a", "../../../b/.git/objects\n");
    assert_eq!(
        scratch.git(&top, &["cat-file", "-t", &tips[1]]).trim(),
        "commit"
    );
    let reader = GitRepository::open(&top, limits()).unwrap();
    assert_eq!(
        reader
            .read_commit(&id(ObjectFormat::Sha1, &tips[1]))
            .unwrap()
            .parents(),
        &[]
    );

    set("a", &format!("{} \n", objects("b").display()));
    assert!(
        scratch
            .try_git(&top, &["cat-file", "-t", &tips[1]])
            .is_none(),
        "Git keeps the space"
    );
    refused("space", "is not a folder");

    set("a", &format!("{}\n", objects("b").display()));
    set("b", "../../../c/.git/objects\n");
    assert_eq!(
        scratch.git(&top, &["cat-file", "-t", &tips[2]]).trim(),
        "commit"
    );
    refused("nested relative", "nested");

    set("b", &format!("{}\n", objects("a").display()));
    assert_eq!(
        scratch.git(&top, &["cat-file", "-t", &tips[1]]).trim(),
        "commit"
    );
    refused("cycle", "appears twice");
    fs::remove_file(objects("b").join("info/alternates")).unwrap();
    set("a", &format!("{0}\n{0}\n", objects("b").display()));
    refused("repeat", "appears twice");
    set("a", &format!("{}\n", objects("a").display()));
    refused("itself", "appears twice");
}

/// A FIFO at the path of a loose object is refused within the time limit, before gix opens it.
#[test]
fn special_loose_objects_never_block_a_read() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let tip = scratch.commit(&repo, "a.txt", "one");
    let loose = repo.join(".git/objects").join(&tip[..2]).join(&tip[2..]);
    replace(&loose, mkfifo);
    let tip = id(ObjectFormat::Sha1, &tip);
    let result = within_seconds(move || {
        let reader = GitRepository::open(&repo, limits()).unwrap();
        (reader.object_kind(&tip), reader.read_commit(&tip))
    });
    assert!(
        matches!(result.0, Err(GitReadError::NotAFile { kind: "FIFO", .. })),
        "{:?}",
        result.0
    );
    assert!(
        matches!(result.1, Err(GitReadError::NotAFile { kind: "FIFO", .. })),
        "{:?}",
        result.1
    );
}

/// A FIFO at the path of a loose object in an alternate is refused too, within the time limit.
#[test]
fn special_loose_objects_in_alternates_never_block_a_read() {
    let scratch = Scratch::new();
    let shared = scratch.init("shared", &[]);
    let tip = scratch.commit(&shared, "a.txt", "one");
    scratch.git(
        scratch.path("").as_path(),
        &["clone", "-q", "--shared", "shared", "clone"],
    );
    let clone = scratch.path("clone");
    assert!(
        !clone
            .join(".git/objects")
            .join(&tip[..2])
            .join(&tip[2..])
            .exists()
    );
    let loose = shared.join(".git/objects").join(&tip[..2]).join(&tip[2..]);
    replace(&loose, mkfifo);
    let tip = id(ObjectFormat::Sha1, &tip);
    let result = within_seconds(move || {
        let reader = GitRepository::open(&clone, limits()).unwrap();
        (reader.object_kind(&tip), reader.read_commit(&tip))
    });
    assert!(
        matches!(result.0, Err(GitReadError::NotAFile { kind: "FIFO", .. })),
        "{:?}",
        result.0
    );
    assert!(
        matches!(result.1, Err(GitReadError::NotAFile { kind: "FIFO", .. })),
        "{:?}",
        result.1
    );
}

/// Loose refs and `HEAD` parse as Git parses them. The target after `ref:` loses the whitespace
/// around it, and the end of the file or whitespace must follow an ID.
#[test]
fn loose_ref_contents_parse_as_in_git() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let tip = scratch.commit(&repo, "a.txt", "one");
    let git = repo.join(".git");
    let tip_id = id(ObjectFormat::Sha1, &tip);
    let branch = |text: &str| {
        fs::write(git.join("refs/heads/probe"), text).unwrap();
        let ours = GitRepository::open(&repo, limits())
            .unwrap()
            .branch_tip("probe");
        let theirs = scratch.try_git(&repo, &["rev-parse", "--verify", "-q", "refs/heads/probe"]);
        (ours, theirs)
    };
    for text in [
        format!("{tip}junk\n"),
        tip[..39].to_string(),
        format!("{tip}\x0c"),
        "ref:\x0crefs/heads/main".to_string(),
    ] {
        let (ours, theirs) = branch(&text);
        assert_eq!(theirs, None, "Git calls {text:?} broken");
        assert!(
            matches!(ours, Err(GitReadError::Ref { .. })),
            "{text:?}: {ours:?}"
        );
    }
    for text in [
        "ref: refs/heads/main  \n".to_string(),
        "ref:refs/heads/main\n".to_string(),
        "ref:\trefs/heads/main \n".to_string(),
        format!("{tip} trailing data\n"),
        tip.clone(),
    ] {
        let (ours, theirs) = branch(&text);
        assert_eq!(
            theirs.as_deref().map(str::trim),
            Some(tip.as_str()),
            "Git reads {text:?}"
        );
        assert_eq!(ours, Ok(Some(tip_id.clone())), "{text:?}");
    }
    let head = |text: &str| {
        fs::write(git.join("HEAD"), text).unwrap();
        let theirs = scratch.try_git(&repo, &["rev-parse", "HEAD"]);
        let ours = GitRepository::open(&repo, limits()).and_then(|reader| reader.head());
        (ours, theirs)
    };
    for text in [
        format!("{tip}zz\n"),
        "ref:\x0crefs/heads/main\n".to_string(),
    ] {
        let (ours, theirs) = head(&text);
        assert_eq!(theirs, None, "Git calls the HEAD {text:?} broken");
        assert!(ours.is_err(), "{text:?}: {ours:?}");
    }
    for text in ["ref:refs/heads/main\n", "ref: refs/heads/main \n"] {
        let (ours, theirs) = head(text);
        assert_eq!(
            theirs.as_deref().map(str::trim),
            Some(tip.as_str()),
            "Git reads {text:?}"
        );
        assert_eq!(
            ours,
            Ok(Head::Branch {
                name: b"refs/heads/main".to_vec(),
                tip: Some(tip_id.clone())
            }),
            "{text:?}"
        );
    }
}

/// A folder without `.git` whose `HEAD` is a folder is not a worktree, not a bad entry.
#[test]
fn folder_with_a_head_folder_is_not_a_worktree() {
    let scratch = Scratch::new();
    let folder = scratch.path("plain");
    fs::create_dir_all(folder.join("HEAD")).unwrap();
    let error = GitRepository::open(&folder, limits()).unwrap_err();
    assert!(
        matches!(error, GitReadError::NotAWorktree { .. }),
        "{error:?}"
    );
}

/// A bare SHA-256 repository with a detached `HEAD` is bare, not a plain folder, and `commondir`
/// loses only its line end, as in Git.
#[test]
fn bare_sha256_and_commondir_spaces_are_read_as_git_reads_them() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &["--object-format=sha256"]);
    let tip = scratch.commit(&repo, "a.txt", "one");
    scratch.git(
        scratch.path("").as_path(),
        &["clone", "-q", "--bare", "repo", "bare.git"],
    );
    let bare = scratch.path("bare.git");
    fs::write(bare.join("HEAD"), format!("{tip}\n")).unwrap();
    assert_eq!(
        scratch
            .git(&bare, &["rev-parse", "--is-bare-repository"])
            .trim(),
        "true"
    );
    let error = GitRepository::open(&bare, limits()).unwrap_err();
    assert!(matches!(error, GitReadError::Bare { .. }), "{error:?}");

    scratch.git(
        &repo,
        &["worktree", "add", "-q", "-b", "linked", "../linked"],
    );
    let linked = scratch.path("linked");
    let commondir = repo.join(".git/worktrees/linked/commondir");
    fs::write(&commondir, "../.. \r\n").unwrap();
    assert!(
        scratch.try_git(&linked, &["rev-parse", "HEAD"]).is_none(),
        "Git keeps the space"
    );
    let error = GitRepository::open(&linked, limits()).unwrap_err();
    assert!(
        matches!(error, GitReadError::NotARepository { .. }),
        "{error:?}"
    );
    fs::write(&commondir, "../..\r\n").unwrap();
    assert_eq!(scratch.rev(&linked, "HEAD"), tip);
    assert!(
        GitRepository::open(&linked, limits())
            .unwrap()
            .is_linked_worktree()
    );
}

/// A ref private to a worktree is read from the Git directory of the worktree and never from
/// the common directory, and a ref that names another worktree is refused.
#[test]
fn worktree_private_refs_are_read_from_the_worktree() {
    let scratch = Scratch::new();
    let main = scratch.init("main", &[]);
    let first = scratch.commit(&main, "a.txt", "one");
    let second = scratch.commit(&main, "a.txt", "two");
    scratch.git(
        &main,
        &["worktree", "add", "-q", "-b", "linked", "../linked"],
    );
    let linked = scratch.path("linked");
    scratch.git(&linked, &["update-ref", "refs/worktree/x", &first]);
    scratch.git(&main, &["update-ref", "refs/worktree/x", &second]);
    scratch.git(
        &main,
        &["symbolic-ref", "refs/heads/alias", "refs/worktree/x"],
    );
    assert_eq!(scratch.rev(&linked, "refs/heads/alias"), first);
    assert_eq!(scratch.rev(&main, "refs/heads/alias"), second);
    let tip = |location: &Path| {
        GitRepository::open(location, limits())
            .unwrap()
            .branch_tip("alias")
    };
    assert_eq!(tip(&linked), Ok(Some(id(ObjectFormat::Sha1, &first))));
    assert_eq!(tip(&main), Ok(Some(id(ObjectFormat::Sha1, &second))));

    // Without its loose file, the private ref falls back to the shared `packed-refs`, as in Git,
    // and the loose file of the main worktree is never read.
    let git = main.join(".git");
    fs::remove_file(git.join("worktrees/linked/refs/worktree/x")).unwrap();
    fs::write(
        git.join("packed-refs"),
        format!("{second} refs/heads/zzz\n{first} refs/worktree/x\n"),
    )
    .unwrap();
    assert_eq!(scratch.rev(&linked, "refs/heads/alias"), first);
    assert_eq!(tip(&linked), Ok(Some(id(ObjectFormat::Sha1, &first))));

    fs::write(git.join("refs/heads/absent"), "ref: refs/worktree/absent\n").unwrap();
    assert!(
        scratch
            .try_git(
                &linked,
                &["rev-parse", "--verify", "-q", "refs/heads/absent"]
            )
            .is_none()
    );
    assert_eq!(
        GitRepository::open(&linked, limits())
            .unwrap()
            .branch_tip("absent"),
        Ok(None)
    );
    // A name with no category lives in the common directory, as in Git.
    fs::write(git.join("refs/heads/plain"), "ref: foo/bar\n").unwrap();
    fs::create_dir_all(git.join("foo")).unwrap();
    fs::create_dir_all(git.join("worktrees/linked/foo")).unwrap();
    fs::write(git.join("foo/bar"), format!("{first}\n")).unwrap();
    fs::write(git.join("worktrees/linked/foo/bar"), format!("{second}\n")).unwrap();
    assert_eq!(scratch.rev(&linked, "refs/heads/plain"), first);
    assert_eq!(
        GitRepository::open(&linked, limits())
            .unwrap()
            .branch_tip("plain"),
        Ok(Some(id(ObjectFormat::Sha1, &first)))
    );
    fs::write(
        git.join("refs/heads/other"),
        "ref: main-worktree/refs/heads/main\n",
    )
    .unwrap();
    let error = GitRepository::open(&linked, limits())
        .unwrap()
        .branch_tip("other");
    assert!(matches!(error, Err(GitReadError::Ref { .. })), "{error:?}");
}

/// A loose ref that is a symbolic link to `refs/...` is a symbolic ref, and a link to a regular
/// file elsewhere is followed, as Git reads them.
#[test]
fn linked_loose_refs_are_read_as_git_reads_them() {
    use std::os::unix::fs::symlink;
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let tip = scratch.commit(&repo, "a.txt", "one");
    let heads = repo.join(".git/refs/heads");
    symlink("refs/heads/main", heads.join("alias")).unwrap();
    let outside = scratch.path("outside-ref");
    fs::write(&outside, format!("{tip}\n")).unwrap();
    symlink(&outside, heads.join("out")).unwrap();
    let reader = GitRepository::open(&repo, limits()).unwrap();
    for name in ["alias", "out"] {
        assert_eq!(
            scratch.rev(&repo, &format!("refs/heads/{name}")),
            tip,
            "Git reads {name}"
        );
        assert_eq!(
            reader.branch_tip(name),
            Ok(Some(id(ObjectFormat::Sha1, &tip))),
            "{name}"
        );
    }
}

/// A change of `alternates` after the open, such as a FIFO or a new alternate, is refused before
/// gix reads the file again, within the time limit.
#[test]
fn alternates_changed_after_the_open_are_refused() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let tip = scratch.commit(&repo, "a.txt", "one");
    let other = scratch.init("other", &[]);
    let missing = scratch.commit(&other, "b.txt", "two");
    let alternates = repo.join(".git/objects/info/alternates");
    let (tip, missing) = (
        id(ObjectFormat::Sha1, &tip),
        id(ObjectFormat::Sha1, &missing),
    );
    let opened = repo.clone();
    let (fifo, keep) = (alternates.clone(), (tip.clone(), missing.clone()));
    let results = within_seconds(move || {
        let reader = GitRepository::open(&opened, limits()).unwrap();
        mkfifo(&fifo);
        (reader.read_commit(&keep.0), reader.object_kind(&keep.1))
    });
    for result in [results.0.map(|_| ()), results.1.map(|_| ())] {
        assert!(
            matches!(result, Err(GitReadError::NotAFile { kind: "FIFO", .. })),
            "{result:?}"
        );
    }
    fs::remove_file(&alternates).unwrap();
    let reader = GitRepository::open(&repo, limits()).unwrap();
    reader.read_commit(&tip).unwrap();
    let path = fs::canonicalize(other.join(".git/objects")).unwrap();
    fs::write(&alternates, format!("{}\n", path.display())).unwrap();
    let error = reader.read_commit(&missing).unwrap_err();
    assert!(
        error.to_string().contains("changed after the open"),
        "{error}"
    );
    assert!(matches!(
        reader.object_kind(&tip),
        Err(GitReadError::Alternate { .. })
    ));
}

/// A symbolic ref to a name outside `refs/` is not looked up below `refs/heads`, and a ref path
/// that crosses a file is a missing ref, as in Git.
#[test]
fn unusual_ref_names_resolve_as_in_git() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let tip = scratch.commit(&repo, "a.txt", "one");
    scratch.git(&repo, &["branch", "FOO"]);
    scratch.git(&repo, &["pack-refs", "--all"]);
    let git = repo.join(".git");
    fs::write(git.join("refs/heads/sym"), "ref: FOO\n").unwrap();
    assert!(
        scratch
            .try_git(&repo, &["rev-parse", "--verify", "-q", "refs/heads/sym"])
            .is_none()
    );
    let reader = GitRepository::open(&repo, limits()).unwrap();
    assert_eq!(reader.branch_tip("sym"), Ok(None));
    assert_eq!(
        reader.branch_tip("FOO"),
        Ok(Some(id(ObjectFormat::Sha1, &tip)))
    );

    fs::write(git.join("refs/heads/main"), format!("{tip}\n")).unwrap();
    fs::write(git.join("HEAD"), "ref: refs/heads/main/sub\n").unwrap();
    assert!(
        scratch
            .try_git(&repo, &["rev-parse", "--verify", "-q", "HEAD"])
            .is_none()
    );
    assert!(
        scratch
            .git(&repo, &["status", "-sb"])
            .contains("No commits yet on main/sub")
    );
    assert_eq!(
        reader.head(),
        Ok(Head::Branch {
            name: b"refs/heads/main/sub".to_vec(),
            tip: None
        })
    );
    assert_eq!(reader.branch_tip("main/sub"), Ok(None));
}

/// A file at the fan-out folder of a loose object does not stop the read of a packed object.
#[test]
fn file_at_a_loose_folder_does_not_stop_a_packed_read() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let tip = scratch.commit(&repo, "a.txt", "one");
    scratch.git(&repo, &["repack", "-q", "-a", "-d"]);
    scratch.git(&repo, &["prune-packed"]);
    let fanout = repo.join(".git/objects").join(&tip[..2]);
    if fanout.exists() {
        fs::remove_dir_all(&fanout).unwrap();
    }
    fs::write(&fanout, "not a folder").unwrap();
    assert_eq!(
        scratch.git(&repo, &["cat-file", "-t", &tip]).trim(),
        "commit"
    );
    let reader = GitRepository::open(&repo, limits()).unwrap();
    reader.read_commit(&id(ObjectFormat::Sha1, &tip)).unwrap();
}

/// A commit reads as Git's `parse_commit_buffer` reads it, from the tree and parent lines,
/// whatever follows them.
#[test]
fn commit_parents_parse_as_in_git() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let parent = scratch.commit(&repo, "a.txt", "one");
    let tree = scratch.rev(&repo, "HEAD^{tree}");
    let reader = GitRepository::open(&repo, limits()).unwrap();
    let bodies = [
        format!("tree {tree}\nparent {parent}\ncommitter a <a@b> 1 +0000\n\nmsg\n"),
        format!("tree {tree}\nparent {parent}\nauthor garbage\ncommitter a <a@b> 1 +0000\n\nm\n"),
        format!(
            "tree {tree}\nparent {}\ncommitter a <a@b> 1 +0000\n\nm\n",
            parent.to_uppercase()
        ),
    ];
    for (index, body) in bodies.iter().enumerate() {
        let file = scratch.path(&format!("body{index}"));
        fs::write(&file, body).unwrap();
        let file = file.to_str().unwrap();
        let hex = scratch.git(
            &repo,
            &["hash-object", "-t", "commit", "--literally", "-w", file],
        );
        let hex = hex.trim();
        let git = scratch.git(&repo, &["rev-list", "--parents", "-n", "1", hex]);
        assert_eq!(
            git.trim(),
            format!("{hex} {parent}"),
            "Git reads body {index}"
        );
        assert_eq!(
            reader_parents(&reader, hex),
            vec![parent.clone()],
            "body {index}"
        );
    }
    for (name, body) in [
        ("short", format!("tree {tree}\nparent {}\n", &parent[..20])),
        ("tree end", format!("tree {tree}\n")),
        ("parent end", format!("tree {tree}\nparent {parent}\n")),
    ] {
        let file = scratch.path(name);
        fs::write(&file, body).unwrap();
        let file = file.to_str().unwrap();
        let hex = scratch.git(
            &repo,
            &["hash-object", "-t", "commit", "--literally", "-w", file],
        );
        let hex = hex.trim();
        // Git refuses a body that ends after its tree or parent lines, and it drops a short
        // parent line at the end without an error. The reader refuses all three, so it never
        // reports a commit without a parent that it has.
        let git = scratch.try_git(&repo, &["rev-list", "--parents", "-n", "1", hex]);
        match name {
            "short" => assert_eq!(git.as_deref().map(str::trim), Some(hex), "{name}"),
            _ => assert!(git.is_none(), "{name}"),
        }
        let error = reader
            .read_commit(&id(ObjectFormat::Sha1, hex))
            .unwrap_err();
        assert!(
            matches!(error, GitReadError::MalformedCommit { .. }),
            "{name}: {error:?}"
        );
    }
}

/// A loose ref that is a link to a folder is missing, with no look in `packed-refs`, as in Git,
/// while a real folder at the path of a ref falls back to `packed-refs`.
#[test]
fn loose_ref_links_to_folders_are_missing() {
    use std::os::unix::fs::symlink;
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let tip = scratch.commit(&repo, "a.txt", "one");
    let git = repo.join(".git");
    let packed = ["p", "q", "r"]
        .map(|name| format!("{tip} refs/heads/{name}\n"))
        .concat();
    fs::write(
        git.join("packed-refs"),
        format!("# pack-refs with: peeled fully-peeled sorted \n{packed}"),
    )
    .unwrap();
    fs::create_dir(git.join("dirp")).unwrap();
    symlink(git.join("dirp"), git.join("refs/heads/p")).unwrap();
    symlink(scratch.path(""), git.join("refs/heads/q")).unwrap();
    fs::create_dir_all(git.join("refs/heads/r/inner")).unwrap();
    let reader = GitRepository::open(&repo, limits()).unwrap();
    for (name, found) in [("p", false), ("q", false), ("r", true)] {
        let git_tip = scratch.try_git(
            &repo,
            &["rev-parse", "--verify", "-q", &format!("refs/heads/{name}")],
        );
        assert_eq!(git_tip.is_some(), found, "Git reads {name}");
        let expected = found.then(|| id(ObjectFormat::Sha1, &tip));
        assert_eq!(reader.branch_tip(name), Ok(expected), "{name}");
    }
}

/// A FIFO at a pack that `multi-pack-index` names does not block a read, because the reader
/// turns the index off and gix reads only regular pack files.
#[test]
fn multi_pack_index_never_blocks_a_read() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let tip = scratch.commit(&repo, "a.txt", "one");
    scratch.git(&repo, &["repack", "-q", "-a", "-d"]);
    scratch.git(&repo, &["prune-packed"]);
    scratch.git(&repo, &["multi-pack-index", "write"]);
    let packs = repo.join(".git/objects/pack");
    assert!(packs.join("multi-pack-index").is_file());
    GitRepository::open(&repo, limits())
        .unwrap()
        .read_commit(&id(ObjectFormat::Sha1, &tip))
        .unwrap();
    let pack = fs::read_dir(&packs)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "pack"))
        .unwrap();
    replace(&pack, mkfifo);
    assert!(
        scratch.try_git(&repo, &["cat-file", "-t", &tip]).is_none(),
        "Git refuses it"
    );
    let tip = id(ObjectFormat::Sha1, &tip);
    let result = within_seconds(move || {
        GitRepository::open(&repo, limits()).and_then(|reader| reader.read_commit(&tip))
    });
    assert!(result.is_err(), "{result:?}");
}

/// A `.git` file whose path ends in whitespace is refused, because gix would strip the space and
/// open another folder than Git does. A `.git` file that names a file is not a repository.
#[test]
fn gitdir_paths_are_read_as_git_reads_them() {
    let scratch = Scratch::new();
    let separate = scratch.path("gd ");
    scratch.git(
        scratch.path("").as_path(),
        &[
            "init",
            "-q",
            "-b",
            "main",
            "--separate-git-dir",
            separate.to_str().unwrap(),
            "sep",
        ],
    );
    let sep = scratch.path("sep");
    scratch.commit(&sep, "a.txt", "one");
    let decoy = scratch.init("gd", &[]);
    scratch.commit(&decoy, "b.txt", "two");
    fs::rename(decoy.join(".git"), scratch.path("decoy-git")).unwrap();
    fs::remove_dir_all(&decoy).unwrap();
    fs::rename(scratch.path("decoy-git"), &decoy).unwrap();
    let git = scratch.git(&sep, &["rev-parse", "--absolute-git-dir"]);
    assert!(
        git.trim_end_matches('\n').ends_with("gd "),
        "Git keeps the space: {git:?}"
    );
    let error = GitRepository::open(&sep, limits()).unwrap_err();
    assert!(
        matches!(error, GitReadError::NotARepository { .. }),
        "{error:?}"
    );
    assert!(
        error.to_string().contains("whitespace") || format!("{error:?}").contains("whitespace")
    );

    let file = scratch.path("plain-file");
    fs::write(&file, "x").unwrap();
    fs::write(sep.join(".git"), format!("gitdir: {}\n", file.display())).unwrap();
    let error = GitRepository::open(&sep, limits()).unwrap_err();
    assert!(
        matches!(error, GitReadError::NotARepository { .. }),
        "{error:?}"
    );
}

/// `HEAD` through a symbolic branch names the last branch of the chain, as Git does, and the
/// commit limit holds for packed and deltified commits too.
#[test]
fn head_names_the_branch_at_the_end_of_the_chain() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let tip = scratch.commit(&repo, "a.txt", "one");
    scratch.git(
        &repo,
        &["symbolic-ref", "refs/heads/alias", "refs/heads/main"],
    );
    scratch.git(&repo, &["switch", "-q", "alias"]);
    assert_eq!(
        fs::read_to_string(repo.join(".git/HEAD")).unwrap(),
        "ref: refs/heads/alias\n"
    );
    assert_eq!(
        scratch.git(&repo, &["symbolic-ref", "HEAD"]).trim(),
        "refs/heads/main"
    );
    let reader = GitRepository::open(&repo, limits()).unwrap();
    assert_eq!(
        reader.head(),
        Ok(Head::Branch {
            name: b"refs/heads/main".to_vec(),
            tip: Some(id(ObjectFormat::Sha1, &tip))
        })
    );
    // A chain that ends at a missing branch names that branch, with no tip.
    scratch.git(
        &repo,
        &["symbolic-ref", "refs/heads/alias", "refs/heads/unborn"],
    );
    assert_eq!(
        scratch.git(&repo, &["symbolic-ref", "HEAD"]).trim(),
        "refs/heads/unborn"
    );
    assert_eq!(
        reader.head(),
        Ok(Head::Branch {
            name: b"refs/heads/unborn".to_vec(),
            tip: None
        })
    );
}

/// `commit_bytes` bounds a whole packed commit and a deltified one, by the size of the commit.
#[test]
fn commit_limit_bounds_packed_and_deltified_commits() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let long = "a long message that makes similar commits deltify in the pack\n".repeat(60);
    for round in 0..6 {
        fs::write(repo.join("a.txt"), format!("round {round}")).unwrap();
        scratch.git(&repo, &["add", "a.txt"]);
        scratch.git(
            &repo,
            &["commit", "-q", "-m", &format!("{long}round {round}")],
        );
    }
    scratch.git(
        &repo,
        &[
            "repack",
            "-q",
            "-a",
            "-d",
            "-f",
            "--depth=50",
            "--window=50",
        ],
    );
    scratch.git(&repo, &["prune-packed"]);
    let deltas = deltified_commits(&scratch, &repo);
    let whole = scratch
        .all_commits(&repo)
        .into_iter()
        .find(|hex| !deltas.contains(hex))
        .expect("one commit is stored whole");
    let delta = deltas
        .first()
        .expect("one commit is stored as a delta")
        .clone();
    for hex in [whole, delta] {
        let size: u64 = scratch
            .git(&repo, &["cat-file", "-s", &hex])
            .trim()
            .parse()
            .unwrap();
        let commit = id(ObjectFormat::Sha1, &hex);
        let reader = GitRepository::open(
            &repo,
            Limits {
                commit_bytes: size,
                ..limits()
            },
        )
        .unwrap();
        reader.read_commit(&commit).unwrap();
        let reader = GitRepository::open(
            &repo,
            Limits {
                commit_bytes: size - 1,
                ..limits()
            },
        )
        .unwrap();
        let error = reader.read_commit(&commit).unwrap_err();
        assert!(
            matches!(error, GitReadError::TooLarge { .. }),
            "{hex}: {error:?}"
        );
    }
}

/// `packed-refs` parses as Git parses it. A carriage return stays in the name, so a file with
/// CRLF line ends finds no branch. A bad name in a file with no header is skipped. A last line
/// with no line feed, or a line that is not a record, is refused, as Git refuses it.
#[test]
fn packed_refs_parse_as_in_git() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let tip = scratch.commit(&repo, "a.txt", "one");
    scratch.git(&repo, &["branch", "x"]);
    scratch.git(&repo, &["pack-refs", "--all"]);
    let packed = repo.join(".git/packed-refs");
    let found = |name: &str| {
        scratch
            .try_git(
                &repo,
                &["rev-parse", "--verify", "-q", &format!("refs/heads/{name}")],
            )
            .map(|hex| id(ObjectFormat::Sha1, hex.trim()))
    };
    let tip = id(ObjectFormat::Sha1, &tip);
    let crlf = fs::read_to_string(&packed).unwrap().replace('\n', "\r\n");
    fs::write(&packed, crlf).unwrap();
    let reader = GitRepository::open(&repo, limits()).unwrap();
    assert_eq!(found("x"), None);
    assert_eq!(reader.branch_tip("x"), Ok(None));
    assert_eq!(
        reader.head(),
        Ok(Head::Branch {
            name: b"refs/heads/main".to_vec(),
            tip: None
        })
    );
    let hex = tip.to_string();
    fs::write(
        &packed,
        format!("{hex} refs/heads/x\n{hex} refs/heads/a..b\n^{hex}\n{hex} refs/heads/main\n"),
    )
    .unwrap();
    assert_eq!(found("main"), Some(tip.clone()));
    assert_eq!(reader.branch_tip("main"), Ok(Some(tip.clone())));
    assert_eq!(reader.branch_tip("x"), Ok(Some(tip.clone())));
    assert_eq!(reader.branch_tip("y"), Ok(None));
    for bad in [
        format!("{hex} refs/heads/main"),
        format!("{hex} refs/heads/main\nhello\n"),
    ] {
        fs::write(&packed, bad).unwrap();
        assert_eq!(found("main"), None);
        assert!(matches!(
            reader.branch_tip("main"),
            Err(GitReadError::Ref { .. })
        ));
    }
}

/// The open works when the current folder of the process has been removed, because the
/// reader gives gix the folder to resolve against. A child process shows it, because the
/// current folder belongs to the whole process.
#[test]
fn open_works_in_a_removed_current_folder() {
    if let Some(repo) = std::env::var_os("GOSSIL_GIT_READER_GONE_CHILD") {
        let gone = PathBuf::from(std::env::var_os("GOSSIL_GIT_READER_GONE_DIR").unwrap());
        std::env::set_current_dir(&gone).unwrap();
        fs::remove_dir(&gone).unwrap();
        assert!(std::env::current_dir().is_err());
        let reader = GitRepository::open(Path::new(&repo), limits()).unwrap();
        let tip = std::env::var("GOSSIL_GIT_READER_GONE_TIP").unwrap();
        assert_eq!(
            reader.head(),
            Ok(Head::Branch {
                name: b"refs/heads/main".to_vec(),
                tip: Some(id(ObjectFormat::Sha1, &tip))
            })
        );
        return;
    }
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    let tip = scratch.commit(&repo, "a.txt", "one");
    let gone = scratch.path("gone");
    fs::create_dir(&gone).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "open_works_in_a_removed_current_folder",
            "--nocapture",
        ])
        .env("GOSSIL_GIT_READER_GONE_CHILD", &repo)
        .env("GOSSIL_GIT_READER_GONE_DIR", &gone)
        .env("GOSSIL_GIT_READER_GONE_TIP", &tip)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "the child fails: {stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A linked worktree whose `commondir` holds only a line end names no common directory.
#[test]
fn commondir_of_only_a_line_end_is_refused() {
    let scratch = Scratch::new();
    let main = scratch.init("main", &[]);
    scratch.commit(&main, "a.txt", "one");
    scratch.git(
        &main,
        &["worktree", "add", "-q", "-b", "feature", "../linked"],
    );
    fs::write(main.join(".git/worktrees/linked/commondir"), "\n").unwrap();
    let error = GitRepository::open(&scratch.path("linked"), limits()).unwrap_err();
    assert!(
        matches!(error, GitReadError::NotARepository { .. })
            && error.to_string().contains("commondir is empty"),
        "{error:?}"
    );
}

/// A Git directory with `HEAD` but without its `objects` or `refs` folder is not a repository.
#[test]
fn git_directory_without_objects_or_refs_is_refused() {
    let scratch = Scratch::new();
    for folder in ["objects", "refs"] {
        let repo = scratch.init(folder, &[]);
        fs::rename(repo.join(".git").join(folder), repo.join("moved")).unwrap();
        let error = GitRepository::open(&repo, limits()).unwrap_err();
        assert!(
            matches!(error, GitReadError::NotARepository { .. })
                && error.to_string().contains(&format!("no folder {folder}")),
            "{folder}: {error:?}"
        );
    }
}

/// `head` reads `HEAD` again, so a `HEAD` removed after the open is a ref error.
#[test]
fn head_removed_after_the_open_is_a_ref_error() {
    let scratch = Scratch::new();
    let repo = scratch.init("repo", &[]);
    scratch.commit(&repo, "a.txt", "one");
    let reader = GitRepository::open(&repo, limits()).unwrap();
    fs::remove_file(repo.join(".git/HEAD")).unwrap();
    let error = reader.head().unwrap_err();
    assert!(
        matches!(&error, GitReadError::Ref { name, .. } if name == b"HEAD")
            && error.to_string().contains("HEAD does not exist"),
        "{error:?}"
    );
}

/// A folder without `.git` whose `HEAD` cannot be parsed in either format is not a worktree,
/// even with `objects` and `refs` folders.
#[test]
fn folder_with_a_garbage_head_is_not_a_worktree() {
    let scratch = Scratch::new();
    let folder = scratch.path("plain");
    fs::create_dir_all(folder.join("objects")).unwrap();
    fs::create_dir_all(folder.join("refs")).unwrap();
    fs::write(folder.join("HEAD"), "not a ref\n").unwrap();
    let error = GitRepository::open(&folder, limits()).unwrap_err();
    assert!(
        matches!(error, GitReadError::NotAWorktree { .. }),
        "{error:?}"
    );
}
