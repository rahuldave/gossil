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
//! The ordinary tests run the CLI in a debug build: 300 small files, past every old
//! ceiling, and 12 files of the largest size. The ignored tests make the capacity
//! targets in a release build (`just commit-capacity`), and give a check-in of
//! 1,000 files to the pinned stock Fossil (`just commit-oracle`).
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, Instant};

/// The largest file that a commit stores: the content ceiling of one artifact.
const LARGEST: usize = 1 << 20;
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

/// Read every file below the fixture home: the repository, its sidecars, the locator
/// and the checkout.
/// # Arguments
/// * `home` - The fixture home.
/// # Returns
/// The bytes of each file, by path.
fn every_byte(home: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut all = BTreeMap::new();
    let mut pending = vec![home.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                all.insert(path.clone(), std::fs::read(&path).unwrap());
            }
        }
    }
    all
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
/// Twelve files of the largest size commit twice and verify. The old ceiling was 5.
/// # Panics
/// Panics if a command refuses.
fn twelve_files_of_the_largest_size_commit_twice() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let names = fixture(home, 12, LARGEST);
    track(home, &names, names.len());
    ok(commit(home, 21));
    let mut changed = content(99, LARGEST);
    changed[0] = b'X';
    std::fs::write(home.join("root").join(&names[3]), changed).unwrap();
    ok(commit(home, 22));
    assert_eq!(
        verify(home),
        "verified: 2 check-ins, 0 issues, 15 tables compared\n"
    );
}

#[test]
/// A file one byte over the largest size is refused, alone or after 99 stored files,
/// and no byte of the repository, its sidecars or the checkout changes.
/// # Panics
/// Panics if the commit passes, gives another message or changes a byte.
fn a_file_over_the_ceiling_is_refused_and_changes_no_byte() {
    for (files, large) in [(1, 0), (200, 99)] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let names = fixture(home, files, 20);
        std::fs::write(
            home.join("root").join(&names[large]),
            content(large, LARGEST + 1),
        )
        .unwrap();
        track(home, &names, names.len());
        let before = every_byte(home);
        refused(commit(home, 21), "recording content output limit");
        assert_eq!(every_byte(home), before);
        assert_eq!(
            verify(home),
            "verified: 0 check-ins, 0 issues, 15 tables compared\n"
        );
    }
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
