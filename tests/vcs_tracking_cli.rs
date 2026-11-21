//! Fresh-process native tracking controls through the shared locator admission.
//!
//! ## Recommendations
//! Qualify both executable aliases after the reviewed open prerequisite is installed.
use std::{
    path::Path,
    process::{Command, Output},
};
/// Keep command invocations independent of inherited Gest selection/preferences.
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
/// Init/open/add/reopen/status/add-again preserve cumulative enrollment and live bytes.
#[test]
fn fresh_process_tracking_is_persistent_and_requires_complete_binding() {
    for binary in [env!("CARGO_BIN_EXE_gos"), env!("CARGO_BIN_EXE_gossil")] {
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
        std::fs::write(root.join("first"), b"one").unwrap();
        std::fs::write(root.join("second"), b"two").unwrap();
        assert!(
            !invoke(binary, home, &root, &repo, "add", &["--path", "first"])
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
        let locator = root.join(".gossil/checkout.json");
        let marker = std::fs::read(&locator).unwrap();
        assert!(
            invoke(binary, home, &root, &repo, "add", &["--path", "first"])
                .status
                .success()
        );
        assert!(
            invoke(binary, home, &root, &repo, "open", &[])
                .status
                .success()
        );
        let result = invoke(binary, home, &root, &repo, "status", &[]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            String::from_utf8(result.stdout).unwrap(),
            "Tracked paths (A: pending addition, R: recorded):\nA\tunselected\t\"first\"\n"
        );
        std::fs::write(root.join("first"), b"edited without re-add").unwrap();
        assert!(
            invoke(binary, home, &root, &repo, "add", &["--path", "second"])
                .status
                .success()
        );
        let result = invoke(binary, home, &root, &repo, "status", &[]);
        assert_eq!(
            String::from_utf8(result.stdout).unwrap(),
            "Tracked paths (A: pending addition, R: recorded):\nA\tunselected\t\"first\"\nA\tunselected\t\"second\"\n"
        );
        assert!(
            !invoke(
                binary,
                home,
                &root,
                &repo,
                "add",
                &["--path", "first", "--path", "missing"]
            )
            .status
            .success()
        );
        assert!(
            !invoke(binary, home, &root, &repo, "status", &["--path", "first"])
                .status
                .success()
        );
        assert!(
            !invoke(
                binary,
                home,
                &root,
                &repo,
                "add",
                &["--private", "yes", "--path", "first"]
            )
            .status
            .success()
        );
        assert_eq!(
            std::fs::read(root.join("first")).unwrap(),
            b"edited without re-add"
        );
        assert_eq!(std::fs::read(&locator).unwrap(), marker);
        std::fs::remove_file(&locator).unwrap();
        let result = invoke(binary, home, &root, &repo, "status", &[]);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("run vcs open"));
        assert!(!locator.exists());
        assert!(
            invoke(binary, home, &root, &repo, "open", &[])
                .status
                .success()
        );
        assert!(
            invoke(binary, home, &root, &repo, "status", &[])
                .status
                .success()
        );
        std::fs::write(&locator, b"malformed").unwrap();
        assert!(
            !invoke(binary, home, &root, &repo, "status", &[])
                .status
                .success()
        );
        assert!(
            !invoke(binary, home, &root, &repo, "add", &["--path", "first"])
                .status
                .success()
        );
        assert_eq!(std::fs::read(&locator).unwrap(), b"malformed");
    }
}

/// Existing commands report library-created scopes and history without new CLI verbs.
/// # Panics
/// Panics if native setup, an explicit library operation or exact status output differs.
#[tokio::test]
async fn status_reports_library_scopes_and_recorded_paths() {
    use gossil::artifact::ParseLimits;
    use gossil::storage::{
        AdmissionLimits, InitializeOptions, Limits, LocalOwnerRepository, PathScope,
        PathScopeSelection, PrivateRecordingOutcome, PrivateRecordingRequest, TrackingLimits,
    };
    let admission = AdmissionLimits {
        content: Limits {
            max_stored_bytes: 1 << 20,
            max_output_bytes: 1 << 20,
            max_chain_depth: 128,
            max_total_stored_bytes: 64 << 20,
            max_total_decoded_bytes: 64 << 20,
        },
        parser: ParseLimits {
            input_bytes: 1 << 20,
            cards: 4096,
            owned_bytes: 8 << 20,
            entries: 4096,
        },
    };
    let tracking = TrackingLimits {
        paths: 16,
        path_bytes: 4096,
        components: 32,
        retained_bytes: 1 << 20,
        io_buffer_bytes: 1024,
    };
    for binary in [env!("CARGO_BIN_EXE_gos"), env!("CARGO_BIN_EXE_gossil")] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let root = home.join("root");
        let repo = home.join("repo");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("first"), b"private bytes").unwrap();
        std::fs::write(root.join("second"), b"shared choice").unwrap();
        std::fs::write(root.join("third"), b"unselected").unwrap();
        let owner = LocalOwnerRepository::initialize(
            &repo,
            InitializeOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "owner",
                date: "2026-11-20T00:00:00",
                user: b"Owner",
                password: "secret",
                sha1: false,
                content_limits: admission.content,
                parse_limits: admission.parser,
            },
        )
        .await
        .unwrap();
        drop(owner);
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
        let marker = std::fs::read(root.join(".gossil/checkout.json")).unwrap();
        assert!(
            invoke(binary, home, &root, &repo, "add", &["--path", "first"])
                .status
                .success()
        );
        let mut owner = LocalOwnerRepository::open(&repo, admission).await.unwrap();
        let view = owner.checkout_view("work", admission).await.unwrap();
        let view = owner
            .select_path_scopes(
                "work",
                &view,
                &[PathScopeSelection {
                    path: "first".into(),
                    scope: PathScope::Private,
                }],
                admission,
                tracking,
            )
            .await
            .unwrap();
        drop(owner);
        let result = invoke(binary, home, &root, &repo, "status", &[]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            String::from_utf8(result.stdout).unwrap(),
            "Tracked paths (A: pending addition, R: recorded):\nA\tprivate\t\"first\"\n"
        );
        let mut owner = LocalOwnerRepository::open(&repo, admission).await.unwrap();
        assert_eq!(owner.checkout_view("work", admission).await.unwrap(), view);
        let outcome = owner
            .commit_private_tracked_files(
                "work",
                PrivateRecordingRequest {
                    expected: &view,
                    paths: &["first"],
                    scope: PathScope::Private,
                    actor: b"Owner",
                    comment: b"library recording",
                    date: b"2026-11-20T00:00:01",
                },
                admission,
                tracking,
            )
            .await
            .unwrap();
        assert!(matches!(outcome, PrivateRecordingOutcome::Recorded { .. }));
        drop(owner);
        let result = invoke(binary, home, &root, &repo, "status", &[]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            String::from_utf8(result.stdout).unwrap(),
            "Tracked paths (A: pending addition, R: recorded):\nR\tprivate\t\"first\"\n"
        );
        assert!(
            invoke(binary, home, &root, &repo, "add", &["--path", "second"])
                .status
                .success()
        );
        let mut owner = LocalOwnerRepository::open(&repo, admission).await.unwrap();
        let view = owner.checkout_view("work", admission).await.unwrap();
        owner
            .select_path_scopes(
                "work",
                &view,
                &[PathScopeSelection {
                    path: "second".into(),
                    scope: PathScope::Shared,
                }],
                admission,
                tracking,
            )
            .await
            .unwrap();
        drop(owner);
        let result = invoke(binary, home, &root, &repo, "add", &["--path", "third"]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let expected = "Tracked paths (A: pending addition, R: recorded):\nR\tprivate\t\"first\"\nA\tshared\t\"second\"\nA\tunselected\t\"third\"\n";
        assert_eq!(String::from_utf8(result.stdout).unwrap(), expected);
        let mut owner = LocalOwnerRepository::open(&repo, admission).await.unwrap();
        let before_status = owner.checkout_view("work", admission).await.unwrap();
        drop(owner);
        let result = invoke(binary, home, &root, &repo, "status", &[]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(String::from_utf8(result.stdout).unwrap(), expected);
        let mut owner = LocalOwnerRepository::open(&repo, admission).await.unwrap();
        assert_eq!(
            owner.checkout_view("work", admission).await.unwrap(),
            before_status
        );
        assert_eq!(std::fs::read(root.join("first")).unwrap(), b"private bytes");
        assert_eq!(
            std::fs::read(root.join("second")).unwrap(),
            b"shared choice"
        );
        assert_eq!(std::fs::read(root.join("third")).unwrap(), b"unselected");
        assert_eq!(
            std::fs::read(root.join(".gossil/checkout.json")).unwrap(),
            marker
        );
    }
}
