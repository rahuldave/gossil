//! Fresh-process timeline/log contract using only already accepted fixture APIs.
//!
//! Snapshot/producer scaffolding adapts tests/native_recording.rs and
//! tests/vcs_recording_cli.rs (Gossil). Manifest decoding is an independent
//! test oracle; this is new slice coverage, not an inherited Fossil suite pass.
//!
//! ## Recommendations
//! Preserve the command-absent RED evidence from real init/open before CLI implementation.
#[path = "support/closed_stdout.rs"]
mod closed_stdout;
#[path = "support/shares.rs"]
mod shares;
use libsql::Value;
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

/// Read complete exact result values from a fixture database.
/// # Arguments
/// * `path` - Fixture repository.
/// * `sql` - Test-owned SELECT.
/// # Returns
/// Complete rows retaining native types and NULLs.
/// # Panics
/// Panics on fixture provider failure.
async fn rows(path: &Path, sql: &str) -> Vec<Vec<Value>> {
    let db = libsql::Builder::new_local(path).build().await.unwrap();
    let conn = db.connect().unwrap();
    let mut result = conn.query(sql, ()).await.unwrap();
    let mut all = Vec::new();
    while let Some(row) = result.next().await.unwrap() {
        let mut cells = Vec::new();
        for i in 0..row.column_count() {
            cells.push(row.get_value(i).unwrap());
        }
        all.push(cells);
    }
    all
}
/// Capture every table and exact value for rollback and NoChange controls.
/// # Arguments
/// * `path` - Fixture repository.
/// # Returns
/// Complete native/local catalog rows, deterministically sorted per table.
/// # Panics
/// Panics on fixture provider or unexpected schema naming failures.
async fn snapshot(path: &Path) -> Vec<(String, Vec<Vec<Value>>)> {
    let mut all = vec![(
        "sqlite_schema".into(),
        rows(
            path,
            "SELECT type,name,tbl_name,rootpage,sql FROM sqlite_schema ORDER BY name",
        )
        .await,
    )];
    for row in rows(
        path,
        "SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name",
    )
    .await
    {
        let Value::Text(name) = &row[0] else {
            panic!("table name");
        };
        assert!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
        let mut values = rows(path, &format!("SELECT * FROM {name}")).await;
        values.sort_by_key(|row| format!("{row:?}"));
        all.push((name.clone(), values));
    }
    all
}
/// Apply test corruption through an independent connection.
/// # Arguments
/// * `path` - Fixture repository.
/// * `sql` - Test-owned mutation, never used by the producer.
/// # Returns
/// Unit after the independent mutation.
/// # Panics
/// Panics if fixture mutation fails.
async fn mutate(path: &Path, sql: &str) {
    let db = libsql::Builder::new_local(path).build().await.unwrap();
    db.connect().unwrap().execute(sql, ()).await.unwrap();
}

/// Inventory all working entries, including locator bytes, modes and symlink targets.
/// # Arguments
/// * `root` - Root whose complete relative inventory is required.
/// # Returns
/// Sorted exact entries, with directories distinguished from regular files.
/// # Panics
/// Panics on fixture filesystem errors or unsupported file kinds.
fn tree(root: &Path) -> Vec<(PathBuf, u32, Vec<u8>)> {
    /// Append every descendant entry in deterministic relative-path order.
    /// # Arguments
    /// * `root` - Stable fixture root used to form relative names.
    /// * `here` - Directory whose immediate entries are read before recursion.
    /// * `out` - Complete inventory accumulator, extended with modes and exact values.
    /// # Returns
    /// Unit after appending every descendant of `here` to `out`.
    /// # Panics
    /// Panics on fixture filesystem errors, paths outside `root` or unsupported kinds.
    fn walk(root: &Path, here: &Path, out: &mut Vec<(PathBuf, u32, Vec<u8>)>) {
        let mut children: Vec<_> = std::fs::read_dir(here)
            .unwrap()
            .map(Result::unwrap)
            .collect();
        children.sort_by_key(|entry| entry.file_name());
        for entry in children {
            let path = entry.path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            let value = if meta.is_dir() {
                b"directory".to_vec()
            } else if meta.is_file() {
                std::fs::read(&path).unwrap()
            } else if meta.file_type().is_symlink() {
                std::fs::read_link(&path)
                    .unwrap()
                    .as_os_str()
                    .as_encoded_bytes()
                    .to_vec()
            } else {
                panic!("unsupported fixture entry")
            };
            out.push((
                path.strip_prefix(root).unwrap().into(),
                meta.permissions().mode(),
                value,
            ));
            if meta.is_dir() {
                walk(root, &path, out);
            }
        }
    }
    let mut out = vec![(
        PathBuf::new(),
        std::fs::metadata(root).unwrap().permissions().mode(),
        b"directory".to_vec(),
    )];
    walk(root, root, &mut out);
    out
}

/// Read a producer's native full manifest without any timeline/parser adapter.
/// # Arguments
/// * `repo` - Native repository filename.
/// * `hash` - Independently obtained full native identity.
/// # Returns
/// Exact decompressed manifest bytes after independent size/digest checks.
/// # Panics
/// Panics on SQL, compression, identity or full-content fixture failures.
async fn manifest(repo: &Path, hash: &str) -> Vec<u8> {
    use std::io::Read;
    let db = libsql::Builder::new_local(repo).build().await.unwrap();
    let connection = db.connect().unwrap();
    let mut rows = connection.query("SELECT size,content,EXISTS(SELECT 1 FROM delta WHERE delta.rid=blob.rid) FROM blob WHERE uuid=?1", libsql::params![hash]).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    assert_eq!(
        row.get::<i64>(2).unwrap(),
        0,
        "fixture must contain full manifests"
    );
    let size = row.get::<i64>(0).unwrap();
    let packed = row.get::<Vec<u8>>(1).unwrap();
    // Local libSQL rows share a statement: materialize all values before stepping.
    assert!(rows.next().await.unwrap().is_none());
    assert_eq!(
        u32::from_be_bytes(packed[..4].try_into().unwrap()) as i64,
        size
    );
    let mut bytes = Vec::new();
    flate2::read::ZlibDecoder::new(&packed[4..])
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes.len() as i64, size);
    let digest = match hash.len() {
        40 => gossil::content_hash::sha1(&bytes).to_vec(),
        64 => gossil::content_hash::sha3(&bytes, 256).unwrap(),
        _ => panic!("full native hash required"),
    };
    assert_eq!(gossil::content_hash::hex(&digest), hash);
    bytes
}

/// Decode one canonical native card with a small independent fixture oracle.
/// # Arguments
/// * `bytes` - Complete independently decompressed native manifest.
/// * `card` - One single-valued card letter, such as D, U or C.
/// # Returns
/// Exact decoded native token, without date or Unicode normalization.
/// # Panics
/// Panics for absent/duplicate cards or unsupported fixture escape sequences.
fn card(bytes: &[u8], card: u8) -> Vec<u8> {
    let matches: Vec<_> = bytes
        .split(|b| *b == b'\n')
        .filter(|line| line.first() == Some(&card) && line.get(1) == Some(&b' '))
        .collect();
    assert_eq!(matches.len(), 1);
    let mut out = Vec::new();
    let mut token = matches[0][2..].iter().copied();
    while let Some(byte) = token.next() {
        if byte != b'\\' {
            out.push(byte);
            continue;
        }
        out.push(match token.next().unwrap() {
            b's' => b' ',
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'v' => 11,
            b'f' => 12,
            b'\\' => b'\\',
            other => panic!("unexpected native escape {other}"),
        });
    }
    out
}

/// Read exactly one native hash through SQL rather than a command receipt.
/// # Arguments
/// * `repo` - Fixture database.
/// * `sql` - Test-owned singleton identity selection.
/// # Returns
/// Full stored native identity.
/// # Panics
/// Panics on SQL failure or non-singleton/non-text results.
async fn identity(repo: &Path, sql: &str) -> String {
    let values = rows(repo, sql).await;
    let [row] = values.as_slice() else {
        panic!("one identity required")
    };
    let [Value::Text(hash)] = row.as_slice() else {
        panic!("text hash required")
    };
    hash.clone()
}
/// Construct a fresh product process with isolated home/config and no Gest selectors.
/// # Arguments
/// * `binary` - Explicit product executable.
/// * `home` - Disposable home/config/data directory.
/// # Returns
/// A command with no inherited Gest environment preferences.
fn command(binary: &str, home: &Path) -> Command {
    let mut command = Command::new(binary);
    for (key, _) in std::env::vars_os() {
        if key.to_str().is_some_and(|key| key.starts_with("GEST_")) {
            command.env_remove(key);
        }
    }
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("XDG_DATA_HOME", home);
    command
}

/// Run a fresh process and retain both output streams for exact assertions.
/// # Arguments
/// * `command` - Fully selected fixture product invocation.
/// # Returns
/// Completed process status and captured output streams.
/// # Panics
/// Panics if the fixture executable cannot start or finish.
fn run(command: &mut Command) -> Output {
    command.output().unwrap()
}

/// Require setup to succeed before exercising timeline, including the command-absent baseline control.
/// # Arguments
/// * `output` - Actual accepted producer command result.
/// # Panics
/// Panics with complete diagnostics if setup fails; this is never timeline RED.
fn setup_ok(output: Output) {
    assert!(
        output.status.success(),
        "fixture setup failed: stdout={:?}, stderr={:?}",
        output.stdout,
        output.stderr
    );
}

/// Run independent asynchronous SQL assertions from a synchronous process test.
/// # Arguments
/// * `future` - Fixture-owned SQL operation to complete.
/// # Returns
/// The operation result on a fresh single-threaded runtime.
/// # Panics
/// Panics if runtime creation or the supplied assertion fails.
fn sql<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

#[test]
/// Initialize/open a real checkout, then demand exact root NDJSON without mutation.
///
/// On the accepted baseline, successful init/open and native metadata assertions
/// must precede the actual command-absent runtime failure. This test deliberately
/// uses no proposed reader API, so absent library exports cannot mask that RED.
/// # Panics
/// Panics on setup failure, missing command, incorrect output or any state change.
fn initial_timeline_reports_root_and_preserves_state() {
    let binary = env!("CARGO_BIN_EXE_gossil");
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let root = home.join("root");
    let repo = home.join("repo");
    std::fs::create_dir(&root).unwrap();
    setup_ok(run(command(binary, home)
        .args([
            "vcs",
            "init",
            "--admin-user",
            "Timeline Owner",
            "--date-override",
            "2026-11-20T00:00:00",
        ])
        .arg(&repo)));
    setup_ok(run(command(binary, home)
        .current_dir(&root)
        .args(["vcs", "open", "--repository"])
        .arg(&repo)
        .args(["--checkout", "work", "--root"])
        .arg(&root)));
    let hash = sql(identity(&repo, "SELECT native_hash FROM g_public_receipt"));
    let native = sql(manifest(&repo, &hash));
    assert_eq!(card(&native, b'D'), b"2026-11-20T00:00:00.000");
    assert_eq!(card(&native, b'U'), b"Timeline Owner");
    assert_eq!(card(&native, b'C'), b"initial empty check-in");
    let prefix = b"C initial\\sempty\\scheck-in\nD 2026-11-20T00:00:00.000\nR d41d8cd98f00b204e9800998ecf8427e\nT *branch * trunk\nT *sym-trunk *\nU Timeline\\sOwner\n";
    let mut expected_native = prefix.to_vec();
    expected_native.extend_from_slice(
        format!(
            "Z {}\n",
            gossil::content_hash::hex(&gossil::artifact_checksum::md5(prefix))
        )
        .as_bytes(),
    );
    assert_eq!(native, expected_native);
    let expected = format!("{{\"hash\":\"{hash}\",\"date\":\"2026-11-20T00:00:00.000\",\"actor\":\"Timeline Owner\",\"comment\":\"initial empty check-in\",\"private\":false}}\n").into_bytes();
    let before = sql(snapshot(&repo));
    let files = tree(&root);
    assert!(root.join(".gossil/checkout.json").is_file());
    let result = run(command(binary, home)
        .current_dir(&root)
        .args(["vcs", "timeline", "--repository"])
        .arg(&repo)
        .args(["--checkout", "work"]));
    // Assert preservation before success so a command refusal still checks all surfaces.
    assert_eq!(sql(snapshot(&repo)), before);
    assert_eq!(tree(&root), files);
    assert!(
        result.status.success(),
        "timeline failed after real init/open: status={:?}, stdout={:?}, stderr={:?}",
        result.status.code(),
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.is_empty(),
        "unexpected successful stderr: {:?}",
        result.stderr
    );
    assert_eq!(result.stdout, expected);
}

/// Own a real accepted init/open checkout through all fresh-process assertions.
struct Fixture {
    /// Disposable home kept alive throughout the test.
    temp: tempfile::TempDir,
    /// Explicit native repository filename.
    repo: PathBuf,
    /// Registered working directory containing the existing owner locator.
    root: PathBuf,
}
impl Fixture {
    /// Initialize/open using only already accepted product commands.
    /// # Arguments
    /// * `actor` - Literal UTF-8 native root actor.
    /// # Returns
    /// Isolated ready checkout with no tracked files.
    /// # Panics
    /// Panics when accepted fixture setup fails.
    fn new(actor: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let binary = env!("CARGO_BIN_EXE_gossil");
        setup_ok(run(command(binary, temp.path())
            .args([
                "vcs",
                "init",
                "--admin-user",
                actor,
                "--date-override",
                "2026-11-20T00:00:00",
            ])
            .arg(&repo)));
        std::fs::set_permissions(&repo, std::fs::Permissions::from_mode(0o600)).unwrap();
        setup_ok(run(command(binary, temp.path())
            .current_dir(&root)
            .args(["vcs", "open", "--repository"])
            .arg(&repo)
            .args(["--checkout", "work", "--root"])
            .arg(&root)));
        Self { temp, repo, root }
    }
    /// Build one fully explicit operation using the existing locator.
    /// # Arguments
    /// * `binary` - Selected executable alias.
    /// * `group` - Selected vcs/v alias.
    /// * `verb` - Selected operation.
    /// # Returns
    /// Command selecting the same explicit repository and checkout.
    fn operation(&self, binary: &str, group: &str, verb: &str) -> Command {
        let mut result = command(binary, self.temp.path());
        result
            .current_dir(&self.root)
            .args([group, verb, "--repository"])
            .arg(&self.repo)
            .args(["--checkout", "work"]);
        result
    }
    /// Add and explicitly scope one real regular file through accepted commands.
    /// # Arguments
    /// * `path` - Exact fixture relative path.
    /// * `bytes` - Initial working bytes.
    /// # Panics
    /// Panics on filesystem or producer command failure.
    fn add_private(&self, path: &str, bytes: &[u8]) {
        std::fs::write(self.root.join(path), bytes).unwrap();
        let binary = env!("CARGO_BIN_EXE_gossil");
        setup_ok(run(self
            .operation(binary, "vcs", "add")
            .args(["--path", path])));
        setup_ok(run(self
            .operation(binary, "vcs", "scope")
            .args(["--path", path, "--scope", "private"])));
    }
    /// Commit actual changed content with literal native metadata.
    /// # Arguments
    /// * `actor` - Exact U-card input.
    /// * `comment` - Exact nonempty C-card input supported by the accepted CLI producer.
    /// * `date` - Strictly later native D-card token.
    /// # Returns
    /// Full stored native identity independently selected through SQL.
    /// # Panics
    /// Panics when the accepted producer fails or returns an unexpected head.
    fn commit(&self, actor: &str, comment: &str, date: &str) -> String {
        setup_ok(run(self
            .operation(env!("CARGO_BIN_EXE_gossil"), "vcs", "commit")
            .args([
                "--scope",
                "private",
                "--all-tracked",
                "--user",
                actor,
                "--comment",
                comment,
                "--date-override",
                date,
            ])));
        let hash = sql(identity(
            &self.repo,
            "SELECT combined_head FROM g_checkout WHERE checkout_id='work'",
        ));
        let native = sql(manifest(&self.repo, &hash));
        assert_eq!(card(&native, b'D'), date.as_bytes());
        assert_eq!(card(&native, b'U'), actor.as_bytes());
        assert_eq!(card(&native, b'C'), comment.as_bytes());
        hash
    }
}

/// Construct exact ordered compact NDJSON from independently known native facts.
/// # Arguments
/// * `hash` - Independently authenticated full native identity.
/// * `date` - Literal native date token.
/// * `actor` - Literal decoded native user.
/// * `comment` - Literal decoded native comment.
/// * `private` - Expected native visibility.
/// # Returns
/// One compact row with exact key order and final LF, using standard JSON strings.
/// # Panics
/// Panics if a literal JSON string cannot be encoded.
fn ndjson(hash: &str, date: &str, actor: &str, comment: &str, private: bool) -> String {
    format!(
        "{{\"hash\":{},\"date\":{},\"actor\":{},\"comment\":{},\"private\":{private}}}\n",
        serde_json::to_string(hash).unwrap(),
        serde_json::to_string(date).unwrap(),
        serde_json::to_string(actor).unwrap(),
        serde_json::to_string(comment).unwrap()
    )
}

/// Require complete exact successful output and empty diagnostic stream.
/// # Arguments
/// * `output` - Fresh product process result.
/// * `expected` - Independently constructed complete compact NDJSON.
/// # Panics
/// Panics for failure, any extra output or byte mismatch.
fn assert_output(output: Output, expected: &str) {
    assert!(
        output.status.success(),
        "status {:?}: {:?}",
        output.status.code(),
        output.stderr
    );
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    assert_eq!(output.stdout, expected.as_bytes());
}

#[test]
/// Preserve complete native ancestry and escaped metadata across all eight aliases.
/// # Panics
/// Panics on missing/extra ancestry, metadata normalization, alias mismatch or mutation.
fn all_aliases_report_complete_native_history_after_dirty_deleted_pending_and_reopen() {
    let f = Fixture::new("Róot 雪");
    let shared = sql(identity(
        &f.repo,
        "SELECT native_hash FROM g_public_receipt",
    ));
    let root_native = sql(manifest(&f.repo, &shared));
    assert_eq!(card(&root_native, b'U'), "Róot 雪".as_bytes());
    assert_eq!(card(&root_native, b'D'), b"2026-11-20T00:00:00.000");
    assert_eq!(card(&root_native, b'C'), b"initial empty check-in");
    f.add_private("first", b"first recorded bytes");
    f.add_private("deleted", b"later missing on disk");
    let first = f.commit("First 雪", ".", "2026-11-20T00:00:01");
    std::fs::write(f.root.join("first"), b"second recorded bytes").unwrap();
    let actor = "Zoë 雪 \"actor\" \\ \n\t";
    let comment = "quoted \"雪\" \\ newline\ncarriage\r tab\t café";
    let second = f.commit(actor, comment, "2026-11-20T00:00:02");
    // Parent cards are native ancestry evidence independent of timeline ordering.
    let parent = card(&sql(manifest(&f.repo, &second)), b'P');
    assert_eq!(parent, format!("{first} {shared}").as_bytes());
    let parent = card(&sql(manifest(&f.repo, &first)), b'P');
    assert_eq!(parent, shared.as_bytes());
    std::fs::write(f.root.join("pending"), b"unrecorded pending addition").unwrap();
    setup_ok(run(f
        .operation(env!("CARGO_BIN_EXE_gossil"), "vcs", "add")
        .args(["--path", "pending"])));
    std::fs::write(f.root.join("first"), b"dirty unrelated value").unwrap();
    std::fs::remove_file(f.root.join("deleted")).unwrap();
    // Reopen existing registration without publishing or reconstructing a locator.
    setup_ok(run(&mut f.operation(
        env!("CARGO_BIN_EXE_gossil"),
        "vcs",
        "open",
    )));
    let expected = ndjson(&second, "2026-11-20T00:00:02", actor, comment, true)
        + &ndjson(&first, "2026-11-20T00:00:01", "First 雪", ".", true)
        + &ndjson(
            &shared,
            "2026-11-20T00:00:00.000",
            "Róot 雪",
            "initial empty check-in",
            false,
        );
    let before = sql(snapshot(&f.repo));
    let files = tree(&f.root);
    for binary in [env!("CARGO_BIN_EXE_gossil"), env!("CARGO_BIN_EXE_gos")] {
        for group in ["vcs", "v"] {
            for verb in ["timeline", "log"] {
                assert_output(run(&mut f.operation(binary, group, verb)), &expected);
                assert_eq!(sql(snapshot(&f.repo)), before);
                assert_eq!(tree(&f.root), files);
            }
        }
    }
}

#[test]
/// Help needs no storage, while unsupported/missing/duplicate selections refuse.
/// # Panics
/// Panics when help touches storage, invalid grammar succeeds or refusal mutates state.
fn strict_selectors_grammar_and_storage_free_help() {
    let temp = tempfile::tempdir().unwrap();
    for binary in [env!("CARGO_BIN_EXE_gossil"), env!("CARGO_BIN_EXE_gos")] {
        for group in ["vcs", "v"] {
            for verb in ["timeline", "log"] {
                for flag in ["--help", "-h"] {
                    let before = tree(temp.path());
                    let output = run(command(binary, temp.path())
                        .current_dir(temp.path())
                        .args([group, verb, flag]));
                    assert!(output.status.success(), "{:?}", output.stderr);
                    assert!(output.stderr.is_empty());
                    let help = String::from_utf8(output.stdout).unwrap();
                    assert!(
                        help.contains("--repository") && help.contains("--checkout"),
                        "{help}"
                    );
                    assert_eq!(tree(temp.path()), before);
                }
            }
        }
    }
    let f = Fixture::new("Owner");
    let repo = f.repo.to_str().unwrap();
    let cases: Vec<Vec<&str>> = vec![
        vec![],
        vec!["--repository", repo],
        vec!["--checkout", "work"],
        vec!["--repository"],
        vec!["--checkout"],
        vec![
            "--repository",
            repo,
            "--repository",
            repo,
            "--checkout",
            "work",
        ],
        vec![
            "--repository",
            repo,
            "--checkout",
            "work",
            "--checkout",
            "work",
        ],
        vec!["--repository", repo, "--checkout", "missing"],
        vec!["--repository", repo, "--checkout", ""],
        vec!["--repository", repo, "--checkout", "work", "unexpected"],
        vec!["--repository", repo, "--checkout", "work", "--limit", "1"],
        vec!["--repository", repo, "--checkout", "work", "--root", "."],
        vec![
            "--repository",
            repo,
            "--checkout",
            "work",
            "--scope",
            "private",
        ],
        vec![
            "--repository",
            repo,
            "--checkout",
            "work",
            "--date",
            "2026-11-20",
        ],
    ];
    let before = sql(snapshot(&f.repo));
    let files = tree(&f.root);
    for verb in ["timeline", "log"] {
        for flags in &cases {
            let output = run(command(env!("CARGO_BIN_EXE_gossil"), f.temp.path())
                .current_dir(&f.root)
                .args(["vcs", verb])
                .args(flags));
            assert_eq!(output.status.code(), Some(1), "{verb} {flags:?}");
            assert!(
                output.stdout.is_empty(),
                "{verb} {flags:?}: {:?}",
                output.stdout
            );
            assert!(!output.stderr.is_empty());
            assert_eq!(sql(snapshot(&f.repo)), before);
            assert_eq!(tree(&f.root), files);
        }
    }
}

/// The number of tests that split the 24 metadata and authority corruptions.
const METADATA_PARTS: usize = 6;

/// Reject forged original metadata and unsupported display amendments without repair, for one share.
/// # Arguments
/// * `part` - The share of the corruptions, below `METADATA_PARTS`. Each corruption gets its own fixture.
/// # Panics
/// Panics if invalid native event columns are displayed, repaired or masked by ownership.
fn metadata_types_nuls_tampering_amendments_and_authority_refuse(part: usize) {
    let mut corruptions = Vec::new();
    for objid in ["objid=1", "objid<>1"] {
        for column in ["user", "comment"] {
            for value in ["NULL", "x'ff'", "char(65,0,66)", "'forged metadata'"] {
                corruptions.push(format!("UPDATE event SET {column}={value} WHERE {objid}"));
            }
        }
        for column in ["euser", "ecomment"] {
            corruptions.push(format!(
                "UPDATE event SET {column}='amended display' WHERE {objid}"
            ));
        }
    }
    corruptions.extend([
        "UPDATE g_checkout SET generation=generation+1".into(),
        "DELETE FROM private".into(),
        "UPDATE plink SET isprim=0".into(),
        "INSERT INTO phantom VALUES(999)".into(),
    ]);
    for mutation in shares::share(corruptions, part, METADATA_PARTS) {
        let f = Fixture::new("Owner");
        f.add_private("first", b"recorded bytes");
        f.commit("Owner", "recorded comment", "2026-11-20T00:00:01");
        assert_eq!(
            std::fs::metadata(&f.repo).unwrap().permissions().mode() & 0o777,
            0o600
        );
        sql(mutate(&f.repo, &mutation));
        let before = sql(snapshot(&f.repo));
        let files = tree(&f.root);
        for verb in ["timeline", "log"] {
            let output = run(&mut f.operation(env!("CARGO_BIN_EXE_gossil"), "vcs", verb));
            assert_eq!(output.status.code(), Some(1), "admitted {mutation}");
            assert!(output.stdout.is_empty(), "partial history for {mutation}");
            let diagnostic = String::from_utf8(output.stderr).unwrap();
            let expected_guard = if mutation.contains("event") {
                if mutation.ends_with("objid=1") {
                    "root event"
                } else {
                    "projection"
                }
            } else if mutation.contains("generation") {
                "generation"
            } else if mutation.contains("phantom") {
                "phantom"
            } else {
                "projection"
            };
            assert!(
                diagnostic.contains(expected_guard),
                "wrong refusal for {mutation}: {diagnostic}"
            );
            assert_eq!(sql(snapshot(&f.repo)), before, "repaired {mutation}");
            assert_eq!(tree(&f.root), files);
        }
    }
}

#[test]
/// Share 0 of the metadata and authority corruptions.
/// # Panics
/// Panics as the shared body does.
fn metadata_types_nuls_tampering_amendments_and_authority_refuse_0() {
    metadata_types_nuls_tampering_amendments_and_authority_refuse(0);
}

#[test]
/// Share 1 of the metadata and authority corruptions.
/// # Panics
/// Panics as the shared body does.
fn metadata_types_nuls_tampering_amendments_and_authority_refuse_1() {
    metadata_types_nuls_tampering_amendments_and_authority_refuse(1);
}

#[test]
/// Share 2 of the metadata and authority corruptions.
/// # Panics
/// Panics as the shared body does.
fn metadata_types_nuls_tampering_amendments_and_authority_refuse_2() {
    metadata_types_nuls_tampering_amendments_and_authority_refuse(2);
}

#[test]
/// Share 3 of the metadata and authority corruptions.
/// # Panics
/// Panics as the shared body does.
fn metadata_types_nuls_tampering_amendments_and_authority_refuse_3() {
    metadata_types_nuls_tampering_amendments_and_authority_refuse(3);
}

#[test]
/// Share 4 of the metadata and authority corruptions.
/// # Panics
/// Panics as the shared body does.
fn metadata_types_nuls_tampering_amendments_and_authority_refuse_4() {
    metadata_types_nuls_tampering_amendments_and_authority_refuse(4);
}

#[test]
/// Share 5 of the metadata and authority corruptions.
/// # Panics
/// Panics as the shared body does.
fn metadata_types_nuls_tampering_amendments_and_authority_refuse_5() {
    metadata_types_nuls_tampering_amendments_and_authority_refuse(5);
}

#[test]
/// Surface the original stdout I/O cause after checked read completion without mutation.
/// # Panics
/// Panics if a closed pipe succeeds, loses its cause or changes durable/working state.
fn closed_stdout_retains_io_cause_and_preserves_existing_commits() {
    let f = Fixture::new("Owner");
    f.add_private("first", b"recorded bytes");
    let head = f.commit("Owner", "durable before read", "2026-11-20T00:00:01");
    let shared = sql(identity(
        &f.repo,
        "SELECT native_hash FROM g_public_receipt",
    ));
    let expected = ndjson(
        &head,
        "2026-11-20T00:00:01",
        "Owner",
        "durable before read",
        true,
    ) + &ndjson(
        &shared,
        "2026-11-20T00:00:00.000",
        "Owner",
        "initial empty check-in",
        false,
    );
    let before = sql(snapshot(&f.repo));
    let files = tree(&f.root);
    let output =
        closed_stdout::configure(f.operation(env!("CARGO_BIN_EXE_gossil"), "vcs", "timeline"))
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
            .wait_with_output()
            .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(
        error.to_ascii_lowercase().contains("broken pipe"),
        "original I/O cause missing: {error}"
    );
    assert_eq!(sql(snapshot(&f.repo)), before);
    assert_eq!(tree(&f.root), files);
    assert_output(
        run(&mut f.operation(env!("CARGO_BIN_EXE_gos"), "v", "log")),
        &expected,
    );
}

#[test]
/// Preserve the accepted CLI producer's nonempty-comment boundary without recording.
/// # Panics
/// Panics if an empty comment is accepted, fails for another cause or changes state.
fn empty_comment_is_a_producer_refusal_without_mutation() {
    let f = Fixture::new("Owner");
    f.add_private("first", b"candidate recorded bytes");
    let before = sql(snapshot(&f.repo));
    let files = tree(&f.root);
    let output = run(f
        .operation(env!("CARGO_BIN_EXE_gossil"), "vcs", "commit")
        .args([
            "--scope",
            "private",
            "--all-tracked",
            "--user",
            "Owner",
            "--comment",
            "",
            "--date-override",
            "2026-11-20T00:00:01",
        ]));
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert!(
        diagnostic.contains("recording comment limit"),
        "{diagnostic}"
    );
    assert_eq!(sql(snapshot(&f.repo)), before);
    assert_eq!(tree(&f.root), files);
}

#[test]
/// Require the existing matching locator and refuse foreign invocation roots without publication.
/// # Panics
/// Panics if history infers/rebuilds a locator, accepts conflicting selectors or mutates state.
fn locator_selection_failures_preserve_all_existing_state() {
    let f = Fixture::new("Owner");
    let other = Fixture::new("Other");
    let binary = env!("CARGO_BIN_EXE_gossil");
    let before = sql(snapshot(&f.repo));
    let other_before = sql(snapshot(&other.repo));
    let files = tree(&f.root);
    let other_files = tree(&other.root);
    let foreign = f.temp.path().join("foreign");
    std::fs::create_dir(&foreign).unwrap();
    let foreign_before = tree(&foreign);
    for verb in ["timeline", "log"] {
        let wrong_repo = run(command(binary, f.temp.path())
            .current_dir(&f.root)
            .args(["vcs", verb, "--repository"])
            .arg(&other.repo)
            .args(["--checkout", "work"]));
        let wrong_checkout = run(command(binary, f.temp.path())
            .current_dir(&f.root)
            .args(["vcs", verb, "--repository"])
            .arg(&f.repo)
            .args(["--checkout", "other"]));
        let foreign_root = run(command(binary, f.temp.path())
            .current_dir(&foreign)
            .args(["vcs", verb, "--repository"])
            .arg(&f.repo)
            .args(["--checkout", "work"]));
        for output in [wrong_repo, wrong_checkout, foreign_root] {
            assert_eq!(output.status.code(), Some(1));
            assert!(output.stdout.is_empty());
            let diagnostic = String::from_utf8(output.stderr).unwrap();
            assert!(diagnostic.contains("locator"), "{diagnostic}");
        }
        assert_eq!(sql(snapshot(&f.repo)), before);
        assert_eq!(sql(snapshot(&other.repo)), other_before);
        assert_eq!(tree(&f.root), files);
        assert_eq!(tree(&other.root), other_files);
        assert_eq!(tree(&foreign), foreign_before);
    }
    let marker = f.root.join(".gossil/checkout.json");
    let saved = f.root.join(".gossil/checkout.saved");
    std::fs::rename(&marker, &saved).unwrap();
    let missing_files = tree(&f.root);
    for verb in ["timeline", "log"] {
        let output = run(&mut f.operation(binary, "vcs", verb));
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let diagnostic = String::from_utf8(output.stderr).unwrap();
        assert!(diagnostic.contains("locator"), "{diagnostic}");
        assert_eq!(sql(snapshot(&f.repo)), before);
        assert_eq!(tree(&f.root), missing_files);
        assert!(
            !marker.exists(),
            "history must not republish a missing locator"
        );
    }
}
