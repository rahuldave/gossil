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
            "Pending additions (scope unselected):\nA\t\"first\"\n"
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
            "Pending additions (scope unselected):\nA\t\"first\"\nA\t\"second\"\n"
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
