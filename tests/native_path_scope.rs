//! Test persistence of explicit path scopes.
//!
//! ## Recommendations
//! Run with complete scope and recording admission integration before release.
use gossil::artifact::ParseLimits;
use gossil::storage::{
    AdmissionLimits, CheckoutView, InitializeOptions, Limits, LocalOwnerRepository, PathScope,
    PathScopeSelection, TrackingLimits,
};
use std::path::{Path, PathBuf};

#[path = "support/proof.rs"]
mod proof;
/// Set explicit finite root and registration ceilings.
fn limits() -> AdmissionLimits {
    AdmissionLimits {
        content: Limits {
            max_stored_bytes: 4096,
            max_output_bytes: 4096,
            max_chain_depth: 4,
            max_total_stored_bytes: 16777216,
            max_total_decoded_bytes: 16777216,
        },
        parser: ParseLimits {
            input_bytes: 4096,
            cards: 4096,
            owned_bytes: 1048576,
            entries: 64,
        },
    }
}
/// Initialize through the public native API without allocating a checkout.
/// # Panics
/// Panics if fixture setup or the expected database operation fails.
async fn initialize(path: &Path) -> LocalOwnerRepository {
    let bounds = limits();
    LocalOwnerRepository::initialize(
        path,
        InitializeOptions {
            repository_id: "0123456789abcdef0123456789abcdef01234567",
            owner_id: "owner",
            date: "2026-11-18 12:34:56.125",
            user: b"Owner",
            password: "secret",
            sha1: false,
            content_limits: bounds.content,
            parse_limits: bounds.parser,
        },
    )
    .await
    .unwrap()
}
/// Create a canonical fixture directory to avoid platform temporary-directory aliases.
/// # Panics
/// Panics if fixture setup or the expected database operation fails.
fn checkout_root(temp: &Path) -> PathBuf {
    let root = temp.join("root");
    std::fs::create_dir(&root).unwrap();
    std::fs::canonicalize(root).unwrap()
}

/// Bound the complete explicit batch and the bytes of one captured file.
fn tracking() -> TrackingLimits {
    TrackingLimits {
        paths: 64,
        path_bytes: 4096,
        components: 32,
        retained_bytes: 16384,
        io_buffer_bytes: 1024,
    }
}

/// Construct an explicit owned selection.
fn selection(path: &str, scope: PathScope) -> PathScopeSelection {
    PathScopeSelection {
        path: path.into(),
        scope,
    }
}
/// Capture every persisted table row using exact provider value debug encodings.
/// # Panics
/// Panics when fixture SQL cannot enumerate the complete database.
async fn snapshot(path: &Path) -> Vec<String> {
    let database = libsql::Builder::new_local(path).build().await.unwrap();
    let connection = database.connect().unwrap();
    let mut catalog = connection
        .query(
            "SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name",
            (),
        )
        .await
        .unwrap();
    let mut names = Vec::new();
    while let Some(row) = catalog.next().await.unwrap() {
        names.push(row.get::<String>(0).unwrap());
    }
    let mut result = Vec::new();
    for name in names {
        let escaped = name.replace('"', "\"\"");
        let mut columns = connection
            .query(&format!("PRAGMA table_info(\"{escaped}\")"), ())
            .await
            .unwrap();
        let mut projections = Vec::new();
        while let Some(column) = columns.next().await.unwrap() {
            let column = column.get::<String>(1).unwrap().replace('"', "\"\"");
            projections.push(format!(
                "typeof(\"{column}\"),hex(CAST(\"{column}\" AS BLOB))"
            ));
        }
        let mut rows = connection
            .query(
                &format!("SELECT {} FROM \"{escaped}\"", projections.join(",")),
                (),
            )
            .await
            .unwrap();
        let mut records = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            let values: Vec<_> = (0..row.column_count())
                .map(|index| format!("{:?}", row.get_value(index).unwrap()))
                .collect();
            records.push(format!("{name}:{values:?}"));
        }
        records.sort();
        result.extend(records);
    }
    result
}
/// Create two tracked paths without selecting either scope.
/// # Panics
/// Panics when fixture initialization or enrollment fails.
async fn fixture(path: &Path, root: &Path) -> (LocalOwnerRepository, CheckoutView) {
    std::fs::write(root.join("alpha"), b"live alpha").unwrap();
    std::fs::write(root.join("beta"), b"live beta").unwrap();
    let mut owner = initialize(path).await;
    let view = owner
        .register_checkout("work", root, limits())
        .await
        .unwrap();
    let view = owner
        .add_files("work", &["alpha", "beta"], &view, limits(), tracking())
        .await
        .unwrap();
    (owner, view)
}
#[tokio::test]
/// Persist partial then mixed selections; equal choices preserve every table and file.
/// # Panics
/// Panics if scope selection changes unrelated state or cannot reopen.
async fn partial_mixed_and_equal_choices_are_durable() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let (mut owner, before) = fixture(&path, &root).await;
    let partial = owner
        .select_path_scopes(
            "work",
            &before,
            &[selection("alpha", PathScope::Private)],
            limits(),
            tracking(),
        )
        .await
        .unwrap();
    assert_eq!(partial.pending_version, before.pending_version + 1);
    assert_eq!(partial.shared_head, before.shared_head);
    assert_eq!(partial.combined_head, before.combined_head);
    assert_eq!(partial.generation, before.generation);
    drop(owner);
    let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    assert_eq!(
        owner.checkout_view("work", limits()).await.unwrap(),
        partial
    );
    let mixed = owner
        .select_path_scopes(
            "work",
            &partial,
            &[
                selection("alpha", PathScope::Private),
                selection("beta", PathScope::Shared),
            ],
            limits(),
            tracking(),
        )
        .await
        .unwrap();
    assert_eq!(mixed.pending_version, partial.pending_version + 1);
    let expected = snapshot(&path).await;
    assert_eq!(
        owner
            .select_path_scopes(
                "work",
                &mixed,
                &[
                    selection("alpha", PathScope::Private),
                    selection("beta", PathScope::Shared)
                ],
                limits(),
                tracking()
            )
            .await
            .unwrap(),
        mixed
    );
    assert_eq!(snapshot(&path).await, expected);
    assert_eq!(std::fs::read(root.join("alpha")).unwrap(), b"live alpha");
    assert_eq!(std::fs::read(root.join("beta")).unwrap(), b"live beta");
    drop(owner);
    assert!(LocalOwnerRepository::open(&path, limits()).await.is_ok());
}
#[tokio::test]
/// Reject all seven stale view fields even for an equal existing choice.
/// # Panics
/// Panics if stale expectations change any table or working bytes.
async fn every_stale_field_refuses_checked_noop() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let (mut owner, before) = fixture(&path, &root).await;
    let current = owner
        .select_path_scopes(
            "work",
            &before,
            &[selection("alpha", PathScope::Private)],
            limits(),
            tracking(),
        )
        .await
        .unwrap();
    let mut variants = vec![current.clone(); 7];
    variants[0].repository_id.push('0');
    variants[1].checkout_id.push('x');
    variants[2].registered_root = root.join("other");
    variants[3].shared_head.push('0');
    variants[4].combined_head = Some(current.shared_head.clone());
    variants[5].generation += 1;
    variants[6].pending_version += 1;
    let expected = snapshot(&path).await;
    for stale in variants {
        assert!(
            owner
                .select_path_scopes(
                    "work",
                    &stale,
                    &[selection("alpha", PathScope::Private)],
                    limits(),
                    tracking()
                )
                .await
                .is_err()
        );
        assert_eq!(snapshot(&path).await, expected);
    }
    assert!(
        owner
            .select_path_scopes(
                "foreign",
                &current,
                &[selection("alpha", PathScope::Private)],
                limits(),
                tracking()
            )
            .await
            .is_err()
    );
    assert_eq!(snapshot(&path).await, expected);
}
#[tokio::test]
/// Validate the entire batch before any new selection or counter update.
/// # Panics
/// Panics if conflicts, unknown paths, aliases or limits publish partial changes.
async fn invalid_batches_preserve_complete_database() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let (mut owner, before) = fixture(&path, &root).await;
    let current = owner
        .select_path_scopes(
            "work",
            &before,
            &[selection("alpha", PathScope::Private)],
            limits(),
            tracking(),
        )
        .await
        .unwrap();
    let expected = snapshot(&path).await;
    let p = PathScope::Private;
    for batch in [
        vec![],
        vec![selection("beta", p), selection("alpha", PathScope::Shared)],
        vec![selection("beta", p), selection("unknown", p)],
        vec![selection("beta", p), selection("beta", p)],
        vec![selection("beta", p), selection("BETA", p)],
        vec![selection("./beta", p)],
        vec![selection("../beta", p)],
        vec![selection("beta/child", p)],
        vec![selection(".gossil/locator", p)],
    ] {
        assert!(
            owner
                .select_path_scopes("work", &current, &batch, limits(), tracking())
                .await
                .is_err()
        );
        assert_eq!(snapshot(&path).await, expected);
    }
    let mut variants = vec![tracking(); 5];
    variants[0].paths = 0;
    variants[1].path_bytes = 1;
    variants[2].components = 0;
    variants[3].io_buffer_bytes = 0;
    variants[4].paths = 1;
    for bounds in variants {
        assert!(
            owner
                .select_path_scopes("work", &current, &[selection("beta", p)], limits(), bounds)
                .await
                .is_err()
        );
        assert_eq!(snapshot(&path).await, expected);
    }
}
#[tokio::test]
/// Reject corrupt scope scalars and identities in the scope command and the full proof,
/// without repair. Open admits them, since it reads no scope row.
/// # Panics
/// Panics if a malformed persisted inventory is admitted or changed by refusal.
async fn corrupt_scope_inventory_refuses_without_repair() {
    for mutation in [
        "UPDATE g_path_scope SET path=x'616c706861'",
        "UPDATE g_path_scope SET path=CAST(x'ff' AS TEXT)",
        "UPDATE g_path_scope SET path=CAST(x'616c00706861' AS TEXT)",
        "UPDATE g_path_scope SET path='unknown'",
        "UPDATE g_path_scope SET scope='invalid'",
        "UPDATE g_path_scope SET checkout_id='foreign'",
        "UPDATE g_checkout SET pending_version=9223372036854775807",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let root = checkout_root(temp.path());
        let (mut owner, before) = fixture(&path, &root).await;
        let current = owner
            .select_path_scopes(
                "work",
                &before,
                &[selection("alpha", PathScope::Private)],
                limits(),
                tracking(),
            )
            .await
            .unwrap();
        drop(owner);
        let database = libsql::Builder::new_local(&path).build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute("PRAGMA foreign_keys=OFF", ())
            .await
            .unwrap();
        connection
            .execute("PRAGMA ignore_check_constraints=ON", ())
            .await
            .unwrap();
        assert_eq!(connection.execute(mutation, ()).await.unwrap(), 1);
        if mutation.contains("path=") && mutation.contains("x'") {
            let mut raw = connection
                .query(
                    "SELECT typeof(path),hex(CAST(path AS BLOB)) FROM g_path_scope",
                    (),
                )
                .await
                .unwrap();
            let row = raw.next().await.unwrap().unwrap();
            let expected_type = if mutation.contains("CAST(") {
                "text"
            } else {
                "blob"
            };
            let expected_hex = if mutation.contains("616c00706861") {
                "616C00706861"
            } else if mutation.contains("x'ff'") {
                "FF"
            } else {
                "616C706861"
            };
            assert_eq!(row.get::<String>(0).unwrap(), expected_type);
            assert_eq!(row.get::<String>(1).unwrap(), expected_hex);
            assert!(raw.next().await.unwrap().is_none());
        }
        drop(connection);
        drop(database);
        let expected = snapshot(&path).await;
        if mutation.contains("pending_version") {
            let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
            let mut exhausted = current.clone();
            exhausted.pending_version = i64::MAX as u64;
            assert!(
                owner
                    .select_path_scopes(
                        "work",
                        &exhausted,
                        &[selection("beta", PathScope::Private)],
                        limits(),
                        tracking()
                    )
                    .await
                    .is_err()
            );
            assert_eq!(
                owner
                    .select_path_scopes(
                        "work",
                        &exhausted,
                        &[selection("alpha", PathScope::Private)],
                        limits(),
                        tracking()
                    )
                    .await
                    .unwrap(),
                exhausted
            );
        } else {
            // Open admits the repository. The full proof refuses the row, and so does
            // the scope command, which reads the scope inventory that it changes.
            let (opened, message) = proof::verify_refusal(&path, limits()).await;
            assert!(opened, "{mutation}");
            let expected_message = if mutation.contains("checkout_id") {
                "repository foreign key integrity failure"
            } else {
                "scope inventory invalid"
            };
            assert_eq!(message, expected_message, "{mutation}");
            let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
            let refused = owner
                .select_path_scopes(
                    "work",
                    &current,
                    &[selection("beta", PathScope::Private)],
                    limits(),
                    tracking(),
                )
                .await
                .unwrap_err();
            assert_eq!(refused.to_string(), "scope inventory invalid", "{mutation}");
            drop(owner);
        }
        assert_eq!(snapshot(&path).await, expected);
    }
}
#[tokio::test]
/// An owner opened before another write sees that write. An old view and a changed
/// registered root identity refuse atomically.
/// # Panics
/// Panics if an old view or a replaced root can publish scope choices.
async fn old_views_and_replaced_root_refuse() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let (mut owner, before) = fixture(&path, &root).await;
    let mut stale = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    let after = owner
        .select_path_scopes(
            "work",
            &before,
            &[selection("alpha", PathScope::Private)],
            limits(),
            tracking(),
        )
        .await
        .unwrap();
    // The second owner reads the view that the first committed, and writes over it.
    let later = stale
        .select_path_scopes(
            "work",
            &after,
            &[selection("beta", PathScope::Shared)],
            limits(),
            tracking(),
        )
        .await
        .unwrap();
    assert_eq!(owner.checkout_view("work", limits()).await.unwrap(), later);
    // An old view no longer matches, and its write refuses without effect.
    let expected = snapshot(&path).await;
    assert!(
        owner
            .select_path_scopes(
                "work",
                &after,
                &[selection("beta", PathScope::Private)],
                limits(),
                tracking()
            )
            .await
            .is_err()
    );
    assert_eq!(snapshot(&path).await, expected);
    std::fs::rename(&root, temp.path().join("original-root")).unwrap();
    std::fs::create_dir(&root).unwrap();
    assert!(
        owner
            .select_path_scopes(
                "work",
                &later,
                &[selection("alpha", PathScope::Private)],
                limits(),
                tracking()
            )
            .await
            .is_err()
    );
    assert_eq!(snapshot(&path).await, expected);
    assert_eq!(
        std::fs::read(temp.path().join("original-root/alpha")).unwrap(),
        b"live alpha"
    );
}
#[tokio::test]
/// The whole-command totals do not bound scope selection.
///
/// A checked no-op and new choices pass under totals of one byte, and the no-op
/// changes nothing.
/// # Panics
/// Panics if a total refuses the batch or the no-op changes a table or working file.
async fn totals_do_not_bound_equal_noop_and_new_choices() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let (mut owner, before) = fixture(&path, &root).await;
    let current = owner
        .select_path_scopes(
            "work",
            &before,
            &[selection("alpha", PathScope::Private)],
            limits(),
            tracking(),
        )
        .await
        .unwrap();
    let expected = snapshot(&path).await;
    let mut tiny = limits();
    tiny.content.max_total_stored_bytes = 1;
    tiny.content.max_total_decoded_bytes = 1;
    let noop = owner
        .select_path_scopes(
            "work",
            &current,
            &[selection("alpha", PathScope::Private)],
            tiny,
            tracking(),
        )
        .await
        .unwrap();
    assert_eq!(noop, current);
    assert_eq!(snapshot(&path).await, expected);
    let next = owner
        .select_path_scopes(
            "work",
            &current,
            &[
                selection("alpha", PathScope::Private),
                selection("beta", PathScope::Shared),
            ],
            tiny,
            tracking(),
        )
        .await
        .unwrap();
    assert_eq!(next.pending_version, current.pending_version + 1);
    assert_eq!(std::fs::read(root.join("alpha")).unwrap(), b"live alpha");
    assert_eq!(std::fs::read(root.join("beta")).unwrap(), b"live beta");
}
#[tokio::test]
/// Keep recorded paths in scope membership beside a newly added unselected path.
/// # Panics
/// Panics if the real producer loses recorded scope facts or infers a new scope.
async fn real_recorded_and_pending_paths_share_complete_scope_inventory() {
    use gossil::storage::{PrivateRecordingOutcome, PrivateRecordingRequest};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let (mut owner, before) = fixture(&path, &root).await;
    let selected = owner
        .select_path_scopes(
            "work",
            &before,
            &[
                selection("alpha", PathScope::Private),
                selection("beta", PathScope::Private),
            ],
            limits(),
            tracking(),
        )
        .await
        .unwrap();
    let recorded = match owner
        .commit_private_tracked_files(
            "work",
            PrivateRecordingRequest {
                expected: &selected,
                paths: &["alpha", "beta"],
                scope: PathScope::Private,
                actor: b"Owner",
                comment: b"record original tracked files",
                date: b"2026-11-20T00:00:01",
            },
            limits(),
            tracking(),
        )
        .await
        .unwrap()
    {
        PrivateRecordingOutcome::Recorded { view, .. } => view,
        PrivateRecordingOutcome::NoChange { .. } => panic!("first recording must create a child"),
    };
    std::fs::write(root.join("gamma"), b"new unselected content").unwrap();
    let pending = owner
        .add_files("work", &["gamma"], &recorded, limits(), tracking())
        .await
        .unwrap();
    drop(owner);
    let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    let database = libsql::Builder::new_local(&path).build().await.unwrap();
    let connection = database.connect().unwrap();
    let mut rows = connection
        .query("SELECT pathname,rid>0 FROM g_vfile ORDER BY pathname", ())
        .await
        .unwrap();
    for (name, recorded) in [("alpha", 1i64), ("beta", 1), ("gamma", 0)] {
        let row = rows.next().await.unwrap().unwrap();
        assert_eq!(row.get::<String>(0).unwrap(), name);
        assert_eq!(row.get::<i64>(1).unwrap(), recorded);
    }
    assert!(rows.next().await.unwrap().is_none());
    let mut choices = connection
        .query("SELECT path,scope FROM g_path_scope ORDER BY path", ())
        .await
        .unwrap();
    for name in ["alpha", "beta"] {
        let row = choices.next().await.unwrap().unwrap();
        assert_eq!(row.get::<String>(0).unwrap(), name);
        assert_eq!(row.get::<String>(1).unwrap(), "private");
    }
    assert!(choices.next().await.unwrap().is_none());
    drop(choices);
    drop(rows);
    drop(connection);
    drop(database);
    let expected = snapshot(&path).await;
    // Gamma is absent from g_path_scope. An explicit private request cannot fill it.
    assert!(
        owner
            .commit_private_tracked_files(
                "work",
                PrivateRecordingRequest {
                    expected: &pending,
                    paths: &["alpha", "beta", "gamma"],
                    scope: PathScope::Private,
                    actor: b"Owner",
                    comment: b"must require gamma selection",
                    date: b"2026-11-20T00:00:02",
                },
                limits(),
                tracking()
            )
            .await
            .is_err()
    );
    assert_eq!(snapshot(&path).await, expected);
    assert_eq!(
        owner
            .select_path_scopes(
                "work",
                &pending,
                &[selection("alpha", PathScope::Private)],
                limits(),
                tracking()
            )
            .await
            .unwrap(),
        pending
    );
    assert_eq!(snapshot(&path).await, expected);
    let mixed = owner
        .select_path_scopes(
            "work",
            &pending,
            &[
                selection("alpha", PathScope::Private),
                selection("gamma", PathScope::Shared),
            ],
            limits(),
            tracking(),
        )
        .await
        .unwrap();
    assert_eq!(mixed.pending_version, pending.pending_version + 1);
    assert_eq!(mixed.shared_head, pending.shared_head);
    assert_eq!(mixed.combined_head, pending.combined_head);
    assert_eq!(mixed.generation, pending.generation);
    drop(owner);
    let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    assert_eq!(owner.checkout_view("work", limits()).await.unwrap(), mixed);
    let expected = snapshot(&path).await;
    assert!(
        owner
            .commit_private_tracked_files(
                "work",
                PrivateRecordingRequest {
                    expected: &mixed,
                    paths: &["alpha", "beta", "gamma"],
                    scope: PathScope::Private,
                    actor: b"Owner",
                    comment: b"mixed scope is not private ready",
                    date: b"2026-11-20T00:00:02",
                },
                limits(),
                tracking()
            )
            .await
            .is_err()
    );
    assert_eq!(snapshot(&path).await, expected);
    assert_eq!(std::fs::read(root.join("alpha")).unwrap(), b"live alpha");
    assert_eq!(std::fs::read(root.join("beta")).unwrap(), b"live beta");
    assert_eq!(
        std::fs::read(root.join("gamma")).unwrap(),
        b"new unselected content"
    );
}
