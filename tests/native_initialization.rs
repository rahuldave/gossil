//! Exercise the public checkout-free initializer and its deliberately empty admission scope.
use gossil::artifact::ParseLimits;
use gossil::storage::{
    AdmissionLimits, CreateOptions, InitializeOptions, Limits, LocalOwnerRepository,
};
use libsql::{Connection, Value};
use std::path::Path;

/// Supply finite bounds for one canonical root.
fn content() -> Limits {
    Limits {
        max_stored_bytes: 4096,
        max_output_bytes: 4096,
        max_chain_depth: 4,
        max_total_stored_bytes: 8192,
        max_total_decoded_bytes: 16384,
    }
}
/// Supply finite parser bounds for the root's exact cards.
fn parser() -> ParseLimits {
    ParseLimits {
        input_bytes: 4096,
        cards: 16,
        owned_bytes: 16384,
        entries: 64,
    }
}
/// Build only the implemented explicit native options.
fn options() -> InitializeOptions<'static> {
    InitializeOptions {
        repository_id: "0123456789abcdef0123456789abcdef01234567",
        owner_id: "owner",
        date: "2026-11-18 12:34:56.125",
        user: b"Local Owner",
        password: "secret",
        sha1: false,
        content_limits: content(),
        parse_limits: parser(),
    }
}
/// Reopen through the external public admission API.
async fn reopen(path: &Path) -> Result<LocalOwnerRepository, gossil::storage::StorageError> {
    LocalOwnerRepository::open(
        path,
        AdmissionLimits {
            content: content(),
            parser: parser(),
        },
    )
    .await
}
/// Establish a native fixture using only the public constructor.
async fn initialize(path: &Path) {
    drop(
        LocalOwnerRepository::initialize(path, options())
            .await
            .unwrap(),
    );
}
/// Open fixture SQL independently; this is not an owner authority path.
async fn connection(path: &Path) -> Connection {
    libsql::Builder::new_local(path)
        .build()
        .await
        .unwrap()
        .connect()
        .unwrap()
}
/// Materialize native fixture values for exact assertions.
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
/// Execute a fixture mutation and require strict owner admission to refuse it.
async fn refuses(sql: &str) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    initialize(&path).await;
    let c = connection(&path).await;
    c.execute_batch(sql).await.unwrap();
    drop(c);
    assert!(reopen(&path).await.is_err(), "admitted {sql}");
}

#[tokio::test]
/// Authenticate root bytes/hash/settings and every empty local table without a checkout.
async fn initialize_has_real_native_root_settings_identity_and_no_checkout() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let mut owner = LocalOwnerRepository::initialize(&path, options())
        .await
        .unwrap();
    let (project, server) = owner.initialization_identity().await.unwrap();
    assert_eq!(project, options().repository_id);
    assert_eq!(server.len(), 40);
    let c = connection(&path).await;
    assert_eq!(
        rows(
            &c,
            "SELECT storage_version,policy_complete,checkout_count FROM g_storage"
        )
        .await,
        vec![vec![
            Value::Integer(4),
            Value::Integer(1),
            Value::Integer(0)
        ]]
    );
    let names = rows(
        &c,
        "SELECT name FROM sqlite_schema WHERE type='table' AND name GLOB 'g_*' ORDER BY name",
    )
    .await;
    assert_eq!(names.len(), 57);
    for row in names {
        let Value::Text(table) = &row[0] else {
            panic!("table name");
        };
        let expected =
            i64::from(["g_storage", "g_local_owner", "g_public_receipt"].contains(&table.as_str()));
        assert_eq!(
            rows(&c, &format!("SELECT count(*) FROM {table}")).await,
            vec![vec![Value::Integer(expected)]],
            "{table}"
        );
    }
    let hashes = rows(&c, "SELECT uuid FROM blob").await;
    let Value::Text(hash) = &hashes[0][0] else {
        panic!("root hash");
    };
    let bytes = owner
        .shared_view()
        .read_content(hash, content())
        .await
        .unwrap();
    assert!(
        std::str::from_utf8(&bytes)
            .unwrap()
            .contains("D 2026-11-18T12:34:56.125\n")
    );
    assert!(
        std::str::from_utf8(&bytes)
            .unwrap()
            .contains("U Local\\sOwner\n")
    );
    assert_eq!(
        hash,
        &gossil::content_hash::hex(&gossil::content_hash::sha3(&bytes, 256).unwrap())
    );
    assert!(
        owner
            .put_private_file_content(hash, &bytes, content())
            .await
            .unwrap_err()
            .operation
            .contains("private writes unsupported")
    );
    assert_eq!(
        owner.initialization_identity().await.unwrap(),
        (project.clone(), server.clone())
    );
    let secret = format!("{project}/Local Owner/secret");
    assert_eq!(
        rows(&c, "SELECT pw,cap FROM user WHERE login='Local Owner'").await,
        vec![vec![
            Value::Text(gossil::content_hash::hex(&gossil::content_hash::sha1(
                secret.as_bytes()
            ))),
            Value::Text("s".into())
        ]]
    );
    assert_eq!(
        rows(&c, "SELECT count(*) FROM reportfmt").await,
        vec![vec![Value::Integer(1)]]
    );
    assert_eq!(
        rows(
            &c,
            "SELECT login,cap FROM user WHERE login<>'Local Owner' ORDER BY login"
        )
        .await,
        vec![
            vec![Value::Text("anonymous".into()), Value::Text("hz".into())],
            vec![Value::Text("developer".into()), Value::Text("ei".into())],
            vec![Value::Text("nobody".into()), Value::Text("gjorz".into())],
            vec![Value::Text("reader".into()), Value::Text("kptw".into())]
        ]
    );
    drop(c);
    drop(owner);
    let mut owner = reopen(&path).await.unwrap();
    assert_eq!(
        owner.initialization_identity().await.unwrap(),
        (project, server)
    );
}

#[tokio::test]
/// Explicit SHA1 changes both the real root hash and stored native policy.
async fn initialize_sha1_selects_real_hash_and_native_policy() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sha1");
    let mut o = options();
    o.sha1 = true;
    let mut owner = LocalOwnerRepository::initialize(&path, o).await.unwrap();
    let c = connection(&path).await;
    let hash = rows(&c, "SELECT uuid FROM blob").await;
    let Value::Text(hash) = &hash[0][0] else {
        panic!("hash");
    };
    let bytes = owner.read_content(hash, content()).await.unwrap();
    assert_eq!(
        hash,
        &gossil::content_hash::hex(&gossil::content_hash::sha1(&bytes))
    );
    assert_eq!(
        rows(&c, "SELECT value FROM config WHERE name='hash-policy'").await,
        vec![vec![Value::Text("0".into())]]
    );
    drop(c);
    drop(owner);
    drop(reopen(&path).await.unwrap());
}

#[tokio::test]
/// Bound catalog1 creation retains its real checkout and admission contract.
async fn bound_catalog1_create_and_reopen_remain_supported() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("bound");
    drop(
        LocalOwnerRepository::create(
            &path,
            CreateOptions {
                repository_id: options().repository_id,
                owner_id: "owner",
                checkout_id: "checkout",
                checkout_root: temp.path(),
                date: b"2026-11-18T00:00:00.000",
                user: b"Owner",
                content_limits: content(),
                parse_limits: parser(),
            },
        )
        .await
        .unwrap(),
    );
    let c = connection(&path).await;
    assert_eq!(
        rows(&c, "SELECT storage_version FROM g_storage").await,
        vec![vec![Value::Integer(1)]]
    );
    assert_eq!(
        rows(&c, "SELECT checkout_id FROM g_checkout").await,
        vec![vec![Value::Text("checkout".into())]]
    );
    drop(c);
    drop(reopen(&path).await.unwrap());
}

#[tokio::test]
/// Invalid identities and administrator bytes refuse before creating a destination.
async fn invalid_explicit_inputs_leave_destination_absent() {
    let temp = tempfile::tempdir().unwrap();
    for variant in [
        "project",
        "owner",
        "empty-user",
        "nul-user",
        "utf8-user",
        "password",
    ] {
        let path = temp.path().join(variant);
        let mut o = options();
        match variant {
            "project" => o.repository_id = "bad",
            "owner" => o.owner_id = "",
            "empty-user" => o.user = b"",
            "nul-user" => o.user = b"bad\0user",
            "utf8-user" => o.user = b"\xff",
            _ => o.password = "bad\0password",
        }
        assert!(
            LocalOwnerRepository::initialize(&path, o).await.is_err(),
            "{variant}"
        );
        assert!(!path.exists(), "{variant}");
    }
}

#[tokio::test]
/// Retain first date/limit errors and report conservative engine cleanup separately.
async fn finite_failures_preserve_primary_error_and_cleanup_uncertainty() {
    let temp = tempfile::tempdir().unwrap();
    for variant in ["date", "content", "parser"] {
        let path = temp.path().join(variant);
        let mut o = options();
        match variant {
            "date" => o.date = "not-a-date",
            "content" => o.content_limits.max_output_bytes = 1,
            _ => o.parse_limits.cards = 1,
        }
        let error = LocalOwnerRepository::initialize(&path, o)
            .await
            .err()
            .unwrap();
        assert!(
            error.operation.contains(match variant {
                "date" => "unrecognized date",
                "content" => "limit",
                _ => "artifact validation",
            }),
            "{}",
            error.operation
        );
        assert!(error.cleanup.as_deref().unwrap().contains("disposal"));
        assert!(path.exists());
        assert!(reopen(&path).await.is_err());
        let c = connection(&path).await;
        assert_eq!(
            rows(
                &c,
                "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'"
            )
            .await,
            vec![vec![Value::Integer(0)]]
        );
    }
}

#[tokio::test]
/// Existing destinations and sidecars survive with their exact original bytes.
async fn existing_destination_and_native_sidecars_are_never_adopted() {
    let temp = tempfile::tempdir().unwrap();
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let path = temp.path().join("repo");
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        let entry = std::path::PathBuf::from(name);
        std::fs::write(&entry, b"retained\0\xff").unwrap();
        let error = LocalOwnerRepository::initialize(&path, options())
            .await
            .err()
            .unwrap();
        if suffix.is_empty() {
            assert!(std::error::Error::source(&error).is_some());
        } else {
            assert!(!path.exists());
        }
        assert_eq!(std::fs::read(&entry).unwrap(), b"retained\0\xff");
        std::fs::remove_file(entry).unwrap();
    }
}

#[tokio::test]
/// Extra real native blobs/history refuse even with plausible zero checkout counters.
async fn populated_native_history_with_zero_counters_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("extra-real-content");
    initialize(&path).await;
    let c = connection(&path).await;
    let bytes = b"actual additional native file content";
    let hash = gossil::content_hash::hex(&gossil::content_hash::sha3(bytes, 256).unwrap());
    let packed = gossil::compression::pack(bytes, -1).unwrap();
    c.execute(
        "INSERT INTO blob(rid,rcvid,size,uuid,content) VALUES(2,1,?1,?2,?3)",
        libsql::params![bytes.len() as i64, hash, packed],
    )
    .await
    .unwrap();
    assert_eq!(
        rows(&c, "SELECT checkout_count FROM g_storage").await,
        vec![vec![Value::Integer(0)]]
    );
    drop(c);
    assert!(reopen(&path).await.is_err());
    for sql in [
        "INSERT INTO event(type,mtime,objid) VALUES('ci',2461363.5,2)",
        "INSERT INTO leaf VALUES(2)",
        "INSERT INTO unclustered VALUES(2)",
        "INSERT INTO rcvfrom(rcvid,uid,mtime) VALUES(2,0,2461363.5)",
        "INSERT INTO filename(name) VALUES('real-file')",
        "INSERT INTO plink(pid,cid,isprim,mtime) VALUES(1,2,1,2461363.5)",
    ] {
        refuses(sql).await;
    }
}

#[tokio::test]
/// Nonempty typed, diagnostic, checkout, merge and recovery namespaces never gain owner authority.
async fn populated_local_namespaces_with_zero_counters_are_refused() {
    for sql in [
        "INSERT INTO g_admission VALUES('hash','owner_unclassified','test')",
        "INSERT INTO g_diagnostic VALUES('scope','ticket','hash','field','code','detail')",
        "INSERT INTO g_checkout VALUES('checkout','/tmp','root',NULL,0,0,'ready')",
        "INSERT INTO g_gest_tags VALUES('scope','tag','tag','2026','2026')",
    ] {
        refuses(sql).await;
    }
}

#[tokio::test]
/// Wrong schema, owner and unsupported catalog markers refuse without upgrades.
async fn wrong_schema_owner_and_unsupported_catalogs_are_refused() {
    for sql in [
        "DROP INDEX delta_i1",
        "DELETE FROM g_local_owner",
        "UPDATE config SET value='wrong' WHERE name='project-code'",
        "UPDATE g_storage SET policy_complete=0",
        "PRAGMA ignore_check_constraints=ON; UPDATE g_storage SET storage_version=5",
        "PRAGMA ignore_check_constraints=ON; UPDATE g_storage SET storage_version=6",
        "UPDATE g_storage SET checkout_count=1",
    ] {
        refuses(sql).await;
    }
}

#[tokio::test]
/// Corrupt root identity, native dates and public projections refuse before authority.
async fn wrong_root_hash_date_and_projections_are_refused() {
    for sql in [
        "UPDATE blob SET uuid=printf('%064d',0)",
        "UPDATE blob SET content=x'00000000'",
        "UPDATE event SET mtime=mtime+1",
        "UPDATE event SET euser='override'",
        "UPDATE tagxref SET value='wrong' WHERE tagid=8",
        "DELETE FROM tagxref WHERE tagid=8",
        "UPDATE g_public_receipt SET validation_class='independent-public-receipt'",
        "INSERT INTO tag(tagname) VALUES('extra')",
        "UPDATE rcvfrom SET mtime=mtime+1",
    ] {
        refuses(sql).await;
    }
}

#[tokio::test]
/// An external native edit invalidates identity reads on an already admitted handle.
async fn initialization_identity_rejects_stale_admission() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let mut owner = LocalOwnerRepository::initialize(&path, options())
        .await
        .unwrap();
    let c = connection(&path).await;
    c.execute(
        "UPDATE config SET value=lower(hex(randomblob(20))) WHERE name='server-code'",
        (),
    )
    .await
    .unwrap();
    assert!(
        owner
            .initialization_identity()
            .await
            .unwrap_err()
            .operation
            .contains("stale")
    );
}

#[tokio::test]
/// Unavailable raw-byte checkout input fails canonicalization before destination creation.
async fn bound_create_refuses_unavailable_raw_checkout_input_without_destination() {
    use std::os::unix::ffi::OsStringExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp
        .path()
        .join(std::ffi::OsString::from_vec(b"checkout-\xff".to_vec()));
    let path = temp.path().join("repo");
    let error = LocalOwnerRepository::create(
        &path,
        CreateOptions {
            repository_id: options().repository_id,
            owner_id: "owner",
            checkout_id: "checkout",
            checkout_root: &root,
            date: b"2026-11-18T00:00:00.000",
            user: b"Owner",
            content_limits: content(),
            parse_limits: parser(),
        },
    )
    .await
    .err()
    .unwrap();
    assert_eq!(error.operation, "checkout root unavailable");
    assert!(std::error::Error::source(&error).is_some());
    assert!(!path.exists());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[tokio::test]
/// Extra unrelated tag history and missing/malformed native server identities refuse admission.
async fn empty_catalog4_refuses_extra_tag_history_and_missing_server_identity() {
    for sql in [
        "INSERT INTO tagxref(tagid,tagtype,srcid,origid,mtime,rid) VALUES(1,1,2,2,2461363.5,2)",
        "DELETE FROM config WHERE name='server-code'",
        "UPDATE config SET value='invalid' WHERE name='server-code'",
        "UPDATE config SET value=40 WHERE name='server-code'",
    ] {
        refuses(sql).await;
    }
}

#[tokio::test]
/// A changed definition with the same object names fails the exact catalog hash check.
async fn empty_catalog4_refuses_same_name_changed_schema_definition() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    initialize(&path).await;
    let c = connection(&path).await;
    c.execute_batch("PRAGMA writable_schema=ON; UPDATE sqlite_schema SET sql=sql||' ' WHERE name='delta_i1'; PRAGMA writable_schema=OFF;").await.unwrap();
    drop(c);
    let error = reopen(&path).await.err().unwrap();
    assert!(
        error.operation.contains("unsupported repository catalog"),
        "{}",
        error.operation
    );
}

#[tokio::test]
/// A second durable owner row cannot hide behind the supported singleton selection.
async fn empty_catalog4_refuses_extra_local_owner_row() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    initialize(&path).await;
    let c = connection(&path).await;
    c.execute_batch("PRAGMA ignore_check_constraints=ON; INSERT INTO g_local_owner(singleton,owner_id) VALUES(2,'second-owner'); PRAGMA ignore_check_constraints=OFF;").await.unwrap();
    assert_eq!(
        rows(&c, "SELECT count(*) FROM g_local_owner").await,
        vec![vec![Value::Integer(2)]]
    );
    drop(c);
    let error = reopen(&path)
        .await
        .err()
        .expect("extra durable owner row must refuse admission");
    assert_eq!(error.operation, "unsupported catalog4 owner cardinality");
}
