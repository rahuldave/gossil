//! Public consumer controls for complete, bounded recorded checkout history.
//!
//! Fixture/snapshot scaffolding adapts tests/native_recording.rs (Gossil).
//! Native manifest/card oracles are independent of the proposed reader.
//! These controls cover the new reader; they do not qualify the inherited Fossil suite.
//!
//! ## Recommendations
//! Run compile-absence and semantic controls separately; retain the checks of open and of the timeline.
use gossil::artifact::ParseLimits;
use gossil::storage::{
    AdmissionLimits, CheckoutView, InitializeOptions, Limits, LocalOwnerRepository, PathScope,
    PathScopeSelection, PrivateRecordingOutcome, PrivateRecordingRequest, RecordedCheckin,
    StorageError, TrackingLimits,
};
use libsql::Value;
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

#[path = "support/proof.rs"]
mod proof;
#[path = "support/shares.rs"]
mod shares;

/// Supply generous finite cumulative admission and operation ceilings.
/// # Arguments
/// No arguments.
/// # Returns
/// Explicit bounded native and parser work.
/// # Errors
/// Cannot fail.
fn limits() -> AdmissionLimits {
    AdmissionLimits {
        content: Limits {
            max_stored_bytes: 1 << 20,
            max_output_bytes: 1 << 20,
            max_chain_depth: 128,
            max_total_stored_bytes: 64 << 20,
            max_total_decoded_bytes: 256 << 20,
        },
        parser: ParseLimits {
            input_bytes: 4 << 20,
            cards: 65536,
            owned_bytes: 16 << 20,
            entries: 8192,
        },
    }
}
/// Supply complete descriptor/path bounds.
/// # Arguments
/// No arguments.
/// # Returns
/// Finite tracking limits.
/// # Errors
/// Cannot fail.
fn tracking() -> TrackingLimits {
    TrackingLimits {
        paths: 128,
        path_bytes: 4096,
        components: 32,
        retained_bytes: 4 << 20,
        io_buffer_bytes: 1024,
    }
}

/// Retain a canonical working root and owned initialized repository fixture.
struct Fixture {
    /// Owns the disposable directory through every reopen assertion.
    _temp: tempfile::TempDir,
    /// Fixture native database filename.
    repo: PathBuf,
    /// Canonical retained working directory.
    root: PathBuf,
    /// Admitted local owner for public operation calls.
    owner: LocalOwnerRepository,
    /// Complete expected public checkout snapshot.
    view: CheckoutView,
}
impl Fixture {
    /// Initialize and enroll two regular/binary paths without hidden scope selection.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// An owned initialized and tracked fixture.
    /// # Panics
    /// Panics on fixture setup failure.
    async fn new() -> Self {
        Self::new_user(b"Owner").await
    }
    /// Initialize the same accepted fixture with an explicit native root actor.
    /// # Arguments
    /// * `user` - Exact UTF-8 root U-card input.
    /// # Returns
    /// Admitted tracked fixture retaining the requested actor bytes.
    /// # Panics
    /// Panics if accepted fixture initialization fails.
    async fn new_user(user: &[u8]) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        std::fs::write(root.join("a"), b"a1").unwrap();
        std::fs::write(root.join("b"), b"\0\xff\n").unwrap();
        let b = limits();
        let mut owner = LocalOwnerRepository::initialize(
            &repo,
            InitializeOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "owner",
                date: "2026-11-20T00:00:00",
                user,
                password: "secret",
                sha1: false,
                content_limits: b.content,
                parse_limits: b.parser,
            },
        )
        .await
        .unwrap();
        std::fs::set_permissions(&repo, std::fs::Permissions::from_mode(0o600)).unwrap();
        let view = owner.register_checkout("work", &root, b).await.unwrap();
        let view = owner
            .add_files("work", &["b", "a"], &view, b, tracking())
            .await
            .unwrap();
        Self {
            _temp: temp,
            repo,
            root,
            owner,
            view,
        }
    }
    /// Select explicit immutable private scope for each currently tracked fixture path.
    /// # Arguments
    /// * `paths` - Exact paths to select.
    /// # Returns
    /// Updates the retained expected checkout view.
    /// # Panics
    /// Panics on fixture setup failure.
    async fn private(&mut self, paths: &[&str]) {
        let selections: Vec<_> = paths
            .iter()
            .map(|path| PathScopeSelection {
                path: (*path).into(),
                scope: PathScope::Private,
            })
            .collect();
        self.view = self
            .owner
            .select_path_scopes("work", &self.view, &selections, limits(), tracking())
            .await
            .unwrap();
    }
    /// Invoke the stable recording request with explicit all-tracked private intent.
    /// # Arguments
    /// * `paths` - Exact duplicate-free complete tracked set.
    /// * `date` - Explicit native date token.
    /// * `comment` - Decoded bounded bracket-free comment.
    /// # Returns
    /// The recorder result without suppressing failures.
    /// # Errors
    /// Propagates any recording refusal.
    async fn record(
        &mut self,
        paths: &[&str],
        date: &[u8],
        comment: &[u8],
    ) -> Result<PrivateRecordingOutcome, StorageError> {
        self.owner
            .commit_private_tracked_files(
                "work",
                PrivateRecordingRequest {
                    expected: &self.view,
                    paths,
                    scope: PathScope::Private,
                    actor: b"Owner",
                    comment,
                    date,
                },
                limits(),
                tracking(),
            )
            .await
    }
    /// Persist a successful fixture recording view.
    /// # Arguments
    /// * `paths` - Complete tracked set.
    /// * `date` - Strictly later native date.
    /// # Returns
    /// Actual manifest name.
    /// # Panics
    /// Panics on failure or unexpected NoChange.
    async fn changed(&mut self, paths: &[&str], date: &[u8]) -> String {
        match self.record(paths, date, b"fixture").await.unwrap() {
            PrivateRecordingOutcome::Recorded { view, native_name } => {
                self.view = view;
                native_name
            }
            PrivateRecordingOutcome::NoChange { .. } => panic!("expected changed commit"),
        }
    }
}

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
/// Compare all public fields without obtaining expected values from the reader.
/// # Arguments
/// * `row` - Public API result under test.
/// * `hash` - Independently authenticated native full identity.
/// * `date` - Literal expected native D-card bytes.
/// * `actor` - Literal expected decoded U-card bytes.
/// * `comment` - Literal expected decoded C-card bytes.
/// * `private` - Expected visibility of this ancestor.
/// # Panics
/// Panics when any field differs or the native identity is abbreviated.
fn assert_row(
    row: &RecordedCheckin,
    hash: &str,
    date: &[u8],
    actor: &[u8],
    comment: &[u8],
    private: bool,
) {
    assert_eq!(row.hash, hash);
    assert!(matches!(row.hash.len(), 40 | 64));
    assert_eq!(row.date, date);
    assert_eq!(row.actor, actor);
    assert_eq!(row.comment, comment);
    assert_eq!(row.private, private);
}

#[tokio::test]
/// Return the real shared root even when working paths are pending additions.
/// # Panics
/// Panics on fixture failure, metadata mismatch, synthetic empty history or mutation.
async fn root_only_is_one_exact_native_checkin_and_preserves_every_surface() {
    let mut f = Fixture::new().await;
    let root = identity(&f.repo, "SELECT native_hash FROM g_public_receipt").await;
    let bytes = manifest(&f.repo, &root).await;
    assert_eq!(card(&bytes, b'D'), b"2026-11-20T00:00:00.000");
    assert_eq!(card(&bytes, b'U'), b"Owner");
    assert_eq!(card(&bytes, b'C'), b"initial empty check-in");
    let before = snapshot(&f.repo).await;
    let files = tree(&f.root);
    let result: Vec<RecordedCheckin> = f.owner.recorded_timeline("work", limits()).await.unwrap();
    assert_eq!(result.len(), 1);
    assert_row(
        &result[0],
        &root,
        b"2026-11-20T00:00:00.000",
        b"Owner",
        b"initial empty check-in",
        false,
    );
    assert_eq!(snapshot(&f.repo).await, before);
    assert_eq!(tree(&f.root), files);
}

#[tokio::test]
/// Keep full ancestry current first despite edits, deleted files, pending paths and reopen.
/// # Panics
/// Panics if history reads working files, drops ancestors, changes bytes or mutates state.
async fn full_ancestry_metadata_and_order_survive_dirty_deleted_pending_and_reopen() {
    let mut f = Fixture::new().await;
    f.private(&["a", "b"]).await;
    let shared = identity(&f.repo, "SELECT native_hash FROM g_public_receipt").await;
    let first = f.changed(&["a", "b"], b"2026-11-20T00:00:01").await;
    std::fs::write(f.root.join("a"), b"second recorded value").unwrap();
    let actor = "Zoë 雪 \"actor\" \\ \n\t";
    let comment = "quoted \"雪\" \\ newline\ncarriage\r tab\t café";
    let outcome = f
        .owner
        .commit_private_tracked_files(
            "work",
            PrivateRecordingRequest {
                expected: &f.view,
                paths: &["a", "b"],
                scope: PathScope::Private,
                actor: actor.as_bytes(),
                comment: comment.as_bytes(),
                date: b"2026-11-20T00:00:02",
            },
            limits(),
            tracking(),
        )
        .await
        .unwrap();
    let second = match outcome {
        PrivateRecordingOutcome::Recorded { view, native_name } => {
            f.view = view;
            native_name
        }
        PrivateRecordingOutcome::NoChange { .. } => panic!("second real commit required"),
    };
    for (hash, date, expected_actor, expected_comment) in [
        (
            &second,
            b"2026-11-20T00:00:02".as_slice(),
            actor.as_bytes(),
            comment.as_bytes(),
        ),
        (
            &first,
            b"2026-11-20T00:00:01".as_slice(),
            b"Owner".as_slice(),
            b"fixture".as_slice(),
        ),
        (
            &shared,
            b"2026-11-20T00:00:00.000".as_slice(),
            b"Owner".as_slice(),
            b"initial empty check-in".as_slice(),
        ),
    ] {
        let native = manifest(&f.repo, hash).await;
        assert_eq!(card(&native, b'D'), date);
        assert_eq!(card(&native, b'U'), expected_actor);
        assert_eq!(card(&native, b'C'), expected_comment);
    }
    std::fs::write(f.root.join("a"), b"dirty unrecorded value").unwrap();
    std::fs::remove_file(f.root.join("b")).unwrap();
    std::fs::write(f.root.join("pending"), b"pending unrecorded").unwrap();
    f.owner
        .add_files("work", &["pending"], &f.view, limits(), tracking())
        .await
        .unwrap();
    drop(f.owner);
    let mut owner = LocalOwnerRepository::open(&f.repo, limits()).await.unwrap();
    let before = snapshot(&f.repo).await;
    let files = tree(&f.root);
    let result = owner.recorded_timeline("work", limits()).await.unwrap();
    assert_eq!(result.len(), 3);
    assert_row(
        &result[0],
        &second,
        b"2026-11-20T00:00:02",
        actor.as_bytes(),
        comment.as_bytes(),
        true,
    );
    assert_row(
        &result[1],
        &first,
        b"2026-11-20T00:00:01",
        b"Owner",
        b"fixture",
        true,
    );
    assert_row(
        &result[2],
        &shared,
        b"2026-11-20T00:00:00.000",
        b"Owner",
        b"initial empty check-in",
        false,
    );
    assert_eq!(snapshot(&f.repo).await, before);
    assert_eq!(tree(&f.root), files);
}

#[tokio::test]
/// Reject absent selections and invalid finite limits rather than returning empty history.
/// # Panics
/// Panics if invalid requests succeed or change any persisted/working surface.
async fn missing_checkout_and_invalid_limits_refuse_without_mutation() {
    let mut f = Fixture::new().await;
    let before = snapshot(&f.repo).await;
    let files = tree(&f.root);
    for checkout in ["", "missing"] {
        assert!(f.owner.recorded_timeline(checkout, limits()).await.is_err());
    }
    let mut invalid = limits();
    invalid.content.max_chain_depth = 0;
    assert!(f.owner.recorded_timeline("work", invalid).await.is_err());
    let mut invalid = limits();
    invalid.parser.cards = 0;
    assert!(f.owner.recorded_timeline("work", invalid).await.is_err());
    assert_eq!(snapshot(&f.repo).await, before);
    assert_eq!(tree(&f.root), files);
    assert_eq!(
        f.owner
            .recorded_timeline("work", limits())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
/// Preserve accepted chronology refusal for equal and backwards recording dates.
/// # Panics
/// Panics if unsupported chronology is admitted or refusal damages existing ancestry.
async fn equal_and_backward_dates_are_refusals_not_sorting_fixtures() {
    let mut f = Fixture::new().await;
    f.private(&["a", "b"]).await;
    let first = f.changed(&["a", "b"], b"2026-11-20T00:00:01").await;
    std::fs::write(f.root.join("a"), b"attempted changed capture").unwrap();
    let before = snapshot(&f.repo).await;
    for date in [
        b"2026-11-20T00:00:01".as_slice(),
        b"2026-11-20T00:00:00".as_slice(),
    ] {
        assert!(
            f.record(&["a", "b"], date, b"refused chronology")
                .await
                .is_err()
        );
        assert_eq!(snapshot(&f.repo).await, before);
        let result = f.owner.recorded_timeline("work", limits()).await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].hash, first);
    }
}

#[tokio::test]
/// Refuse projection changes in the full proof and never repair corruption on a read.
///
/// Another connection makes each change while the owner is open. Open admits each
/// change but the generation. The timeline refuses a changed generation and a forged
/// comment, and the full proof refuses every change.
/// # Panics
/// Panics if corruption is repaired or the full proof admits it.
async fn corrupt_surfaces_refuse_verify_without_repair() {
    for (sql, expected) in [
        (
            "UPDATE event SET comment='forged' WHERE objid=1",
            "root event differs from canonical bytes",
        ),
        (
            "UPDATE g_checkout SET generation=generation+1",
            "unsupported recording checkout state",
        ),
        ("DELETE FROM private", "recording native projection differs"),
        (
            "UPDATE plink SET isprim=0",
            "recording native projection differs",
        ),
        (
            "INSERT INTO phantom VALUES(999)",
            "unsupported nonempty catalog6 table: phantom",
        ),
    ] {
        let mut f = Fixture::new().await;
        f.private(&["a", "b"]).await;
        f.changed(&["a", "b"], b"2026-11-20T00:00:01").await;
        assert_eq!(
            std::fs::metadata(&f.repo).unwrap().permissions().mode() & 0o777,
            0o600
        );
        mutate(&f.repo, sql).await;
        let before = snapshot(&f.repo).await;
        let files = tree(&f.root);
        let timeline = f.owner.recorded_timeline("work", limits()).await;
        // Every command reads the generation and the pending version of the checkout,
        // so open and the timeline refuse a generation that the pending version cannot
        // follow.
        if sql.contains("generation") {
            assert_eq!(timeline.unwrap_err().operation, expected, "{sql}");
        } else if sql.contains("comment") {
            let refused = timeline.unwrap_err().operation;
            assert_eq!(refused, "recorded event metadata differs from its manifest");
        }
        assert_eq!(snapshot(&f.repo).await, before, "{sql}");
        assert_eq!(tree(&f.root), files);
        drop(f.owner);
        let (opened, refusal) = proof::verify_refusal(&f.repo, limits()).await;
        let admitted = !sql.contains("generation");
        assert_eq!((opened, refusal.as_str()), (admitted, expected), "{sql}");
        assert_eq!(snapshot(&f.repo).await, before);
    }
}

/// Select one cumulative public admission ceiling without changing per-object limits.
/// # Arguments
/// * `limits` - Generous operation limits to modify.
/// * `axis` - Stored, decoded, parser input, cards, owned bytes or entries.
/// * `value` - Candidate finite cumulative ceiling.
/// # Panics
/// Panics for a non-test axis or a value that cannot fit the parser platform type.
fn ceiling(limits: &mut AdmissionLimits, axis: usize, value: u64) {
    match axis {
        0 => limits.content.max_total_stored_bytes = value,
        1 => limits.content.max_total_decoded_bytes = value,
        2 => limits.parser.input_bytes = usize::try_from(value).unwrap(),
        3 => limits.parser.cards = usize::try_from(value).unwrap(),
        4 => limits.parser.owned_bytes = usize::try_from(value).unwrap(),
        5 => limits.parser.entries = usize::try_from(value).unwrap(),
        _ => panic!("unknown cumulative ceiling"),
    }
}

/// The number of tests that split the six cumulative ceilings, one ceiling each.
const CUMULATIVE_PARTS: usize = 6;

/// Find the boundary of each ceiling in one share, then prove exact success and one-under refusal.
///
/// The reader gives each manifest its own parser ceilings and uses no total
/// content ceiling. So each parser boundary is that of the largest single
/// manifest, below the sum over the chain, and any total content ceiling passes.
/// # Arguments
/// * `part` - The share of the six ceilings, below `CUMULATIVE_PARTS`, on its own fixture.
/// # Panics
/// Panics if a parser ceiling covers the whole chain, a total content ceiling is
/// used, a failed read mutates state, or the smallest passing ceiling also accepts
/// its predecessor.
async fn cumulative_content_and_parser_limits_have_complete_history_boundaries(part: usize) {
    let mut f = Fixture::new().await;
    f.private(&["a", "b"]).await;
    f.changed(&["a", "b"], b"2026-11-20T00:00:01").await;
    std::fs::write(f.root.join("a"), b"next").unwrap();
    f.changed(&["a", "b"], b"2026-11-20T00:00:02").await;
    drop(f.owner);
    let before = snapshot(&f.repo).await;
    let files = tree(&f.root);
    let (mut native_bytes, mut largest_bytes) = (0_u64, 0_u64);
    let (mut native_cards, mut largest_cards) = (0_u64, 0_u64);
    for row in rows(
        &f.repo,
        "SELECT uuid FROM blob JOIN event ON event.objid=blob.rid WHERE event.type='ci'",
    )
    .await
    {
        let [Value::Text(hash)] = row.as_slice() else {
            panic!("native identity required")
        };
        let bytes = manifest(&f.repo, hash).await;
        let cards = bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .count() as u64;
        native_bytes += bytes.len() as u64;
        native_cards += cards;
        largest_bytes = largest_bytes.max(bytes.len() as u64);
        largest_cards = largest_cards.max(cards);
    }
    for axis in shares::share((0..6).collect(), part, CUMULATIVE_PARTS) {
        if axis < 2 {
            // The total content ceilings are not used.
            let mut zero = limits();
            ceiling(&mut zero, axis, 0);
            let mut owner = LocalOwnerRepository::open(&f.repo, limits()).await.unwrap();
            assert_eq!(
                owner.recorded_timeline("work", zero).await.unwrap().len(),
                3
            );
            assert_eq!(snapshot(&f.repo).await, before);
            continue;
        }
        let mut low = 0_u64;
        let mut high = 256 << 20;
        while high - low > 1 {
            let midpoint = low + (high - low) / 2;
            let mut bound = limits();
            ceiling(&mut bound, axis, midpoint);
            let mut owner = LocalOwnerRepository::open(&f.repo, limits()).await.unwrap();
            if owner.recorded_timeline("work", bound).await.is_ok() {
                high = midpoint;
            } else {
                low = midpoint;
            }
        }
        // Each manifest has its own ceilings, so the boundary is that of the largest
        // manifest, not the sum over the chain.
        if axis == 2 {
            assert!(high <= largest_bytes && high < native_bytes, "{high}");
        }
        if axis == 3 {
            assert!(high <= largest_cards && high < native_cards, "{high}");
        }
        let mut exact = limits();
        ceiling(&mut exact, axis, high);
        let mut owner = LocalOwnerRepository::open(&f.repo, limits()).await.unwrap();
        assert_eq!(
            owner.recorded_timeline("work", exact).await.unwrap().len(),
            3
        );
        ceiling(&mut exact, axis, high - 1);
        assert!(
            owner.recorded_timeline("work", exact).await.is_err(),
            "axis {axis}"
        );
        assert_eq!(snapshot(&f.repo).await, before);
        assert_eq!(tree(&f.root), files);
    }
}

#[tokio::test]
/// Share 0 of the cumulative ceilings, for total stored bytes.
/// # Panics
/// Panics as the shared body does.
async fn cumulative_content_and_parser_limits_have_complete_history_boundaries_0() {
    cumulative_content_and_parser_limits_have_complete_history_boundaries(0).await;
}

#[tokio::test]
/// Share 1 of the cumulative ceilings, for total decoded bytes.
/// # Panics
/// Panics as the shared body does.
async fn cumulative_content_and_parser_limits_have_complete_history_boundaries_1() {
    cumulative_content_and_parser_limits_have_complete_history_boundaries(1).await;
}

#[tokio::test]
/// Share 2 of the cumulative ceilings, for parser input bytes.
/// # Panics
/// Panics as the shared body does.
async fn cumulative_content_and_parser_limits_have_complete_history_boundaries_2() {
    cumulative_content_and_parser_limits_have_complete_history_boundaries(2).await;
}

#[tokio::test]
/// Share 3 of the cumulative ceilings, for parser cards.
/// # Panics
/// Panics as the shared body does.
async fn cumulative_content_and_parser_limits_have_complete_history_boundaries_3() {
    cumulative_content_and_parser_limits_have_complete_history_boundaries(3).await;
}

#[tokio::test]
/// Share 4 of the cumulative ceilings, for parser owned bytes.
/// # Panics
/// Panics as the shared body does.
async fn cumulative_content_and_parser_limits_have_complete_history_boundaries_4() {
    cumulative_content_and_parser_limits_have_complete_history_boundaries(4).await;
}

#[tokio::test]
/// Share 5 of the cumulative ceilings, for parser entries.
/// # Panics
/// Panics as the shared body does.
async fn cumulative_content_and_parser_limits_have_complete_history_boundaries_5() {
    cumulative_content_and_parser_limits_have_complete_history_boundaries(5).await;
}

#[tokio::test]
/// Preserve complete Unicode root actor bytes and a one-byte private comment.
/// # Panics
/// Panics on normalization, synthetic metadata, ancestry loss or read mutation.
async fn unicode_root_actor_and_short_private_comment_remain_exact() {
    let mut f = Fixture::new_user("Róot 雪".as_bytes()).await;
    let shared = identity(&f.repo, "SELECT native_hash FROM g_public_receipt").await;
    assert_eq!(
        card(&manifest(&f.repo, &shared).await, b'U'),
        "Róot 雪".as_bytes()
    );
    f.private(&["a", "b"]).await;
    let head = match f
        .record(&["a", "b"], b"2026-11-20T00:00:01", b".")
        .await
        .unwrap()
    {
        PrivateRecordingOutcome::Recorded { native_name, .. } => native_name,
        PrivateRecordingOutcome::NoChange { .. } => panic!("real private commit required"),
    };
    assert_eq!(card(&manifest(&f.repo, &head).await, b'C'), b".");
    let before = snapshot(&f.repo).await;
    let files = tree(&f.root);
    let result = f.owner.recorded_timeline("work", limits()).await.unwrap();
    assert_eq!(result.len(), 2);
    assert_row(
        &result[0],
        &head,
        b"2026-11-20T00:00:01",
        b"Owner",
        b".",
        true,
    );
    assert_row(
        &result[1],
        &shared,
        b"2026-11-20T00:00:00.000",
        "Róot 雪".as_bytes(),
        b"initial empty check-in",
        false,
    );
    assert_eq!(snapshot(&f.repo).await, before);
    assert_eq!(tree(&f.root), files);
}

/// The number of tests that split the 20 original event mutations.
const EVENT_PARTS: usize = 4;

/// Refuse original event NULL/BLOB/NUL/tampering and unsupported amended columns, for one share.
///
/// Open admits each mutation, and the full proof of `gos v verify` refuses it. The
/// timeline refuses a user or comment that is not text.
/// # Arguments
/// * `part` - The share of the mutations, below `EVENT_PARTS`. Each mutation gets its own fixture.
/// # Panics
/// Panics if the full proof admits root or private metadata, or if a refusal changes a byte.
async fn original_event_metadata_and_amendments_cannot_bypass_native_validation(part: usize) {
    let mut mutations = Vec::new();
    for selection in ["objid=1", "objid<>1"] {
        for column in ["user", "comment"] {
            for value in ["NULL", "x'ff'", "char(65,0,66)", "'forged metadata'"] {
                mutations.push(format!(
                    "UPDATE event SET {column}={value} WHERE {selection}"
                ));
            }
        }
        for column in ["euser", "ecomment"] {
            mutations.push(format!(
                "UPDATE event SET {column}='amended display' WHERE {selection}"
            ));
        }
    }
    for mutation in shares::share(mutations, part, EVENT_PARTS) {
        let mut f = Fixture::new().await;
        f.private(&["a", "b"]).await;
        f.changed(&["a", "b"], b"2026-11-20T00:00:01").await;
        assert_eq!(
            std::fs::metadata(&f.repo).unwrap().permissions().mode() & 0o777,
            0o600
        );
        mutate(&f.repo, &mutation).await;
        let before = snapshot(&f.repo).await;
        let files = tree(&f.root);
        let timeline = f.owner.recorded_timeline("work", limits()).await;
        // The timeline prints the user and the comment, so it refuses a value that is
        // not text, and text that is not the card of the manifest. It prints no
        // amendment, so it trusts `euser` and `ecomment`, which only the full proof checks.
        let printed = if mutation.contains("NULL") || mutation.contains("x'ff'") {
            Some("recorded event metadata must be original text")
        } else if mutation.contains("SET user=") || mutation.contains("SET comment=") {
            Some("recorded event metadata differs from its manifest")
        } else {
            None
        };
        match printed {
            Some(message) => assert_eq!(timeline.unwrap_err().operation, message, "{mutation}"),
            None => assert!(timeline.is_ok(), "{mutation}"),
        }
        assert_eq!(snapshot(&f.repo).await, before);
        assert_eq!(tree(&f.root), files);
        drop(f.owner);
        let (opened, refusal) = proof::verify_refusal(&f.repo, limits()).await;
        let expected = if mutation.ends_with("objid=1") {
            "root event differs from canonical bytes"
        } else {
            "recording native projection differs"
        };
        assert_eq!((opened, refusal.as_str()), (true, expected), "{mutation}");
        assert_eq!(snapshot(&f.repo).await, before);
    }
}

#[tokio::test]
/// Share 0 of the original event mutations.
/// # Panics
/// Panics as the shared body does.
async fn original_event_metadata_and_amendments_cannot_bypass_native_validation_0() {
    original_event_metadata_and_amendments_cannot_bypass_native_validation(0).await;
}

#[tokio::test]
/// Share 1 of the original event mutations.
/// # Panics
/// Panics as the shared body does.
async fn original_event_metadata_and_amendments_cannot_bypass_native_validation_1() {
    original_event_metadata_and_amendments_cannot_bypass_native_validation(1).await;
}

#[tokio::test]
/// Share 2 of the original event mutations.
/// # Panics
/// Panics as the shared body does.
async fn original_event_metadata_and_amendments_cannot_bypass_native_validation_2() {
    original_event_metadata_and_amendments_cannot_bypass_native_validation(2).await;
}

#[tokio::test]
/// Share 3 of the original event mutations.
/// # Panics
/// Panics as the shared body does.
async fn original_event_metadata_and_amendments_cannot_bypass_native_validation_3() {
    original_event_metadata_and_amendments_cannot_bypass_native_validation(3).await;
}
