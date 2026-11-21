//! Current recorded checkout identity through every executable and group alias.
//!
//! ## Recommendations
//! Compare real authenticated heads and complete logical state before and after info.
#[path = "support/closed_stdout.rs"]
mod closed_stdout;

use gossil::artifact::ParseLimits;
use gossil::storage::{AdmissionLimits, CheckoutView, Limits, LocalOwnerRepository};
use std::{
    path::Path,
    process::{Command, Output},
};

/// Isolate native commands from inherited Gest configuration.
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
/// Require successful fixture setup and preserve child diagnostics on failure.
fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
/// Read the real checkout identity independently through existing authenticated storage APIs.
fn view(repository: &Path) -> CheckoutView {
    let limits = AdmissionLimits {
        content: Limits {
            max_stored_bytes: 1048576,
            max_output_bytes: 1048576,
            max_chain_depth: 128,
            max_total_stored_bytes: 67108864,
            max_total_decoded_bytes: 67108864,
        },
        parser: ParseLimits {
            input_bytes: 1048576,
            cards: 4096,
            owned_bytes: 8388608,
            entries: 4096,
        },
    };
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async {
            let mut owner = LocalOwnerRepository::open(repository, limits)
                .await
                .unwrap();
            owner.checkout_view("work", limits).await.unwrap()
        })
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

/// Adapt Fossil set-manifest.test:68-73: info's checkout equals independently authenticated identity.
/// Copyright (c) 2016 D. Richard Hipp; inherited assertion under the Simplified BSD License.
/// Native paths use JSON strings and private-head is none before the first recording.
#[test]
fn initial_info_reports_exact_identity_and_preserves_state() {
    for binary in [env!("CARGO_BIN_EXE_gossil"), env!("CARGO_BIN_EXE_gos")] {
        for group in ["vcs", "v"] {
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path();
            let root = home.join("root");
            std::fs::create_dir(&root).unwrap();
            let repository = home.join("repository.db");
            success(
                command(binary, home)
                    .args([group, "init"])
                    .arg(&repository)
                    .output()
                    .unwrap(),
            );
            success(
                command(binary, home)
                    .current_dir(&root)
                    .args([group, "open", "--repository"])
                    .arg(&repository)
                    .args(["--checkout", "work", "--root"])
                    .arg(&root)
                    .output()
                    .unwrap(),
            );
            let working = root.join("ordinary");
            std::fs::write(&working, b"untracked\0working bytes").unwrap();
            let locator = root.join(".gossil/checkout.json");
            let locator_before = std::fs::read(&locator).unwrap();
            let identity = view(&repository);
            assert!(identity.combined_head.is_none());
            assert_eq!(
                identity.registered_root,
                std::fs::canonicalize(&root).unwrap()
            );
            let before = state(&repository);
            let output = success(
                command(binary, home)
                    .current_dir(&root)
                    .args([group, "info", "--repository"])
                    .arg(&repository)
                    .args(["--checkout", "work"])
                    .output()
                    .unwrap(),
            );
            let expected = format!(
                "repository: {}\nlocal-root: {}\ncheckout: {}\nshared-head: {}\nprivate-head: none\n",
                serde_json::to_string(repository.to_str().unwrap()).unwrap(),
                serde_json::to_string(identity.registered_root.to_str().unwrap()).unwrap(),
                identity.shared_head,
                identity.shared_head
            );
            assert_eq!(output.stdout, expected.as_bytes());
            assert!(output.stderr.is_empty());
            assert_eq!(state(&repository), before);
            assert_eq!(std::fs::read(&locator).unwrap(), locator_before);
            assert_eq!(
                std::fs::read(&working).unwrap(),
                b"untracked\0working bytes"
            );
            assert_eq!(view(&repository), identity);
        }
    }
}

/// Real opened checkout shared by lifecycle and refusal controls.
struct Fixture {
    /// Retain the isolated home until every process exits.
    temp: tempfile::TempDir,
    /// Existing repository selector.
    repository: std::path::PathBuf,
    /// Registered working root.
    root: std::path::PathBuf,
}
impl Fixture {
    /// Initialize and open with real CLI identity producers.
    fn new(binary: &str, group: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root-\"\\\nλ");
        std::fs::create_dir(&root).unwrap();
        let repository = temp.path().join("repository-\"\\\nλ.db");
        success(
            command(binary, temp.path())
                .args([group, "init"])
                .arg(&repository)
                .output()
                .unwrap(),
        );
        success(
            command(binary, temp.path())
                .current_dir(&root)
                .args([group, "open", "--repository"])
                .arg(&repository)
                .args(["--checkout", "work", "--root"])
                .arg(&root)
                .output()
                .unwrap(),
        );
        Self {
            temp,
            repository,
            root,
        }
    }
    /// Build explicit selection with the requested operation and group alias.
    fn operation(&self, binary: &str, group: &str, verb: &str) -> Command {
        let mut result = command(binary, self.temp.path());
        result
            .current_dir(&self.root)
            .args([group, verb, "--repository"])
            .arg(&self.repository)
            .args(["--checkout", "work"]);
        result
    }
    /// Verify every field against independently admitted heads and preserve all logical state.
    fn info(&self, binary: &str, group: &str, selector: &Path) {
        let identity = view(&self.repository);
        let before = state(&self.repository);
        let locator = std::fs::read(self.root.join(".gossil/checkout.json")).unwrap();
        let working = self.root.join("ordinary");
        let working_before = std::fs::read(&working).ok();
        let output = success(
            command(binary, self.temp.path())
                .current_dir(&self.root)
                .args([group, "info", "--repository"])
                .arg(selector)
                .args(["--checkout", "work"])
                .output()
                .unwrap(),
        );
        let expected = format!(
            "repository: {}\nlocal-root: {}\ncheckout: {}\nshared-head: {}\nprivate-head: {}\n",
            serde_json::to_string(selector.to_str().unwrap()).unwrap(),
            serde_json::to_string(identity.registered_root.to_str().unwrap()).unwrap(),
            identity
                .combined_head
                .as_deref()
                .unwrap_or(&identity.shared_head),
            identity.shared_head,
            identity.combined_head.as_deref().unwrap_or("none")
        );
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(output.stderr.is_empty());
        assert_eq!(state(&self.repository), before);
        assert_eq!(
            std::fs::read(self.root.join(".gossil/checkout.json")).unwrap(),
            locator
        );
        assert_eq!(std::fs::read(working).ok(), working_before);
        assert_eq!(view(&self.repository), identity);
    }
    /// Refuse with no stdout and without changing database or checkout files.
    fn refuse(&self, mut command: Command, cause: &str) {
        let before = state(&self.repository);
        let locator = std::fs::read(self.root.join(".gossil/checkout.json")).ok();
        let working = std::fs::read(self.root.join("ordinary")).ok();
        let output = command.output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(cause),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(state(&self.repository), before);
        assert_eq!(
            std::fs::read(self.root.join(".gossil/checkout.json")).ok(),
            locator
        );
        assert_eq!(std::fs::read(self.root.join("ordinary")).ok(), working);
    }
}

/// Repeated recordings change effective checkout; uncommitted edits do not change recorded identity.
#[test]
fn recording_reopen_alias_paths_and_output_failure() {
    for binary in [env!("CARGO_BIN_EXE_gossil"), env!("CARGO_BIN_EXE_gos")] {
        for group in ["vcs", "v"] {
            let fixture = Fixture::new(binary, group);
            let initial = view(&fixture.repository);
            fixture.info(binary, group, &fixture.repository);
            let relative = Path::new("..").join(fixture.repository.file_name().unwrap());
            fixture.info(binary, group, &relative);
            let alias = fixture.temp.path().join("alias.db");
            std::os::unix::fs::symlink(&fixture.repository, &alias).unwrap();
            fixture.info(binary, group, &alias);
            std::fs::write(fixture.root.join("ordinary"), b"first\0bytes").unwrap();
            success(
                fixture
                    .operation(binary, group, "add")
                    .args(["--path", "ordinary"])
                    .output()
                    .unwrap(),
            );
            success(
                fixture
                    .operation(binary, group, "scope")
                    .args(["--scope", "private", "--path", "ordinary"])
                    .output()
                    .unwrap(),
            );
            let mut previous = initial.shared_head.clone();
            for (date, bytes) in [
                ("2099-01-01T00:00:01", b"first\0bytes".as_slice()),
                ("2099-01-01T00:00:02", b"second bytes".as_slice()),
            ] {
                std::fs::write(fixture.root.join("ordinary"), bytes).unwrap();
                success(
                    fixture
                        .operation(binary, group, "commit")
                        .args([
                            "--scope",
                            "private",
                            "--all-tracked",
                            "--user",
                            "Info Owner",
                            "--comment",
                            "identity control",
                            "--date-override",
                            date,
                        ])
                        .output()
                        .unwrap(),
                );
                let recorded = view(&fixture.repository);
                assert_eq!(recorded.shared_head, initial.shared_head);
                assert_ne!(recorded.combined_head.as_ref().unwrap(), &previous);
                previous = recorded.combined_head.clone().unwrap();
                fixture.info(binary, group, &fixture.repository);
                std::fs::write(fixture.root.join("ordinary"), b"uncommitted edit").unwrap();
                fixture.info(binary, group, &fixture.repository);
                success(fixture.operation(binary, group, "open").output().unwrap());
                fixture.info(binary, group, &fixture.repository);
            }
            let before = state(&fixture.repository);
            let locator = std::fs::read(fixture.root.join(".gossil/checkout.json")).unwrap();
            let output = closed_stdout::configure(fixture.operation(binary, group, "info"))
                .output()
                .unwrap();
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("Broken pipe"));
            assert_eq!(state(&fixture.repository), before);
            assert_eq!(
                std::fs::read(fixture.root.join(".gossil/checkout.json")).unwrap(),
                locator
            );
            assert_eq!(
                std::fs::read(fixture.root.join("ordinary")).unwrap(),
                b"uncommitted edit"
            );
        }
    }
}

/// Grammar and admission failures keep stdout empty and preserve real persisted state.
#[test]
fn grammar_locator_configuration_and_corruption_refusals() {
    use std::os::unix::ffi::OsStringExt;
    for binary in [env!("CARGO_BIN_EXE_gossil"), env!("CARGO_BIN_EXE_gos")] {
        for group in ["vcs", "v"] {
            let fixture = Fixture::new(binary, group);
            for suffix in [
                vec!["--json"],
                vec!["--root", "."],
                vec!["tip"],
                vec!["--repository", "other"],
                vec!["--checkout", "other"],
                vec!["--checkout"],
            ] {
                let mut command = fixture.operation(binary, group, "info");
                command.args(suffix);
                fixture.refuse(command, "Usage:");
            }
            for args in [
                vec![],
                vec!["--repository"],
                vec!["--checkout"],
                vec!["--checkout", "work"],
                vec!["--repository", "missing"],
            ] {
                let mut command = command(binary, fixture.temp.path());
                command
                    .current_dir(&fixture.root)
                    .args([group, "info"])
                    .args(args);
                fixture.refuse(command, "requires");
            }
            let mut parent = command(binary, fixture.temp.path());
            parent.current_dir(&fixture.root).arg(group);
            fixture.refuse(parent, "Usage:");
            for flag in ["--repository", "--checkout"] {
                let mut invalid = command(binary, fixture.temp.path());
                invalid
                    .current_dir(&fixture.root)
                    .args([group, "info", flag])
                    .arg(std::ffi::OsString::from_vec(vec![0xff]));
                fixture.refuse(invalid, "UTF-8");
            }
            let mut wrong = command(binary, fixture.temp.path());
            wrong
                .current_dir(&fixture.root)
                .args([group, "info", "--repository"])
                .arg(&fixture.repository)
                .args(["--checkout", "wrong"]);
            fixture.refuse(wrong, "explicit checkout conflicts");
            let other = fixture.temp.path().join("other.db");
            success(
                command(binary, fixture.temp.path())
                    .args([group, "init"])
                    .arg(&other)
                    .output()
                    .unwrap(),
            );
            let mut wrong = command(binary, fixture.temp.path());
            wrong
                .current_dir(&fixture.root)
                .args([group, "info", "--repository"])
                .arg(&other)
                .args(["--checkout", "work"]);
            fixture.refuse(wrong, "explicit repository conflicts");
            let mut configured = fixture.operation(binary, group, "info");
            configured.env("GEST_DATABASE__URL", &other);
            fixture.refuse(
                configured,
                "configured database URL/host provider is unsupported",
            );
            let storage = fixture.temp.path().join("configured");
            std::fs::create_dir(&storage).unwrap();
            std::fs::copy(&other, storage.join("gossil.db")).unwrap();
            let mut configured = fixture.operation(binary, group, "info");
            configured.env("GEST_STORAGE__DATA_DIR", &storage);
            fixture.refuse(configured, "configured storage conflicts");
            let nested = fixture.root.join("nested");
            std::fs::create_dir(&nested).unwrap();
            std::fs::create_dir(nested.join(".gossil")).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                nested.join(".gossil"),
                std::fs::Permissions::from_mode(0o700),
            )
            .unwrap();
            let mut nearest = fixture.operation(binary, group, "info");
            nearest.current_dir(&nested);
            fixture.refuse(nearest, "complete checkout locator");
            let locator = fixture.root.join(".gossil/checkout.json");
            let original = std::fs::read(&locator).unwrap();
            std::fs::remove_file(&locator).unwrap();
            fixture.refuse(
                fixture.operation(binary, group, "info"),
                "complete checkout locator",
            );
            std::fs::write(&locator, b"corrupt locator").unwrap();
            std::fs::set_permissions(&locator, std::fs::Permissions::from_mode(0o600)).unwrap();
            fixture.refuse(
                fixture.operation(binary, group, "info"),
                "invalid or incomplete checkout locator",
            );
            std::fs::write(&locator, original).unwrap();
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
                .block_on(async {
                    let database = libsql::Builder::new_local(&fixture.repository)
                        .build()
                        .await
                        .unwrap();
                    database
                        .connect()
                        .unwrap()
                        .execute("UPDATE g_checkout SET shared_head='corrupt'", ())
                        .await
                        .unwrap();
                });
            fixture.refuse(
                fixture.operation(binary, group, "info"),
                "unsupported recording checkout state",
            );
        }
    }
}

/// Help observes no filesystem selections, including an absent repository.
#[test]
fn help_needs_no_checkout_and_creates_nothing() {
    for binary in [env!("CARGO_BIN_EXE_gossil"), env!("CARGO_BIN_EXE_gos")] {
        for group in ["vcs", "v"] {
            let temp = tempfile::tempdir().unwrap();
            for flag in ["--help", "-h"] {
                let output = success(
                    command(binary, temp.path())
                        .current_dir(temp.path())
                        .args([group, "info", flag])
                        .output()
                        .unwrap(),
                );
                assert_eq!(
                    output.stdout,
                    b"Usage: gossil vcs info --repository REPO --checkout ID\n"
                );
                assert!(output.stderr.is_empty());
                assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
            }
        }
    }
}
