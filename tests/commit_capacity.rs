//! Check that `gos v commit` and the commands around it take checkouts of many files.
//!
//! Before the fix, one ledger bounded the whole command. Each file charged about
//! 300 KB of estimated traversal work, read and store work, and a commit also read
//! back every file of its parent and grandparent. Under the CLI ceilings, with a
//! root 11 directories deep, `add` of one batch refused 73 small files and
//! `commit` refused 71 small files, 62 files of 10 KB or 6 files of 1 MB, each with
//! "total decoded content limit". Now the ceilings bound each file, each artifact
//! and each row. Each command reads each file once and holds the bytes of one file
//! at a time.
//!
//! The ordinary tests run the CLI in a debug build. They check these cases:
//!
//! - 300 small files, past every old ceiling.
//! - 3 files of 1 MiB and one byte, past the old file ceiling.
//! - The refusal of a sparse file one byte over the ceiling of 2,147,418,109 bytes,
//!   in `add` and in `commit`.
//! - The warning above 20,000,000 bytes.
//! - A commit that waits behind a held write lock.
//!
//! The ignored tests make the capacity targets in a release build
//! (`just commit-capacity`), and give a check-in of 1,000 files to the pinned stock
//! Fossil (`just commit-oracle`). `tests/limits_capacity.rs` compares Gossil with
//! Fossil and Git (`just limits-capacity`).
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, Instant};

/// The largest file that `add` reads and a commit stores, the largest file that stock Fossil reads.
const CEILING: u64 = 2_147_418_109;
/// The largest file before this change, 1 MiB.
const OLD_CEILING: usize = 1 << 20;
/// The paths that one `add` or `scope` names in the capacity test.
const BATCH: usize = 1000;

/// Run `gos` in the fixture home, with the checkout root as working directory.
/// # Arguments
/// * `home` - The fixture home.
/// * `args` - The arguments after the executable.
/// # Returns
/// The finished process output.
fn run(home: &Path, args: &[&str]) -> Output {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_gos"));
    for (key, _) in std::env::vars_os() {
        if key.to_str().is_some_and(|key| key.starts_with("GEST_")) {
            command.env_remove(key);
        }
    }
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("XDG_DATA_HOME", home)
        .env("GOSSIL_USER", "tester")
        .current_dir(home.join("root"))
        .args(args)
        .output()
        .unwrap()
}

/// Run one verb of `gos v` on the fixture repository and checkout `work`.
/// # Arguments
/// * `home` - The fixture home.
/// * `verb` - The verb and its own flags.
/// # Returns
/// The finished process output.
fn vcs(home: &Path, verb: &[&str]) -> Output {
    let repo = home.join("repo");
    let mut args = vec![
        "v",
        verb[0],
        "--repository",
        repo.to_str().unwrap(),
        "--checkout",
        "work",
    ];
    args.extend_from_slice(&verb[1..]);
    run(home, &args)
}

/// Require a command to succeed with an empty stderr.
/// # Arguments
/// * `output` - The finished process output.
/// # Returns
/// Standard output.
fn ok(output: Output) -> String {
    assert!(
        output.status.success(),
        "status {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout).unwrap()
}

/// Give the name of file `n`, in directories of 100 files.
/// # Arguments
/// * `n` - The number of the file.
/// # Returns
/// A relative path such as `d001/f00123.txt`.
fn name(n: usize) -> String {
    format!("d{:03}/f{n:05}.txt", n / 100)
}

/// Give `size` bytes of content that differ for each file.
/// # Arguments
/// * `n` - The number of the file.
/// * `size` - The length of the content.
/// # Returns
/// The content.
fn content(n: usize, size: usize) -> Vec<u8> {
    format!("file {n} ").bytes().cycle().take(size).collect()
}

/// Make a repository, open the checkout `work`, and write `files` files of `size` bytes.
/// # Arguments
/// * `home` - An empty fixture home.
/// * `files` - The count of files.
/// * `size` - The size of each file.
/// # Returns
/// The names of the files.
fn fixture(home: &Path, files: usize, size: usize) -> Vec<String> {
    let root = home.join("root");
    std::fs::create_dir(&root).unwrap();
    let repo = home.join("repo");
    ok(run(
        home,
        &[
            "v",
            "init",
            "--date-override",
            "2026-11-20T00:00:00",
            repo.to_str().unwrap(),
        ],
    ));
    let root = root.canonicalize().unwrap();
    ok(vcs(home, &["open", "--root", root.to_str().unwrap()]));
    let names: Vec<String> = (0..files).map(name).collect();
    for (n, name) in names.iter().enumerate() {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content(n, size)).unwrap();
    }
    names
}

/// Track `names` and select the private scope for them, in batches of `batch`.
/// # Arguments
/// * `home` - The fixture home.
/// * `names` - The files.
/// * `batch` - The paths that one command names.
fn track(home: &Path, names: &[String], batch: usize) {
    for batch in names.chunks(batch) {
        let mut add = vec!["add"];
        let mut scope = vec!["scope", "--scope", "private"];
        for name in batch {
            add.extend(["--path", name]);
            scope.extend(["--path", name]);
        }
        ok(vcs(home, &add));
        ok(vcs(home, &scope));
    }
}

/// Run `gos v commit` of every tracked file.
/// # Arguments
/// * `home` - The fixture home.
/// * `day` - The day of November 2026 of the date override.
/// # Returns
/// The finished process output.
fn commit(home: &Path, day: u32) -> Output {
    let date = format!("2026-11-{day:02}T12:00:00");
    vcs(
        home,
        &[
            "commit",
            "--scope",
            "private",
            "--all-tracked",
            "--user",
            "tester",
            "--comment",
            "save",
            "--date-override",
            &date,
        ],
    )
}

/// Run `gos v verify` on the fixture repository.
/// # Arguments
/// * `home` - The fixture home.
/// # Returns
/// Standard output of a successful proof.
fn verify(home: &Path) -> String {
    let repo = home.join("repo");
    ok(run(
        home,
        &["v", "verify", "--repository", repo.to_str().unwrap()],
    ))
}

/// One file below the fixture home: its bytes, or for a file over 1 MiB, such as a
/// sparse file at the ceiling, its size, modification time and inode.
#[derive(Debug, PartialEq, Eq)]
enum Seen {
    /// The bytes of a small file.
    Bytes(Vec<u8>),
    /// The size, modification time and inode of a large file.
    Facts(u64, std::time::SystemTime, u64),
}

/// Read every file below the fixture home: the repository, its sidecars, the locator
/// and the checkout.
/// # Arguments
/// * `home` - The fixture home.
/// # Returns
/// What was seen of each file, by path.
fn every_byte(home: &Path) -> BTreeMap<PathBuf, Seen> {
    use std::os::unix::fs::MetadataExt;
    let mut all = BTreeMap::new();
    let mut pending = vec![home.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = std::fs::metadata(&path).unwrap();
            if meta.is_dir() {
                pending.push(path);
            } else if meta.len() > OLD_CEILING as u64 {
                let facts = Seen::Facts(meta.len(), meta.modified().unwrap(), meta.ino());
                all.insert(path, facts);
            } else {
                all.insert(path.clone(), Seen::Bytes(std::fs::read(&path).unwrap()));
            }
        }
    }
    all
}

/// Make a sparse file of `size` bytes, so that no test writes gigabytes.
/// # Arguments
/// * `path` - The file.
/// * `size` - Its length.
fn sparse(path: &Path, size: u64) {
    std::fs::File::create(path).unwrap().set_len(size).unwrap();
}

/// Give the refusal of a file over the ceiling.
/// # Arguments
/// * `name` - The tracked path.
/// # Returns
/// The message of `add` and of `commit`.
fn over_ceiling(name: &str) -> String {
    format!(
        "the file \"{name}\" has 2147418110 bytes, more than the 2147418109 bytes that one file may have"
    )
}

/// Require a refusal with exit 1, no output and the exact diagnostic `message`.
/// # Arguments
/// * `output` - The finished process output.
/// * `message` - The fixed refusal.
fn refused(output: Output, message: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(output.stdout.is_empty());
    assert_eq!(stderr, format!("{message}\n"));
}

#[test]
/// A checkout of 300 small files, past each old ceiling, goes through add, scope,
/// status, commit, a second commit and verify, with `add` and `scope` of one batch.
/// # Panics
/// Panics if a command refuses or prints the wrong line.
fn three_hundred_small_files_commit_twice() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let names = fixture(home, 300, 20);
    track(home, &names, names.len());
    let status = ok(vcs(home, &["status"]));
    let rows: Vec<&str> = status.lines().skip(1).collect();
    assert_eq!(rows.len(), 300);
    assert!(rows.iter().all(|line| line.starts_with("A\tprivate\t")));
    assert!(ok(commit(home, 21)).starts_with("Recorded private check-in: "));
    std::fs::write(home.join("root").join(&names[7]), b"changed").unwrap();
    assert!(ok(commit(home, 22)).starts_with("Recorded private check-in: "));
    // A third commit of the same files records nothing.
    assert_eq!(ok(commit(home, 23)), "No changes\n");
    let status = ok(vcs(home, &["status"]));
    assert_eq!(
        status
            .lines()
            .filter(|l| l.starts_with("R\tprivate\t"))
            .count(),
        300
    );
    assert_eq!(
        verify(home),
        "verified: 2 check-ins, 0 issues, 15 tables compared\n"
    );
}

#[test]
/// Three files of 1 MiB and one byte, past the old ceiling of 1 MiB, commit twice and
/// verify.
/// # Panics
/// Panics if a command refuses.
fn files_past_the_old_ceiling_commit_twice() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let names = fixture(home, 3, OLD_CEILING + 1);
    track(home, &names, names.len());
    ok(commit(home, 21));
    let mut changed = content(99, OLD_CEILING + 1);
    changed[0] = b'X';
    std::fs::write(home.join("root").join(&names[1]), changed).unwrap();
    ok(commit(home, 22));
    assert_eq!(
        verify(home),
        "verified: 2 check-ins, 0 issues, 15 tables compared\n"
    );
}

#[test]
/// A sparse file one byte over the ceiling is refused by `add`, alone or in a batch
/// of 200, and by `commit` after it grew past the ceiling, after 99 stored files.
/// Each refusal names the file, its size and the ceiling, reads none of its bytes and
/// changes no byte of the repository, its sidecars or the checkout.
/// # Panics
/// Panics if a command passes, gives another message or changes a byte.
fn a_file_over_the_ceiling_is_refused_by_add_and_commit_and_changes_no_byte() {
    for files in [1, 200] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let names = fixture(home, files, 20);
        let large = files / 2;
        sparse(&home.join("root").join(&names[large]), CEILING + 1);
        let before = every_byte(home);
        let mut add = vec!["add"];
        for name in &names {
            add.extend(["--path", name]);
        }
        refused(vcs(home, &add), &over_ceiling(&names[large]));
        assert_eq!(every_byte(home), before);
    }
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let names = fixture(home, 200, 20);
    track(home, &names, names.len());
    sparse(&home.join("root").join(&names[99]), CEILING + 1);
    let before = every_byte(home);
    refused(commit(home, 21), &over_ceiling(&names[99]));
    assert_eq!(every_byte(home), before);
    assert_eq!(
        verify(home),
        "verified: 0 check-ins, 0 issues, 15 tables compared\n"
    );
}

#[test]
/// A file over 20,000,000 bytes gives one warning line on stderr in `add` and in a
/// commit that stores it, as Fossil asks before such a commit, and no question. A
/// commit with no change, or one that stores only another file, gives no warning.
/// # Panics
/// Panics if a command refuses, or a warning is missing or extra.
fn a_large_file_gives_a_warning_and_no_question() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let names = fixture(home, 1, 20);
    sparse(&home.join("root").join("large.bin"), 20_000_001);
    let warning = "warning: the file \"large.bin\" has 20000001 bytes, more than 20000000\n";
    let mut add = vec!["add", "--path", "large.bin"];
    add.extend(["--path", &names[0]]);
    let out = vcs(home, &add);
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stderr), warning);
    ok(vcs(
        home,
        &[
            "scope",
            "--scope",
            "private",
            "--path",
            "large.bin",
            "--path",
            &names[0],
        ],
    ));
    let out = commit(home, 21);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stderr), warning);
    assert_eq!(ok(commit(home, 22)), "No changes\n");
    // A check-in that stores only the small file gives no warning for the large one.
    std::fs::write(home.join("root").join(&names[0]), b"changed").unwrap();
    let out = commit(home, 23);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
}

#[test]
/// A commit that starts while another connection holds the write lock for 7 seconds,
/// past the old busy timeout of 5 seconds, waits within the timeout of 15 seconds and
/// succeeds.
/// # Panics
/// Panics if the commit ends before the lock goes, or refuses.
fn a_commit_waits_behind_a_held_write_lock() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let names = fixture(home, 3, 20);
    track(home, &names, names.len());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let database = runtime
        .block_on(libsql::Builder::new_local(home.join("repo")).build())
        .unwrap();
    let holder = database.connect().unwrap();
    runtime
        .block_on(holder.execute("BEGIN IMMEDIATE", ()))
        .unwrap();
    let repo = home.join("repo");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_gos"))
        .env("HOME", home)
        .env("GOSSIL_USER", "tester")
        .current_dir(home.join("root"))
        .args(["v", "commit", "--repository"])
        .arg(&repo)
        .args([
            "--checkout",
            "work",
            "--scope",
            "private",
            "--all-tracked",
            "--user",
            "tester",
            "--comment",
            "wait",
            "--date-override",
            "2026-11-21T12:00:00",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_secs(7));
    assert!(child.try_wait().unwrap().is_none());
    runtime.block_on(holder.execute("ROLLBACK", ())).unwrap();
    drop(holder);
    assert!(ok(child.wait_with_output().unwrap()).starts_with("Recorded private check-in: "));
}

#[test]
/// A file that cannot be read stops a commit of 200 files after 99 are stored, and no
/// byte changes. After the fault is gone, the commit passes.
/// # Panics
/// Panics if the commit passes, gives another message or changes a byte.
fn an_unreadable_file_in_the_middle_changes_no_byte() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let names = fixture(home, 200, 20);
    track(home, &names, names.len());
    ok(commit(home, 21));
    for name in &names {
        std::fs::write(home.join("root").join(name), b"second").unwrap();
    }
    let fault = home.join("root").join(&names[99]);
    let before = every_byte(home);
    std::fs::set_permissions(&fault, std::fs::Permissions::from_mode(0o000)).unwrap();
    refused(commit(home, 22), "checkout filesystem syscall failed");
    std::fs::set_permissions(&fault, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(every_byte(home), before);
    ok(commit(home, 22));
    assert_eq!(
        verify(home),
        "verified: 2 check-ins, 0 issues, 15 tables compared\n"
    );
}

/// Time one command.
/// # Arguments
/// * `run` - The command.
/// # Returns
/// The time and the result of the command.
fn timed<T>(run: impl FnOnce() -> T) -> (Duration, T) {
    let start = Instant::now();
    let result = run();
    (start.elapsed(), result)
}

/// Make one capacity target, and print the time of each command.
/// # Arguments
/// * `files` - The count of files.
/// * `size` - The size of each file.
/// # Panics
/// Panics if a command refuses.
fn target(files: usize, size: usize) {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let names = fixture(home, files, size);
    let (add, ()) = timed(|| track(home, &names, BATCH));
    let (status, out) = timed(|| ok(vcs(home, &["status"])));
    assert_eq!(out.lines().count(), files + 1);
    let (first, _) = timed(|| ok(commit(home, 21)));
    let (first_verify, _) = timed(|| verify(home));
    let mut changed = content(files, size);
    changed[0] = b'X';
    std::fs::write(home.join("root").join(&names[files / 2]), changed).unwrap();
    let (second, _) = timed(|| ok(commit(home, 22)));
    let (second_status, _) = timed(|| ok(vcs(home, &["status"])));
    let (second_verify, out) = timed(|| verify(home));
    assert_eq!(out, "verified: 2 check-ins, 0 issues, 15 tables compared\n");
    println!(
        "{files} files of {size} bytes: add and scope {add:.2?}, status {status:.2?}, \
         commit {first:.2?}, verify {first_verify:.2?}, second commit {second:.2?}, \
         status {second_status:.2?}, verify {second_verify:.2?}"
    );
}

#[test]
#[ignore = "capacity run of several minutes; run just commit-capacity"]
/// Commit 5,000 small files and 200 files of 1 MB, change one file of each and
/// commit again, and run status and verify, printing each time.
/// # Panics
/// Panics if a command refuses.
fn capacity_targets() {
    target(5000, 20);
    target(200, 1_000_000);
}

#[test]
#[ignore = "needs the pinned Fossil reference in GOSSIL_FOSSIL_REFERENCE; run just commit-oracle"]
/// Stock Fossil reads a check-in of 1,000 files: `test-integrity` finds no error and
/// the file list of the check-in is the tracked set.
/// # Panics
/// Panics if Fossil refuses the repository or lists other files.
fn stock_fossil_reads_a_thousand_file_checkin() {
    let reference =
        std::env::var_os("GOSSIL_FOSSIL_REFERENCE").expect("explicit reference executable");
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let names = fixture(home, 1000, 100);
    track(home, &names, names.len());
    let recorded = ok(commit(home, 21));
    let hash = recorded
        .strip_prefix("Recorded private check-in: ")
        .unwrap()
        .trim_end();
    let repo = home.join("repo");
    let fossil = |args: &[&str]| {
        let output = std::process::Command::new("jlog")
            .arg("run")
            .arg(&reference)
            .args(args)
            .arg("-R")
            .arg(&repo)
            .env("HOME", home)
            .env("FOSSIL_HOME", home)
            .env("USER", "tester")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "fossil {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    let integrity = fossil(&["test-integrity", "--parse"]);
    assert!(integrity.contains(" 0 errors\n"), "{integrity}");
    // The shared root and the check-in.
    assert!(integrity.contains("  2 manifests\n"), "{integrity}");
    let listed: Vec<String> = fossil(&["ls", "-r", hash])
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(listed, names);
}
