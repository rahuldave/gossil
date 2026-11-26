//! Check that `timeline` and `log` print pages of a long history.
//!
//! Before the fix, both commands parsed the whole chain under one parser ledger of
//! 4,096 cards, and refused a chain of 455 one-file check-ins, or 75 check-ins of
//! 50 files. A debug build makes about seven one-file check-ins a second, so the
//! ordinary tests cannot reach 455 quickly. One test gives the reader parser
//! ceilings that fit about one manifest, so that a short chain is far past what a
//! whole-chain ledger of those ceilings admits. Another checks the pages through
//! the CLI. The fixture
//! makes the repository with `gos v init` and `gos v open`, then commits through
//! the public library writer, which is faster than one process for each check-in.
//! The commands under test run as fresh processes with the ceilings of the CLI. The
//! ignored test makes 5,000 check-ins and prints the time of each command.
//! `just timeline-capacity` runs it in a release build.
use gossil::storage::{
    AdmissionLimits, Limits, LocalOwnerRepository, PathScope, PrivateRecordingOutcome,
    PrivateRecordingRequest, TrackingLimits,
};
use libsql::Value;
use std::path::Path;
use std::process::Output;
use std::time::Instant;

/// The paths that one `add` or `scope` of the fixture names.
const BATCH: usize = 25;
/// The one-file check-ins of the CLI page test, past two default pages.
const CHECKINS: usize = 45;

/// Give the ceilings of one `gos v` command, as `vcs_open::limits` sets them.
/// # Returns
/// The content and parser ceilings of the CLI.
fn cli_limits() -> AdmissionLimits {
    AdmissionLimits {
        content: Limits {
            max_stored_bytes: 1 << 20,
            max_output_bytes: 1 << 20,
            max_chain_depth: 128,
            max_total_stored_bytes: 64 << 20,
            max_total_decoded_bytes: 64 << 20,
        },
        parser: gossil::artifact::ParseLimits {
            input_bytes: 1 << 20,
            cards: 4096,
            owned_bytes: 8 << 20,
            entries: 4096,
        },
    }
}

/// Give the tracking ceilings of `gos v commit`.
/// # Returns
/// The path and read ceilings of the CLI.
fn tracking() -> TrackingLimits {
    TrackingLimits {
        paths: 4096,
        path_bytes: 4096,
        components: 128,
        retained_bytes: 64 << 20,
        io_buffer_bytes: 65536,
    }
}

/// Run `gos` in the fixture home, with the checkout root `root` as working directory.
/// # Arguments
/// * `binary` - The executable.
/// * `home` - The fixture home.
/// * `args` - The arguments after the executable.
/// # Returns
/// The finished process output.
fn run(binary: &str, home: &Path, args: &[&str]) -> Output {
    let mut command = std::process::Command::new(binary);
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
    run(env!("CARGO_BIN_EXE_gos"), home, &args)
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

/// Make a repository, open the checkout `work`, and track `files` private files.
/// # Arguments
/// * `home` - An empty fixture home.
/// * `files` - The count of tracked files.
/// # Returns
/// The names of the tracked files.
fn fixture(home: &Path, files: usize) -> Vec<String> {
    let root = home.join("root");
    std::fs::create_dir(&root).unwrap();
    ok(run(
        env!("CARGO_BIN_EXE_gos"),
        home,
        &[
            "v",
            "init",
            "--date-override",
            "2026-11-20T00:00:00",
            home.join("repo").to_str().unwrap(),
        ],
    ));
    let root = root.canonicalize().unwrap();
    ok(vcs(home, &["open", "--root", root.to_str().unwrap()]));
    let names: Vec<String> = (0..files).map(|n| format!("file{n:03}")).collect();
    for batch in names.chunks(BATCH) {
        let mut add = vec!["add"];
        let mut scope = vec!["scope", "--scope", "private"];
        for name in batch {
            std::fs::write(root.join(name), "content").unwrap();
            add.extend(["--path", name]);
            scope.extend(["--path", name]);
        }
        ok(vcs(home, &add));
        ok(vcs(home, &scope));
    }
    names
}

/// Spell the date of check-in `n`, one second after the one before it.
/// # Arguments
/// * `n` - The number of the check-in, below 86,400.
/// # Returns
/// A native date on 2026-11-21.
fn date(n: usize) -> String {
    format!(
        "2026-11-21T{:02}:{:02}:{:02}",
        n / 3600,
        n / 60 % 60,
        n % 60
    )
}

/// Commit check-ins `from` to `to`, not included, each changing the first file.
/// # Arguments
/// * `home` - The fixture home.
/// * `names` - Every tracked file.
/// * `from` - The number of the first check-in.
/// * `to` - The number after the last check-in.
async fn commit(home: &Path, names: &[String], from: usize, to: usize) {
    let mut owner = LocalOwnerRepository::open(home.join("repo"), cli_limits())
        .await
        .unwrap();
    let paths: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut view = owner.checkout_view("work", cli_limits()).await.unwrap();
    for n in from..to {
        std::fs::write(home.join("root").join(&names[0]), format!("content {n}")).unwrap();
        let outcome = owner
            .commit_private_tracked_files(
                "work",
                PrivateRecordingRequest {
                    expected: &view,
                    paths: &paths,
                    scope: PathScope::Private,
                    actor: b"tester",
                    comment: format!("commit {n}").as_bytes(),
                    date: date(n).as_bytes(),
                },
                cli_limits(),
                tracking(),
            )
            .await
            .unwrap_or_else(|e| panic!("check-in {n}: {e:?}"));
        let PrivateRecordingOutcome::Recorded { view: next, .. } = outcome else {
            panic!("check-in {n} recorded nothing");
        };
        view = next;
    }
}

/// The four executable and group forms.
/// # Returns
/// Each executable with each group name.
fn forms() -> [(&'static str, &'static str); 4] {
    [
        (env!("CARGO_BIN_EXE_gossil"), "vcs"),
        (env!("CARGO_BIN_EXE_gossil"), "v"),
        (env!("CARGO_BIN_EXE_gos"), "vcs"),
        (env!("CARGO_BIN_EXE_gos"), "v"),
    ]
}

/// Run `timeline` or `log` through one form, with page options.
/// # Arguments
/// * `home` - The fixture home.
/// * `form` - The executable and the group name.
/// * `verb` - `timeline` or `log`.
/// * `page` - The page options.
/// # Returns
/// The finished process output.
fn page(home: &Path, form: (&str, &str), verb: &str, page: &[&str]) -> Output {
    let repo = home.join("repo");
    let mut args = vec![
        form.1,
        verb,
        "--repository",
        repo.to_str().unwrap(),
        "--checkout",
        "work",
    ];
    args.extend_from_slice(page);
    run(form.0, home, &args)
}

/// Run `gos v timeline` with page options and split its lines.
/// # Arguments
/// * `home` - The fixture home.
/// * `options` - The page options.
/// # Returns
/// Each line of the output, with its LF.
fn lines(home: &Path, options: &[&str]) -> Vec<String> {
    let out = ok(page(home, forms()[3], "timeline", options));
    out.split_inclusive('\n').map(str::to_owned).collect()
}

/// Run `gos v verify` on the fixture repository.
/// # Arguments
/// * `home` - The fixture home.
/// # Returns
/// The finished process output.
fn verify(home: &Path) -> Output {
    let repo = home.join("repo");
    run(
        env!("CARGO_BIN_EXE_gos"),
        home,
        &["v", "verify", "--repository", repo.to_str().unwrap()],
    )
}

/// Read one row of a fixture repository through an independent connection.
/// # Arguments
/// * `home` - The fixture home.
/// * `sql` - A test-owned statement.
/// * `args` - Its bound values.
/// # Returns
/// Every row of the result.
async fn sql(home: &Path, sql: &str, args: Vec<Value>) -> Vec<Vec<Value>> {
    let db = libsql::Builder::new_local(home.join("repo"))
        .build()
        .await
        .unwrap();
    let conn = db.connect().unwrap();
    let mut rows = conn.query(sql, args).await.unwrap();
    let mut all = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        all.push(
            (0..row.column_count())
                .map(|i| row.get_value(i).unwrap())
                .collect(),
        );
    }
    all
}

/// Give the hash of one line of NDJSON output.
/// # Arguments
/// * `line` - One object of the output.
/// # Returns
/// The value of its `hash` key.
fn hash(line: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(line).unwrap();
    value["hash"].as_str().unwrap().to_owned()
}

/// Require a refusal with exit 1, no output and a diagnostic that holds `message`.
/// # Arguments
/// * `output` - The finished process output.
/// * `message` - The fixed refusal.
fn refused(output: Output, message: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(output.stdout.is_empty());
    assert!(stderr.contains(message), "{stderr}");
}

#[tokio::test]
/// Print pages of a history longer than two pages through every form, and keep the other commands working.
/// # Panics
/// Panics if a page differs from the matching lines of the whole chain, or a command refuses.
async fn a_long_history_prints_pages() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let names = fixture(home, 1);
    commit(home, &names, 0, CHECKINS).await;
    // The whole chain, newest first, ends with the shared root.
    let all = lines(home, &["--limit", "0"]);
    assert_eq!(all.len(), CHECKINS + 1);
    let head = sql(home, "SELECT combined_head FROM g_checkout", vec![]).await;
    assert_eq!(head, vec![vec![Value::Text(hash(&all[0]))]]);
    for (n, line) in all[..CHECKINS].iter().enumerate() {
        let comment = format!(
            "\"comment\":\"commit {}\",\"private\":true}}",
            CHECKINS - 1 - n
        );
        assert!(line.ends_with(&format!("{comment}\n")), "{line}");
    }
    assert!(all[CHECKINS].ends_with("\"private\":false}\n"));
    // The default page is the newest 20 entries, the same through each form and verb.
    let newest = all[..20].concat();
    for form in forms() {
        for verb in ["timeline", "log"] {
            assert_eq!(ok(page(home, form, verb, &[])), newest);
            assert_eq!(ok(page(home, form, verb, &["-n", "0"])), all.concat());
        }
    }
    assert_eq!(lines(home, &["-n", "5", "--offset", "3"]), all[3..8]);
    assert_eq!(lines(home, &["--offset", "3", "--limit", "5"]), all[3..8]);
    assert_eq!(lines(home, &["-n", "-4"]), all[..4]);
    assert_eq!(lines(home, &["--offset", "20"]), all[20..40]);
    let last = CHECKINS.to_string();
    assert_eq!(lines(home, &["--offset", &last]), all[CHECKINS..]);
    let before = (CHECKINS - 1).to_string();
    assert_eq!(
        lines(home, &["--offset", &before, "-n", "5"]),
        all[CHECKINS - 1..]
    );
    assert_eq!(
        lines(home, &["--offset", &before, "-n", "0"]),
        all[CHECKINS - 1..]
    );
    let past = (CHECKINS + 1).to_string();
    assert!(lines(home, &["--offset", &past]).is_empty());
    // The other commands still work on the same history.
    ok(vcs(home, &["status"]));
    std::fs::write(home.join("root").join(&names[0]), "one more").unwrap();
    ok(vcs(
        home,
        &[
            "commit",
            "--scope",
            "private",
            "--all-tracked",
            "--user",
            "tester",
            "--comment",
            "one more",
            "--date-override",
            "2026-11-22T00:00:00",
        ],
    ));
    let next = lines(home, &["-n", "2"]);
    assert!(next[0].contains("\"comment\":\"one more\""));
    assert_eq!(next[1], all[0]);
    ok(verify(home));
}

#[tokio::test]
/// Read a chain far past what one whole-chain ledger of the same ceilings admits.
///
/// The parser ceilings fit about two one-file manifests, and the total content
/// ceilings fit almost nothing. Before the fix, the reader charged every manifest
/// to one ledger of these ceilings and refused the chain. Now each manifest has
/// its own ceilings, and the totals are not used.
/// # Panics
/// Panics if the reader refuses the chain or a page differs from the whole chain.
async fn each_manifest_has_its_own_parser_ceilings() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let names = fixture(home, 1);
    commit(home, &names, 0, 30).await;
    let mut tight = cli_limits();
    tight.parser.cards = 20;
    tight.parser.entries = 20;
    tight.content.max_total_stored_bytes = 1;
    tight.content.max_total_decoded_bytes = 1;
    let mut owner = LocalOwnerRepository::open(home.join("repo"), cli_limits())
        .await
        .unwrap();
    let all = owner.recorded_timeline("work", tight).await.unwrap();
    assert_eq!(all.len(), 31);
    assert!(all[..30].iter().all(|row| row.private));
    assert!(!all[30].private);
    let page = owner
        .recorded_timeline_page("work", 28, 5, tight)
        .await
        .unwrap();
    assert_eq!(page, all[28..]);
    assert!(
        owner
            .recorded_timeline_page("work", 31, 0, tight)
            .await
            .unwrap()
            .is_empty()
    );
    // One manifest still has the ceilings of one artifact.
    tight.parser.cards = 3;
    let error = owner.recorded_timeline("work", tight).await.unwrap_err();
    assert_eq!(error.operation, "recording native artifact validation");
    // Each parser ceiling alone: the smallest value that admits the newest entry
    // also admits the whole chain, and one less refuses both.
    for axis in 0..4 {
        let set = |value: usize| {
            let mut limits = cli_limits();
            match axis {
                0 => limits.parser.input_bytes = value,
                1 => limits.parser.cards = value,
                2 => limits.parser.owned_bytes = value,
                _ => limits.parser.entries = value,
            }
            limits
        };
        let (mut low, mut high) = (0, 1 << 20);
        while high - low > 1 {
            let mid = low + (high - low) / 2;
            if owner
                .recorded_timeline_page("work", 0, 1, set(mid))
                .await
                .is_ok()
            {
                high = mid;
            } else {
                low = mid;
            }
        }
        assert_eq!(
            owner
                .recorded_timeline("work", set(high))
                .await
                .unwrap()
                .len(),
            31,
            "axis {axis}"
        );
        assert!(
            owner
                .recorded_timeline("work", set(high - 1))
                .await
                .is_err(),
            "axis {axis}"
        );
    }
}

#[tokio::test]
/// Refuse a bad manifest or event inside the page, and leave one beyond it to `verify`.
///
/// A page reads the manifests from the current check-in to its end, and the event
/// rows of the entries that it prints. It does not read past its end, and `gos v
/// verify` reads everything.
/// # Panics
/// Panics if a page reads past its end, admits a bad entry inside it, or `verify` admits either.
async fn a_page_checks_what_it_reads_and_verify_checks_the_rest() {
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let names = fixture(home, 1);
    commit(home, &names, 0, 30).await;
    let all = lines(home, &["-n", "0"]);
    // Entry 25, counted from 0 at the newest, is past the first 25 entries.
    let target = hash(&all[25]);
    let rows = sql(
        home,
        "SELECT content FROM blob WHERE uuid=?1",
        vec![Value::Text(target.clone())],
    )
    .await;
    let Value::Blob(original) = rows[0][0].clone() else {
        panic!("packed manifest");
    };
    // The same size, with one changed byte of the comment. The hash no longer matches.
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(
        &mut flate2::read::ZlibDecoder::new(&original[4..]),
        &mut bytes,
    )
    .unwrap();
    let at = bytes.windows(9).position(|w| w == b"commit\\s4").unwrap();
    bytes[at] = b'C';
    let mut packed = original[..4].to_vec();
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&bytes).unwrap();
    packed.extend(encoder.finish().unwrap());
    let write = |content: Vec<u8>| {
        let target = target.clone();
        async move {
            let db = libsql::Builder::new_local(home.join("repo"))
                .build()
                .await
                .unwrap();
            db.connect()
                .unwrap()
                .execute(
                    "UPDATE blob SET content=?1 WHERE uuid=?2",
                    vec![Value::Blob(content), Value::Text(target)],
                )
                .await
                .unwrap();
        }
    };
    write(packed).await;
    let manifest = "native content hash mismatch";
    assert_eq!(lines(home, &[]), all[..20]);
    assert_eq!(lines(home, &["-n", "25"]), all[..25]);
    assert_eq!(lines(home, &["--offset", "5", "-n", "20"]), all[5..25]);
    for options in [
        &["-n", "26"][..],
        &["-n", "0"],
        &["--offset", "25", "-n", "1"],
        &["--offset", "40"],
    ] {
        for verb in ["timeline", "log"] {
            refused(page(home, forms()[0], verb, options), manifest);
        }
    }
    let out = verify(home);
    assert_eq!(out.status.code(), Some(74));
    assert!(String::from_utf8_lossy(&out.stderr).contains(manifest));
    write(original).await;
    ok(verify(home));
    // A printed user or comment must be that of its manifest. One beyond the page is
    // not read.
    let events = "recorded event metadata differs from its manifest";
    for (entry, column) in [(3, "user"), (24, "comment")] {
        let set = format!(
            "UPDATE event SET {column}='forged' WHERE objid=(SELECT rid FROM blob WHERE uuid=?1)"
        );
        let undo = format!(
            "UPDATE event SET {column}=?2 WHERE objid=(SELECT rid FROM blob WHERE uuid=?1)"
        );
        let target = Value::Text(hash(&all[entry]));
        let saved = sql(
            home,
            &format!("SELECT {column} FROM event WHERE objid=(SELECT rid FROM blob WHERE uuid=?1)"),
            vec![target.clone()],
        )
        .await;
        sql(home, &set, vec![target.clone()]).await;
        refused(page(home, forms()[2], "log", &["-n", "25"]), events);
        refused(
            page(
                home,
                forms()[2],
                "log",
                &["--offset", &entry.to_string(), "-n", "1"],
            ),
            events,
        );
        assert_eq!(lines(home, &["-n", &entry.to_string()]), all[..entry]);
        let skip = (entry + 1).to_string();
        assert_eq!(
            lines(home, &["--offset", &skip, "-n", "2"]),
            all[entry + 1..entry + 3]
        );
        assert_eq!(verify(home).status.code(), Some(74));
        sql(home, &undo, vec![target, saved[0][0].clone()]).await;
    }
    ok(verify(home));
}

#[test]
/// Refuse a limit or an offset that is not an integer with exit 1, through every form.
/// # Panics
/// Panics if a bad value is admitted, prints output, or exits with another status.
fn a_bad_limit_or_offset_exits_1() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    fixture(home, 0);
    for form in forms() {
        for verb in ["timeline", "log"] {
            for (options, message) in [
                (
                    &["--limit", "x"][..],
                    "timeline --limit requires an integer",
                ),
                (&["-n", "1.5"], "timeline --limit requires an integer"),
                (&["-n", ""], "timeline --limit requires an integer"),
                (
                    &["--offset", "-1"],
                    "timeline --offset requires a nonnegative integer",
                ),
                (
                    &["--offset", "two"],
                    "timeline --offset requires a nonnegative integer",
                ),
                (&["--limit"], "timeline option requires a value"),
                (
                    &["-n", "1", "--limit", "2"],
                    "Usage: gossil vcs timeline|log",
                ),
                (&["--limit=3"], "Usage: gossil vcs timeline|log"),
            ] {
                refused(page(home, form, verb, options), message);
            }
            // Before the first check-in the root is the only entry.
            let root = ok(page(home, form, verb, &["-n", "1"]));
            assert!(root.ends_with("\"private\":false}\n"));
            assert_eq!(root.lines().count(), 1);
            assert_eq!(ok(page(home, form, verb, &["--offset", "1"])), "");
        }
    }
}

#[tokio::test]
#[ignore = "makes 5,000 check-ins; run it with just timeline-capacity"]
/// Make 5,000 one-file check-ins and print the time of each command at 1,000 and 5,000.
/// # Panics
/// Panics if a command refuses.
async fn five_thousand_checkins() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let names = fixture(home, 1);
    let mut done = 0;
    for count in [1000, 5000] {
        let start = Instant::now();
        commit(home, &names, done, count).await;
        println!("{count} check-ins: built in {:?}", start.elapsed());
        let timed = |label: &str, output: Output| {
            println!("{count} check-ins: {label}");
            ok(output)
        };
        for (label, options) in [
            ("timeline", &[][..]),
            ("timeline --limit 0", &["--limit", "0"]),
        ] {
            let start = Instant::now();
            let out = timed(label, page(home, forms()[3], "timeline", options));
            println!("  {:?}, {} lines", start.elapsed(), out.lines().count());
        }
        let start = Instant::now();
        timed("log", page(home, forms()[3], "log", &[]));
        println!("  {:?}", start.elapsed());
        let start = Instant::now();
        timed("status", vcs(home, &["status"]));
        println!("  {:?}", start.elapsed());
        std::fs::write(home.join("root").join(&names[0]), format!("cli {count}")).unwrap();
        let start = Instant::now();
        // The check-in of the CLI takes the next number, so later dates stay later.
        let date = date(count);
        done = count + 1;
        timed(
            "commit",
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
                    "cli",
                    "--date-override",
                    &date,
                ],
            ),
        );
        println!("  {:?}", start.elapsed());
        let start = Instant::now();
        timed("verify", verify(home));
        println!("  {:?}", start.elapsed());
    }
}
