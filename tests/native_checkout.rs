//! External controls for the strict single empty-root native checkout API.
use gossil::artifact::ParseLimits;
use gossil::storage::{AdmissionLimits, InitializeOptions, Limits, LocalOwnerRepository};
use libsql::{Connection, Value};
use std::path::{Path, PathBuf};

/// Set explicit finite root and registration ceilings.
fn limits() -> AdmissionLimits {
    AdmissionLimits {
        content: Limits {
            max_stored_bytes: 4096,
            max_output_bytes: 4096,
            max_chain_depth: 4,
            max_total_stored_bytes: 8192,
            // Catalog4 admission charges exact schema/authority and complete operation work.
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
/// Independently inspect or tamper a fixture; this does not convey owner authority.
/// # Panics
/// Panics if fixture setup or the expected database operation fails.
async fn connection(path: &Path) -> Connection {
    libsql::Builder::new_local(path)
        .build()
        .await
        .unwrap()
        .connect()
        .unwrap()
}
/// Materialize a small exact fixture query.
/// # Panics
/// Panics if fixture setup or the expected database operation fails.
async fn rows(c: &Connection, sql: &str) -> Vec<Vec<Value>> {
    let mut rows = c.query(sql, ()).await.unwrap();
    let mut result = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        result.push(
            (0..row.column_count())
                .map(|i| row.get_value(i).unwrap())
                .collect(),
        );
    }
    result
}

#[tokio::test]
/// Register, reopen and authenticate actual heads, native keys and unchanged unselected files.
/// # Panics
/// Panics if fixture setup fails or an asserted registration or admission result differs.
async fn register_reopen_preserves_unselected_files_and_native_keys() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    std::fs::write(root.join("ordinary"), b"untracked\0bytes").unwrap();
    std::fs::create_dir(root.join(".git")).unwrap();
    std::fs::write(root.join(".git/config"), b"nested git bytes").unwrap();
    let mut owner = initialize(&path).await;
    let view = owner
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    assert_eq!(
        view.repository_id,
        "0123456789abcdef0123456789abcdef01234567"
    );
    assert_eq!(view.checkout_id, "work");
    assert_eq!(view.registered_root, root);
    assert_eq!(
        (
            view.combined_head.clone(),
            view.generation,
            view.pending_version
        ),
        (None, 0, 0)
    );
    assert_eq!(owner.checkout_view("work", limits()).await.unwrap(), view);
    drop(owner);
    let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    assert_eq!(owner.checkout_view("work", limits()).await.unwrap(), view);
    assert_eq!(
        std::fs::read(root.join("ordinary")).unwrap(),
        b"untracked\0bytes"
    );
    assert_eq!(
        std::fs::read(root.join(".git/config")).unwrap(),
        b"nested git bytes"
    );
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    let c = connection(&path).await;
    assert_eq!(rows(&c, "SELECT checkout_count,(SELECT count(*) FROM g_checkout_owner),(SELECT count(*) FROM g_vvar),(SELECT count(*) FROM g_vfile) FROM g_storage").await, vec![vec![Value::Integer(1),Value::Integer(1),Value::Integer(3),Value::Integer(0)]]);
    assert_eq!(
        rows(&c, "SELECT value FROM g_vvar WHERE name='checkout'").await,
        vec![vec![Value::Text("1".into())]]
    );
    assert_eq!(
        rows(&c, "SELECT value FROM g_vvar WHERE name='checkout-hash'").await,
        vec![vec![Value::Text(view.shared_head)]]
    );
    let native = rows(&c, "SELECT value FROM g_vvar WHERE name='fingerprint'").await;
    let parts = rows(
        &c,
        "SELECT quote(uid),datetime(mtime),quote(nonce),quote(ipaddr) FROM rcvfrom WHERE rcvid=1",
    )
    .await;
    let mut md5 = gossil::artifact_checksum::Md5Checksum::new();
    for value in &parts[0] {
        let Value::Text(text) = value else {
            panic!("native text conversion");
        };
        md5.update(text.as_bytes());
    }
    let expected = format!(
        "1/{}",
        md5.finish()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    assert_eq!(native, vec![vec![Value::Text(expected)]]);
}

#[tokio::test]
/// Refuse missing/duplicate identities, second registration, and retain catalog4 private-write refusal.
/// # Panics
/// Panics if fixture setup fails or an asserted registration or admission result differs.
async fn selection_and_duplicate_errors_never_change_binding() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let mut owner = initialize(&path).await;
    assert!(owner.checkout_view("missing", limits()).await.is_err());
    drop(owner);
    let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    let expected = owner
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    for (id, candidate) in [("work", root.clone()), ("other", root.join("."))] {
        assert!(
            owner
                .register_checkout(id, &candidate, limits())
                .await
                .is_err()
        );
        drop(owner);
        owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
        assert_eq!(
            owner.checkout_view("work", limits()).await.unwrap(),
            expected
        );
    }
    let other = temp.path().join("other");
    std::fs::create_dir(&other).unwrap();
    assert!(
        owner
            .register_checkout("other", &other, limits())
            .await
            .is_err()
    );
    drop(owner);
    let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    assert!(owner.checkout_view("wrong", limits()).await.is_err());
    drop(owner);
    let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    assert_eq!(
        owner
            .put_private_file_content(
                "0000000000000000000000000000000000000000",
                b"x",
                limits().content
            )
            .await
            .unwrap_err()
            .to_string(),
        "private writes unsupported for empty catalog4 scope"
    );
    drop(owner);
    assert!(LocalOwnerRepository::open(&path, limits()).await.is_ok());
}

#[tokio::test]
/// Reject malformed identities and roots before any registration rows exist.
/// # Panics
/// Panics if fixture setup fails or an asserted registration or admission result differs.
async fn invalid_inputs_leave_zero_checkout_reopenable() {
    use std::os::unix::ffi::OsStringExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let mut owner = initialize(&path).await;
    for id in [String::new(), "bad\0id".into(), "x".repeat(1025)] {
        assert!(owner.register_checkout(&id, &root, limits()).await.is_err());
    }
    let file = temp.path().join("file");
    std::fs::write(&file, b"file").unwrap();
    let invalid = temp.path().join(std::ffi::OsString::from_vec(vec![0xff]));
    // The API rejects these bytes before filesystem lookup, including on UTF-8-only filesystems.
    assert_eq!(
        owner
            .register_checkout("work", &invalid, limits())
            .await
            .unwrap_err()
            .to_string(),
        "checkout root is not UTF-8"
    );
    for candidate in [
        temp.path().join("missing"),
        file,
        PathBuf::from("bad\0root"),
        PathBuf::from("x".repeat(4097)),
    ] {
        assert!(
            owner
                .register_checkout("work", &candidate, limits())
                .await
                .is_err()
        );
    }
    drop(owner);
    assert!(LocalOwnerRepository::open(&path, limits()).await.is_ok());
    let c = connection(&path).await;
    assert_eq!(rows(&c, "SELECT checkout_count,(SELECT count(*) FROM g_checkout),(SELECT count(*) FROM g_checkout_owner),(SELECT count(*) FROM g_vvar) FROM g_storage").await, vec![vec![Value::Integer(0);4]]);
}

#[tokio::test]
/// Each independently tampered registration/native authority category must fail public reopen.
/// # Panics
/// Panics if fixture setup fails or an asserted registration or admission result differs.
async fn registration_tamper_inventory_is_rejected() {
    let mutations = [
        "DELETE FROM g_checkout",
        "DELETE FROM g_checkout_owner",
        "DELETE FROM g_vvar WHERE name='fingerprint'",
        "INSERT INTO g_checkout SELECT 'extra',root||'/extra',shared_head,NULL,0,0,'ready' FROM g_checkout",
        "INSERT INTO g_checkout_owner SELECT 'extra',owner_id,root_device,root_inode FROM g_checkout_owner",
        "INSERT INTO g_vvar SELECT checkout_id,'extra','value' FROM g_checkout",
        "UPDATE g_storage SET checkout_count=0",
        "UPDATE g_storage SET checkout_count=2",
        "PRAGMA ignore_check_constraints=ON; UPDATE g_storage SET checkout_count='bad'",
        "PRAGMA ignore_check_constraints=ON; UPDATE g_storage SET checkout_count=-1",
        "UPDATE g_checkout SET shared_head='bad'",
        "UPDATE g_checkout SET combined_head=shared_head",
        "UPDATE g_checkout SET generation=1",
        "UPDATE g_checkout SET pending_version=1",
        "UPDATE g_checkout SET availability='blocked'",
        "UPDATE g_checkout_owner SET owner_id='other'",
        "UPDATE g_checkout_owner SET root_device='0'",
        "UPDATE g_checkout_owner SET root_inode='0'",
        "UPDATE g_checkout SET root=root||'/.'",
        "UPDATE g_checkout SET checkout_id=char(0)",
        "UPDATE g_vvar SET value='bad' WHERE name='checkout'",
        "UPDATE g_vvar SET value='bad' WHERE name='checkout-hash'",
        "UPDATE g_vvar SET value='bad' WHERE name='fingerprint'",
        "UPDATE g_vvar SET value=x'31' WHERE name='checkout'",
        "UPDATE rcvfrom SET nonce='changed'",
        "UPDATE rcvfrom SET uid=1",
        "UPDATE event SET comment='changed'",
        "INSERT INTO filename(name) VALUES('unsupported')",
        "INSERT INTO g_vfile(checkout_id,id,pathname) VALUES('work',1,'unsupported')",
        "UPDATE g_checkout SET root=zeroblob(4097)",
        "UPDATE g_checkout_owner SET root_device=zeroblob(65536)",
    ];
    for sql in mutations {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let root = checkout_root(temp.path());
        let mut owner = initialize(&path).await;
        owner
            .register_checkout("work", &root, limits())
            .await
            .unwrap();
        drop(owner);
        let c = connection(&path).await;
        c.execute_batch("PRAGMA foreign_keys=OFF").await.unwrap();
        c.execute_batch(sql).await.unwrap();
        drop(c);
        assert!(
            LocalOwnerRepository::open(&path, limits()).await.is_err(),
            "admitted {sql}"
        );
    }
}

#[tokio::test]
/// A directory replaced at the same canonical pathname cannot inherit the registration.
/// # Panics
/// Panics if fixture setup fails or an asserted registration or admission result differs.
async fn root_replacement_and_symlink_are_rejected() {
    use std::os::unix::fs::symlink;
    for symbolic in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let root = checkout_root(temp.path());
        let mut owner = initialize(&path).await;
        owner
            .register_checkout("work", &root, limits())
            .await
            .unwrap();
        let moved = root.with_extension("saved");
        std::fs::rename(&root, &moved).unwrap();
        if symbolic {
            symlink(&moved, &root).unwrap();
        } else {
            std::fs::create_dir(&root).unwrap();
        }
        assert!(owner.checkout_view("work", limits()).await.is_err());
        drop(owner);
        assert!(LocalOwnerRepository::open(&path, limits()).await.is_err());
    }
}

#[tokio::test]
/// Another handle's successful registration invalidates the original owner's admission proof.
/// # Panics
/// Panics if fixture setup fails or an asserted registration or admission result differs.
async fn stale_owner_requires_reopen_after_registration() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let mut stale = initialize(&path).await;
    let mut other = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    other
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    assert!(
        stale
            .register_checkout("other", &root, limits())
            .await
            .is_err()
    );
    assert!(stale.checkout_view("work", limits()).await.is_err());
    drop(stale);
    assert!(
        LocalOwnerRepository::open(&path, limits())
            .await
            .unwrap()
            .checkout_view("work", limits())
            .await
            .is_ok()
    );
}

#[tokio::test]
/// Exact-root recovery refuses absence, another root and unauthorized or multiple registrations.
/// # Panics
/// Panics if fixture setup fails or an asserted registration or admission result differs.
async fn exact_root_resolution_is_authorized_unique_and_read_only() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let mut owner = initialize(&path).await;
    assert!(owner.checkout_view_for_root(&root, limits()).await.is_err());
    drop(owner);
    let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    let expected = owner
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    assert_eq!(
        owner.checkout_view_for_root(&root, limits()).await.unwrap(),
        expected
    );
    let other = temp.path().join("other");
    std::fs::create_dir(&other).unwrap();
    assert!(
        owner
            .checkout_view_for_root(&other, limits())
            .await
            .is_err()
    );
    drop(owner);
    for sql in [
        "UPDATE g_checkout_owner SET owner_id='unauthorized'",
        "INSERT INTO g_checkout SELECT 'second',root||'/other',shared_head,NULL,0,0,'ready' FROM g_checkout; UPDATE g_storage SET checkout_count=2",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let root = checkout_root(temp.path());
        let mut owner = initialize(&path).await;
        owner
            .register_checkout("work", &root, limits())
            .await
            .unwrap();
        let c = connection(&path).await;
        c.execute_batch("PRAGMA foreign_keys=OFF").await.unwrap();
        c.execute_batch(sql).await.unwrap();
        assert!(owner.checkout_view_for_root(&root, limits()).await.is_err());
        drop(c);
        drop(owner);
        assert!(LocalOwnerRepository::open(&path, limits()).await.is_err());
    }
}

#[tokio::test]
/// Legacy catalog1 remains reopenable and refuses all catalog4-only checkout methods.
/// # Panics
/// Panics if fixture setup fails or an asserted registration or admission result differs.
async fn catalog1_preserves_legacy_admission_and_refuses_new_api() {
    use gossil::storage::CreateOptions;
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
    assert!(owner.register_checkout("new", &root, bounds).await.is_err());
    assert!(owner.checkout_view("legacy", bounds).await.is_err());
    assert!(owner.checkout_view_for_root(&root, bounds).await.is_err());
    drop(owner);
    assert!(LocalOwnerRepository::open(&path, bounds).await.is_ok());
}

#[tokio::test]
/// Caller root aliases resolve to one canonical registration while persisted-root links are refused.
/// # Panics
/// Panics if fixture setup fails or an asserted registration or admission result differs.
async fn caller_aliases_resolve_to_the_canonical_registration() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let alias = temp.path().join("alias");
    symlink(&root, &alias).unwrap();
    let mut owner = initialize(&path).await;
    let view = owner
        .register_checkout("work", &alias, limits())
        .await
        .unwrap();
    assert_eq!(view.registered_root, root);
    assert_eq!(
        owner
            .checkout_view_for_root(&alias.join("."), limits())
            .await
            .unwrap(),
        view
    );
    assert!(
        owner
            .register_checkout("second", &root, limits())
            .await
            .is_err()
    );
    drop(owner);
    assert_eq!(
        LocalOwnerRepository::open(&path, limits())
            .await
            .unwrap()
            .checkout_view_for_root(&alias, limits())
            .await
            .unwrap(),
        view
    );
}

#[tokio::test]
/// Register the actual SHA1 root and refuse smaller admission ceilings without durable effects.
/// # Panics
/// Panics if fixture setup fails or an asserted registration or admission result differs.
async fn sha1_head_and_finite_registration_limits_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let bounds = limits();
    let mut owner = LocalOwnerRepository::initialize(
        &path,
        InitializeOptions {
            repository_id: "0123456789abcdef0123456789abcdef01234567",
            owner_id: "owner",
            date: "2026-11-18 12:34:56.125",
            user: b"Owner",
            password: "secret",
            sha1: true,
            content_limits: bounds.content,
            parse_limits: bounds.parser,
        },
    )
    .await
    .unwrap();
    let mut tight = bounds;
    tight.parser.input_bytes = 1;
    assert!(owner.register_checkout("work", &root, tight).await.is_err());
    tight = bounds;
    tight.content.max_output_bytes = 1;
    assert!(owner.register_checkout("work", &root, tight).await.is_err());
    drop(owner);
    let mut owner = LocalOwnerRepository::open(&path, bounds).await.unwrap();
    let view = owner
        .register_checkout("work", &root, bounds)
        .await
        .unwrap();
    assert_eq!(view.shared_head.len(), 40);
    drop(owner);
    assert_eq!(
        LocalOwnerRepository::open(&path, bounds)
            .await
            .unwrap()
            .checkout_view("work", bounds)
            .await
            .unwrap(),
        view
    );
}

#[tokio::test]
/// Reject unsupported scalar fields before retaining oversized text or blob values.
/// # Panics
/// Panics if fixture setup fails, rejection occurs at another stage, or durable state changes.
async fn oversized_checkout_scalar_fields_are_rejected_before_row_materialization() {
    let mut mismatches = Vec::new();
    for field in ["combined_head", "generation", "pending_version"] {
        for (kind, value) in [
            ("text", Value::Text("x".repeat(8192))),
            ("blob", Value::Blob(vec![0; 8192])),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("repo");
            let root = checkout_root(temp.path());
            let mut owner = initialize(&path).await;
            let expected = owner
                .register_checkout("work", &root, limits())
                .await
                .unwrap();
            drop(owner);
            let c = connection(&path).await;
            // Field names are the fixed reviewed inventory above; values remain bound.
            c.execute(&format!("UPDATE g_checkout SET {field}=?1"), vec![value])
                .await
                .unwrap();
            let inventory = format!(
                "SELECT typeof({field}),length(CAST({field} AS BLOB)),checkout_count,(SELECT count(*) FROM g_checkout_owner),(SELECT count(*) FROM g_vvar) FROM g_checkout CROSS JOIN g_storage"
            );
            let before = rows(&c, &inventory).await;
            assert_eq!(
                before,
                vec![vec![
                    Value::Text(kind.into()),
                    Value::Integer(8192),
                    Value::Integer(1),
                    Value::Integer(1),
                    Value::Integer(3),
                ]]
            );
            let error = match LocalOwnerRepository::open(&path, limits()).await {
                Ok(_) => panic!("admitted oversized {kind} in {field}"),
                Err(error) => error,
            };
            if error.to_string() != "unsupported recording checkout state" {
                mismatches.push(format!("{field}/{kind}: {error}"));
            }
            assert_eq!(rows(&c, &inventory).await, before);
            let original = if field == "combined_head" {
                Value::Null
            } else {
                Value::Integer(0)
            };
            c.execute(&format!("UPDATE g_checkout SET {field}=?1"), vec![original])
                .await
                .unwrap();
            drop(c);
            assert_eq!(
                LocalOwnerRepository::open(&path, limits())
                    .await
                    .unwrap()
                    .checkout_view("work", limits())
                    .await
                    .unwrap(),
                expected
            );
        }
    }
    assert!(
        mismatches.is_empty(),
        "wrong rejection stages: {mismatches:?}"
    );
}

/// Initial operation and root traversal charges fail before registration changes any state.
/// # Panics
/// Panics if an early refusal consumes the owner's proof or a later registration fails.
#[tokio::test]
async fn checkout_entry_work_limits_leave_registration_and_owner_proof_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let mut owner = initialize(&path).await;
    let mut tiny = limits();
    tiny.content.max_total_decoded_bytes = 1;
    assert_eq!(
        owner
            .register_checkout("work", &root, tiny)
            .await
            .unwrap_err()
            .operation,
        "total decoded content limit"
    );
    assert_eq!(
        owner
            .checkout_view("work", tiny)
            .await
            .unwrap_err()
            .operation,
        "total decoded content limit"
    );
    assert_eq!(
        owner
            .checkout_view_for_root(&root, tiny)
            .await
            .unwrap_err()
            .operation,
        "total decoded content limit"
    );
    let operation = limits().parser.input_bytes.min(4096 + 1024) * 8 + "owner".len() * 4 + 8192;
    let traversal = root.as_os_str().as_encoded_bytes().len() + (libc::PATH_MAX as usize) * 4;
    let mut short = limits();
    short.content.max_total_decoded_bytes = (operation + traversal - 1) as u64;
    assert_eq!(
        owner
            .register_checkout("work", &root, short)
            .await
            .unwrap_err()
            .operation,
        "total decoded content limit"
    );
    assert_eq!(
        owner
            .checkout_view_for_root(&root, short)
            .await
            .unwrap_err()
            .operation,
        "total decoded content limit"
    );
    let view = owner
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    assert_eq!(view.pending_version, 0);
    assert_eq!(
        owner.checkout_view_for_root(&root, limits()).await.unwrap(),
        view
    );
}
