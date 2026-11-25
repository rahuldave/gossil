//! Check the catalog line through fresh processes: creation at catalog 6, its
//! fingerprint, the refusal of older and unknown markers, and the native
//! `ticket-table` setting.
//!
//! ## Recommendations
//! Make every admitted fixture through the product. A refusal fixture may write an
//! older marker with plain SQL, because it checks a refusal and not an admission (CAT-11).
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// The registered raw schema fingerprint of catalog 6, as `src/storage/schema.rs` holds it.
const CATALOG6_FINGERPRINT: &str =
    "0cb960d2106b8d59368148fe029764533adaf8ca799842587259e2abc9358631";

/// Keep command invocations independent of inherited Gest selection and preferences.
/// # Arguments
/// * `binary` - Product executable to invoke.
/// * `home` - Isolated fixture home and platform configuration directory.
/// # Returns
/// A command with inherited Gest environment values removed and fixture paths set.
fn command(binary: &str, home: &Path) -> Command {
    let mut value = Command::new(binary);
    for (key, _) in std::env::vars_os() {
        if key.to_str().is_some_and(|key| key.starts_with("GEST_")) {
            value.env_remove(key);
        }
    }
    value
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("XDG_DATA_HOME", home);
    value
}

/// Run one product command and require success.
/// # Arguments
/// * `command` - The prepared command.
/// # Returns
/// The captured output.
/// # Panics
/// Panics when the process fails to start or exits nonzero.
fn succeed(mut command: Command) -> Output {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// Run a query on a fixture connection and keep every value.
/// # Arguments
/// * `path` - Existing fixture database.
/// * `sql` - Fixture query.
/// # Returns
/// The rows, each as owned values.
/// # Panics
/// Panics on fixture SQL failure.
fn rows(path: &Path, sql: &str) -> Vec<Vec<libsql::Value>> {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async {
            let database = libsql::Builder::new_local(path).build().await.unwrap();
            let connection = database.connect().unwrap();
            let mut rows = connection.query(sql, ()).await.unwrap();
            let mut values = Vec::new();
            while let Some(row) = rows.next().await.unwrap() {
                values.push(
                    (0..row.column_count())
                        .map(|index| row.get_value(index).unwrap())
                        .collect(),
                );
            }
            values
        })
}

/// Run fixture SQL that writes, outside every product admission path.
/// # Arguments
/// * `path` - Existing fixture database.
/// * `sql` - One or more fixture statements.
/// # Panics
/// Panics on fixture SQL failure.
fn write(path: &Path, sql: &str) {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async {
            let database = libsql::Builder::new_local(path).build().await.unwrap();
            database
                .connect()
                .unwrap()
                .execute_batch(sql)
                .await
                .unwrap();
        });
}

/// Compute the raw schema fingerprint the way the catalog check does.
///
/// Each value of `type,name,tbl_name,sql` gets an eight-byte big-endian length and
/// its bytes, and a null gets the length `u64::MAX`.
/// # Arguments
/// * `path` - Existing repository.
/// # Returns
/// The lowercase SHA3-256 of the encoding.
/// # Panics
/// Panics on fixture SQL failure or a value that is neither text nor null.
fn fingerprint(path: &Path) -> String {
    let mut bytes = Vec::new();
    for row in rows(
        path,
        "SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name",
    ) {
        for value in row {
            match value {
                libsql::Value::Text(text) => {
                    bytes.extend_from_slice(&(text.len() as u64).to_be_bytes());
                    bytes.extend_from_slice(text.as_bytes());
                }
                libsql::Value::Null => bytes.extend_from_slice(&u64::MAX.to_be_bytes()),
                other => panic!("schema value {other:?}"),
            }
        }
    }
    gossil::content_hash::hex(&gossil::content_hash::sha3(&bytes, 256).unwrap())
}

/// Create a repository with `gos v init` or `gos v new` in a fresh home.
/// # Arguments
/// * `binary` - Product executable.
/// * `verb` - `init` or `new`.
/// # Returns
/// The temporary home, which owns the files, and the repository path.
fn create(binary: &str, verb: &str) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    succeed({
        let mut init = command(binary, temp.path());
        init.args(["v", verb]).arg(&repo);
        init
    });
    (temp, repo)
}

#[test]
/// CAT-T01 and CAT-T02: every creating command makes catalog 6 with the registered
/// fingerprint, the three empty audit tables and the `ticket-table` setting.
fn init_and_new_create_catalog6_with_the_registered_fingerprint() {
    for binary in [env!("CARGO_BIN_EXE_gos"), env!("CARGO_BIN_EXE_gossil")] {
        for verb in ["init", "new"] {
            let (_temp, repo) = create(binary, verb);
            assert_eq!(
                rows(
                    &repo,
                    "SELECT storage_version,checkout_count FROM g_storage"
                ),
                vec![vec![libsql::Value::Integer(6), libsql::Value::Integer(0)]]
            );
            assert_eq!(fingerprint(&repo), CATALOG6_FINGERPRINT);
            for table in ["transactions", "transaction_events", "g_command_audit"] {
                assert_eq!(
                    rows(&repo, &format!("SELECT count(*) FROM {table}")),
                    vec![vec![libsql::Value::Integer(0)]],
                    "{table}"
                );
            }
            let storage = rows(
                &repo,
                "SELECT sql FROM sqlite_schema WHERE name='g_storage'",
            );
            assert!(
                storage[0][0]
                    .as_text()
                    .unwrap()
                    .contains("CHECK(storage_version=6)")
            );
            let ticket = ticket_table(&repo);
            assert!(ticket.contains("\"gest_id\" TEXT"), "{ticket}");
        }
    }
}

/// Read the one `ticket-table` setting of a repository.
/// # Arguments
/// * `path` - Existing repository.
/// # Returns
/// The stored ticket schema.
/// # Panics
/// Panics unless exactly one text row exists.
fn ticket_table(path: &Path) -> String {
    let value = rows(path, "SELECT value FROM config WHERE name='ticket-table'");
    let [row] = value.as_slice() else {
        panic!("ticket-table rows {value:?}");
    };
    let [libsql::Value::Text(text)] = row.as_slice() else {
        panic!("ticket-table value {row:?}");
    };
    text.clone()
}

/// Read the column names of one table in order.
/// # Arguments
/// * `path` - Existing repository.
/// * `table` - Table name.
/// # Returns
/// The column names.
fn columns(path: &Path, table: &str) -> Vec<String> {
    rows(
        path,
        &format!("SELECT name FROM pragma_table_info('{table}')"),
    )
    .into_iter()
    .map(|row| row[0].as_text().unwrap().clone())
    .collect()
}

#[test]
/// The `ticket-table` setting recreates the installed ticket tables exactly, as the
/// `ticket_create_table` step of a stock `fossil rebuild` runs it.
fn ticket_table_setting_recreates_the_gest_ticket_columns() {
    let (_temp, repo) = create(env!("CARGO_BIN_EXE_gos"), "init");
    let schema = |path: &Path| {
        rows(
            path,
            "SELECT name,sql FROM sqlite_schema WHERE tbl_name IN('ticket','ticketchng') ORDER BY name",
        )
    };
    let before = schema(&repo);
    assert_eq!(before.len(), 4, "{before:?}");
    let ticket = columns(&repo, "ticket");
    let change = columns(&repo, "ticketchng");
    for column in ["gest_id", "gossil_kind", "title", "status", "priority"] {
        assert!(ticket.contains(&column.to_owned()), "ticket {column}");
        assert!(change.contains(&column.to_owned()), "ticketchng {column}");
    }
    let setting = ticket_table(&repo);
    write(
        &repo,
        &format!("DROP TABLE IF EXISTS ticket; DROP TABLE IF EXISTS ticketchng; {setting}"),
    );
    assert_eq!(schema(&repo), before);
    assert_eq!(columns(&repo, "ticket"), ticket);
    assert_eq!(columns(&repo, "ticketchng"), change);
}

/// Invoke one checkout command with explicit repository and checkout selections.
/// # Arguments
/// * `binary` - Product executable.
/// * `home` - Isolated fixture home.
/// * `root` - Invocation directory.
/// * `repo` - Repository path.
/// * `args` - Verb and its flags.
/// # Returns
/// The captured output.
fn invoke(binary: &str, home: &Path, root: &Path, repo: &Path, args: &[&str]) -> Output {
    command(binary, home)
        .current_dir(root)
        .args(["v", args[0], "--repository"])
        .arg(repo)
        .args(["--checkout", "work"])
        .args(&args[1..])
        .output()
        .unwrap()
}

/// Every command family that opens an existing repository, with its explicit flags.
const FAMILIES: [&[&str]; 9] = [
    &["open"],
    &["info"],
    &["status"],
    &["add", "--path", "file"],
    &["scope", "--scope", "private", "--path", "file"],
    &[
        "commit",
        "--scope",
        "private",
        "--all-tracked",
        "--user",
        "CLI Owner",
        "--comment",
        "c",
        "--date-override",
        "2099-01-01T00:00:01",
    ],
    &["timeline"],
    &["log"],
    &["open", "--root", "ROOT2"],
];

/// Create a repository with one registered checkout, then replace its marker.
/// # Arguments
/// * `binary` - Product executable.
/// * `marker` - SQL that changes `g_storage`, outside every product path.
/// # Returns
/// The home, the registered root, a second unregistered root and the repository.
fn registered_with(binary: &str, marker: &str) -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let (temp, repo) = create(binary, "init");
    let root = temp.path().join("root");
    let second = temp.path().join("second");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&second).unwrap();
    succeed({
        let mut open = command(binary, temp.path());
        open.current_dir(&root)
            .args(["v", "open", "--repository"])
            .arg(&repo)
            .args(["--checkout", "work", "--root"])
            .arg(&root);
        open
    });
    std::fs::write(root.join("file"), b"content").unwrap();
    write(&repo, marker);
    (temp, root, second, repo)
}

/// Read the repository and its sidecars, so that a refusal can be shown to keep every byte.
/// # Arguments
/// * `repo` - Repository path.
/// # Returns
/// The bytes of the database and of each sidecar that exists.
fn bytes(repo: &Path) -> Vec<(String, Option<Vec<u8>>)> {
    ["", "-wal", "-shm", "-journal"]
        .iter()
        .map(|suffix| {
            let mut name = repo.as_os_str().to_owned();
            name.push(suffix);
            (suffix.to_string(), std::fs::read(&name).ok())
        })
        .collect()
}

/// Run each command family and require a refusal that keeps every byte.
/// # Arguments
/// * `binary` - Product executable.
/// * `marker` - SQL that changes `g_storage`.
/// * `code` - The exit status that each refusal must have.
/// * `message` - Text that standard error must equal, or contain when `exact` is false.
/// * `exact` - Whether standard error must equal `message` exactly.
fn refuse_every_family(binary: &str, marker: &str, code: i32, message: &str, exact: bool) {
    let (temp, root, second, repo) = registered_with(binary, marker);
    let canonical = std::fs::canonicalize(&repo).unwrap();
    let expected = message.replace("REPO", canonical.to_str().unwrap());
    let before = bytes(&repo);
    let locator = std::fs::read(root.join(".gossil/checkout.json")).unwrap();
    for family in FAMILIES {
        let args: Vec<&str> = family
            .iter()
            .map(|arg| {
                if *arg == "ROOT2" {
                    second.to_str().unwrap()
                } else {
                    arg
                }
            })
            .collect();
        let output = invoke(binary, temp.path(), &root, &repo, &args);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(code), "{family:?}: {stderr}");
        if exact {
            assert_eq!(stderr, expected, "{family:?}");
        } else {
            assert!(stderr.contains(&expected), "{family:?}: {stderr}");
        }
        assert!(output.stdout.is_empty(), "{family:?}");
        assert_eq!(bytes(&repo), before, "{family:?}");
        assert_eq!(
            std::fs::read(root.join(".gossil/checkout.json")).unwrap(),
            locator
        );
        assert!(!second.join(".gossil").exists(), "{family:?}");
    }
}

/// The three lines of CAT-07, with REPO in place of the repository path.
const OLDER: &str = "gossil: repository REPO is at catalog CATALOG, and this build needs catalog 6.\n\
Gossil has no upgrade path before its first real release.\n\
Create a new repository with gos v init or gos i init.\n";

#[test]
/// CAT-T03: catalogs 4 and 5 are refused by every command family before any effect,
/// with exit 78 and the message of CAT-07.
fn older_catalogs_are_refused_with_exit_78_and_every_byte_kept() {
    for catalog in [4, 5] {
        refuse_every_family(
            env!("CARGO_BIN_EXE_gos"),
            &format!(
                "PRAGMA ignore_check_constraints=ON; UPDATE g_storage SET storage_version={catalog}"
            ),
            78,
            &OLDER.replace("CATALOG", &catalog.to_string()),
            true,
        );
    }
    refuse_every_family(
        env!("CARGO_BIN_EXE_gossil"),
        "PRAGMA ignore_check_constraints=ON; UPDATE g_storage SET storage_version=4",
        78,
        &OLDER.replace("CATALOG", "4"),
        true,
    );
}

#[test]
/// CAT-T04: an unknown marker, a higher marker and no marker are refused without
/// the older-catalog exit status, and every byte stays unchanged.
fn unknown_higher_and_absent_markers_are_refused_with_every_byte_kept() {
    for marker in [
        "PRAGMA ignore_check_constraints=ON; UPDATE g_storage SET storage_version=3",
        "PRAGMA ignore_check_constraints=ON; UPDATE g_storage SET storage_version=7",
        "PRAGMA ignore_check_constraints=ON; UPDATE g_storage SET storage_version=x'36'",
        "DELETE FROM g_storage",
    ] {
        refuse_every_family(
            env!("CARGO_BIN_EXE_gos"),
            marker,
            1,
            "unsupported storage version",
            false,
        );
    }
}

/// Run the pinned Fossil reference through jlog and return its standard output.
/// # Arguments
/// * `fossil` - The explicit reference executable.
/// * `args` - Fossil arguments.
/// # Returns
/// Standard output.
/// # Panics
/// Panics when the reference fails.
fn fossil_run(fossil: &std::ffi::OsStr, args: &[&std::ffi::OsStr]) -> Vec<u8> {
    let output = Command::new("jlog")
        .arg("run")
        .arg(fossil)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
#[ignore = "requires explicit pinned native Fossil; run just catalog-interop"]
/// A new repository passes stock `fossil test-integrity`, and stock `fossil rebuild`
/// of a copy keeps the Gest ticket columns because of the `ticket-table` setting.
fn stock_fossil_checks_a_new_catalog6_repository() {
    use std::ffi::OsStr;
    let fossil =
        std::env::var_os("GOSSIL_FOSSIL_REFERENCE").expect("explicit reference executable");
    let (temp, repo) = create(env!("CARGO_BIN_EXE_gos"), "init");
    let integrity = String::from_utf8(fossil_run(
        &fossil,
        &[
            OsStr::new("test-integrity"),
            OsStr::new("-R"),
            repo.as_os_str(),
        ],
    ))
    .unwrap();
    assert!(integrity.contains("0 errors"), "{integrity}");
    let copy = temp.path().join("copy");
    std::fs::copy(&repo, &copy).unwrap();
    let ticket = columns(&copy, "ticket");
    let change = columns(&copy, "ticketchng");
    fossil_run(&fossil, &[OsStr::new("rebuild"), copy.as_os_str()]);
    assert_eq!(columns(&copy, "ticket"), ticket);
    assert_eq!(columns(&copy, "ticketchng"), change);
    assert!(ticket.contains(&"gest_id".to_owned()));
}

/// Each entry names an audit table and gives the SQL that inserts one row in it, with the parent rows that its foreign keys need.
const AUDIT_ROWS: [(&str, &str); 3] = [
    (
        "transactions",
        "INSERT INTO transactions VALUES('t','p',NULL,'task create',NULL,'2026-11-24T00:00:00Z')",
    ),
    (
        "transaction_events",
        "INSERT INTO transactions VALUES('t','p',NULL,'task create',NULL,'2026-11-24T00:00:00Z');\
         INSERT INTO transaction_events VALUES('e','t',NULL,'insert','r','g_gest_tasks',NULL,NULL,NULL,'2026-11-24T00:00:00Z')",
    ),
    (
        "g_command_audit",
        "INSERT INTO transactions VALUES('t','p',NULL,'task create',NULL,'2026-11-24T00:00:00Z');\
         INSERT INTO g_local_command VALUES('c',(SELECT owner_id FROM g_local_owner),'p',0,1,x'00',0,'gest_entity',NULL);\
         INSERT INTO g_command_audit VALUES('c','t',(SELECT owner_id FROM g_local_owner))",
    ),
];

#[test]
/// `gos` refuses a row in any audit table, with and without a registered
/// checkout. The refusal leaves every byte of the repository unchanged.
fn audit_table_rows_are_refused_with_and_without_a_registration() {
    let binary = env!("CARGO_BIN_EXE_gos");
    for (table, sql) in AUDIT_ROWS {
        let message = format!("unsupported nonempty catalog6 table: {table}");
        let (temp, repo) = create(binary, "init");
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        write(&repo, sql);
        let before = bytes(&repo);
        let output = command(binary, temp.path())
            .current_dir(&root)
            .args(["v", "open", "--repository"])
            .arg(&repo)
            .args(["--checkout", "work", "--root"])
            .arg(&root)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{table}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(&message),
            "{table}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(bytes(&repo), before, "{table}");
        assert!(!root.join(".gossil").exists(), "{table}");
        let (temp, root, _second, repo) = registered_with(binary, sql);
        let output = invoke(binary, temp.path(), &root, &repo, &["status"]);
        assert_eq!(output.status.code(), Some(1), "{table}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(&message),
            "{table}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
