//! Native executable registration and locator recovery without Gest project initialization.
use std::os::unix::fs::PermissionsExt;
use std::{
    path::Path,
    process::{Command, Output},
};

/// Supply isolated preferences without supplying project or author configuration.
fn command(home: &Path) -> Command {
    command_for(env!("CARGO_BIN_EXE_gossil"), home)
}
/// Isolate each executable alias from ambient inherited configuration.
fn command_for(binary: &str, home: &Path) -> Command {
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
        .env_remove("GEST_CONFIG")
        .env_remove("GEST_STORAGE__DATA_DIR")
        .env_remove("GEST_DATABASE__URL");
    command
}
/// Execute a chosen explicit native checkout operation.
/// # Panics
/// Panics if fixture setup, child I/O or an expected repository or filesystem operation fails.
fn open(home: &Path, repository: &Path, root: &Path, flags: &[&str]) -> Output {
    command(home)
        .args(["vcs", "open", "--repository"])
        .arg(repository)
        .current_dir(root)
        .arg("--checkout")
        .arg("native")
        .args(flags)
        .output()
        .unwrap()
}
/// First registration publishes real identities; exact-root recovery changes neither identity.
#[test]
/// # Panics
/// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
fn register_reopen_and_missing_locator_recovery_need_no_project() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let repository = temp.path().join("gossil.db");
    let init = command(temp.path())
        .args(["vcs", "init"])
        .arg(&repository)
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let registered = open(
        temp.path(),
        &repository,
        &root,
        &["--root", root.to_str().unwrap()],
    );
    assert!(
        registered.status.success(),
        "{}",
        String::from_utf8_lossy(&registered.stderr)
    );
    let path = root.join(".gossil/checkout.json");
    let bytes = std::fs::read(&path).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 4);
    assert_eq!(value["checkout_id"], "native");
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(root.join(".gossil"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert!(open(temp.path(), &repository, &root, &[]).status.success());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_file(&path).unwrap();
    assert!(open(temp.path(), &repository, &root, &[]).status.success());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(
        !command(temp.path())
            .current_dir(&root)
            .args(["vcs", "open", "--repository"])
            .arg(&repository)
            .args(["--checkout", "wrong"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(!root.join(".gest").exists());
}
/// Incomplete markers require existing facts; malformed locators are preserved as conflicts.
#[test]
/// # Panics
/// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
fn incomplete_and_conflicting_markers_never_allocate_or_replace() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let marker = root.join(".gossil");
    std::fs::create_dir(&marker).unwrap();
    std::fs::set_permissions(&marker, std::fs::Permissions::from_mode(0o700)).unwrap();
    let missing = temp.path().join("absent.db");
    assert!(!open(temp.path(), &missing, &root, &[]).status.success());
    assert!(!missing.exists());
    let repository = temp.path().join("gossil.db");
    assert!(
        command(temp.path())
            .args(["vcs", "init"])
            .arg(&repository)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(!open(temp.path(), &repository, &root, &[]).status.success());
    let incomplete_registration = open(
        temp.path(),
        &repository,
        &root,
        &["--root", root.to_str().unwrap()],
    );
    assert!(!incomplete_registration.status.success());
    let incomplete_stderr = String::from_utf8_lossy(&incomplete_registration.stderr);
    assert!(
        incomplete_stderr.contains("incomplete checkout marker forbids new registration"),
        "expected incomplete-marker registration refusal, got: {incomplete_stderr}"
    );
    let path = marker.join("checkout.json");
    std::fs::write(&path, b"malformed").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let malformed_locator = open(temp.path(), &repository, &root, &[]);
    assert!(!malformed_locator.status.success());
    let malformed_stderr = String::from_utf8_lossy(&malformed_locator.stderr);
    assert!(
        malformed_stderr.contains("invalid or incomplete checkout locator"),
        "expected malformed-locator refusal, got: {malformed_stderr}"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"malformed");
}
/// Effective URL routes and explicit configured directory conflicts reject before marker creation.
#[test]
/// # Panics
/// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
fn configuration_preflight_applies_to_explicit_repository_flags() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let repository = temp.path().join("gossil.db");
    assert!(
        command(temp.path())
            .args(["vcs", "init"])
            .arg(&repository)
            .output()
            .unwrap()
            .status
            .success()
    );
    let config = temp.path().join("gest.toml");
    std::fs::write(
        &config,
        "[database]\nurl='sqlite:///not-a-native-selection'\n",
    )
    .unwrap();
    let unsupported_provider = command(temp.path())
        .env("GEST_CONFIG", &config)
        .args(["vcs", "open", "--repository"])
        .arg(&repository)
        .arg("--root")
        .arg(&root)
        .args(["--checkout", "native"])
        .output()
        .unwrap();
    assert!(!unsupported_provider.status.success());
    let provider_stderr = String::from_utf8_lossy(&unsupported_provider.stderr);
    assert!(
        provider_stderr.contains("configured database URL/host provider is unsupported"),
        "expected configured-provider preflight refusal, got: {provider_stderr}"
    );
    assert!(!root.join(".gossil").exists());
    let other = temp.path().join("other");
    std::fs::create_dir(&other).unwrap();
    assert!(
        command(temp.path())
            .args(["vcs", "init"])
            .arg(other.join("gossil.db"))
            .output()
            .unwrap()
            .status
            .success()
    );
    std::fs::write(
        &config,
        format!("[storage]\ndata_dir={:?}\n", other.to_str().unwrap()),
    )
    .unwrap();
    let conflicting_storage = command(temp.path())
        .env("GEST_CONFIG", &config)
        .args(["vcs", "open", "--repository"])
        .arg(&repository)
        .arg("--root")
        .arg(&root)
        .args(["--checkout", "native"])
        .output()
        .unwrap();
    assert!(!conflicting_storage.status.success());
    let storage_stderr = String::from_utf8_lossy(&conflicting_storage.stderr);
    assert!(
        storage_stderr.contains("configured storage conflicts with explicit repository"),
        "expected configured-storage preflight refusal, got: {storage_stderr}"
    );
    assert!(!root.join(".gossil").exists());
}

/// Read every application table through a read-only connection, preserving native value classes.
/// # Panics
/// Panics if fixture setup, child I/O or an expected repository or filesystem operation fails.
async fn state(path: &Path) -> Vec<(String, Vec<Vec<libsql::Value>>)> {
    let database = libsql::Builder::new_local(path)
        .flags(libsql::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .build()
        .await
        .unwrap();
    let connection = database.connect().unwrap();
    let mut names = connection
        .query(
            "SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name",
            (),
        )
        .await
        .unwrap();
    let mut result = Vec::new();
    while let Some(row) = names.next().await.unwrap() {
        let name: String = row.get(0).unwrap();
        let mut cursor = connection
            .query(
                &format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")),
                (),
            )
            .await
            .unwrap();
        let mut rows = Vec::new();
        while let Some(row) = cursor.next().await.unwrap() {
            rows.push(
                (0..row.column_count())
                    .map(|i| row.get_value(i).unwrap())
                    .collect::<Vec<_>>(),
            );
        }
        rows.sort_by_cached_key(|row| format!("{row:?}"));
        result.push((name, rows));
    }
    result
}
/// Failed exact-root recovery and wrong pinned repository preserve every table and marker byte.
#[tokio::test]
/// # Panics
/// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
async fn exact_root_recovery_rejects_absent_replaced_or_wrong_repository_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let repository = temp.path().join("gossil.db");
    assert!(
        command(temp.path())
            .args(["vcs", "init"])
            .arg(&repository)
            .output()
            .unwrap()
            .status
            .success()
    );
    let empty = state(&repository).await;
    assert!(!open(temp.path(), &repository, &root, &[]).status.success());
    assert_eq!(state(&repository).await, empty);
    assert!(!root.join(".gossil").exists());
    assert!(
        open(
            temp.path(),
            &repository,
            &root,
            &["--root", root.to_str().unwrap()]
        )
        .status
        .success()
    );
    let path = root.join(".gossil/checkout.json");
    let bytes = std::fs::read(&path).unwrap();
    let before = state(&repository).await;
    let other = temp.path().join("other.db");
    assert!(
        command(temp.path())
            .args(["vcs", "init"])
            .arg(&other)
            .output()
            .unwrap()
            .status
            .success()
    );
    let other_before = state(&other).await;
    assert!(!open(temp.path(), &other, &root, &[]).status.success());
    assert_eq!(state(&repository).await, before);
    assert_eq!(state(&other).await, other_before);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_file(&path).unwrap();
    assert!(open(temp.path(), &repository, &root, &[]).status.success());
    assert_eq!(state(&repository).await, before);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    // Replace the whole root while preserving locator bytes: device/inode admission must reject.
    let moved = temp.path().join("moved-root");
    std::fs::rename(&root, &moved).unwrap();
    std::fs::create_dir(&root).unwrap();
    std::fs::rename(moved.join(".gossil"), root.join(".gossil")).unwrap();
    assert!(!open(temp.path(), &repository, &root, &[]).status.success());
    assert_eq!(state(&repository).await, before);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}
/// Source controls inherited from the adopted config composition run in separate processes.
#[test]
/// # Panics
/// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
fn inherited_configuration_shape_case_relative_and_read_failures_precede_mutation() {
    for (index, mode) in [
        "invalid-utf8",
        "scalar",
        "scalar-only",
        "route-type",
        "mixed-case",
        "relative",
        "host",
        "layer",
    ]
    .into_iter()
    .enumerate()
    {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let data = temp.path().join("data");
        std::fs::create_dir(&data).unwrap();
        let repository = data.join("gossil.db");
        assert!(
            command(temp.path())
                .args(["vcs", "init"])
                .arg(&repository)
                .output()
                .unwrap()
                .status
                .success()
        );
        let config = temp.path().join("gest.toml");
        std::fs::write(
            &config,
            format!(
                "[storage]\ndata_dir={:?}\n",
                temp.path().join("wrong-selection").to_str().unwrap()
            ),
        )
        .unwrap();
        let mut selected = command(temp.path());
        selected.current_dir(&root).env("GEST_CONFIG", &config);
        match mode {
            "invalid-utf8" => std::fs::write(&config, [255]).unwrap(),
            "scalar" => {
                selected
                    .env("GEST_STORAGE", "scalar")
                    .env("GEST_STORAGE__DATA_DIR", &data);
            }
            "scalar-only" => {
                selected.env("GEST_STORAGE", "scalar");
            }
            "route-type" => {
                std::fs::write(&config, "[database]\nurl=42\n").unwrap();
            }
            "mixed-case" => {
                selected.env("GEST_storage__data_dir", &data);
            }
            "relative" => {
                selected.env("GEST_STORAGE__DATA_DIR", "ignored-relative");
            }
            "host" => {
                selected.env("GEST_DATABASE__HOST", "credentials-must-not-be-echoed");
            }
            "layer" => {
                std::fs::create_dir(root.join(".config")).unwrap();
                std::fs::write(
                    root.join(".config/gest.toml"),
                    "[storage]\ndata_dir='/wrong-local'\n",
                )
                .unwrap();
                std::fs::write(
                    root.join(".gest.toml"),
                    format!("[storage]\ndata_dir={:?}\n", data.to_str().unwrap()),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let result = selected
            .args(["vcs", "open", "--repository"])
            .arg(&repository)
            .args(["--checkout", "native", "--root"])
            .arg(&root)
            .output()
            .unwrap();
        let expected = matches!(mode, "mixed-case" | "relative" | "layer");
        assert_eq!(
            result.status.success(),
            expected,
            "fixture {index}/{mode}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        if !expected {
            assert!(!root.join(".gossil").exists());
            assert!(
                !String::from_utf8_lossy(&result.stderr).contains("credentials-must-not-be-echoed")
            );
        }
    }
}
/// Source executable aliases share one bounded command with registration requested only by `--root`.
#[test]
/// # Panics
/// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
fn bounded_grammar_requires_explicit_ids_and_routes_both_binary_and_group_aliases() {
    for (index, (binary, group)) in [
        (env!("CARGO_BIN_EXE_gossil"), "vcs"),
        (env!("CARGO_BIN_EXE_gos"), "v"),
    ]
    .into_iter()
    .enumerate()
    {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let repository = temp.path().join("gossil.db");
        assert!(
            command(temp.path())
                .args(["vcs", "init"])
                .arg(&repository)
                .output()
                .unwrap()
                .status
                .success()
        );
        let result = command_for(binary, temp.path())
            .current_dir(&root)
            .args([group, "open", "--repository"])
            .arg(&repository)
            .args(["--checkout", "native", "--root"])
            .arg(&root)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "alias {index}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        for args in [
            vec!["--repository"],
            vec!["--checkout", "native"],
            vec!["--register"],
            vec!["--checkout", "native", "--checkout", "duplicate"],
        ] {
            let result = command(temp.path())
                .args(["vcs", "open"])
                .args(args)
                .output()
                .unwrap();
            assert!(!result.status.success());
        }
    }
}

/// Invocation configuration belongs to cwd, even when first registration targets a sibling directory.
#[tokio::test]
/// # Panics
/// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
async fn invocation_cwd_configuration_cannot_be_bypassed_by_explicit_target_root() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().join("invocation");
    let target = temp.path().join("checkout");
    std::fs::create_dir(&cwd).unwrap();
    std::fs::create_dir(&target).unwrap();
    let repository = temp.path().join("gossil.db");
    assert!(
        command(temp.path())
            .args(["vcs", "init"])
            .arg(&repository)
            .output()
            .unwrap()
            .status
            .success()
    );
    std::fs::write(
        cwd.join(".gest.toml"),
        "[database]\nurl='sqlite:///private-cwd-route'\n",
    )
    .unwrap();
    let before = state(&repository).await;
    let rejected = command(temp.path())
        .current_dir(&cwd)
        .args(["vcs", "open", "--repository"])
        .arg(&repository)
        .args(["--checkout", "native", "--root"])
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        !rejected.status.success(),
        "cwd URL must reject before registration"
    );
    let diagnostic = String::from_utf8_lossy(&rejected.stderr);
    assert!(
        diagnostic.contains("configured database URL/host provider is unsupported"),
        "{diagnostic}"
    );
    assert!(!diagnostic.contains("private-cwd-route"));
    assert!(!target.join(".gossil").exists());
    assert_eq!(state(&repository).await, before);
}

/// A real closed stdout peer fails after publication; both registration and reopen retain durable state.
#[tokio::test]
/// # Panics
/// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
async fn output_broken_pipe_preserves_published_registration_and_reopen_state() {
    use std::os::fd::FromRawFd;
    use std::process::Stdio;
    for reopening in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let repository = temp.path().join("gossil.db");
        assert!(
            command(temp.path())
                .args(["vcs", "init"])
                .arg(&repository)
                .output()
                .unwrap()
                .status
                .success()
        );
        if reopening {
            assert!(
                open(
                    temp.path(),
                    &repository,
                    &root,
                    &["--root", root.to_str().unwrap()]
                )
                .status
                .success()
            );
        }
        let mut descriptors = [0; 2];
        // SAFETY: pipe initializes exactly two descriptors in this writable array.
        assert_eq!(unsafe { libc::pipe(descriptors.as_mut_ptr()) }, 0);
        // SAFETY: each newly allocated descriptor is adopted exactly once and closed by File.
        let reader = unsafe { std::fs::File::from_raw_fd(descriptors[0]) };
        // SAFETY: this is the distinct newly allocated write descriptor.
        let writer = unsafe { std::fs::File::from_raw_fd(descriptors[1]) };
        drop(reader);
        let mut attempt = command(temp.path());
        attempt
            .current_dir(&root)
            .args(["vcs", "open", "--repository"])
            .arg(&repository)
            .args(["--checkout", "native"]);
        if !reopening {
            attempt.arg("--root").arg(&root);
        }
        let failed = attempt
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
        let path = root.join(".gossil/checkout.json");
        let bytes = std::fs::read(&path).unwrap();
        let published: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(published["checkout_id"], "native");
        let before = state(&repository).await;
        assert!(open(temp.path(), &repository, &root, &[]).status.success());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(state(&repository).await, before);
    }
}

/// Explicit registration preserves its exact requested root; only rootless reopen discovers ancestors.
#[tokio::test]
/// # Panics
/// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
async fn explicit_child_registration_refuses_ancestor_reuse_but_rootless_reopen_discovers_it() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let child = root.join("child");
    std::fs::create_dir_all(&child).unwrap();
    let repository = temp.path().join("gossil.db");
    assert!(
        command(temp.path())
            .args(["vcs", "init"])
            .arg(&repository)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        open(
            temp.path(),
            &repository,
            &root,
            &["--root", root.to_str().unwrap()]
        )
        .status
        .success()
    );
    let locator = root.join(".gossil/checkout.json");
    let bytes = std::fs::read(&locator).unwrap();
    let before = state(&repository).await;
    let rejected = open(
        temp.path(),
        &repository,
        &child,
        &["--root", child.to_str().unwrap()],
    );
    assert!(
        !rejected.status.success(),
        "explicit child root must not silently reuse ancestor registration"
    );
    let diagnostic = String::from_utf8_lossy(&rejected.stderr);
    assert!(
        diagnostic.contains("explicit checkout root differs from discovered registration root"),
        "{diagnostic}"
    );
    assert_eq!(state(&repository).await, before);
    assert_eq!(std::fs::read(&locator).unwrap(), bytes);
    assert!(!child.join(".gossil").exists());
    assert!(open(temp.path(), &repository, &child, &[]).status.success());
    assert!(
        open(
            temp.path(),
            &repository,
            &root,
            &["--root", root.to_str().unwrap()]
        )
        .status
        .success()
    );
    assert_eq!(state(&repository).await, before);
    assert_eq!(std::fs::read(&locator).unwrap(), bytes);
    assert!(!child.join(".gossil").exists());
}

/// An explicit existing regular file cannot become a checkout root or cause registration.
#[tokio::test]
/// # Panics
/// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
async fn non_directory_checkout_root_rejects_without_database_or_marker_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let repository = temp.path().join("gossil.db");
    assert!(
        command(temp.path())
            .args(["vcs", "init"])
            .arg(&repository)
            .output()
            .unwrap()
            .status
            .success()
    );
    let root_file = temp.path().join("file-root");
    std::fs::write(&root_file, b"unchanged").unwrap();
    let before = state(&repository).await;
    let result = command(temp.path())
        .current_dir(temp.path())
        .args(["vcs", "open", "--repository"])
        .arg(&repository)
        .args(["--checkout", "native", "--root"])
        .arg(&root_file)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("checkout root is not a directory"));
    assert_eq!(state(&repository).await, before);
    assert_eq!(std::fs::read(&root_file).unwrap(), b"unchanged");
    assert!(!temp.path().join(".gossil").exists());
}

/// Rootless recovery propagates real caller-ID and persisted repository-ID conflicts without publication.
#[tokio::test]
/// # Panics
/// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
async fn rootless_recovery_rejects_wrong_checkout_and_same_path_repository_identity() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let repository = temp.path().join("gossil.db");
    assert!(
        command(temp.path())
            .args(["vcs", "init"])
            .arg(&repository)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        open(
            temp.path(),
            &repository,
            &root,
            &["--root", root.to_str().unwrap()]
        )
        .status
        .success()
    );
    let locator = root.join(".gossil/checkout.json");
    let original = std::fs::read(&locator).unwrap();
    let before = state(&repository).await;
    std::fs::remove_file(&locator).unwrap();
    let wrong_checkout = command(temp.path())
        .current_dir(&root)
        .args(["vcs", "open", "--repository"])
        .arg(&repository)
        .args(["--checkout", "wrong"])
        .output()
        .unwrap();
    assert!(!wrong_checkout.status.success());
    assert!(
        String::from_utf8_lossy(&wrong_checkout.stderr)
            .contains("explicit checkout differs from exact-root registration")
    );
    assert!(!locator.exists());
    assert_eq!(state(&repository).await, before);
    assert!(open(temp.path(), &repository, &root, &[]).status.success());
    assert_eq!(std::fs::read(&locator).unwrap(), original);
    let mut conflicting: serde_json::Value = serde_json::from_slice(&original).unwrap();
    let wrong_identity = if conflicting["repository_id"] == "f".repeat(40) {
        "a".repeat(40)
    } else {
        "f".repeat(40)
    };
    conflicting["repository_id"] = serde_json::Value::String(wrong_identity);
    let conflicting = serde_json::to_vec(&conflicting).unwrap();
    std::fs::write(&locator, &conflicting).unwrap();
    let wrong_repository = open(temp.path(), &repository, &root, &[]);
    assert!(!wrong_repository.status.success());
    assert!(
        String::from_utf8_lossy(&wrong_repository.stderr)
            .contains("checkout locator differs from admitted native binding")
    );
    assert_eq!(std::fs::read(&locator).unwrap(), conflicting);
    assert_eq!(state(&repository).await, before);
}
