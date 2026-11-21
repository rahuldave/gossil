//! Check explicit scope selection and private recording through fresh processes.
//!
//! ## Recommendations
//! Qualify both aliases with the shared locator and complete producer admission.
use std::{
    path::Path,
    process::{Command, Output},
};
/// Keep command invocations independent of inherited Gest selection/preferences.
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
/// Invoke one fresh process with explicit repository and registration selections.
/// # Arguments
/// * `binary` - Product executable to invoke.
/// * `home` - Isolated fixture home and configuration directory.
/// * `root` - Invocation directory inside the registered checkout.
/// * `repo` - Explicit repository database filename.
/// * `verb` - Checkout operation within the `v` group.
/// * `flags` - Additional explicit options for that operation.
/// # Returns
/// Exit status and captured standard output and error from the fresh process.
/// # Panics
/// Panics if the fixture process cannot be started or its output cannot be collected.
fn invoke(
    binary: &str,
    home: &Path,
    root: &Path,
    repo: &Path,
    verb: &str,
    flags: &[&str],
) -> Output {
    command(binary, home)
        .current_dir(root)
        .args(["v", verb, "--repository"])
        .arg(repo)
        .args(["--checkout", "work"])
        .args(flags)
        .output()
        .unwrap()
}
/// Capture every persisted table row using exact provider value debug encodings.
/// # Arguments
/// * `path` - Existing fixture database filename.
/// # Returns
/// Ordered table rows with scalar types and byte encodings preserved.
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
/// Capture complete safe raw database state from a synchronous process control.
/// # Arguments
/// * `path` - Existing fixture database filename.
/// # Returns
/// Deterministically ordered all-table scalar types and byte encodings.
/// # Panics
/// Panics if fixture SQL or runtime construction fails.
fn state(path: &Path) -> Vec<String> {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(snapshot(path))
}
/// Read the recorded checkout head independently of the command's printed receipt.
/// # Arguments
/// * `path` - Existing fixture repository with one recorded checkout named work.
/// # Returns
/// The exact persisted private manifest name.
/// # Panics
/// Panics on fixture SQL failure, a missing head or unexpected result cardinality.
fn recorded_head(path: &Path) -> String {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async {
            let database = libsql::Builder::new_local(path).build().await.unwrap();
            let connection = database.connect().unwrap();
            let mut rows = connection
                .query(
                    "SELECT combined_head FROM g_checkout WHERE checkout_id='work'",
                    (),
                )
                .await
                .unwrap();
            let head = rows
                .next()
                .await
                .unwrap()
                .unwrap()
                .get::<String>(0)
                .unwrap();
            assert!(rows.next().await.unwrap().is_none());
            head
        })
}
/// Check the current check-in's persisted metadata independently of CLI output.
/// # Arguments
/// * `path` - Existing fixture database with a recorded checkout named work.
/// * `actor` - Literal expected actor passed to the CLI.
/// * `comment` - Literal expected comment passed to the CLI.
/// * `date` - Literal expected date whose native SQL value must be stored.
/// # Panics
/// Panics if fixture SQL fails, metadata differs or the current event is not unique.
fn assert_recorded_metadata(path: &Path, actor: &str, comment: &str, date: &str) {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async {
            let database = libsql::Builder::new_local(path).build().await.unwrap();
            let connection = database.connect().unwrap();
            let mut rows = connection
                .query(
                    "SELECT event.user, event.comment, event.mtime=julianday(?1) \
                     FROM g_checkout JOIN blob ON blob.uuid=g_checkout.combined_head \
                     JOIN event ON event.objid=blob.rid \
                     WHERE g_checkout.checkout_id='work' AND event.type='ci'",
                    libsql::params![date],
                )
                .await
                .unwrap();
            let row = rows.next().await.unwrap().unwrap();
            assert_eq!(row.get::<String>(0).unwrap(), actor);
            assert_eq!(row.get::<String>(1).unwrap(), comment);
            assert_eq!(
                row.get::<i64>(2).unwrap(),
                1,
                "stored date differs from {date}"
            );
            assert!(rows.next().await.unwrap().is_none());
        });
}
/// Supply complete explicit recording metadata with a deterministic future date.
/// # Returns
/// Private all-tracked recording flags independent of the invocation clock.
fn commit_flags() -> [&'static str; 9] {
    [
        "--scope",
        "private",
        "--all-tracked",
        "--user",
        "CLI Owner",
        "--comment",
        "private child",
        "--date-override",
        "2099-01-01T00:00:01",
    ]
}
#[test]
/// Record edits through both aliases, preserve commits after output failure, and reject mixed scopes.
/// # Panics
/// Panics if either alias infers scope, loses durable state or changes working files.
fn fresh_process_scope_and_private_recording_lifecycle() {
    for binary in [env!("CARGO_BIN_EXE_gos"), env!("CARGO_BIN_EXE_gossil")] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let root = home.join("root");
        std::fs::create_dir(&root).unwrap();
        let repo = home.join("repo");
        let init = command(binary, home)
            .args(["v", "init"])
            .arg(&repo)
            .output()
            .unwrap();
        assert!(
            init.status.success(),
            "{}",
            String::from_utf8_lossy(&init.stderr)
        );
        let open = invoke(
            binary,
            home,
            &root,
            &repo,
            "open",
            &["--root", root.to_str().unwrap()],
        );
        assert!(
            open.status.success(),
            "{}",
            String::from_utf8_lossy(&open.stderr)
        );
        let locator = root.join(".gossil/checkout.json");
        let marker = std::fs::read(&locator).unwrap();
        std::fs::write(root.join("first"), b"live original").unwrap();
        let before = state(&repo);
        assert!(
            !invoke(
                binary,
                home,
                &root,
                &repo,
                "scope",
                &["--scope", "private", "--path", "first"]
            )
            .status
            .success()
        );
        assert_eq!(state(&repo), before);
        assert!(
            invoke(binary, home, &root, &repo, "add", &["--path", "first"])
                .status
                .success()
        );
        let unselected = state(&repo);
        assert!(
            !invoke(binary, home, &root, &repo, "commit", &commit_flags())
                .status
                .success()
        );
        assert_eq!(state(&repo), unselected);
        let scope = invoke(
            binary,
            home,
            &root,
            &repo,
            "scope",
            &["--scope", "private", "--path", "first"],
        );
        assert!(
            scope.status.success(),
            "{}",
            String::from_utf8_lossy(&scope.stderr)
        );
        assert_eq!(scope.stdout, b"Selected scopes: 1\n");
        let selected = state(&repo);
        assert!(
            invoke(
                binary,
                home,
                &root,
                &repo,
                "scope",
                &["--scope", "private", "--path", "first"]
            )
            .status
            .success()
        );
        assert_eq!(state(&repo), selected);
        let result = invoke(binary, home, &root, &repo, "commit", &commit_flags());
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let first_head = recorded_head(&repo);
        assert_recorded_metadata(&repo, "CLI Owner", "private child", "2099-01-01T00:00:01");
        assert_eq!(
            String::from_utf8(result.stdout).unwrap(),
            format!("Recorded private check-in: {first_head}\n")
        );
        let recorded = state(&repo);
        assert_ne!(recorded, selected);
        let status = invoke(binary, home, &root, &repo, "status", &[]);
        assert!(status.status.success());
        assert_eq!(
            String::from_utf8(status.stdout).unwrap(),
            "Tracked paths (A: pending addition, R: recorded):\nR\tprivate\t\"first\"\n"
        );
        let mut noop_flags = commit_flags();
        noop_flags[8] = "2099-01-01T00:00:02";
        let noop = invoke(binary, home, &root, &repo, "commit", &noop_flags);
        assert!(
            noop.status.success(),
            "{}",
            String::from_utf8_lossy(&noop.stderr)
        );
        assert_eq!(noop.stdout, b"No changes\n");
        assert_eq!(state(&repo), recorded);
        std::fs::write(root.join("first"), b"edited without re-add").unwrap();
        let mut repeated_flags = commit_flags();
        repeated_flags[4] = "Second Owner";
        repeated_flags[6] = "updated private child";
        repeated_flags[8] = "2099-01-01T12:00:00";
        let mut descriptors = [0; 2];
        // SAFETY: pipe initializes exactly two descriptors in this writable array.
        assert_eq!(unsafe { libc::pipe(descriptors.as_mut_ptr()) }, 0);
        use std::os::fd::FromRawFd;
        use std::process::Stdio;
        // SAFETY: each new read descriptor is adopted once and closed by File.
        let reader = unsafe { std::fs::File::from_raw_fd(descriptors[0]) };
        // SAFETY: this distinct new write descriptor is adopted once and closed by File.
        let writer = unsafe { std::fs::File::from_raw_fd(descriptors[1]) };
        drop(reader);
        let failed = command(binary, home)
            .current_dir(&root)
            .args(["v", "commit", "--repository"])
            .arg(&repo)
            .args(["--checkout", "work"])
            .args(repeated_flags)
            .stdout(Stdio::from(writer))
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
            .wait_with_output()
            .unwrap();
        assert_eq!(failed.status.code(), Some(1));
        let diagnostic = String::from_utf8_lossy(&failed.stderr);
        assert!(
            diagnostic.contains("Broken pipe") || diagnostic.contains("broken pipe"),
            "{diagnostic}"
        );
        let repeated = state(&repo);
        assert_ne!(repeated, recorded);
        assert_ne!(recorded_head(&repo), first_head);
        assert_recorded_metadata(
            &repo,
            "Second Owner",
            "updated private child",
            "2099-01-01T12:00:00",
        );
        for table in ["plink:", "event:", "mlink:"] {
            assert_eq!(
                repeated.iter().filter(|row| row.starts_with(table)).count(),
                3,
                "{table}"
            );
        }
        assert!(
            invoke(binary, home, &root, &repo, "open", &[])
                .status
                .success()
        );
        assert_eq!(state(&repo), repeated);
        let mut repeated_noop = commit_flags();
        repeated_noop[8] = "2099-01-02T00:00:00";
        let nochange = invoke(binary, home, &root, &repo, "commit", &repeated_noop);
        assert!(
            nochange.status.success(),
            "{}",
            String::from_utf8_lossy(&nochange.stderr)
        );
        assert_eq!(nochange.stdout, b"No changes\n");
        assert_eq!(state(&repo), repeated);
        assert_eq!(
            std::fs::read(root.join("first")).unwrap(),
            b"edited without re-add"
        );
        std::fs::write(root.join("second"), b"new unselected").unwrap();
        assert!(
            invoke(binary, home, &root, &repo, "add", &["--path", "second"])
                .status
                .success()
        );
        let status = invoke(binary, home, &root, &repo, "status", &[]);
        assert!(status.status.success());
        assert_eq!(
            String::from_utf8(status.stdout).unwrap(),
            "Tracked paths (A: pending addition, R: recorded):\nR\tprivate\t\"first\"\nA\tunselected\t\"second\"\n"
        );
        let added = state(&repo);
        assert!(
            !invoke(binary, home, &root, &repo, "commit", &noop_flags)
                .status
                .success()
        );
        assert_eq!(state(&repo), added);
        assert!(
            invoke(
                binary,
                home,
                &root,
                &repo,
                "scope",
                &["--scope", "shared", "--path", "second"]
            )
            .status
            .success()
        );
        let mixed = state(&repo);
        assert!(
            !invoke(binary, home, &root, &repo, "commit", &noop_flags)
                .status
                .success()
        );
        assert_eq!(state(&repo), mixed);
        assert!(
            !invoke(
                binary,
                home,
                &root,
                &repo,
                "scope",
                &["--scope", "private", "--path", "second"]
            )
            .status
            .success()
        );
        assert_eq!(state(&repo), mixed);
        assert!(
            invoke(binary, home, &root, &repo, "open", &[])
                .status
                .success()
        );
        assert_eq!(state(&repo), mixed);
        assert_eq!(
            std::fs::read(root.join("first")).unwrap(),
            b"edited without re-add"
        );
        assert_eq!(
            std::fs::read(root.join("second")).unwrap(),
            b"new unselected"
        );
        assert_eq!(std::fs::read(&locator).unwrap(), marker);
        // Existing exact selection still requires the complete locator binding.
        std::fs::remove_file(&locator).unwrap();
        let expected = state(&repo);
        assert!(
            !invoke(
                binary,
                home,
                &root,
                &repo,
                "scope",
                &["--scope", "private", "--path", "first"]
            )
            .status
            .success()
        );
        assert!(
            !invoke(binary, home, &root, &repo, "commit", &noop_flags)
                .status
                .success()
        );
        assert_eq!(state(&repo), expected);
    }
}
#[test]
/// Reject incomplete grammar and private recording of shared scopes without writes.
/// # Panics
/// Panics if malformed selectors or metadata mutate durable state.
fn explicit_grammar_and_shared_recording_refuse_without_writes() {
    let binary = env!("CARGO_BIN_EXE_gos");
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let root = home.join("root");
    std::fs::create_dir(&root).unwrap();
    let repo = home.join("repo");
    assert!(
        command(binary, home)
            .args(["v", "init"])
            .arg(&repo)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        invoke(
            binary,
            home,
            &root,
            &repo,
            "open",
            &["--root", root.to_str().unwrap()]
        )
        .status
        .success()
    );
    let expected = state(&repo);
    for flags in [
        vec![],
        vec!["--path", "first"],
        vec!["--scope", "private"],
        vec!["--scope", "automatic", "--path", "first"],
        vec![
            "--scope",
            "private",
            "--path",
            "first",
            "--checkout",
            "other",
        ],
        vec!["--scope", "private", "--path", "first", "--all-tracked"],
    ] {
        assert!(
            !invoke(binary, home, &root, &repo, "scope", &flags)
                .status
                .success()
        );
        assert_eq!(state(&repo), expected);
    }
    let complete = commit_flags();
    for remove in [0usize, 2, 3, 5, 7] {
        let mut flags = complete.to_vec();
        let count = if remove == 2 { 1 } else { 2 };
        flags.drain(remove..remove + count);
        assert!(
            !invoke(binary, home, &root, &repo, "commit", &flags)
                .status
                .success()
        );
        assert_eq!(state(&repo), expected);
    }
    for suffix in [
        vec!["--path", "first"],
        vec!["--scope", "private"],
        vec!["--all-tracked"],
        vec!["--user", "Other"],
        vec!["--force"],
        vec!["--date-override", "2099-01-01T00:00:02"],
    ] {
        let mut flags = complete.to_vec();
        flags.extend(suffix);
        assert!(
            !invoke(binary, home, &root, &repo, "commit", &flags)
                .status
                .success()
        );
        assert_eq!(state(&repo), expected);
    }
    let mut shared = complete;
    shared[1] = "shared";
    assert!(
        !invoke(binary, home, &root, &repo, "commit", &shared)
            .status
            .success()
    );
    assert_eq!(state(&repo), expected);
    std::fs::write(root.join("shared-note"), b"keep shared bytes").unwrap();
    assert!(
        invoke(
            binary,
            home,
            &root,
            &repo,
            "add",
            &["--path", "shared-note"]
        )
        .status
        .success()
    );
    assert!(
        invoke(
            binary,
            home,
            &root,
            &repo,
            "scope",
            &["--scope", "shared", "--path", "shared-note"]
        )
        .status
        .success()
    );
    let shared_state = state(&repo);
    let status = invoke(binary, home, &root, &repo, "status", &[]);
    assert!(status.status.success());
    assert_eq!(
        String::from_utf8(status.stdout).unwrap(),
        "Tracked paths (A: pending addition, R: recorded):\nA\tshared\t\"shared-note\"\n"
    );
    let refused = invoke(binary, home, &root, &repo, "commit", &complete);
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr)
            .contains("recording requires explicit private scope for every tracked path")
    );
    let refused = invoke(
        binary,
        home,
        &root,
        &repo,
        "scope",
        &["--scope", "private", "--path", "shared-note"],
    );
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("scope selection is immutable"));
    assert_eq!(state(&repo), shared_state);
    assert_eq!(
        std::fs::read(root.join("shared-note")).unwrap(),
        b"keep shared bytes"
    );
}
