//! Exercise shipping build metadata helpers in isolated real-process fixtures.
use super::*;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{
    fs,
    io::Read,
    process::{Child, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

/// Select one isolated helper fixture in the child test process.
const CASE: &str = "GOSSIL_BUILD_METADATA_HELPER_CASE";
/// Name the shipping helper control for exact child test selection.
const SELECTOR: &str = "controls::shipping_metadata_helper_contract";

/// Bound one owned child and its Git descendants; the live leader reserves its group identity.
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            #[cfg(unix)]
            {
                let _ = Command::new("/bin/kill")
                    .args(["-KILL", &format!("-{}", self.0.id())])
                    .status();
            }
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

/// Use real Git with fixture-local actor configuration, never repository or global user state.
/// # Panics
/// Panics if Git cannot run successfully or returns invalid UTF-8.
fn git(arguments: &[&str]) -> String {
    let result = Command::new("git")
        .args([
            "-c",
            "user.name=Metadata Helper",
            "-c",
            "user.email=metadata-helper@example.invalid",
        ])
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().to_owned()
}

/// Cancellation cleanup terminates and reaps its actual owned sleeper instead of leaving a child behind.
/// # Panics
/// Panics if the fixture cannot start or cleanup leaves its child alive.
#[test]
fn owned_fixture_cleanup_reaps_its_child() {
    let mut command = Command::new("/bin/sleep");
    command
        .arg("60")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    command.process_group(0);
    let child = OwnedChild(command.spawn().unwrap());
    let pid = child.0.id().to_string();
    drop(child);
    assert!(
        !Command::new("/bin/kill")
            .args(["-0", &pid])
            .output()
            .unwrap()
            .status
            .success()
    );
}

/// Real programs exercise successful, empty, invalid UTF-8, failed-status and missing-command results.
/// # Panics
/// Panics if a command result differs from the metadata output contract.
fn command_contracts() {
    assert_eq!(
        command_text("/usr/bin/printf", &["  actual output  "]),
        Some("actual output".into())
    );
    assert_eq!(command_text("/usr/bin/printf", &["\\377"]), None);
    assert_eq!(command_text("/usr/bin/true", &[]), None);
    assert_eq!(command_text("/usr/bin/false", &[]), None);
    assert_eq!(command_text("gossil-missing-metadata-program", &[]), None);
}

/// Exercise the actual shipping helpers inside an isolated environment, then print a post-call receipt.
/// # Panics
/// Panics if fixture setup fails, a child times out or metadata differs from the expected contract.
#[test]
fn shipping_metadata_helper_contract() {
    if let Ok(case) = std::env::var(CASE) {
        command_contracts();
        match case.as_str() {
            "outside" => {
                assert!(git_path("HEAD").is_none());
                assert!(git_short_sha().is_none());
                let date = chrono_free_date();
                assert_eq!(date.len(), 10);
                assert_eq!(&date[4..5], "-");
                assert_eq!(&date[7..8], "-");
                main();
            }
            "missing" => {
                assert!(git_path("HEAD").is_none());
                assert!(git_short_sha().is_none());
                assert_eq!(chrono_free_date(), "unknown");
                main();
            }
            "failed" => {
                // The owned PATH contains real programs that exist but exit unsuccessfully.
                assert!(command_text("git", &["--version"]).is_none());
                assert!(git_path("HEAD").is_none());
                assert!(git_short_sha().is_none());
                assert_eq!(chrono_free_date(), "unknown");
                main();
            }
            "unborn" | "attached" | "detached" | "packed" => {
                git(&["init", "--initial-branch=metadata-fixture", "--template="]);
                let head = git_path("HEAD").unwrap();
                assert!(head.is_file());
                if case != "unborn" {
                    git(&["commit", "--no-gpg-sign", "--allow-empty", "-m", "fixture"]);
                    let expected = git(&["rev-parse", "--short", "HEAD"]);
                    assert_eq!(git_short_sha().as_deref(), Some(expected.as_str()));
                } else {
                    assert!(git_short_sha().is_none());
                }
                if case == "detached" {
                    git(&["checkout", "--detach"]);
                }
                if case == "packed" {
                    git(&["pack-refs", "--all", "--prune"]);
                    assert!(!git_path("refs/heads/metadata-fixture").unwrap().exists());
                    assert!(git_path("packed-refs").unwrap().is_file());
                }
                main();
            }
            _ => panic!("unknown shipping metadata case"),
        }
        println!("SHIPPING_METADATA_COMPLETED:{case}");
        return;
    }
    for case in [
        "outside", "missing", "unborn", "attached", "detached", "packed", "failed",
    ] {
        let directory = TempDir::new().unwrap();
        let output_path = directory.path().join("stdout");
        let error_path = directory.path().join("stderr");
        let mut command = Command::new(std::env::current_exe().unwrap());
        for (name, _) in
            std::env::vars_os().filter(|(name, _)| name.to_string_lossy().starts_with("GIT_"))
        {
            command.env_remove(name);
        }
        command
            .args(["--exact", SELECTOR, "--nocapture", "--test-threads=1"])
            .current_dir(directory.path())
            .env(CASE, case)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("GIT_CONFIG")
            .env_remove("GIT_CONFIG_COUNT")
            .env_remove("GIT_CONFIG_PARAMETERS")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .stdout(Stdio::from(fs::File::create(&output_path).unwrap()))
            .stderr(Stdio::from(fs::File::create(&error_path).unwrap()));
        if case == "missing" {
            command.env("PATH", directory.path());
        }
        if case == "failed" {
            // Copy the same real unsuccessful program used by command_contracts.
            let programs = directory.path().join("failing-programs");
            fs::create_dir(&programs).unwrap();
            for name in ["git", "date"] {
                fs::copy("/usr/bin/false", programs.join(name)).unwrap();
            }
            command.env("PATH", programs);
        }
        #[cfg(unix)]
        command.process_group(0);
        let mut child = OwnedChild(command.spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(20);
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "shipping metadata child timed out: {case}"
            );
            thread::sleep(Duration::from_millis(10));
        };
        let mut stdout = String::new();
        fs::File::open(output_path)
            .unwrap()
            .read_to_string(&mut stdout)
            .unwrap();
        let stderr = fs::read_to_string(error_path).unwrap();
        assert!(status.success(), "{case}: {stdout}\n{stderr}");
        assert!(stdout.contains("1 passed; 0 failed"), "{stdout}");
        assert_eq!(
            stdout
                .matches(&format!("SHIPPING_METADATA_COMPLETED:{case}"))
                .count(),
            1,
            "{stdout}"
        );
        assert!(stdout.contains("cargo:rerun-if-changed=build.rs"));
        assert!(stdout.contains("cargo:rustc-env=BUILD_DATE="));
        assert!(stdout.contains("cargo:rustc-env=GIT_SHA="));
        if matches!(case, "missing" | "outside" | "unborn") {
            assert!(stdout.contains("cargo:rustc-env=GIT_SHA=unknown"));
        }
        if case == "missing" {
            assert!(stdout.contains("cargo:rustc-env=BUILD_DATE=unknown"));
        }
        if matches!(case, "unborn" | "attached" | "detached" | "packed") {
            assert!(stdout.contains("HEAD"));
        }
        if case == "attached" {
            assert!(stdout.contains("refs/heads/metadata-fixture"));
        }
        if case == "packed" {
            assert!(stdout.contains("packed-refs"));
        }
        // Cargo must watch only existing metadata inputs that can change this revision.
        let watched: Vec<&str> = stdout
            .lines()
            .filter_map(|line| {
                line.split_once("cargo:rerun-if-changed=")
                    .map(|(_, path)| path)
            })
            .collect();
        let expected = match case {
            "outside" | "missing" | "failed" => vec!["build.rs"],
            "detached" => vec!["build.rs", ".git/HEAD"],
            "attached" => vec!["build.rs", ".git/HEAD", ".git/refs/heads/metadata-fixture"],
            "unborn" => vec!["build.rs", ".git/HEAD", ".git/refs/heads"],
            "packed" => vec![
                "build.rs",
                ".git/HEAD",
                ".git/refs/heads",
                ".git/packed-refs",
            ],
            _ => unreachable!(),
        };
        assert_eq!(watched, expected, "{case}: {stdout}");
        if case == "failed" {
            assert!(stdout.contains("cargo:rustc-env=GIT_SHA=unknown"));
            assert!(stdout.contains("cargo:rustc-env=BUILD_DATE=unknown"));
        }
    }
}
