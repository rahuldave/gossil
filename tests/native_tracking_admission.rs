//! Controls for native pending additions; no recording commit or frozen staging.
//!
//! ## Recommendations
//! Run these controls after the shared native tracked-state admission hooks land.
use gossil::artifact::ParseLimits;
use gossil::storage::{
    AdmissionLimits, InitializeOptions, Limits, LocalOwnerRepository, TrackingLimits,
};
use std::path::{Path, PathBuf};

#[path = "support/shares.rs"]
mod shares;
/// Set explicit finite root and registration ceilings.
fn limits() -> AdmissionLimits {
    AdmissionLimits {
        content: Limits {
            max_stored_bytes: 4096,
            max_output_bytes: 4096,
            max_chain_depth: 4,
            max_total_stored_bytes: 8192,
            // Catalog6 admission charges exact schema/authority and complete operation work.
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

/// Bound the complete explicit batch and its cumulative retained bytes.
fn tracking() -> TrackingLimits {
    TrackingLimits {
        paths: 64,
        path_bytes: 4096,
        components: 32,
        retained_bytes: 16384,
        io_buffer_bytes: 1024,
    }
}

/// Snapshot every persisted application table and schema for exact logical preservation.
/// # Panics
/// Panics on any query or decoding error; no failed observation counts as preserved state.
async fn logical_state(connection: &libsql::Connection) -> Vec<(String, Vec<Vec<libsql::Value>>)> {
    let mut tables = connection.query("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name", ()).await.unwrap();
    let mut names = vec!["sqlite_schema".to_owned()];
    while let Some(row) = tables.next().await.unwrap() {
        names.push(row.get::<String>(0).unwrap());
    }
    let mut snapshot = Vec::new();
    for name in names {
        let quoted = name.replace('"', "\"\"");
        let mut rows = connection
            .query(&format!(r#"SELECT * FROM "{quoted}""#), ())
            .await
            .unwrap();
        let mut values = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            values.push(
                (0..row.column_count())
                    .map(|index| row.get_value(index).unwrap())
                    .collect(),
            );
        }
        snapshot.push((name, values));
    }
    snapshot
}

/// Exercise a complete freshly produced row before a single hostile mutation.
/// # Panics
/// Panics if fixture creation, valid reopen, or rejection of corruption fails.
async fn reject_mutation(sql: &str) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let mut owner = initialize(&path).await;
    let view = owner
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    std::fs::write(root.join("first"), b"unchanged").unwrap();
    owner
        .add_files("work", &["first"], &view, limits(), tracking())
        .await
        .unwrap();
    drop(owner);
    drop(LocalOwnerRepository::open(&path, limits()).await.unwrap());
    let db = libsql::Builder::new_local(&path).build().await.unwrap();
    let c = db.connect().unwrap();
    c.execute("PRAGMA foreign_keys=OFF", ()).await.unwrap();
    let expected_constraint = match sql {
        "UPDATE g_storage SET storage_version=1" => {
            Some("CHECK constraint failed: storage_version=6")
        }
        "UPDATE g_vfile SET id=NULL" => Some("NOT NULL constraint failed: g_vfile.id"),
        "UPDATE g_vfile SET checkout_id=NULL" => {
            Some("NOT NULL constraint failed: g_vfile.checkout_id")
        }
        "UPDATE g_checkout SET pending_version=NULL" => {
            Some("NOT NULL constraint failed: g_checkout.pending_version")
        }
        "UPDATE g_checkout SET pending_version=-1" => {
            Some("CHECK constraint failed: pending_version>=0")
        }
        _ => None,
    };
    let before = logical_state(&c).await;
    match (expected_constraint, c.execute(sql, ()).await) {
        (None, Ok(changed)) => assert!(changed > 0, "mutation did not inject: {sql}"),
        (Some(expected), Err(libsql::Error::SqliteFailure(code, message))) => {
            assert_eq!(
                code & 0xff,
                19,
                "unrelated SQLite error for {sql}: {message}"
            );
            assert_eq!(message, expected, "wrong constraint for {sql}");
            assert_eq!(
                logical_state(&c).await,
                before,
                "constraint changed logical state: {sql}"
            );
        }
        (_, result) => panic!("unexpected mutation result for {sql}: {result:?}"),
    }
    drop(c);
    drop(db);
    if expected_constraint.is_some() {
        drop(LocalOwnerRepository::open(&path, limits()).await.unwrap());
    } else {
        assert!(
            LocalOwnerRepository::open(&path, limits()).await.is_err(),
            "accepted {sql}"
        );
    }
    assert_eq!(std::fs::read(root.join("first")).unwrap(), b"unchanged");
}

/// The number of tests that split the 88 tracking mutations.
const TRACKING_PARTS: usize = 5;

/// Every native scalar, nullable field, binding, path and contiguous row ID is sealed.
/// # Arguments
/// * `part` - The share of the 88 mutations, below `TRACKING_PARTS`. Each mutation gets its own fixture.
/// # Panics
/// Panics if schema constraints or full reopen admission accept a mutated row.
async fn all_native_tracking_fields_refuse_corruption(part: usize) {
    let mut mutations: Vec<String> = Vec::new();
    for field in [
        "id", "vid", "chnged", "deleted", "isexe", "islink", "rid", "mrid", "mtime",
    ] {
        for value in ["NULL", "x'00'", "'invalid'", "0.5"] {
            mutations.push(format!("UPDATE g_vfile SET {field}={value}"));
        }
    }
    for (field, value) in [
        ("id", "0"),
        ("id", "2"),
        ("vid", "0"),
        ("vid", "2"),
        ("chnged", "1"),
        ("deleted", "1"),
        ("isexe", "2"),
        ("isexe", "-1"),
        ("islink", "1"),
        ("rid", "1"),
        ("mrid", "1"),
        ("origname", "''"),
        ("mhash", "''"),
        ("origname", "x'00'"),
        ("mhash", "x'00'"),
        ("checkout_id", "NULL"),
        ("checkout_id", "'other'"),
        ("checkout_id", "x'00'"),
    ] {
        mutations.push(format!("UPDATE g_vfile SET {field}={value}"));
    }
    for value in [
        "NULL",
        "x'00'",
        "''",
        "'../outside'",
        "'/absolute'",
        "'a//b'",
        "'a/./b'",
        "'.gossil/checkout.json'",
        "'a\\b'",
        "CAST(x'006162' AS TEXT)",
        "CAST(x'610062' AS TEXT)",
        "CAST(x'616200' AS TEXT)",
        "CAST(x'80' AS TEXT)",
        "CAST(x'61ff62' AS TEXT)",
        "CAST(x'c080' AS TEXT)",
        "CAST(x'eda080' AS TEXT)",
    ] {
        mutations.push(format!("UPDATE g_vfile SET pathname={value}"));
    }
    for assignment in [
        "pending_version=-1",
        "pending_version=NULL",
        "pending_version='invalid'",
        "pending_version=0.5",
        "pending_version=0",
        "generation=1",
        "combined_head='unsupported'",
    ] {
        mutations.push(format!("UPDATE g_checkout SET {assignment}"));
    }
    mutations.push("UPDATE g_checkout_owner SET owner_id='other'".into());
    mutations.push("UPDATE g_checkout SET checkout_id='other'".into());
    for name in ["FIRST", "first/child"] {
        mutations.push(format!("INSERT INTO g_vfile SELECT checkout_id,2,vid,chnged,deleted,isexe,islink,rid,mrid,mtime,'{name}',origname,mhash FROM g_vfile"));
    }
    mutations.push("DELETE FROM g_checkout_owner".into());
    mutations.push("DELETE FROM g_vvar".into());
    mutations.push("UPDATE g_storage SET storage_version=1".into());
    mutations.push("INSERT INTO g_path_scope VALUES('work','first','shared')".into());
    mutations.push("INSERT INTO g_path_scope VALUES('work','first','private')".into());
    mutations.push("INSERT INTO g_pending_path(checkout_id,path,action,selected_scope) VALUES('work','first','remove','shared')".into());
    mutations.push(
        "INSERT INTO g_vmerge(checkout_id,id,merge,mhash) VALUES('work',1,1,'opaque')".into(),
    );
    for mutation in shares::share(mutations, part, TRACKING_PARTS) {
        reject_mutation(&mutation).await;
    }
}

/// Share 0 of the tracking mutations.
/// # Panics
/// Panics as the shared body does.
#[tokio::test]
async fn all_native_tracking_fields_refuse_corruption_0() {
    all_native_tracking_fields_refuse_corruption(0).await;
}

/// Share 1 of the tracking mutations.
/// # Panics
/// Panics as the shared body does.
#[tokio::test]
async fn all_native_tracking_fields_refuse_corruption_1() {
    all_native_tracking_fields_refuse_corruption(1).await;
}

/// Share 2 of the tracking mutations.
/// # Panics
/// Panics as the shared body does.
#[tokio::test]
async fn all_native_tracking_fields_refuse_corruption_2() {
    all_native_tracking_fields_refuse_corruption(2).await;
}

/// Share 3 of the tracking mutations.
/// # Panics
/// Panics as the shared body does.
#[tokio::test]
async fn all_native_tracking_fields_refuse_corruption_3() {
    all_native_tracking_fields_refuse_corruption(3).await;
}

/// Share 4 of the tracking mutations.
/// # Panics
/// Panics as the shared body does.
#[tokio::test]
async fn all_native_tracking_fields_refuse_corruption_4() {
    all_native_tracking_fields_refuse_corruption(4).await;
}

/// A valid produced inventory is bounded independently at reopen and listing.
/// # Panics
/// Panics if lower inventory/text/component bounds fail to reject a complete inventory.
#[tokio::test]
async fn complete_tracking_inventory_obeys_admission_and_request_bounds() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let mut owner = initialize(&path).await;
    let view = owner
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    std::fs::create_dir(root.join("dir")).unwrap();
    std::fs::write(root.join("dir/first"), b"one").unwrap();
    std::fs::write(root.join("second"), b"two").unwrap();
    owner
        .add_files(
            "work",
            &["dir/first", "second"],
            &view,
            limits(),
            tracking(),
        )
        .await
        .unwrap();
    for bound in [
        TrackingLimits {
            paths: 1,
            ..tracking()
        },
        TrackingLimits {
            path_bytes: 3,
            ..tracking()
        },
        TrackingLimits {
            components: 1,
            ..tracking()
        },
    ] {
        assert!(owner.pending_adds("work", limits(), bound).await.is_err());
    }
    drop(owner);
    let mut bound = limits();
    bound.parser.entries = 1;
    assert!(LocalOwnerRepository::open(&path, bound).await.is_err());
    assert!(LocalOwnerRepository::open(&path, limits()).await.is_ok());
}

/// Empty registration remains admitted; native rows without registrations never do.
/// # Panics
/// Panics if empty-state handling or unregistered fabricated-row refusal regresses.
#[tokio::test]
async fn zero_registration_rejects_fabricated_tracking_rows() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    drop(initialize(&path).await);
    drop(LocalOwnerRepository::open(&path, limits()).await.unwrap());
    let db = libsql::Builder::new_local(&path).build().await.unwrap();
    let c = db.connect().unwrap();
    c.execute("PRAGMA foreign_keys=OFF", ()).await.unwrap();
    c.execute(
        "INSERT INTO g_vfile VALUES('work',1,1,0,0,0,0,0,0,0,'first',NULL,NULL)",
        (),
    )
    .await
    .unwrap();
    drop(c);
    drop(db);
    assert!(LocalOwnerRepository::open(&path, limits()).await.is_err());
}

/// Genuine catalog1 rejects add/status and retains its exact root-projection predicate.
/// # Panics
/// Panics if legacy admission changes or producer-shaped rows enter catalog1.
#[tokio::test]
async fn catalog1_never_admits_native_pending_additions() {
    use gossil::storage::{CheckoutView, CreateOptions};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let bounds = limits();
    let mut owner = LocalOwnerRepository::create(
        &path,
        CreateOptions {
            repository_id: "0123456789abcdef0123456789abcdef01234567",
            owner_id: "owner",
            checkout_id: "legacy",
            checkout_root: &root,
            date: b"2026-11-18T12:34:56.125",
            user: b"Owner",
            content_limits: bounds.content,
            parse_limits: bounds.parser,
        },
    )
    .await
    .unwrap();
    std::fs::write(root.join("first"), b"unchanged").unwrap();
    let wanted = CheckoutView {
        repository_id: "0123456789abcdef0123456789abcdef01234567".into(),
        checkout_id: "legacy".into(),
        registered_root: root.clone(),
        shared_head: String::new(),
        combined_head: None,
        generation: 0,
        pending_version: 0,
    };
    assert_eq!(
        owner
            .add_files("legacy", &["first"], &wanted, bounds, tracking())
            .await
            .unwrap_err()
            .operation,
        "native checkout requires catalog6"
    );
    assert_eq!(
        owner
            .pending_adds("legacy", bounds, tracking())
            .await
            .unwrap_err()
            .operation,
        "native checkout requires catalog6"
    );
    drop(owner);
    drop(LocalOwnerRepository::open(&path, bounds).await.unwrap());
    let db = libsql::Builder::new_local(&path).build().await.unwrap();
    let c = db.connect().unwrap();
    c.execute("INSERT INTO g_vfile SELECT checkout_id,1,(SELECT min(rid) FROM blob),0,0,0,0,0,0,0,'first',NULL,NULL FROM g_checkout", ()).await.unwrap();
    c.execute("UPDATE g_checkout SET pending_version=1", ())
        .await
        .unwrap();
    drop(c);
    drop(db);
    assert_eq!(
        LocalOwnerRepository::open(&path, bounds)
            .await
            .err()
            .unwrap()
            .operation,
        "root projection contradicts canonical bytes"
    );
    assert_eq!(std::fs::read(root.join("first")).unwrap(), b"unchanged");
}

/// Read the complete logical database through an independent test connection.
async fn disk_state(path: &Path) -> Vec<(String, Vec<Vec<libsql::Value>>)> {
    let db = libsql::Builder::new_local(path).build().await.unwrap();
    logical_state(&db.connect().unwrap()).await
}

/// Foreign admitted authority and every expected-view field refuse mutation, including no-op.
#[tokio::test]
async fn foreign_and_altered_expected_views_preserve_all_state() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let foreign_root = temp.path().join("foreign-root");
    std::fs::create_dir(&foreign_root).unwrap();
    let mut owner = initialize(&path).await;
    let initial = owner
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    std::fs::write(root.join("first"), b"first working bytes").unwrap();
    std::fs::write(root.join("new"), b"new working bytes").unwrap();
    let view = owner
        .add_files("work", &["first"], &initial, limits(), tracking())
        .await
        .unwrap();
    let pending = owner
        .pending_adds("work", limits(), tracking())
        .await
        .unwrap();
    drop(owner);
    let foreign_path = temp.path().join("foreign-repo");
    let bounds = limits();
    let mut foreign_owner = LocalOwnerRepository::initialize(
        &foreign_path,
        InitializeOptions {
            repository_id: "ffffffffffffffffffffffffffffffffffffffff",
            owner_id: "foreign-owner",
            date: "2026-11-18 12:34:56.125",
            user: b"Foreign",
            password: "secret",
            sha1: false,
            content_limits: bounds.content,
            parse_limits: bounds.parser,
        },
    )
    .await
    .unwrap();
    let foreign = foreign_owner
        .register_checkout("foreign", &foreign_root, limits())
        .await
        .unwrap();
    drop(foreign_owner);
    let before = disk_state(&path).await;
    let foreign_before = disk_state(&foreign_path).await;
    let mut candidates = vec![foreign, initial];
    for index in 0..7 {
        let mut changed = view.clone();
        match index {
            0 => changed.repository_id = "different-repository".into(),
            1 => changed.checkout_id = "different-checkout".into(),
            2 => changed.registered_root = foreign_root.clone(),
            3 => changed.shared_head = "different-head".into(),
            4 => changed.combined_head = Some("different-combined".into()),
            5 => changed.generation = 1,
            6 => changed.pending_version = view.pending_version + 1,
            _ => unreachable!(),
        }
        candidates.push(changed);
    }
    for wanted in candidates {
        for requested in [&["first"][..], &["new"][..]] {
            let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
            assert_eq!(
                owner
                    .add_files("work", requested, &wanted, limits(), tracking())
                    .await
                    .unwrap_err()
                    .operation,
                "tracking checkout changed"
            );
            drop(owner);
            let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
            assert_eq!(owner.checkout_view("work", limits()).await.unwrap(), view);
            assert_eq!(
                owner
                    .pending_adds("work", limits(), tracking())
                    .await
                    .unwrap(),
                pending
            );
            drop(owner);
            assert_eq!(disk_state(&path).await, before);
            assert_eq!(disk_state(&foreign_path).await, foreign_before);
            assert_eq!(
                std::fs::read(root.join("first")).unwrap(),
                b"first working bytes"
            );
            assert_eq!(
                std::fs::read(root.join("new")).unwrap(),
                b"new working bytes"
            );
            assert_eq!(std::fs::read_dir(&foreign_root).unwrap().count(), 0);
        }
    }
}

/// Encode every stored cell as SQLite type and exact raw bytes, avoiding hostile TEXT decoding.
async fn raw_logical_state(
    connection: &libsql::Connection,
) -> Vec<(String, Vec<Vec<libsql::Value>>)> {
    let mut tables = connection.query("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name", ()).await.unwrap();
    let mut names = vec!["sqlite_schema".to_owned()];
    while let Some(row) = tables.next().await.unwrap() {
        names.push(row.get::<String>(0).unwrap());
    }
    let mut snapshot = Vec::new();
    for table in names {
        let quoted = table.replace('"', "\"\"");
        let mut columns = connection
            .query(&format!(r#"PRAGMA table_info("{quoted}")"#), ())
            .await
            .unwrap();
        let mut selections = Vec::new();
        while let Some(row) = columns.next().await.unwrap() {
            let name = row.get::<String>(1).unwrap().replace('"', "\"\"");
            selections.push(format!(r#"typeof("{name}"),hex(CAST("{name}" AS BLOB))"#));
        }
        let mut rows = connection
            .query(
                &format!(r#"SELECT {} FROM "{quoted}""#, selections.join(",")),
                (),
            )
            .await
            .unwrap();
        let mut values = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            values.push(
                (0..row.column_count())
                    .map(|index| row.get_value(index).unwrap())
                    .collect(),
            );
        }
        snapshot.push((table, values));
    }
    snapshot
}

/// Run all seven hostile registration-field cases without fixture or observation panics masking API results.
/// Each API open has its own OS thread and Tokio runtime so one panic cannot skip later cases.
#[test]
fn registration_text_boundary_runs_complete_raw_byte_matrix() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut failures = Vec::new();
    for (table, field) in [
        ("g_checkout", "checkout_id"),
        ("g_checkout", "root"),
        ("g_checkout", "shared_head"),
        ("g_checkout", "availability"),
        ("g_checkout_owner", "owner_id"),
        ("g_checkout_owner", "root_device"),
        ("g_checkout_owner", "root_inode"),
    ] {
        for form in [
            "nul_suffix",
            "invalid_utf8_same_length",
            "blob_original_bytes",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("repo");
            let root = checkout_root(temp.path());
            std::fs::write(root.join("working"), b"untouched working bytes").unwrap();
            runtime.block_on(async {
                let mut owner = initialize(&path).await;
                owner
                    .register_checkout("work", &root, limits())
                    .await
                    .unwrap();
                drop(owner);
                drop(LocalOwnerRepository::open(&path, limits()).await.unwrap());
            });
            let before = runtime.block_on(async {
                let db = libsql::Builder::new_local(&path).build().await.unwrap();
                let c = db.connect().unwrap();
                c.execute("PRAGMA foreign_keys=OFF", ()).await.unwrap();
                // Deliberately reach admission's decoder even when availability CHECK would reject injection.
                // These are fixture-connection settings, never repository API settings.
                c.execute("PRAGMA ignore_check_constraints=ON", ())
                    .await
                    .unwrap();
                let mut rows = c
                    .query(&format!("SELECT CAST({field} AS BLOB) FROM {table}"), ())
                    .await
                    .unwrap();
                let original = rows
                    .next()
                    .await
                    .unwrap()
                    .unwrap()
                    .get::<Vec<u8>>(0)
                    .unwrap();
                assert!(!original.is_empty());
                drop(rows);
                let mut bytes = original;
                match form {
                    "nul_suffix" => bytes.extend_from_slice(b"\0suffix"),
                    "invalid_utf8_same_length" => bytes[0] = 0xff,
                    "blob_original_bytes" => (),
                    _ => unreachable!(),
                }
                let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
                let value = if form == "blob_original_bytes" {
                    format!("x'{hex}'")
                } else {
                    format!("CAST(x'{hex}' AS TEXT)")
                };
                assert_eq!(
                    c.execute(&format!("UPDATE {table} SET {field}={value}"), ())
                        .await
                        .unwrap(),
                    1
                );
                if field == "checkout_id" {
                    // All registration joins and native keys must reach the corrupted identity.
                    for reference in ["g_checkout_owner", "g_vvar", "g_vfile"] {
                        c.execute(&format!("UPDATE {reference} SET checkout_id={value}"), ())
                            .await
                            .unwrap();
                    }
                }
                let mut raw = c
                    .query(
                        &format!("SELECT typeof({field}),hex(CAST({field} AS BLOB)) FROM {table}"),
                        (),
                    )
                    .await
                    .unwrap();
                let observed = raw.next().await.unwrap().unwrap();
                assert_eq!(
                    observed.get::<String>(0).unwrap(),
                    if form == "blob_original_bytes" {
                        "blob"
                    } else {
                        "text"
                    }
                );
                assert_eq!(observed.get::<String>(1).unwrap(), hex.to_ascii_uppercase());
                raw_logical_state(&c).await
            });
            let api_path = path.clone();
            let outcome = std::thread::spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .build()
                    .unwrap()
                    .block_on(async {
                        match LocalOwnerRepository::open(&api_path, limits()).await {
                            Ok(owner) => {
                                drop(owner);
                                "ACCEPTED".to_owned()
                            }
                            Err(error) => format!("REFUSED: {}", error.operation),
                        }
                    })
            })
            .join();
            match outcome {
                Ok(message) if message.starts_with("REFUSED:") => (),
                Ok(message) => failures.push(format!("{table}.{field}/{form}: {message}")),
                Err(panic) => {
                    let detail = panic
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| panic.downcast_ref::<&str>().copied())
                        .unwrap_or("non-string panic");
                    failures.push(format!("{table}.{field}/{form}: API PANICKED: {detail}"));
                }
            }
            let after = runtime.block_on(async {
                let db = libsql::Builder::new_local(&path).build().await.unwrap();
                raw_logical_state(&db.connect().unwrap()).await
            });
            if after != before {
                failures.push(format!("{table}.{field}/{form}: APPLICATION STATE CHANGED"));
            }
            if std::fs::read(root.join("working")).unwrap() != b"untouched working bytes" {
                failures.push(format!("{table}.{field}/{form}: WORKING BYTES CHANGED"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "registration matrix failures (all 21 cases attempted):\n{}",
        failures.join("\n")
    );
}

/// Rejected empty/oversize batches and existing aliases preserve exact application facts.
#[tokio::test]
async fn batch_limits_and_existing_aliases_preserve_pending_state() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    std::fs::write(root.join("first"), b"unchanged").unwrap();
    let mut owner = initialize(&path).await;
    let initial = owner
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    let view = owner
        .add_files("work", &["first"], &initial, limits(), tracking())
        .await
        .unwrap();
    drop(owner);
    let before = disk_state(&path).await;
    for (paths, bound, error) in [
        (&[][..], tracking(), "tracking batch count limit"),
        (
            &["first", "new"][..],
            TrackingLimits {
                paths: 1,
                ..tracking()
            },
            "tracking batch count limit",
        ),
        (&["FIRST"][..], tracking(), "aliased tracked path"),
        (&["first/child"][..], tracking(), "aliased tracked path"),
    ] {
        let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
        assert_eq!(
            owner
                .add_files("work", paths, &view, limits(), bound)
                .await
                .unwrap_err()
                .operation,
            error
        );
        drop(owner);
        assert_eq!(disk_state(&path).await, before);
        assert_eq!(std::fs::read(root.join("first")).unwrap(), b"unchanged");
    }
}
