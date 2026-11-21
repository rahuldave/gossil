//! Test explicit private recording of complete file trees.
//!
//! ## Recommendations
//! Run these controls with the accepted tracking base and complete private admission checks.
use gossil::artifact::ParseLimits;
use gossil::storage::{
    AdmissionLimits, CheckoutView, InitializeOptions, Limits, LocalOwnerRepository, PathScope,
    PathScopeSelection, PrivateRecordingOutcome, PrivateRecordingRequest, StorageError,
    TrackingLimits,
};
use libsql::Value;
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

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
                user: b"Owner",
                password: "secret",
                sha1: false,
                content_limits: b.content,
                parse_limits: b.parser,
            },
        )
        .await
        .unwrap();
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

#[tokio::test]
/// Three real recordings reproduce the native five/eight/three/four/eleven fixture.
/// # Arguments
/// No arguments.
/// # Returns
/// Unit on exact full-tree/parent/native projection equality.
/// # Panics
/// Panics on any contract regression.
async fn three_commits_exact_native_projection_and_reopen() {
    let mut f = Fixture::new().await;
    f.private(&["a", "b"]).await;
    let shared = f.view.shared_head.clone();
    let clock_before = fixture_date(&f.repo, "now").await;
    let one = f.changed(&["b", "a"], b"2026-11-20T00:00:01").await;
    std::fs::write(f.root.join("a"), b"a2").unwrap();
    let two = f.changed(&["a", "b"], b"2026-11-20T00:00:02").await;
    std::fs::set_permissions(f.root.join("b"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(f.root.join("c"), b"").unwrap();
    f.view = f
        .owner
        .add_files("work", &["c"], &f.view, limits(), tracking())
        .await
        .unwrap();
    f.private(&["c"]).await;
    let three = f.changed(&["c", "b", "a"], b"2026-11-20T00:00:03").await;
    assert_eq!(rows(&f.repo,"SELECT (SELECT count(*) FROM plink),(SELECT count(*) FROM mlink),(SELECT count(*) FROM filename),(SELECT count(*) FROM event),(SELECT count(*) FROM tagxref)").await,vec![vec![Value::Integer(5),Value::Integer(8),Value::Integer(3),Value::Integer(4),Value::Integer(11)]]);
    assert_eq!(
        rows(&f.repo, "SELECT count(*) FROM mlink WHERE pid=-1").await,
        vec![vec![Value::Integer(0)]]
    );
    assert_eq!(rows(&f.repo,"SELECT count(*) FROM mlink WHERE mid=(SELECT rid FROM blob ORDER BY rid DESC LIMIT 1) AND isaux=1 AND pid=0").await,vec![vec![Value::Integer(2)]]);
    for (name, parent) in [
        (&one, format!("P {shared}\n")),
        (&two, format!("P {one} {shared}\n")),
        (&three, format!("P {two} {shared}\n")),
    ] {
        let bytes = f.owner.read_content(name, limits().content).await.unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains(&parent));
        assert_eq!(text.lines().filter(|s| s.starts_with("T ")).count(), 3);
    }
    let clock_after = fixture_date(&f.repo, "now").await;
    assert_complete_three_tree_oracle(&f, [&one, &two, &three], clock_before, clock_after).await;
    assert_eq!(f.view.shared_head, shared);
    assert_eq!(f.view.generation, 3);
    let expected = f.view.clone();
    let Fixture {
        _temp,
        owner,
        repo,
        root,
        ..
    } = f;
    drop(owner);
    let mut reopened = LocalOwnerRepository::open(&repo, limits()).await.unwrap();
    assert_eq!(
        reopened.checkout_view("work", limits()).await.unwrap(),
        expected
    );
    assert_eq!(std::fs::read(root.join("a")).unwrap(), b"a2");
    assert_eq!(std::fs::read(root.join("b")).unwrap(), b"\0\xff\n");
}

#[tokio::test]
/// Record integral and fractional native dates through the public API and reopen each history.
/// # Arguments
/// No arguments.
/// # Returns
/// Unit after exact stored date types, unchanged no-op state and working bytes agree.
/// # Panics
/// Panics on a recording, native projection, reopen or no-op regression.
async fn private_numeric_dates_preserve_affinity_through_recording_and_reopen() {
    let mut f = Fixture::new().await;
    f.private(&["a", "b"]).await;
    let shared = f.view.shared_head.clone();
    for (index, (date, expected)) in [
        (b"2026-11-20T12:00:00".as_slice(), Value::Integer(2461365)),
        (b"2026-11-21T12:00:00".as_slice(), Value::Integer(2461366)),
        (
            b"2026-11-21T12:00:00.001".as_slice(),
            Value::Real(2461366.0 + 1.0 / 86400000.0),
        ),
        (b"2026-11-22T00:00:00".as_slice(), Value::Real(2461366.5)),
    ]
    .into_iter()
    .enumerate()
    {
        let bytes = format!("date case {index}").into_bytes();
        std::fs::write(f.root.join("a"), &bytes).unwrap();
        f.changed(&["a", "b"], date).await;
        assert_eq!(f.view.shared_head, shared);
        assert_eq!(f.view.generation, index as u64 + 1);
        let selected = "(SELECT rid FROM blob ORDER BY rid DESC LIMIT 1)";
        assert_eq!(
            rows(
                &f.repo,
                &format!("SELECT mtime,omtime FROM event WHERE objid={selected}")
            )
            .await,
            vec![vec![expected.clone(), expected.clone()]]
        );
        assert_eq!(
            rows(
                &f.repo,
                &format!("SELECT mtime FROM plink WHERE cid={selected} ORDER BY isprim DESC")
            )
            .await,
            vec![vec![expected.clone()]; if index == 0 { 1 } else { 2 }]
        );
        assert_eq!(
            rows(
                &f.repo,
                &format!("SELECT mtime FROM tagxref WHERE rid={selected} ORDER BY tagid")
            )
            .await,
            vec![vec![expected]; 3]
        );
        let mut reopened = LocalOwnerRepository::open(&f.repo, limits()).await.unwrap();
        assert_eq!(
            reopened.checkout_view("work", limits()).await.unwrap(),
            f.view
        );
        f.owner = reopened;
        let before = snapshot(&f.repo).await;
        assert!(matches!(
            f.record(&["b", "a"], b"2026-11-24T00:00:00", b"no change").await.unwrap(),
            PrivateRecordingOutcome::NoChange { view } if view == f.view
        ));
        assert_eq!(snapshot(&f.repo).await, before);
        assert_eq!(std::fs::read(f.root.join("a")).unwrap(), bytes);
        assert_eq!(std::fs::read(f.root.join("b")).unwrap(), b"\0\xff\n");
    }
}

#[tokio::test]
/// Metadata and mtime-only requests are checked while preserving complete database state.
/// # Arguments
/// No arguments.
/// # Returns
/// Unit for exact NoChange behavior.
/// # Panics
/// Panics if any native value/counter/classification changes or malformed metadata passes.
async fn nochange_and_metadata_refusals_preserve_every_table() {
    let mut f = Fixture::new().await;
    f.private(&["a", "b"]).await;
    f.changed(&["a", "b"], b"2026-11-20T00:00:01").await;
    let before = snapshot(&f.repo).await;
    let view = f.view.clone();
    // Rewriting identical bytes changes the observation time without a content edit.
    std::fs::write(f.root.join("a"), b"a1").unwrap();
    match f
        .record(&["b", "a"], b"2026-11-20T00:00:02", b"checked no-op")
        .await
        .unwrap()
    {
        PrivateRecordingOutcome::NoChange { view: actual } => assert_eq!(actual, view),
        _ => panic!("unexpected change"),
    }
    for (date, comment) in [
        (
            b"2026-11-20T00:00:02".as_slice(),
            b"harmless [ text".as_slice(),
        ),
        (b"2026-11-20T00:00:01", b"bad date"),
        (b"invalid", b"bad date"),
        (b"2026-11-20T00:00:02", b"\0"),
    ] {
        assert!(f.record(&["a", "b"], date, comment).await.is_err());
    }
    for actor in [b"".as_slice(), b"\0".as_slice(), b"\xff".as_slice()] {
        assert!(
            f.owner
                .commit_private_tracked_files(
                    "work",
                    PrivateRecordingRequest {
                        expected: &f.view,
                        paths: &["a", "b"],
                        scope: PathScope::Private,
                        actor,
                        comment: b"valid",
                        date: b"2026-11-20T00:00:02"
                    },
                    limits(),
                    tracking()
                )
                .await
                .is_err()
        );
    }
    assert_eq!(snapshot(&f.repo).await, before);
}

#[tokio::test]
/// Mode-only change creates a manifest and primary/auxiliary links without replacing content.
/// # Arguments
/// No arguments.
/// # Returns
/// Unit for exact mode-only behavior.
/// # Panics
/// Panics on native identity or projection regression.
async fn mode_only_retains_payload_receipt_and_uses_zero_auxiliary_parent_content() {
    let mut f = Fixture::new().await;
    f.private(&["a", "b"]).await;
    f.changed(&["a", "b"], b"2026-11-20T00:00:01").await;
    let payload=rows(&f.repo,"SELECT b.rid,b.uuid,b.rcvid,b.content FROM blob b WHERE b.rid IN(SELECT rid FROM g_vfile) ORDER BY b.rid").await;
    std::fs::set_permissions(f.root.join("a"), std::fs::Permissions::from_mode(0o755)).unwrap();
    f.changed(&["a", "b"], b"2026-11-20T00:00:02").await;
    assert_eq!(rows(&f.repo,"SELECT b.rid,b.uuid,b.rcvid,b.content FROM blob b WHERE b.rid IN(SELECT rid FROM g_vfile) ORDER BY b.rid").await,payload);
    assert_eq!(rows(&f.repo,"SELECT pid=fid,isaux,mperm FROM mlink WHERE mid=(SELECT rid FROM blob ORDER BY rid DESC LIMIT 1) ORDER BY isaux").await,vec![vec![Value::Integer(1),Value::Integer(0),Value::Integer(1)],vec![Value::Integer(0),Value::Integer(1),Value::Integer(1)]]);
}

#[tokio::test]
/// Paths added after a recording remain unselected; reusing existing payloads preserves `rcvid`.
/// # Arguments
/// No arguments.
/// # Returns
/// Unit for recorded-plus-pending admission and dedup behavior.
/// # Panics
/// Panics on implicit future scope, premature writes or changed existing provenance.
async fn recorded_plus_new_unselected_then_dedup_recording() {
    let mut f = Fixture::new().await;
    f.private(&["a", "b"]).await;
    f.changed(&["a", "b"], b"2026-11-20T00:00:01").await;
    let original=rows(&f.repo,"SELECT b.rid,b.uuid,b.rcvid,b.content FROM blob b JOIN g_vfile v ON v.rid=b.rid WHERE v.pathname='a'").await;
    std::fs::write(f.root.join("c"), b"a1").unwrap();
    f.view = f
        .owner
        .add_files("work", &["c"], &f.view, limits(), tracking())
        .await
        .unwrap();
    f.owner = LocalOwnerRepository::open(&f.repo, limits()).await.unwrap();
    assert_eq!(
        f.owner.checkout_view("work", limits()).await.unwrap(),
        f.view
    );
    let before = snapshot(&f.repo).await;
    assert!(
        f.record(&["a", "b", "c"], b"2026-11-20T00:00:02", b"unselected")
            .await
            .is_err()
    );
    assert!(
        f.record(&["a", "b"], b"2026-11-20T00:00:02", b"partial")
            .await
            .is_err()
    );
    assert_eq!(snapshot(&f.repo).await, before);
    assert_eq!(
        rows(&f.repo, "SELECT count(*) FROM g_path_scope WHERE path='c'").await,
        vec![vec![Value::Integer(0)]]
    );
    assert_eq!(
        rows(&f.repo, "SELECT count(*) FROM g_pending_path").await,
        vec![vec![Value::Integer(0)]]
    );
    f.private(&["c"]).await;
    f.changed(&["c", "a", "b"], b"2026-11-20T00:00:02").await;
    assert_eq!(rows(&f.repo,"SELECT b.rid,b.uuid,b.rcvid,b.content FROM blob b JOIN g_vfile v ON v.rid=b.rid WHERE v.pathname='a'").await,original);
    assert_eq!(
        rows(
            &f.repo,
            "SELECT a.rid=c.rid FROM g_vfile a,g_vfile c WHERE a.pathname='a' AND c.pathname='c'"
        )
        .await,
        vec![vec![Value::Integer(1)]]
    );
}

#[tokio::test]
/// Reusing public S bytes as opaque file content preserves public classification and receipt.
/// # Arguments
/// No arguments.
/// # Returns
/// Unit for classification-preserving native dedup.
/// # Panics
/// Panics on private promotion or changed public blob provenance.
async fn public_payload_reuse_does_not_reclassify_native_identity() {
    let mut f = Fixture::new().await;
    let shared = f.view.shared_head.clone();
    let bytes = f
        .owner
        .read_content(&shared, limits().content)
        .await
        .unwrap();
    std::fs::write(f.root.join("a"), bytes).unwrap();
    let original = rows(
        &f.repo,
        "SELECT rid,uuid,rcvid,content FROM blob ORDER BY rid",
    )
    .await;
    f.private(&["a", "b"]).await;
    f.changed(&["a", "b"], b"2026-11-20T00:00:01").await;
    assert_eq!(rows(&f.repo,"SELECT rid,uuid,rcvid,content FROM blob WHERE rid=(SELECT rid FROM g_vfile WHERE pathname='a')").await,original);
    assert_eq!(
        rows(
            &f.repo,
            "SELECT count(*) FROM private WHERE rid=(SELECT rid FROM g_vfile WHERE pathname='a')"
        )
        .await,
        vec![vec![Value::Integer(0)]]
    );
    assert_eq!(
        f.owner
            .shared_view()
            .read_content(&shared, limits().content)
            .await
            .unwrap(),
        std::fs::read(f.root.join("a")).unwrap()
    );
}

#[tokio::test]
/// Every one of the seven public expectation fields participates in no-op admission.
/// # Arguments
/// No arguments.
/// # Returns
/// Unit after stale expectation refusals preserve all native tables.
/// # Panics
/// Panics if a stale or foreign field is ignored.
async fn stale_each_of_all_seven_expectation_fields_refuses_even_nochange() {
    let mut f = Fixture::new().await;
    f.private(&["a", "b"]).await;
    f.changed(&["a", "b"], b"2026-11-20T00:00:01").await;
    let before = snapshot(&f.repo).await;
    for field in 0..7 {
        let mut wanted = f.view.clone();
        match field {
            0 => wanted.repository_id.push('x'),
            1 => wanted.checkout_id.push('x'),
            2 => wanted.registered_root.push("foreign"),
            3 => wanted.shared_head = "0".repeat(64),
            4 => wanted.combined_head = None,
            5 => wanted.generation += 1,
            _ => wanted.pending_version += 1,
        };
        assert!(
            f.owner
                .commit_private_tracked_files(
                    "work",
                    PrivateRecordingRequest {
                        expected: &wanted,
                        paths: &["a", "b"],
                        scope: PathScope::Private,
                        actor: b"Owner",
                        comment: b"no-op",
                        date: b"2026-11-20T00:00:02"
                    },
                    limits(),
                    tracking()
                )
                .await
                .is_err()
        );
    }
    assert_eq!(snapshot(&f.repo).await, before);
}

#[tokio::test]
/// Shared, partial, duplicate and unselected requests never produce canonical rows.
/// # Arguments
/// No arguments.
/// # Returns
/// Unit after complete pre-mutation refusal controls.
/// # Panics
/// Panics on unsupported-scope acceptance or database mutation.
async fn selection_and_explicit_scope_refusals_are_atomic() {
    let mut f = Fixture::new().await;
    let before = snapshot(&f.repo).await;
    assert!(
        f.record(&["a", "b"], b"2026-11-20T00:00:01", b"unselected")
            .await
            .is_err()
    );
    assert_eq!(snapshot(&f.repo).await, before);
    f.private(&["a"]).await;
    let before = snapshot(&f.repo).await;
    for paths in [
        &["a"][..],
        &["a", "a"][..],
        &["a", "b"][..],
        &["a", "b", "unknown"][..],
    ] {
        assert!(
            f.record(paths, b"2026-11-20T00:00:01", b"bad set")
                .await
                .is_err()
        );
    }
    assert_eq!(snapshot(&f.repo).await, before);
    f.view = f
        .owner
        .select_path_scopes(
            "work",
            &f.view,
            &[PathScopeSelection {
                path: "b".into(),
                scope: PathScope::Shared,
            }],
            limits(),
            tracking(),
        )
        .await
        .unwrap();
    let before = snapshot(&f.repo).await;
    assert!(
        f.record(&["a", "b"], b"2026-11-20T00:00:01", b"mixed")
            .await
            .is_err()
    );
    assert!(
        f.owner
            .commit_private_tracked_files(
                "work",
                PrivateRecordingRequest {
                    expected: &f.view,
                    paths: &["a", "b"],
                    scope: PathScope::Shared,
                    actor: b"Owner",
                    comment: b"shared",
                    date: b"2026-11-20T00:00:01"
                },
                limits(),
                tracking()
            )
            .await
            .is_err()
    );
    assert_eq!(snapshot(&f.repo).await, before);
}

#[tokio::test]
/// Missing files and cumulative multi-file byte exhaustion retain the admitted head.
/// # Arguments
/// No arguments.
/// # Returns
/// Unit after descriptor and aggregate-budget refusals.
/// # Panics
/// Panics if any refused request leaves content, counters or metadata behind.
async fn capture_and_cumulative_budget_failures_are_atomic() {
    let mut f = Fixture::new().await;
    f.private(&["a", "b"]).await;
    let before = snapshot(&f.repo).await;
    let mut small = tracking();
    small.retained_bytes = 4;
    assert!(
        f.owner
            .commit_private_tracked_files(
                "work",
                PrivateRecordingRequest {
                    expected: &f.view,
                    paths: &["a", "b"],
                    scope: PathScope::Private,
                    actor: b"Owner",
                    comment: b"over batch",
                    date: b"2026-11-20T00:00:01"
                },
                limits(),
                small
            )
            .await
            .is_err()
    );
    let mut small = limits();
    small.content.max_total_decoded_bytes = 1024;
    assert!(
        f.owner
            .commit_private_tracked_files(
                "work",
                PrivateRecordingRequest {
                    expected: &f.view,
                    paths: &["a", "b"],
                    scope: PathScope::Private,
                    actor: b"Owner",
                    comment: b"work",
                    date: b"2026-11-20T00:00:01"
                },
                small,
                tracking()
            )
            .await
            .is_err()
    );
    std::fs::remove_file(f.root.join("b")).unwrap();
    assert!(
        f.record(&["a", "b"], b"2026-11-20T00:00:01", b"missing")
            .await
            .is_err()
    );
    assert_eq!(snapshot(&f.repo).await, before);
}

#[tokio::test]
/// Enumerate independent projection, provenance, scope and checkout corruptions.
/// # Arguments
/// No arguments.
/// # Returns
/// Unit after each independently produced fixture refuses reopen without repair.
/// # Panics
/// Panics if any unsupported exact-state mutation is admitted or repaired.
async fn every_native_projection_and_bookkeeping_surface_rejects_corruption() {
    for sql in [
        "UPDATE plink SET isprim=0",
        "UPDATE plink SET baseid=pid",
        "DELETE FROM mlink",
        "UPDATE mlink SET pid=-1",
        "UPDATE mlink SET isaux=1",
        "UPDATE mlink SET mperm=2",
        "UPDATE filename SET name='wrong' WHERE name='a'",
        "UPDATE event SET uid=0 WHERE objid<>1",
        "UPDATE tagxref SET srcid=0 WHERE rid<>1",
        "DELETE FROM leaf WHERE rid<>1",
        "INSERT INTO unclustered SELECT rid FROM private LIMIT 1",
        "DELETE FROM unsent",
        "DELETE FROM private",
        "UPDATE blob SET rcvid=1 WHERE rid<>1",
        "UPDATE rcvfrom SET uid=1 WHERE rcvid>1",
        "UPDATE rcvfrom SET nonce='unexpected' WHERE rcvid>1",
        "INSERT INTO rcvfrom(uid,mtime) VALUES(0,julianday('now'))",
        "UPDATE g_vfile SET mrid=0",
        "UPDATE g_vfile SET vid=1",
        "UPDATE g_path_scope SET scope='shared'",
        "UPDATE g_vvar SET value='1/00000000000000000000000000000000' WHERE name='fingerprint'",
        "UPDATE g_checkout SET generation=generation+1",
    ] {
        let mut f = Fixture::new().await;
        f.private(&["a", "b"]).await;
        f.changed(&["a", "b"], b"2026-11-20T00:00:01").await;
        let Fixture {
            _temp, owner, repo, ..
        } = f;
        drop(owner);
        mutate(&repo, sql).await;
        let corrupt = snapshot(&repo).await;
        assert!(
            LocalOwnerRepository::open(&repo, limits()).await.is_err(),
            "admitted {sql}"
        );
        assert_eq!(snapshot(&repo).await, corrupt, "repaired {sql}");
    }
}

/// Borrow the fixture's selected SQL engine for independent parser validation.
struct FixtureDates(libsql::Connection);
impl gossil::artifact::DateProvider for FixtureDates {
    /// Convert a native date using the same provider without recording admission.
    /// # Arguments
    /// * `token` - Exact optional native date bytes.
    /// # Returns
    /// Provider-native Julian day or unavailable conversion.
    /// # Errors
    /// Retains SQL, missing-row and invalid-type failures.
    async fn julianday(
        &self,
        token: Option<&[u8]>,
    ) -> Result<Option<f64>, gossil::artifact::DateProviderError> {
        async {
            let value = token.map_or(Value::Null, |t| Value::Blob(t.to_vec()));
            let mut rows = self
                .0
                .query("SELECT julianday(CAST(?1 AS TEXT))", [value])
                .await?;
            let row = rows
                .next()
                .await?
                .ok_or_else(|| std::io::Error::other("missing date"))?;
            match row.get_value(0)? {
                Value::Null => Ok(None),
                Value::Real(day) => Ok(Some(day)),
                _ => Err(Box::<dyn std::error::Error + Send + Sync>::from(
                    std::io::Error::other("invalid date type"),
                )),
            }
        }
        .await
        .map_err(gossil::artifact::DateProviderError)
    }
}

/// Read the independently retained full manifest without consulting its projections.
/// # Arguments
/// * `repo` - Closed fixture database.
/// * `name` - Exact existing manifest identity.
/// # Returns
/// UTF-8 full native manifest, independently decompressed from its stored envelope.
/// # Panics
/// Panics on missing content, wrong types or invalid native compression.
async fn fixture_manifest(repo: &Path, name: &str) -> String {
    let db = libsql::Builder::new_local(repo).build().await.unwrap();
    let conn = db.connect().unwrap();
    let mut result = conn
        .query("SELECT content FROM blob WHERE uuid=?1", [name])
        .await
        .unwrap();
    let packed: Vec<u8> = result.next().await.unwrap().unwrap().get(0).unwrap();
    assert!(result.next().await.unwrap().is_none());
    let gossil::compression::Unpacked::Bytes(bytes) =
        gossil::compression::unpack(&packed, 1 << 20).unwrap()
    else {
        panic!("full manifest envelope");
    };
    String::from_utf8(bytes).unwrap()
}

/// Replace only the selected fixture manifest and its checkout selection identity.
/// # Arguments
/// * `repo` - Closed independently mutated fixture.
/// * `old` - Exact original selected manifest name.
/// * `body` - Complete pre-Z native cards authored by the negative control.
/// * `sha1` - Whether the control tests the unsupported private SHA1 policy.
/// # Returns
/// Unit after parser-valid bytes, checksum, envelope and selection are installed.
/// # Panics
/// Panics if the fixture is syntactically invalid or its target identity is not unique.
async fn replace_fixture_manifest(repo: &Path, old: &str, body: &str, sha1: bool) {
    let checksum = gossil::content_hash::hex(&gossil::artifact_checksum::md5(body.as_bytes()));
    let bytes = format!("{body}Z {checksum}\n").into_bytes();
    let name = if sha1 {
        gossil::content_hash::hex(&gossil::content_hash::sha1(&bytes))
    } else {
        gossil::content_hash::hex(&gossil::content_hash::sha3(&bytes, 256).unwrap())
    };
    let db = libsql::Builder::new_local(repo).build().await.unwrap();
    let conn = db.connect().unwrap();
    gossil::artifact::parse_artifact(&bytes, limits().parser, &FixtureDates(conn.clone()))
        .await
        .unwrap();
    let packed = gossil::compression::pack(&bytes, -1).unwrap();
    assert_eq!(
        conn.execute(
            "UPDATE blob SET uuid=?1,size=?2,content=?3 WHERE uuid=?4",
            vec![
                Value::Text(name.clone()),
                Value::Integer(bytes.len() as i64),
                Value::Blob(packed),
                Value::Text(old.into())
            ]
        )
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        conn.execute(
            "UPDATE g_checkout SET combined_head=?1 WHERE combined_head=?2",
            vec![Value::Text(name), Value::Text(old.into())]
        )
        .await
        .unwrap(),
        1
    );
}

#[tokio::test]
/// Parser-valid native artifacts outside the private family are refused without repair.
/// # Arguments
/// No arguments.
/// # Returns
/// Unit after every independent family violation reaches its exact intended guard.
/// # Panics
/// Panics on parser failure, an earlier unexpected refusal, admitted history or mutation.
async fn parser_valid_unsupported_histories_refuse_without_repair() {
    for (case, expected) in [
        ("sha1", "recording manifest hash policy"),
        ("parents", "recording exact paired parents"),
        ("mode", "recording file mode"),
        ("rename", "recording rename unsupported"),
        ("date", "recording SQL parent date ordering"),
        (
            "unchanged",
            "recording unchanged historical commit unsupported",
        ),
        ("deletion", "recording deletion unsupported"),
        ("extra-card", "recording noncanonical full manifest"),
        ("bracket-comment", "unsupported private manifest grammar"),
    ] {
        let mut f = Fixture::new().await;
        f.private(&["a", "b"]).await;
        let previous = f.changed(&["a", "b"], b"2026-11-20T00:00:01").await;
        std::fs::write(f.root.join("a"), b"a2").unwrap();
        let current = f.changed(&["a", "b"], b"2026-11-20T00:00:02").await;
        let Fixture {
            _temp,
            owner,
            repo,
            root,
            view,
        } = f;
        drop(owner);
        // The neighboring unmodified two-commit fixture must remain admissible.
        drop(LocalOwnerRepository::open(&repo, limits()).await.unwrap());
        let original = fixture_manifest(&repo, &current).await;
        let prior = fixture_manifest(&repo, &previous).await;
        let mut cards: Vec<String> = original
            .lines()
            .filter(|line| !line.starts_with("Z "))
            .map(str::to_owned)
            .collect();
        match case {
            "sha1" => (),
            "parents" => {
                let line = cards
                    .iter_mut()
                    .find(|line| line.starts_with("P "))
                    .unwrap();
                *line = format!("P {} {previous}", view.shared_head);
            }
            "mode" | "rename" => {
                let line = cards
                    .iter_mut()
                    .find(|line| line.starts_with("F a "))
                    .unwrap();
                line.push_str(if case == "mode" { " l" } else { " x old-a" });
            }
            "date" => {
                *cards
                    .iter_mut()
                    .find(|line| line.starts_with("D "))
                    .unwrap() = "D 2026-11-20T00:00:01".into()
            }
            "unchanged" => {
                for line in cards.iter_mut().filter(|line| line.starts_with("F ")) {
                    let path = line.split_whitespace().nth(1).unwrap();
                    *line = prior
                        .lines()
                        .find(|old| old.starts_with(&format!("F {path} ")))
                        .unwrap()
                        .into();
                }
            }
            "deletion" => cards.retain(|line| !line.starts_with("F b ")),
            "bracket-comment" => {
                *cards
                    .iter_mut()
                    .find(|line| line.starts_with("C "))
                    .unwrap() = "C [literal".into();
            }
            "extra-card" => {
                let at = cards
                    .iter()
                    .position(|line| line.starts_with("T "))
                    .unwrap();
                cards.insert(at, "R 00000000000000000000000000000000".into());
            }
            _ => unreachable!("enumerated fixture case"),
        }
        let body = format!("{}\n", cards.join("\n"));
        replace_fixture_manifest(&repo, &current, &body, case == "sha1").await;
        let before = snapshot(&repo).await;
        let files = [
            std::fs::read(root.join("a")).unwrap(),
            std::fs::read(root.join("b")).unwrap(),
        ];
        let modes = [
            std::fs::metadata(root.join("a"))
                .unwrap()
                .permissions()
                .mode(),
            std::fs::metadata(root.join("b"))
                .unwrap()
                .permissions()
                .mode(),
        ];
        let error = LocalOwnerRepository::open(&repo, limits())
            .await
            .err()
            .expect("unsupported private history");
        assert_eq!(error.operation, expected, "case {case}");
        assert_eq!(snapshot(&repo).await, before, "repaired {case}");
        assert_eq!(
            [
                std::fs::read(root.join("a")).unwrap(),
                std::fs::read(root.join("b")).unwrap()
            ],
            files
        );
        assert_eq!(
            [
                std::fs::metadata(root.join("a"))
                    .unwrap()
                    .permissions()
                    .mode(),
                std::fs::metadata(root.join("b"))
                    .unwrap()
                    .permissions()
                    .mode(),
            ],
            modes,
            "changed modes for {case}"
        );
    }
}

#[tokio::test]
/// Reject invalid provenance, fingerprints and tracked relationships even when their scalar values are valid.
/// # Arguments
/// No arguments.
/// # Returns
/// Unit after exact policy causes and complete table/working-byte preservation.
/// # Panics
/// Panics if a relationship is accepted, masked by another error or repaired.
async fn provenance_and_tracking_relationships_are_exact() {
    for (sql, expected) in [
        (
            "UPDATE blob SET rcvid=1 WHERE rid=(SELECT rid FROM g_vfile WHERE pathname='a')",
            "recording first-use receipt mismatch",
        ),
        (
            "UPDATE g_vvar SET value=x'00' WHERE name='fingerprint'",
            "recording fingerprint type",
        ),
        (
            "UPDATE g_vvar SET value=NULL WHERE name='fingerprint'",
            "recording fingerprint type",
        ),
        (
            "UPDATE g_vfile SET rid=1,mrid=1 WHERE pathname='a'",
            "recording tracked/manifest mismatch",
        ),
        (
            "UPDATE g_vfile SET isexe=1 WHERE pathname='a'",
            "recording tracked/manifest mismatch",
        ),
        (
            "DELETE FROM g_vfile WHERE pathname='a'",
            "recording selected file missing",
        ),
    ] {
        let mut f = Fixture::new().await;
        f.private(&["a", "b"]).await;
        f.changed(&["a", "b"], b"2026-11-20T00:00:01").await;
        let Fixture {
            _temp,
            owner,
            repo,
            root,
            ..
        } = f;
        drop(owner);
        if sql.starts_with("DELETE") {
            mutate(&repo, "DELETE FROM g_path_scope WHERE path='a'").await;
        }
        mutate(&repo, sql).await;
        if sql.contains("value=x'") {
            assert_eq!(
                rows(
                    &repo,
                    "SELECT typeof(value) FROM g_vvar WHERE name='fingerprint'"
                )
                .await,
                vec![vec![Value::Text("blob".into())]]
            );
        }
        let before = snapshot(&repo).await;
        let files = [
            std::fs::read(root.join("a")).unwrap(),
            std::fs::read(root.join("b")).unwrap(),
        ];
        let error = LocalOwnerRepository::open(&repo, limits())
            .await
            .err()
            .expect("invalid relationship");
        assert_eq!(error.operation, expected, "mutation {sql}");
        assert_eq!(snapshot(&repo).await, before);
        assert_eq!(
            [
                std::fs::read(root.join("a")).unwrap(),
                std::fs::read(root.join("b")).unwrap()
            ],
            files
        );
    }
}

#[tokio::test]
/// An authentic retained older checksum cannot select the latest private head.
/// # Arguments
/// No arguments.
/// # Returns
/// Unit after exact current witness/head/latest policy refusal.
/// # Panics
/// Panics if algorithm-valid historical witness is accepted as current authority.
async fn valid_old_fingerprint_is_rejected_for_current_private_head() {
    let mut f = Fixture::new().await;
    let old = rows(&f.repo, "SELECT value FROM g_vvar WHERE name='fingerprint'").await;
    f.private(&["a", "b"]).await;
    f.changed(&["a", "b"], b"2026-11-20T00:00:01").await;
    let Value::Text(value) = &old[0][0] else {
        panic!("fingerprint type");
    };
    assert!(value.bytes().all(|b| b.is_ascii_hexdigit() || b == b'/'));
    let Fixture {
        _temp, owner, repo, ..
    } = f;
    drop(owner);
    mutate(
        &repo,
        &format!("UPDATE g_vvar SET value='{value}' WHERE name='fingerprint'"),
    )
    .await;
    let before = snapshot(&repo).await;
    assert!(LocalOwnerRepository::open(&repo, limits()).await.is_err());
    assert_eq!(snapshot(&repo).await, before);
}

#[tokio::test]
/// Later lexically earlier paths retain persistent filename IDs during replay.
/// # Arguments
/// No arguments.
/// # Returns
/// Unit after exact native-key mlink projection, poststate validation and reopen.
/// # Panics
/// Panics if pathname ordering is mistaken for persistent native filename order.
async fn lexically_earlier_new_path_and_changed_old_path_keep_native_link_order() {
    let mut f = Fixture::new().await;
    f.private(&["a", "b"]).await;
    f.changed(&["a", "b"], b"2026-11-20T00:00:01").await;
    std::fs::write(f.root.join("0"), b"new earlier pathname").unwrap();
    std::fs::write(f.root.join("a"), b"a2").unwrap();
    f.view = f
        .owner
        .add_files("work", &["0"], &f.view, limits(), tracking())
        .await
        .unwrap();
    f.private(&["0"]).await;
    f.changed(&["0", "a", "b"], b"2026-11-20T00:00:02").await;
    assert_eq!(
        rows(&f.repo, "SELECT fnid,name FROM filename ORDER BY fnid").await,
        vec![
            vec![Value::Integer(1), Value::Text("a".into())],
            vec![Value::Integer(2), Value::Text("b".into())],
            vec![Value::Integer(3), Value::Text("0".into())]
        ]
    );
    assert_eq!(rows(&f.repo,"SELECT fnid,isaux FROM mlink WHERE mid=(SELECT rid FROM blob ORDER BY rid DESC LIMIT 1) ORDER BY fnid,isaux").await,vec![vec![Value::Integer(1),Value::Integer(0)],vec![Value::Integer(1),Value::Integer(1)],vec![Value::Integer(3),Value::Integer(0)],vec![Value::Integer(3),Value::Integer(1)]]);
    let expected = f.view.clone();
    let Fixture {
        _temp,
        owner,
        repo,
        root,
        ..
    } = f;
    drop(owner);
    let mut reopened = LocalOwnerRepository::open(&repo, limits()).await.unwrap();
    assert_eq!(
        reopened.checkout_view("work", limits()).await.unwrap(),
        expected
    );
    assert_eq!(
        std::fs::read(root.join("0")).unwrap(),
        b"new earlier pathname"
    );
    assert_eq!(std::fs::read(root.join("a")).unwrap(), b"a2");
}

/// Resolve a fixture's independently computed native name to its actual RID.
/// # Arguments
/// * `repo` - Fixture database.
/// * `name` - Known lowercase full native identity from authored bytes/output receipt.
/// # Returns
/// Its unique exact native RID, without consulting any projection under assertion.
/// # Panics
/// Panics if the identity is absent, ambiguous or malformed.
async fn fixture_rid(repo: &Path, name: &str) -> i64 {
    assert!(
        matches!(name.len(), 40 | 64)
            && name
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    let result = rows(repo, &format!("SELECT rid FROM blob WHERE uuid='{name}'")).await;
    let [row] = result.as_slice() else {
        panic!("fixture RID cardinality");
    };
    let [Value::Integer(id)] = row.as_slice() else {
        panic!("fixture RID type");
    };
    *id
}
/// Convert a separately selected SQL clock/date scalar to a finite Julian day.
/// # Arguments
/// * `repo` - Fixture selected SQL provider.
/// * `token` - Trusted test literal date, or now.
/// # Returns
/// Exact provider-native finite date value.
/// # Panics
/// Panics on unsupported test token, unavailable or nonfinite conversion.
async fn fixture_date(repo: &Path, token: &str) -> f64 {
    assert!(
        token == "now"
            || token
                .bytes()
                .all(|b| b.is_ascii_digit() || b"-:T.".contains(&b))
    );
    let result = rows(repo, &format!("SELECT julianday('{token}')")).await;
    let [row] = result.as_slice() else {
        panic!("fixture date cardinality");
    };
    let [Value::Real(date)] = row.as_slice() else {
        panic!("fixture date type");
    };
    assert!(date.is_finite());
    *date
}
/// Compare complete native rowsets with the pinned three-tree oracle contract.
/// # Arguments
/// * `f` - Fixture whose authored trees are a1/binary, a2/binary, a2/executable-binary/empty.
/// * `names` - Three actual manifest identities returned by successful recording.
/// * `clock_before` - Selected SQL clock independently sampled before recording starts.
/// * `clock_after` - Selected SQL clock independently sampled after all three recordings.
/// # Returns
/// Unit for complete native graph, metadata, classification and receipt equality.
/// # Panics
/// Panics on any extra/missing row, wrong type/NULL, native field or receipt policy.
/// Native IDs bind independently authored content identities; projections never
/// supply expected parent/content/mode/isaux/tag/event/classification values.
async fn assert_complete_three_tree_oracle(
    f: &Fixture,
    names: [&str; 3],
    clock_before: f64,
    clock_after: f64,
) {
    let hash =
        |bytes: &[u8]| gossil::content_hash::hex(&gossil::content_hash::sha3(bytes, 256).unwrap());
    let s = fixture_rid(&f.repo, &f.view.shared_head).await;
    let a1 = fixture_rid(&f.repo, &hash(b"a1")).await;
    let b = fixture_rid(&f.repo, &hash(b"\0\xff\n")).await;
    let a2 = fixture_rid(&f.repo, &hash(b"a2")).await;
    let empty = fixture_rid(&f.repo, &hash(b"")).await;
    let l1 = fixture_rid(&f.repo, names[0]).await;
    let l2 = fixture_rid(&f.repo, names[1]).await;
    let l3 = fixture_rid(&f.repo, names[2]).await;
    let d0 = fixture_date(&f.repo, "2026-11-20T00:00:00").await;
    let d1 = fixture_date(&f.repo, "2026-11-20T00:00:01").await;
    let d2 = fixture_date(&f.repo, "2026-11-20T00:00:02").await;
    let d3 = fixture_date(&f.repo, "2026-11-20T00:00:03").await;
    let i = Value::Integer;
    let real = Value::Real;
    let text = |s: &str| Value::Text(s.into());
    let null = Value::Null;
    assert_eq!(
        rows(
            &f.repo,
            "SELECT pid,cid,isprim,mtime,baseid FROM plink ORDER BY cid,isprim DESC"
        )
        .await,
        vec![
            vec![i(s), i(l1), i(1), real(d1), null.clone()],
            vec![i(l1), i(l2), i(1), real(d2), null.clone()],
            vec![i(s), i(l2), i(0), real(d2), null.clone()],
            vec![i(l2), i(l3), i(1), real(d3), null.clone()],
            vec![i(s), i(l3), i(0), real(d3), null.clone()]
        ]
    );
    assert_eq!(
        rows(
            &f.repo,
            "SELECT mid,fid,pmid,pid,fnid,pfnid,mperm,isaux FROM mlink ORDER BY mid,fnid,isaux"
        )
        .await,
        vec![
            vec![i(l1), i(a1), i(s), i(0), i(1), i(0), i(0), i(0)],
            vec![i(l1), i(b), i(s), i(0), i(2), i(0), i(0), i(0)],
            vec![i(l2), i(a2), i(l1), i(a1), i(1), i(0), i(0), i(0)],
            vec![i(l2), i(a2), i(s), i(0), i(1), i(0), i(0), i(1)],
            vec![i(l3), i(b), i(l2), i(b), i(2), i(0), i(1), i(0)],
            vec![i(l3), i(b), i(s), i(0), i(2), i(0), i(1), i(1)],
            vec![i(l3), i(empty), i(l2), i(0), i(3), i(0), i(0), i(0)],
            vec![i(l3), i(empty), i(s), i(0), i(3), i(0), i(0), i(1)]
        ]
    );
    assert_eq!(
        rows(&f.repo, "SELECT fnid,name FROM filename ORDER BY fnid").await,
        vec![
            vec![i(1), text("a")],
            vec![i(2), text("b")],
            vec![i(3), text("c")]
        ]
    );
    let events: Vec<_> = [
        (s, d0, "initial empty check-in"),
        (l1, d1, "fixture"),
        (l2, d2, "fixture"),
        (l3, d3, "fixture"),
    ]
    .into_iter()
    .map(|(rid, date, comment)| {
        vec![
            text("ci"),
            real(date),
            i(rid),
            null.clone(),
            null.clone(),
            null.clone(),
            null.clone(),
            text("Owner"),
            null.clone(),
            text(comment),
            null.clone(),
            real(date),
        ]
    })
    .collect();
    assert_eq!(rows(&f.repo,"SELECT type,mtime,objid,tagid,uid,bgcolor,euser,user,ecomment,comment,brief,omtime FROM event ORDER BY objid").await,events);
    let code = rows(&f.repo, "SELECT value FROM config WHERE name='server-code'").await;
    let Value::Text(code) = &code[0][0] else {
        panic!("replica type");
    };
    let branch = format!("gossil-private/{code}");
    let mut tags = vec![
        vec![i(8), i(2), i(s), i(s), text("trunk"), real(d0), i(s)],
        vec![i(12), i(2), i(s), i(s), null.clone(), real(d0), i(s)],
    ];
    for (rid, date) in [(l1, d1), (l2, d2), (l3, d3)] {
        for (tag, kind, value) in [
            (8, 2, text(&branch)),
            (12, 0, null.clone()),
            (13, 2, null.clone()),
        ] {
            tags.push(vec![
                i(tag),
                i(kind),
                i(rid),
                i(rid),
                value,
                real(date),
                i(rid),
            ]);
        }
    }
    assert_eq!(
        rows(
            &f.repo,
            "SELECT tagid,tagtype,srcid,origid,value,mtime,rid FROM tagxref ORDER BY rid,tagid"
        )
        .await,
        tags
    );
    assert_eq!(
        rows(&f.repo, "SELECT rid FROM leaf ORDER BY rid").await,
        vec![vec![i(s)], vec![i(l3)]]
    );
    let mut private = [a1, b, l1, a2, l2, empty, l3];
    private.sort_unstable();
    let private: Vec<_> = private.into_iter().map(|rid| vec![i(rid)]).collect();
    assert_eq!(
        rows(&f.repo, "SELECT rid FROM private ORDER BY rid").await,
        private
    );
    assert_eq!(
        rows(&f.repo, "SELECT rid FROM unsent ORDER BY rid").await,
        private
    );
    assert_eq!(
        rows(&f.repo, "SELECT rid FROM unclustered ORDER BY rid").await,
        vec![vec![i(s)]]
    );
    let mut provenance = vec![
        vec![i(s), i(1)],
        vec![i(a1), i(2)],
        vec![i(b), i(2)],
        vec![i(l1), i(2)],
        vec![i(a2), i(3)],
        vec![i(l2), i(3)],
        vec![i(empty), i(4)],
        vec![i(l3), i(4)],
    ];
    provenance.sort_by_key(|row| match row[0] {
        Value::Integer(id) => id,
        _ => unreachable!(),
    });
    assert_eq!(
        rows(&f.repo, "SELECT rid,rcvid FROM blob ORDER BY rid").await,
        provenance
    );
    let mut receipts = vec![vec![i(1), i(0), real(d0), null.clone(), null.clone()]];
    for id in 2..=4 {
        // Only unpredictable receipt time is sampled independently, then checked
        // against surrounding SQL clock observations. All remaining fields and
        // complete receipt cardinality are authored expectations.
        let clock = rows(
            &f.repo,
            &format!("SELECT mtime FROM rcvfrom WHERE rcvid={id}"),
        )
        .await;
        let [row] = clock.as_slice() else {
            panic!("operation clock cardinality");
        };
        let [Value::Real(time)] = row.as_slice() else {
            panic!("operation clock type");
        };
        assert!(time.is_finite() && *time >= clock_before && *time <= clock_after);
        receipts.push(vec![i(id), i(0), real(*time), null.clone(), null.clone()]);
    }
    assert_eq!(
        rows(
            &f.repo,
            "SELECT rcvid,uid,mtime,nonce,ipaddr FROM rcvfrom ORDER BY rcvid"
        )
        .await,
        receipts
    );
}
