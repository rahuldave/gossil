//! Integration controls for report acceptance, evidence and child cleanup.
use gossil::{parse_report, validate_report};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

/// Build an explicit reference configuration for one control suite.
fn config(root: &Path, suite: &str) -> gossil::Config {
    let project = Path::new(env!("CARGO_MANIFEST_DIR"));
    gossil::Config {
        role: "reference".into(),
        revision: "b3a1c8efb2731dffbd5cc38c58ac349746de6da6".into(),
        tcl: project.join(".jlog/bin/tclsh"),
        driver: project.join("tests/fixtures/controls/tester.tcl"),
        executable: project.join(".jlog/bin/fossil"),
        suite: suite.into(),
        run_dir: root.join(suite),
        timeout_ms: 3000,
        seed: 20261117,
        inputs: vec![
            project.join("tests/fixtures/fossil/tester.tcl"),
            project.join("tests/fixtures/fossil/enable.patch"),
        ],
        capabilities: BTreeMap::from([
            ("tls".into(), "disabled".into()),
            ("fuse".into(), "disabled".into()),
        ]),
    }
}

#[test]
/// Verify that acceptance allows only executed, matching assertions.
fn accepts_only_executed_matching_assertions() {
    let log = "test contains-selector 1 .a OK\ntest test-framework-control OK\n***** Final results: 0 errors out of 2 tests\n";
    let report = parse_report(log);
    assert_eq!(report.assertions[0].name, "contains-selector 1 .a");
    assert!(validate_report(&report, Some(0)).is_ok());
}

#[test]
/// Keep malformed numeric summaries invalid even when a valid summary follows.
fn malformed_summary_numbers_are_permanently_rejected() {
    for (errors, total) in [
        ("9", "broken"),
        ("broken", "1"),
        ("broken", "broken"),
        ("0", "999999999999999999999999999999999999999"),
        ("999999999999999999999999999999999999999", "1"),
    ] {
        for suffix in ["", "***** Final results: 0 errors out of 1 tests\n"] {
            let log = format!(
                "test real OK\n***** Final results: {errors} errors out of {total} tests\n{suffix}"
            );
            let report = parse_report(&log);
            assert_eq!(report.summary_count, if suffix.is_empty() { 1 } else { 2 });
            assert!(report.malformed, "{log}");
            assert!(validate_report(&report, Some(0)).is_err(), "{log}");
        }
    }
}

#[test]
/// Reject malformed-then-valid output through the actual executable interface.
fn malformed_summary_controls_never_exit_successfully() {
    let temp = tempfile::tempdir().unwrap();
    for suite in [
        "bad-total",
        "bad-errors",
        "bad-both",
        "overflow-total",
        "overflow-errors",
        "duplicate-summary",
    ] {
        let path = temp.path().join(format!("{suite}.json"));
        fs::write(
            &path,
            serde_json::to_vec(&config(temp.path(), suite)).unwrap(),
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_gossil-conformance"))
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{suite}: {:?}", output);
        let run: gossil::Run =
            serde_json::from_slice(&fs::read(temp.path().join(suite).join("report.json")).unwrap())
                .unwrap();
        assert_eq!(run.exit_code, Some(0));
        assert!(!run.accepted);
        assert!(run.report.malformed);
        assert_eq!(run.report.summary_count, 2);
        assert_eq!(run.status, "blocked");
    }
}

#[test]
/// Verify rejection of false success, missing counts and nonzero process exits.
fn rejects_false_success_missing_counts_and_nonzero_processes() {
    for log in [
        "Ok\n",
        "***** Final results: 0 errors out of 0 tests\n",
        "test test-framework-empty OK\n***** Final results: 0 errors out of 1 tests\n",
        "test real OK\n***** Final results: 0 errors out of 2 tests\n",
        "test real FAILED\n***** Final results: 1 errors out of 1 tests\n",
        "test real FAILED (knownBug)\n***** Final results: 0 errors out of 1 tests\n",
        "test real FAILED!\n***** Final results: 1 errors out of 1 tests\n",
        "test real UNKNOWN\n***** Final results: 0 errors out of 1 tests\n",
        "test real OK nonsense\n***** Final results: 0 errors out of 1 tests\n",
        "test real OK\ntest real OK\n***** Final results: 0 errors out of 2 tests\n",
        "test real OK\n***** Final results: broken\n",
        "test real OK\n***** Final results: 0 errors out of 1 tests\n***** Final results: 0 errors out of 1 tests\n",
        "test real OK\n***** Skipped tests: no-TLS\n***** Final results: 0 errors out of 1 tests\n",
    ] {
        assert!(
            validate_report(&parse_report(log), Some(0)).is_err(),
            "{log}"
        );
    }
    let report = parse_report("test real OK\n***** Final results: 0 errors out of 1 tests\n");
    assert!(validate_report(&report, Some(1)).is_err());
    assert!(validate_report(&report, None).is_err());
}

#[test]
/// Verify execution of the inherited driver and retention of failed results.
fn runs_real_inherited_driver_and_retains_failed_results() {
    let temp = tempfile::tempdir().unwrap();
    for (suite, accepted) in [
        ("pass", true),
        ("fail", false),
        ("empty", false),
        ("known", false),
        ("known-pass", true),
        ("stderr-only", false),
        ("nonutf8", false),
        ("skipped", false),
        ("truncated", false),
    ] {
        let run = gossil::execute(config(temp.path(), suite)).unwrap();
        assert_eq!(
            run.accepted, accepted,
            "{suite}: {} {}",
            run.stdout, run.stderr
        );
        assert!(run.config.run_dir.join("report.json").is_file());
        assert_eq!(run.hashes.len(), 6);
        assert_eq!(
            run.environment["HOME"],
            fs::canonicalize(&run.config.run_dir)
                .unwrap()
                .display()
                .to_string()
        );
        assert!(!run.timed_out);
        if suite == "known" {
            assert_eq!(run.report.assertions[0].state, "fail");
            assert!(run.report.assertions[0].known_bug);
        }
        if suite == "fail" {
            assert_eq!(run.exit_code, Some(1));
        }
    }
}

#[test]
/// Verify rejection of missing inputs, nonexecutables, reused directories and invalid configuration.
fn rejects_missing_inputs_nonexecutables_reused_directories_and_invalid_config() {
    let temp = tempfile::tempdir().unwrap();
    let mut c = config(temp.path(), "pass");
    c.executable = temp.path().join("absent");
    assert!(gossil::execute(c).is_err());
    assert!(!temp.path().join("pass").exists());
    let inert = temp.path().join("inert");
    fs::write(&inert, "").unwrap();
    fs::set_permissions(&inert, fs::Permissions::from_mode(0o644)).unwrap();
    let mut c = config(temp.path(), "pass");
    c.executable = inert;
    assert!(gossil::execute(c).is_err());
    let setuid = temp.path().join("setuid");
    fs::write(&setuid, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&setuid, fs::Permissions::from_mode(0o4755)).unwrap();
    let mut c = config(temp.path(), "pass");
    c.executable = setuid;
    assert!(gossil::execute(c).is_err());
    let mut c = config(temp.path(), "pass");
    c.role = "fallback".into();
    assert!(gossil::execute(c).is_err());
    let mut c = config(temp.path(), "pass");
    c.timeout_ms = 0;
    assert!(gossil::execute(c).is_err());
    assert!(gossil::execute(config(temp.path(), "../pass")).is_err());
    assert!(gossil::execute(config(temp.path(), "missing-suite")).is_err());
    fs::create_dir(temp.path().join("pass")).unwrap();
    assert!(gossil::execute(config(temp.path(), "pass")).is_err());
}

#[test]
/// Verify termination of owned descendants on timeout and retention of truncated output.
fn kills_owned_descendants_on_timeout_and_preserves_truncated_output() {
    let temp = tempfile::tempdir().unwrap();
    let control_marker = temp.path().join("uncontained-control.txt");
    assert!(
        Command::new("/bin/sh")
            .args(["-c", "sleep 0.05; echo alive > \"$1\"", "sh"])
            .arg(&control_marker)
            .status()
            .unwrap()
            .success()
    );
    assert!(control_marker.is_file());
    let mut c = config(temp.path(), "timeout");
    c.timeout_ms = 1000;
    let run = gossil::execute(c).unwrap();
    assert!(run.timed_out);
    assert!(!run.accepted);
    assert_eq!(run.exit_code, None);
    assert!(run.stderr.contains("owned child:"));
    std::thread::sleep(std::time::Duration::from_millis(2300));
    assert!(!run.config.run_dir.join("escaped-child.txt").exists());
    let run = gossil::execute(config(temp.path(), "early-exit")).unwrap();
    assert!(run.accepted);
    std::thread::sleep(std::time::Duration::from_millis(2300));
    assert!(!run.config.run_dir.join("escaped-child.txt").exists());
}

#[test]
/// Verify that the CLI reports actual status and setup failures.
fn cli_reports_actual_status_and_setup_failures() {
    let temp = tempfile::tempdir().unwrap();
    for (suite, exit) in [("pass", 0), ("fail", 1)] {
        let path = temp.path().join(format!("{suite}.json"));
        fs::write(
            &path,
            serde_json::to_vec(&config(temp.path(), suite)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            Command::new(env!("CARGO_BIN_EXE_gossil-conformance"))
                .arg(path)
                .status()
                .unwrap()
                .code(),
            Some(exit)
        );
    }
    assert_eq!(gossil::cli(&[]), 2);
    assert_eq!(gossil::cli(&["/nonexistent/config".into()]), 2);
    let bad = temp.path().join("bad.json");
    fs::write(&bad, "not json").unwrap();
    assert_eq!(gossil::cli(&[bad.display().to_string()]), 2);
}
