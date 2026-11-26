//! Compare the size, count and lock limits of `gos` with stock Fossil and Git.
//!
//! Each ignored test builds one tree, runs the same steps in `gos`, in the pinned
//! Fossil reference and in `git`, and prints one Markdown row for each step: the
//! time, the peak resident memory from `/usr/bin/time -l`, and the size of the
//! repository on disk. Run them in a release build with `just limits-capacity`.
//! Fossil runs with `HOME` set to a scratch folder. When the pinned Fossil is absent,
//! its rows are left out. The tables of the source tree, the 20,000 files and the file
//! at the ceiling then say so in a note. Git must be installed.
//!
//! The file builds on macOS only, because it measures with the BSD flag `time -l`
//! and copies with `cp -c`. On other systems it holds no tests.
//!
//! The cases are the source tree of Gossil itself at `6fb025a`, 20,000 small files in
//! nested directories, random files of 100 MB and 250 MB, a file at the ceiling that
//! stock Fossil reads back, a file over the ceiling, a task with a description of
//! 1 MB, and a commit behind a write lock held for 20 seconds. Random
//! files of 600 MB and 999,000,000 bytes are ignored tests that the recipe skips.
//! Run them by hand. The 21 symbolic links of the source tree are removed for all three
//! tools, because `gos` tracks regular files only.
#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

/// The commit of the source tree of Gossil itself.
const SOURCE: &str = "6fb025a";
/// The largest file that `gos` stores, the largest file that stock Fossil reads.
const CEILING: u64 = 2_147_418_109;

/// One measured command.
struct Run {
    /// The finished output of the command, without the lines of `time`.
    output: Output,
    /// The wall time.
    time: Duration,
    /// The peak resident memory, in bytes.
    peak: u64,
}

/// Run a command under `/usr/bin/time -l`.
/// # Arguments
/// * `command` - The command, with its environment and directory.
/// # Returns
/// The output, the wall time and the peak memory.
/// # Panics
/// Panics if `time` cannot run or prints no figures.
fn measure(command: &mut Command) -> Run {
    let program = command.get_program().to_owned();
    let args: Vec<_> = command.get_args().map(ToOwned::to_owned).collect();
    let mut timed = Command::new("/usr/bin/time");
    timed.arg("-l").arg(program).args(args);
    if let Some(dir) = command.get_current_dir() {
        timed.current_dir(dir);
    }
    for (key, value) in command.get_envs() {
        match value {
            Some(value) => timed.env(key, value),
            None => timed.env_remove(key),
        };
    }
    let mut output = timed.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let mut time = None;
    let mut peak = None;
    let mut kept = Vec::new();
    for line in stderr.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if line.contains(" real ") && words.get(1) == Some(&"real") {
            time = words[0].parse::<f64>().ok();
        } else if line.ends_with("maximum resident set size") {
            peak = words[0].parse::<u64>().ok();
        } else if !line.starts_with("        ") && !line.trim().is_empty() {
            kept.push(line);
        }
    }
    output.stderr = kept.join("\n").into_bytes();
    Run {
        output,
        time: Duration::from_secs_f64(time.expect("time")),
        peak: peak.expect("peak"),
    }
}

/// The Markdown rows of one case.
struct Table {
    /// The name of the case.
    case: String,
    /// The rows: tool, step, result, time, peak memory.
    rows: Vec<String>,
}

impl Table {
    /// Start the rows of a case.
    /// # Arguments
    /// * `case` - The name of the case.
    /// # Returns
    /// An empty table.
    fn new(case: &str) -> Self {
        Self {
            case: case.to_owned(),
            rows: Vec::new(),
        }
    }
    /// Add a row for a measured step.
    /// # Arguments
    /// * `tool` - `gos`, `fossil` or `git`.
    /// * `step` - The step.
    /// * `run` - The measured command.
    fn add(&mut self, tool: &str, step: &str, run: &Run) {
        let result = if run.output.status.success() {
            "ok".to_owned()
        } else {
            let text = String::from_utf8_lossy(&run.output.stderr);
            let first = text.lines().next().unwrap_or("").replace('|', "/");
            format!("status {:?}: {first}", run.output.status.code())
        };
        self.rows.push(format!(
            "| {} | {tool} | {step} | {result} | {:.2} s | {} MB |",
            self.case,
            run.time.as_secs_f64(),
            run.peak / 1_000_000
        ));
    }
    /// Add a row for the size of a repository.
    /// # Arguments
    /// * `tool` - `gos`, `fossil` or `git`.
    /// * `bytes` - The size on disk.
    fn size(&mut self, tool: &str, bytes: u64) {
        self.rows.push(format!(
            "| {} | {tool} | repository on disk | {} MB | | |",
            self.case,
            bytes / 1_000_000
        ));
    }
    /// Add a row of free text.
    /// # Arguments
    /// * `tool` - The tool.
    /// * `step` - The step.
    /// * `text` - The result.
    fn note(&mut self, tool: &str, step: &str, text: &str) {
        self.rows
            .push(format!("| {} | {tool} | {step} | {text} | | |", self.case));
    }
    /// Print the rows.
    fn print(&self) {
        println!("| Case | Tool | Step | Result | Time | Peak memory |");
        println!("|---|---|---|---|---|---|");
        for row in &self.rows {
            println!("{row}");
        }
    }
}

/// Give the size of every file below a path.
/// # Arguments
/// * `path` - A file or a directory.
/// # Returns
/// The sum of the allocated sizes, in bytes, as `du` counts them.
fn disk(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(path).unwrap();
    if meta.is_dir() {
        std::fs::read_dir(path)
            .unwrap()
            .map(|entry| disk(&entry.unwrap().path()))
            .sum()
    } else {
        meta.blocks() * 512
    }
}

/// Give the pinned Fossil reference, if it exists.
/// # Returns
/// The path of the executable.
fn fossil_binary() -> Option<PathBuf> {
    let path = std::env::var_os("GOSSIL_FOSSIL_REFERENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join(".jlog/bin/fossil"));
    path.exists().then_some(path)
}

/// The scratch folders of one tool in one case.
struct Place {
    /// The home of the tool.
    home: PathBuf,
    /// The checkout root, a copy of the tree.
    root: PathBuf,
    /// The repository file, or `.git` for Git.
    repo: PathBuf,
}

/// Copy a tree for one tool, as a clone where the file system allows it.
/// # Arguments
/// * `base` - The folder of the case.
/// * `tool` - The tool.
/// * `tree` - The tree to copy, or `None` for an empty root.
/// # Returns
/// The folders of the tool.
fn place(base: &Path, tool: &str, tree: Option<&Path>) -> Place {
    let home = base.join(tool);
    std::fs::create_dir_all(&home).unwrap();
    let root = home.join("root");
    match tree {
        Some(tree) => assert!(
            Command::new("cp")
                .arg("-c")
                .arg("-R")
                .arg(tree)
                .arg(&root)
                .status()
                .unwrap()
                .success()
        ),
        None => std::fs::create_dir(&root).unwrap(),
    }
    let root = root.canonicalize().unwrap();
    let repo = if tool == "git" {
        root.join(".git")
    } else {
        home.join("repo")
    };
    Place { home, root, repo }
}

/// List the regular files below a root, relative to it, in byte order.
/// # Arguments
/// * `root` - The root.
/// # Returns
/// The paths.
fn files(root: &Path) -> Vec<String> {
    let mut all = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                pending.push(path);
            } else if meta.is_file() {
                let relative = path.strip_prefix(root).unwrap();
                all.push(relative.to_str().unwrap().to_owned());
            }
        }
    }
    all.sort();
    all
}

/// Make a `gos` command in a place.
/// # Arguments
/// * `place` - The folders of the tool.
/// * `args` - The arguments after the executable.
/// # Returns
/// The command.
fn gos(place: &Place, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gos"));
    for (key, _) in std::env::vars_os() {
        if key.to_str().is_some_and(|key| key.starts_with("GEST_")) {
            command.env_remove(key);
        }
    }
    command
        .env("HOME", &place.home)
        .env("XDG_CONFIG_HOME", &place.home)
        .env("XDG_DATA_HOME", &place.home)
        .env("GOSSIL_USER", "tester")
        .current_dir(&place.root)
        .args(args);
    command
}

/// Make a `gos v VERB` command on the repository and checkout `work` of a place.
/// # Arguments
/// * `place` - The folders of the tool.
/// * `verb` - The verb and its flags.
/// # Returns
/// The command.
fn gos_v(place: &Place, verb: &[&str]) -> Command {
    let repo = place.repo.to_str().unwrap();
    let mut args = vec!["v", verb[0], "--repository", repo, "--checkout", "work"];
    args.extend_from_slice(&verb[1..]);
    gos(place, &args)
}

/// Make a Fossil command in a place, with a scratch `HOME`.
/// # Arguments
/// * `fossil` - The executable.
/// * `place` - The folders of the tool.
/// * `args` - The arguments.
/// # Returns
/// The command.
fn fossil(fossil: &Path, place: &Place, args: &[&str]) -> Command {
    let mut command = Command::new(fossil);
    command
        .env("HOME", &place.home)
        .env("FOSSIL_HOME", &place.home)
        .env("USER", "tester")
        .current_dir(&place.root)
        .args(args);
    command
}

/// Make a Git command in a place, with no global or system configuration.
/// # Arguments
/// * `place` - The folders of the tool.
/// * `args` - The arguments.
/// # Returns
/// The command.
fn git(place: &Place, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .env("HOME", &place.home)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "tester")
        .env("GIT_AUTHOR_EMAIL", "tester@example.invalid")
        .env("GIT_COMMITTER_NAME", "tester")
        .env("GIT_COMMITTER_EMAIL", "tester@example.invalid")
        .current_dir(&place.root)
        .args(args);
    command
}

/// Run a command that must succeed.
/// # Arguments
/// * `command` - The command.
/// # Returns
/// Its standard output.
/// # Panics
/// Panics if it fails.
fn must(command: &mut Command) -> String {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Require a measured step to succeed.
/// # Arguments
/// * `run` - The step.
/// * `what` - The name of the step, for the panic.
/// # Returns
/// The standard output.
/// # Panics
/// Panics if the step failed.
fn passed(run: &Run, what: &str) -> String {
    assert!(
        run.output.status.success(),
        "{what}: {}",
        String::from_utf8_lossy(&run.output.stderr)
    );
    String::from_utf8_lossy(&run.output.stdout).into_owned()
}

/// Add the date flags of a `gos` commit on day `day` of November 2026.
/// # Arguments
/// * `day` - The day.
/// # Returns
/// The verb and its flags.
fn commit_verb(day: u32) -> Vec<String> {
    [
        "commit",
        "--scope",
        "private",
        "--all-tracked",
        "--user",
        "tester",
        "--comment",
        "save",
        "--date-override",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .chain([format!("2026-11-{day:02}T12:00:00")])
    .collect()
}

/// Open a new `gos` repository on a place, and track and select every file of the root.
/// # Arguments
/// * `place` - The folders of the tool.
/// * `table` - The rows of the case.
/// # Returns
/// The tracked paths.
fn gos_track(place: &Place, table: &mut Table) -> Vec<String> {
    must(&mut gos(
        place,
        &[
            "v",
            "init",
            "--date-override",
            "2026-11-20T00:00:00",
            place.repo.to_str().unwrap(),
        ],
    ));
    must(&mut gos_v(
        place,
        &["open", "--root", place.root.to_str().unwrap()],
    ));
    let names: Vec<String> = files(&place.root)
        .into_iter()
        .filter(|name| !name.starts_with(".gossil/"))
        .collect();
    let mut add_time = Duration::ZERO;
    let mut add_peak = 0;
    for verb in ["add", "scope"] {
        for batch in names.chunks(5000) {
            let mut args = vec![verb];
            if verb == "scope" {
                args.extend(["--scope", "private"]);
            }
            for name in batch {
                args.extend(["--path", name]);
            }
            let run = measure(&mut gos_v(place, &args));
            if !run.output.status.success() {
                table.add("gos", verb, &run);
                return Vec::new();
            }
            add_time += run.time;
            add_peak = add_peak.max(run.peak);
        }
    }
    table.rows.push(format!(
        "| {} | gos | add and scope of {} files | ok | {:.2} s | {} MB |",
        table.case,
        names.len(),
        add_time.as_secs_f64(),
        add_peak / 1_000_000
    ));
    names
}

/// Change one line of a file.
/// # Arguments
/// * `path` - The file.
fn edit(path: &Path) {
    let mut bytes = std::fs::read(path).unwrap();
    bytes.extend_from_slice(b"\nchanged line\n");
    std::fs::write(path, bytes).unwrap();
}

/// Run the steps of a tree case in all three tools: track and commit, a second
/// commit after one changed line, status, and the full check.
/// # Arguments
/// * `case` - The name of the case.
/// * `tree` - The tree.
/// * `changed` - The path of the file to change, relative to the tree.
/// # Returns
/// The rows, and the `gos` repository with the hash of its first check-in.
fn tree_case(case: &str, tree: &Path, changed: &str, base: &Path) -> (Table, Place, String) {
    let mut table = Table::new(case);
    let place = place(base, "gos", Some(tree));
    let names = gos_track(&place, &mut table);
    assert!(!names.is_empty(), "gos refused the tree");
    let verb = commit_verb(21);
    let verb: Vec<&str> = verb.iter().map(String::as_str).collect();
    let run = measure(&mut gos_v(&place, &verb));
    table.add("gos", "commit", &run);
    let first = passed(&run, "gos commit");
    let hash = first
        .trim_end()
        .strip_prefix("Recorded private check-in: ")
        .unwrap()
        .to_owned();
    edit(&place.root.join(changed));
    let verb = commit_verb(22);
    let verb: Vec<&str> = verb.iter().map(String::as_str).collect();
    let run = measure(&mut gos_v(&place, &verb));
    table.add("gos", "second commit, one line changed", &run);
    passed(&run, "gos second commit");
    let run = measure(&mut gos_v(&place, &["status"]));
    table.add("gos", "status", &run);
    passed(&run, "gos status");
    let repo = place.repo.to_str().unwrap().to_owned();
    let run = measure(&mut gos(&place, &["v", "verify", "--repository", &repo]));
    table.add("gos", "verify", &run);
    passed(&run, "gos verify");
    table.size("gos", disk(&place.repo));

    if let Some(binary) = fossil_binary() {
        let f = self::place(base, "fossil", Some(tree));
        let repo = f.repo.to_str().unwrap().to_owned();
        must(&mut fossil(&binary, &f, &["init", &repo]));
        must(&mut fossil(&binary, &f, &["open", "--force", &repo]));
        let run = measure(&mut fossil(&binary, &f, &["add", "--dotfiles", "."]));
        table.add("fossil", "add", &run);
        let run = measure(&mut fossil(
            &binary,
            &f,
            &["commit", "--no-warnings", "-m", "save"],
        ));
        table.add("fossil", "commit", &run);
        edit(&f.root.join(changed));
        let run = measure(&mut fossil(
            &binary,
            &f,
            &["commit", "--no-warnings", "-m", "again"],
        ));
        table.add("fossil", "second commit, one line changed", &run);
        let run = measure(&mut fossil(&binary, &f, &["status"]));
        table.add("fossil", "status", &run);
        let run = measure(&mut fossil(&binary, &f, &["test-integrity"]));
        table.add("fossil", "test-integrity", &run);
        table.size("fossil", disk(&f.repo));
    } else {
        table.note("fossil", "all", "not run: no Fossil reference");
    }

    let g = self::place(base, "git", Some(tree));
    must(&mut git(&g, &["init", "-q"]));
    let run = measure(&mut git(&g, &["add", "-A"]));
    table.add("git", "add", &run);
    let run = measure(&mut git(&g, &["commit", "-q", "-m", "save"]));
    table.add("git", "commit", &run);
    edit(&g.root.join(changed));
    let run = measure(&mut git(&g, &["commit", "-q", "-a", "-m", "again"]));
    table.add("git", "second commit, one line changed", &run);
    let run = measure(&mut git(&g, &["status", "--porcelain"]));
    table.add("git", "status", &run);
    let run = measure(&mut git(&g, &["fsck", "--no-progress"]));
    table.add("git", "fsck", &run);
    table.size("git", disk(&g.repo));
    (table, place, hash)
}

#[test]
#[ignore = "capacity run of several minutes; run just limits-capacity"]
/// The source tree of Gossil itself commits, changes and verifies in `gos`, and stock
/// Fossil reads the result. `test-integrity` finds no error, and `fossil ls` lists
/// every file of the first check-in.
/// # Panics
/// Panics if a `gos` step refuses, or Fossil does not read the repository.
fn own_source_tree() {
    let temp = tempfile::tempdir().unwrap();
    let tree = temp.path().join("tree");
    std::fs::create_dir(&tree).unwrap();
    let index = temp.path().join("index");
    must(
        Command::new("git")
            .env("GIT_INDEX_FILE", &index)
            .arg("-C")
            .arg(env!("CARGO_MANIFEST_DIR"))
            .arg("--work-tree")
            .arg(&tree)
            .args(["checkout", SOURCE, "--", "."]),
    );
    let mut links = 0;
    let mut pending = vec![tree.clone()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if meta.file_type().is_symlink() {
                std::fs::remove_file(&path).unwrap();
                links += 1;
            } else if meta.is_dir() {
                pending.push(path);
            }
        }
    }
    let names = files(&tree);
    let base = temp.path().join("run");
    let (mut table, place, hash) = tree_case(
        &format!("own tree, {} files", names.len()),
        &tree,
        "src/lib.rs",
        &base,
    );
    table.note("all", "tree", &format!("{links} symbolic links removed"));
    if let Some(binary) = fossil_binary() {
        let repo = place.repo.to_str().unwrap();
        let run = measure(&mut fossil(
            &binary,
            &place,
            &["test-integrity", "--parse", "-R", repo],
        ));
        let integrity = passed(&run, "fossil test-integrity of the gos repository");
        assert!(integrity.contains(" 0 errors\n"), "{integrity}");
        table.add("fossil", "test-integrity of the gos repository", &run);
        let listed = must(&mut fossil(
            &binary,
            &place,
            &["ls", "-r", &hash, "-R", repo],
        ));
        let listed: std::collections::BTreeSet<&str> = listed.lines().collect();
        let tracked: std::collections::BTreeSet<&str> = names.iter().map(String::as_str).collect();
        let only_fossil: Vec<_> = listed.difference(&tracked).collect();
        let only_gos: Vec<_> = tracked.difference(&listed).collect();
        assert!(
            only_fossil.is_empty() && only_gos.is_empty(),
            "only in fossil ls: {only_fossil:?}; only tracked: {only_gos:?}"
        );
        table.note(
            "fossil",
            "ls of the first gos check-in",
            &format!("{} files, the tracked set", listed.len()),
        );
    }
    table.print();
}

#[test]
#[ignore = "capacity run of several minutes; run just limits-capacity"]
/// 20,000 files of 1 KB in nested directories commit, change and verify.
/// # Panics
/// Panics if a `gos` step refuses.
fn twenty_thousand_small_files() {
    let temp = tempfile::tempdir().unwrap();
    let tree = temp.path().join("tree");
    for n in 0..20_000 {
        let path = tree.join(format!(
            "a{:02}/b{:02}/f{n:05}.txt",
            n / 1000,
            (n / 50) % 20
        ));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let line = format!("file {n} ");
        std::fs::write(path, &line.repeat(1024 / line.len() + 1).as_bytes()[..1024]).unwrap();
    }
    let (table, _, _) = tree_case(
        "20,000 files of 1 KB",
        &tree,
        "a10/b00/f10000.txt",
        &temp.path().join("run"),
    );
    table.print();
}

/// Run one large random file through `add`, `commit` and the full check in all three
/// tools, and require the peak memory of each `gos` step to stay within 4 times the file.
/// # Arguments
/// * `size` - The size of the file.
/// # Panics
/// Panics if a `gos` step refuses or uses more than 4 times the file.
fn large_file(size: u64) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("data.bin");
    must(Command::new("sh").arg("-c").arg(format!(
        "head -c {size} /dev/urandom > '{}'",
        source.display()
    )));
    let case = format!("one random file of {} MB", size / 1_000_000);
    let mut table = Table::new(&case);
    let base = temp.path().join("run");
    let tree = temp.path().join("tree");
    std::fs::create_dir(&tree).unwrap();
    std::fs::rename(&source, tree.join("data.bin")).unwrap();
    let place = place(&base, "gos", Some(&tree));
    gos_track(&place, &mut table);
    let verb = commit_verb(21);
    let verb: Vec<&str> = verb.iter().map(String::as_str).collect();
    let run = measure(&mut gos_v(&place, &verb));
    table.add("gos", "commit", &run);
    passed(&run, "gos commit");
    let commit_peak = run.peak;
    let repo = place.repo.to_str().unwrap().to_owned();
    let run = measure(&mut gos(&place, &["v", "verify", "--repository", &repo]));
    table.add("gos", "verify", &run);
    passed(&run, "gos verify");
    table.size("gos", disk(&place.repo));
    table.note(
        "gos",
        "peak of commit over the file",
        &format!("{:.2} times", commit_peak as f64 / size as f64),
    );
    std::fs::remove_dir_all(&place.home).unwrap();
    if let Some(binary) = fossil_binary() {
        let f = self::place(&base, "fossil", Some(&tree));
        let repo = f.repo.to_str().unwrap().to_owned();
        must(&mut fossil(&binary, &f, &["init", &repo]));
        must(&mut fossil(&binary, &f, &["open", "--force", &repo]));
        must(&mut fossil(&binary, &f, &["add", "data.bin"]));
        let run = measure(&mut fossil(
            &binary,
            &f,
            &["commit", "--no-warnings", "-m", "save"],
        ));
        table.add("fossil", "commit", &run);
        let run = measure(&mut fossil(&binary, &f, &["test-integrity"]));
        table.add("fossil", "test-integrity", &run);
        table.size("fossil", disk(&f.repo));
        std::fs::remove_dir_all(&f.home).unwrap();
    }
    let g = self::place(&base, "git", Some(&tree));
    must(&mut git(&g, &["init", "-q"]));
    let run = measure(&mut git(&g, &["add", "-A"]));
    table.add("git", "add", &run);
    let run = measure(&mut git(&g, &["commit", "-q", "-m", "save"]));
    table.add("git", "commit", &run);
    let run = measure(&mut git(&g, &["fsck", "--no-progress"]));
    table.add("git", "fsck", &run);
    table.size("git", disk(&g.repo));
    table.print();
    assert!(
        commit_peak <= 4 * size,
        "gos commit used {commit_peak} bytes for a file of {size}"
    );
}

#[test]
#[ignore = "capacity run of several minutes; run just limits-capacity"]
/// A random file of 100 MB.
fn random_file_of_100_mb() {
    large_file(100_000_000);
}

#[test]
#[ignore = "capacity run of several minutes; run just limits-capacity"]
/// A random file of 250 MB.
fn random_file_of_250_mb() {
    large_file(250_000_000);
}

#[test]
#[ignore = "capacity run of several minutes and gigabytes of memory; run by hand"]
/// A random file of 600 MB.
fn random_file_of_600_mb() {
    large_file(600_000_000);
}

#[test]
#[ignore = "capacity run of several minutes and gigabytes of memory; run by hand"]
/// A random file of 999,000,000 bytes, whose envelope is just under the stored ceiling.
fn random_file_of_999_mb() {
    large_file(999_000_000);
}

#[test]
#[ignore = "capacity run; run just limits-capacity"]
/// A sparse file one byte over the ceiling is refused by `add`, which changes no byte.
/// # Panics
/// Panics if `add` accepts it or changes a byte of the repository.
fn a_file_over_the_ceiling_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let mut table = Table::new("sparse file of 2,147,418,110 bytes");
    let place = place(&temp.path().join("run"), "gos", None);
    let big = place.root.join("big.bin");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(CEILING + 1)
        .unwrap();
    must(&mut gos(
        &place,
        &[
            "v",
            "init",
            "--date-override",
            "2026-11-20T00:00:00",
            place.repo.to_str().unwrap(),
        ],
    ));
    must(&mut gos_v(
        &place,
        &["open", "--root", place.root.to_str().unwrap()],
    ));
    let before = std::fs::read(&place.repo).unwrap();
    let run = measure(&mut gos_v(&place, &["add", "--path", "big.bin"]));
    table.add("gos", "add", &run);
    assert_eq!(run.output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&run.output.stderr),
        "the file \"big.bin\" has 2147418110 bytes, more than the 2147418109 bytes that one file may have"
    );
    assert_eq!(std::fs::read(&place.repo).unwrap(), before);
    assert_eq!(std::fs::metadata(&big).unwrap().len(), CEILING + 1);
    table.note("gos", "repository bytes", "unchanged");
    table.print();
}

#[test]
#[ignore = "capacity run of a minute and 3 GB of memory; run just limits-capacity"]
/// A sparse file of exactly the ceiling commits and verifies in `gos`, and stock
/// Fossil reads it back. `test-integrity` finds no error.
/// # Panics
/// Panics if a step refuses, or Fossil does not read the file.
fn stock_fossil_reads_a_file_at_the_ceiling() {
    let temp = tempfile::tempdir().unwrap();
    let mut table = Table::new("sparse file of 2,147,418,109 bytes");
    let tree = temp.path().join("tree");
    std::fs::create_dir(&tree).unwrap();
    std::fs::File::create(tree.join("big.bin"))
        .unwrap()
        .set_len(CEILING)
        .unwrap();
    let place = place(&temp.path().join("run"), "gos", Some(&tree));
    gos_track(&place, &mut table);
    let verb = commit_verb(21);
    let verb: Vec<&str> = verb.iter().map(String::as_str).collect();
    let run = measure(&mut gos_v(&place, &verb));
    table.add("gos", "commit", &run);
    passed(&run, "gos commit");
    let repo = place.repo.to_str().unwrap().to_owned();
    let run = measure(&mut gos(&place, &["v", "verify", "--repository", &repo]));
    table.add("gos", "verify", &run);
    passed(&run, "gos verify");
    if let Some(binary) = fossil_binary() {
        let run = measure(&mut fossil(
            &binary,
            &place,
            &["test-integrity", "-R", &repo],
        ));
        table.add("fossil", "test-integrity of the gos repository", &run);
        let integrity = passed(&run, "fossil test-integrity");
        assert!(integrity.contains(" 0 errors\n"), "{integrity}");
    } else {
        table.note("fossil", "all", "not run: no Fossil reference");
    }
    table.print();
}

#[test]
#[cfg(target_os = "macos")]
#[ignore = "capacity run; run just limits-capacity"]
/// Create, show and list a task with a description of 1 MB. Fossil runs
/// `ticket add` with the same comment, and in the recorded run Fossil stopped
/// with a segmentation fault. Git has no issue tracker. The test could not run
/// on Linux, because Linux refuses one argument of more than 131,072 bytes.
/// `tests/issue_task_cli.rs` creates a task of 120,000 bytes on every system.
/// # Panics
/// Panics if a `gos` step refuses.
fn a_task_of_one_megabyte() {
    let temp = tempfile::tempdir().unwrap();
    let mut table = Table::new("task with a description of 1 MB");
    let place = place(&temp.path().join("run"), "gos", None);
    let description = "word ".repeat(200_000);
    must(&mut gos(
        &place,
        &[
            "v",
            "init",
            "--date-override",
            "2026-11-20T00:00:00",
            place.repo.to_str().unwrap(),
        ],
    ));
    must(&mut gos_v(
        &place,
        &["open", "--root", place.root.to_str().unwrap()],
    ));
    must(&mut gos(&place, &["i", "init"]));
    let run = measure(&mut gos(
        &place,
        &["i", "task", "create", "Large", "-d", &description, "--json"],
    ));
    table.add("gos", "task create", &run);
    let created: serde_json::Value = serde_json::from_str(&passed(&run, "create")).unwrap();
    let id = created["id"].as_str().unwrap().to_owned();
    let run = measure(&mut gos(&place, &["i", "task", "show", &id[..8], "--json"]));
    table.add("gos", "task show", &run);
    assert!(passed(&run, "show").contains(&description.trim_end().to_owned()[..1000]));
    let run = measure(&mut gos(&place, &["i", "task", "list", "--json"]));
    table.add("gos", "task list", &run);
    passed(&run, "list");
    let repo = place.repo.to_str().unwrap().to_owned();
    let run = measure(&mut gos(&place, &["v", "verify", "--repository", &repo]));
    table.add("gos", "verify", &run);
    passed(&run, "verify");
    if let Some(binary) = fossil_binary() {
        let f = self::place(&temp.path().join("run"), "fossil", None);
        let repo = f.repo.to_str().unwrap().to_owned();
        must(&mut fossil(&binary, &f, &["init", &repo]));
        must(&mut fossil(&binary, &f, &["open", "--force", &repo]));
        let run = measure(&mut fossil(
            &binary,
            &f,
            &["ticket", "add", "title", "Large", "comment", &description],
        ));
        table.add("fossil", "ticket add", &run);
        let run = measure(&mut fossil(&binary, &f, &["ticket", "show", "0"]));
        table.add("fossil", "ticket show", &run);
    }
    table.note("git", "all", "no issue tracker");
    table.print();
}

#[test]
#[ignore = "capacity run of a minute; run just limits-capacity"]
/// A commit and a `task list` that start while another connection holds the write lock
/// for 20 seconds wait and succeed. Each waits one busy timeout of 15 seconds, and then
/// the retry waits for the rest. Fossil and Git meet the same held lock.
/// # Panics
/// Panics if the `gos` commit refuses.
fn a_commit_behind_a_lock_held_for_twenty_seconds() {
    let temp = tempfile::tempdir().unwrap();
    let mut table = Table::new("write lock held for 20 s");
    let base = temp.path().join("run");
    let tree = temp.path().join("tree");
    std::fs::create_dir(&tree).unwrap();
    std::fs::write(tree.join("a.txt"), b"a\n").unwrap();
    let place = place(&base, "gos", Some(&tree));
    gos_track(&place, &mut table);
    let hold = |repo: &Path| {
        let repo = repo.to_owned();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            let database = runtime
                .block_on(libsql::Builder::new_local(&repo).build())
                .unwrap();
            let holder = database.connect().unwrap();
            runtime
                .block_on(holder.execute("BEGIN IMMEDIATE", ()))
                .unwrap();
            std::thread::sleep(Duration::from_secs(20));
            runtime.block_on(holder.execute("ROLLBACK", ())).unwrap();
        })
    };
    let holder = hold(&place.repo);
    std::thread::sleep(Duration::from_millis(500));
    let verb = commit_verb(21);
    let verb: Vec<&str> = verb.iter().map(String::as_str).collect();
    let run = measure(&mut gos_v(&place, &verb));
    table.add("gos", "commit", &run);
    holder.join().unwrap();
    passed(&run, "gos commit behind the lock");
    must(&mut gos(&place, &["i", "init"]));
    let holder = hold(&place.repo);
    std::thread::sleep(Duration::from_millis(500));
    let run = measure(&mut gos(&place, &["i", "task", "list"]));
    table.add("gos", "task list", &run);
    holder.join().unwrap();
    passed(&run, "gos task list behind the lock");
    if let Some(binary) = fossil_binary() {
        let f = self::place(&base, "fossil", Some(&tree));
        let repo = f.repo.to_str().unwrap().to_owned();
        must(&mut fossil(&binary, &f, &["init", &repo]));
        must(&mut fossil(&binary, &f, &["open", "--force", &repo]));
        must(&mut fossil(&binary, &f, &["add", "a.txt"]));
        let holder = hold(&f.repo);
        std::thread::sleep(Duration::from_millis(500));
        let run = measure(&mut fossil(
            &binary,
            &f,
            &["commit", "--no-warnings", "-m", "save"],
        ));
        table.add("fossil", "commit", &run);
        holder.join().unwrap();
    }
    let g = self::place(&base, "git", Some(&tree));
    must(&mut git(&g, &["init", "-q"]));
    must(&mut git(&g, &["add", "-A"]));
    let lock = g.repo.join("index.lock");
    std::fs::write(&lock, b"").unwrap();
    let run = measure(&mut git(&g, &["commit", "-q", "-m", "save"]));
    table.add("git", "commit with index.lock present", &run);
    std::fs::remove_file(&lock).unwrap();
    table.print();
}
