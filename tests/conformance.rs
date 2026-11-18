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
        invocation_plan: None,
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

/// An explicit pinned plan distinguishes repeated original names without dropping occurrences.
#[test]
fn pinned_plan_duplicate_regression() {
    use sha2::{Digest, Sha256};
    let temp = tempfile::tempdir().unwrap();
    let mut cfg = config(temp.path(), "repeated");
    let strict = gossil::execute(config(temp.path(), "repeated")).unwrap();
    assert!(!strict.accepted);
    assert!(strict.report.malformed);
    assert_eq!(strict.report.assertions.len(), 3);
    cfg.run_dir = temp.path().join("planned");
    let suite = cfg.driver.with_file_name("repeated.test");
    let corpus = serde_json::json!({"suite":"repeated","revision":cfg.revision,"files":{"test/repeated.test":format!("{:x}",Sha256::digest(fs::read(&suite).unwrap()))}});
    fs::create_dir(temp.path().join("test")).unwrap();
    fs::copy(&suite, temp.path().join("test/repeated.test")).unwrap();
    let corpus_path = temp.path().join("corpus.json");
    let corpus_bytes = serde_json::to_vec(&corpus).unwrap();
    fs::write(&corpus_path, &corpus_bytes).unwrap();
    let corpus_hash = format!("{:x}", Sha256::digest(&corpus_bytes));
    let plan = serde_json::json!({"schema":"gossil.invocation-plan.v1","suite":"repeated","revision":cfg.revision,"suite_sha256":format!("{:x}",Sha256::digest(fs::read(&suite).unwrap())),"corpus_manifest_sha256":corpus_hash,"rows":[
       {"name":"same","id":{"suite":"repeated","position":{"kind":"corpus","source":"test/repeated.test","iteration":0,"slot":1}}},
       {"name":"same","id":{"suite":"repeated","position":{"kind":"corpus","source":"test/repeated.test","iteration":0,"slot":2}}},
       {"name":"test-framework-repeated","id":{"suite":"repeated","position":{"kind":"framework"}}}
    ]});
    let plan_bytes = serde_json::to_vec(&plan).unwrap();
    let plan_path = temp.path().join("plan.json");
    fs::write(&plan_path, &plan_bytes).unwrap();
    let mut value = serde_json::to_value(&cfg).unwrap();
    value["invocation_plan"] = serde_json::json!({"path":plan_path,"sha256":format!("{:x}",Sha256::digest(&plan_bytes)),"expected_invocations":3,"corpus_manifest":corpus_path,"corpus_manifest_sha256":corpus_hash,"corpus_root":temp.path()});
    cfg = serde_json::from_value(value).unwrap();
    let run = gossil::execute(cfg).unwrap();
    assert!(run.accepted, "{:?}", run.report);
    assert_eq!(run.report.assertions.len(), 3);
    assert_eq!(run.report.assertions[0].name, "same");
    assert_eq!(run.report.assertions[1].name, "same");
}

/// Hash exact expected bytes for an opt-in fixture.
fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
/// Construct an actual process control with a pinned source/corpus and ordered plan.
fn planned_control(root: &Path, output: &str, tail: &str) -> (gossil::Config, serde_json::Value) {
    fs::create_dir_all(root.join("test")).unwrap();
    fs::write(root.join("control.test"), b"original suite").unwrap();
    fs::write(root.join("test/control.test"), b"original suite").unwrap();
    let driver = root.join("driver.sh");
    fs::write(
        &driver,
        format!("#!/bin/sh\ncat <<'END_LOG'\n{output}END_LOG\n{tail}\n"),
    )
    .unwrap();
    let mut cfg = config(root, "control");
    fs::set_permissions(&driver, fs::Permissions::from_mode(0o755)).unwrap();
    cfg.driver = driver.clone();
    cfg.tcl = driver;
    cfg.executable = "/usr/bin/true".into();
    cfg.run_dir = root.join("run");
    let corpus = serde_json::json!({"suite":"control","revision":cfg.revision,"files":{"test/control.test":digest(b"original suite")}});
    let bytes = serde_json::to_vec(&corpus).unwrap();
    let corpus_path = root.join("corpus.json");
    fs::write(&corpus_path, &bytes).unwrap();
    let names = ["same", "middle", "same", "test-framework-control"];
    let rows:Vec<_>=names.iter().enumerate().map(|(i,name)|serde_json::json!({"name":name,"id":{"suite":"control","position":if i==3 {serde_json::json!({"kind":"framework"})}else{serde_json::json!({"kind":"corpus","source":"test/control.test","iteration":0,"slot":i+1})}}})).collect();
    let plan = serde_json::json!({"schema":"gossil.invocation-plan.v1","suite":"control","revision":cfg.revision,"suite_sha256":digest(b"original suite"),"corpus_manifest_sha256":digest(&bytes),"rows":rows});
    cfg.invocation_plan = Some(gossil::PlanPin {
        path: root.join("plan.json"),
        sha256: String::new(),
        expected_invocations: 4,
        corpus_manifest: corpus_path,
        corpus_manifest_sha256: digest(&bytes),
        corpus_root: root.into(),
    });
    repin(&mut cfg, &plan);
    (cfg, plan)
}
/// Re-pin deliberate plan mutations; a pin mismatch is tested separately.
fn repin(cfg: &mut gossil::Config, plan: &serde_json::Value) {
    let bytes = serde_json::to_vec(plan).unwrap();
    let pin = cfg.invocation_plan.as_mut().unwrap();
    fs::write(&pin.path, &bytes).unwrap();
    pin.sha256 = digest(&bytes);
}
/// Valid log containing an independently named middle assertion and a repeated name.
const PLANNED_LOG: &str = "test same OK\ntest middle OK\ntest same OK\ntest test-framework-control OK\n***** Final results: 0 errors out of 4 tests\n";

/// Plan mode preserves every lexical, count, failure and process acceptance condition.
#[test]
fn planned_acceptance_controls() {
    let bad = [
        PLANNED_LOG.replacen("test same OK\n", "", 1),
        PLANNED_LOG.replace("test middle OK", "test wrong OK"),
        PLANNED_LOG.replace("test middle OK", "test same OK"),
        PLANNED_LOG.replacen(
            "test same OK\ntest middle OK",
            "test middle OK\ntest same OK",
            1,
        ),
        PLANNED_LOG.replace("test middle OK", "test middle FAILED (knownBug)!"),
        PLANNED_LOG.replace("test middle OK", "test middle OK invalid"),
        PLANNED_LOG.replace("0 errors out of 4", "broken errors out of 4"),
        PLANNED_LOG.replace(
            "0 errors out of 4",
            "0 errors out of 9999999999999999999999999999",
        ),
        format!("{PLANNED_LOG}***** Final results: 0 errors out of 4 tests\n"),
        format!("***** Final results: broken errors out of 4 tests\n{PLANNED_LOG}"),
        PLANNED_LOG.replace("***** Final results: 0 errors out of 4 tests\n", ""),
        PLANNED_LOG.replace("out of 4", "out of 3"),
        format!("{PLANNED_LOG}***** Skipped tests: ignored\n"),
        format!("test same OK\n{PLANNED_LOG}"),
        "test test-framework-control OK\n***** Final results: 0 errors out of 1 tests\n".into(),
    ];
    for output in bad {
        let temp = tempfile::tempdir().unwrap();
        let (cfg, _) = planned_control(temp.path(), &output, "");
        let run = gossil::execute(cfg).unwrap();
        assert!(!run.accepted, "{output}");
        assert_eq!(run.invocation_plan.as_ref().unwrap().rows.len(), 4);
    }
    for (tail, timeout) in [
        ("exit 1", 3000),
        ("kill -TERM $$", 3000),
        ("printf '\\377'", 3000),
        ("sleep 10", 2000),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let (mut cfg, _) = planned_control(temp.path(), PLANNED_LOG, tail);
        cfg.timeout_ms = timeout;
        let run = gossil::execute(cfg).unwrap();
        assert!(!run.accepted, "{tail}");
        assert_eq!(
            run.report.assertions.len(),
            4,
            "tail {tail}; stdout {:?}; stderr {:?}",
            run.stdout,
            run.stderr
        );
        if timeout == 2000 {
            assert!(run.timed_out);
            assert_eq!(run.cleanup, "signaled-owned-group");
            assert_ne!(run.exit_code, Some(0));
        }
    }
    // A failing duplicate occurrence remains failed despite a zero-error summary.
    let temp = tempfile::tempdir().unwrap();
    let (cfg, _) = planned_control(
        temp.path(),
        &PLANNED_LOG.replacen("test same OK", "test same FAILED (knownBug)!", 1),
        "",
    );
    let run = gossil::execute(cfg).unwrap();
    assert!(!run.accepted);
    assert_eq!(run.report.assertions[0].state, "fail");
    assert!(run.report.assertions[0].known_bug);
}

/// Reject unpinned, malformed or source-mismatched plans before creating the run directory.
#[test]
fn plan_binding_controls() {
    for case in 0..25 {
        let temp = tempfile::tempdir().unwrap();
        let (mut cfg, mut plan) = planned_control(temp.path(), PLANNED_LOG, "");
        match case {
            0 => plan["rows"] = serde_json::json!([]),
            1 => plan["rows"][1]["id"] = plan["rows"][0]["id"].clone(),
            2 => plan["rows"][0]["name"] = "".into(),
            3 => plan["rows"][0]["id"]["suite"] = "".into(),
            4 => plan["suite"] = "wrong".into(),
            5 => plan["revision"] = "wrong".into(),
            6 => plan["schema"] = "wrong".into(),
            7 => plan["suite_sha256"] = "0".repeat(64).into(),
            8 => plan["corpus_manifest_sha256"] = "0".repeat(64).into(),
            9 => plan["rows"][0]["id"]["position"]["source"] = "../test/control.test".into(),
            10 => plan["rows"][0]["id"]["position"]["slot"] = 0.into(),
            11 => {
                plan["rows"][0]["id"]["position"] = serde_json::json!({"kind":"empty","index":0});
            }
            12 => {
                plan["rows"] = serde_json::json!([{"name":"framework","id":{"suite":"control","position":{"kind":"framework"}}}]);
            }
            13 => {
                plan["rows"][0]["id"] = serde_json::json!({});
            }
            _ => {}
        }
        repin(&mut cfg, &plan);
        match case {
            14 => cfg.invocation_plan.as_mut().unwrap().sha256 = "0".repeat(64),
            15 => cfg.invocation_plan.as_mut().unwrap().corpus_manifest_sha256 = "0".repeat(64),
            16 => fs::write(temp.path().join("test/control.test"), b"changed").unwrap(),
            17 => cfg.invocation_plan.as_mut().unwrap().path = "relative".into(),
            18 => {
                let pin = cfg.invocation_plan.as_mut().unwrap();
                fs::write(&pin.path, b"not json").unwrap();
                pin.sha256 = digest(b"not json");
            }
            _ => {}
        }
        if (19..=22).contains(&case) {
            let pin = cfg.invocation_plan.as_mut().unwrap();
            let mut manifest: serde_json::Value =
                serde_json::from_slice(&fs::read(&pin.corpus_manifest).unwrap()).unwrap();
            if case == 19 {
                manifest["suite"] = "wrong".into();
            }
            if case == 20 {
                manifest["revision"] = "wrong".into();
            }
            if case == 21 {
                manifest["files"] = serde_json::json!({});
            }
            if case == 22 {
                manifest["files"] = serde_json::json!({"../escape":digest(b"original suite")});
            }
            let bytes = serde_json::to_vec(&manifest).unwrap();
            fs::write(&pin.corpus_manifest, &bytes).unwrap();
            pin.corpus_manifest_sha256 = digest(&bytes);
            plan["corpus_manifest_sha256"] = pin.corpus_manifest_sha256.clone().into();
            repin(&mut cfg, &plan);
        }
        if case == 23 {
            cfg.invocation_plan.as_mut().unwrap().expected_invocations = 3;
        }
        if case == 24 {
            plan["rows"] = serde_json::json!([{"name":"framework","id":{"suite":"control","position":{"kind":"framework"}}}]);
            cfg.invocation_plan.as_mut().unwrap().expected_invocations = 1;
            repin(&mut cfg, &plan);
        }
        assert!(gossil::execute(cfg).is_err(), "case {case}");
        assert!(!temp.path().join("run").exists());
    }
}

/// The executable interface consumes expected pins and retains exact plan rows and bytes.
#[test]
fn plan_cli_and_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let (cfg, plan) = planned_control(temp.path(), PLANNED_LOG, "");
    let path = temp.path().join("config.json");
    fs::write(&path, serde_json::to_vec(&cfg).unwrap()).unwrap();
    assert!(
        Command::new(env!("CARGO_BIN_EXE_gossil-conformance"))
            .arg(&path)
            .output()
            .unwrap()
            .status
            .success()
    );
    let run: gossil::Run =
        serde_json::from_slice(&fs::read(cfg.run_dir.join("report.json")).unwrap()).unwrap();
    assert!(run.accepted);
    assert_eq!(run.invocation_plan.unwrap().rows.len(), 4);
    assert!(
        run.report
            .assertions
            .iter()
            .all(|a| a.expected_invocation_id.is_some())
    );
    assert_eq!(
        fs::read(cfg.run_dir.join("invocation-plan.json")).unwrap(),
        serde_json::to_vec(&plan).unwrap()
    );
    let mut value = serde_json::to_value(&cfg).unwrap();
    value["run_dir"] = serde_json::json!(temp.path().join("bad-pin"));
    value["invocation_plan"]["sha256"] = "0".repeat(64).into();
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(
        Command::new(env!("CARGO_BIN_EXE_gossil-conformance"))
            .arg(&path)
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
}

/// Regenerate the complete plan from relocated original bytes in actual Tcl order.
#[test]
fn complete_delta1_plan_integrity_and_relocation() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR"));
    let evidence = project.join("evidence/delta-encoder");
    let index: serde_json::Value =
        serde_json::from_slice(&fs::read(evidence.join("plan-integrity.json")).unwrap()).unwrap();
    let archive = evidence.join("delta1-plan.tar.gz");
    assert_eq!(
        digest(&fs::read(&archive).unwrap()),
        index["archive_sha256"]
    );
    let extract = |name: &str| {
        let out = Command::new("tar")
            .arg("-xOf")
            .arg(&archive)
            .arg(name)
            .output()
            .unwrap();
        assert!(out.status.success());
        assert_eq!(digest(&out.stdout), index["files"][name]);
        out.stdout
    };
    let plan: serde_json::Value = serde_json::from_slice(&extract("invocation-plan.json")).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&extract("corpus-manifest.json")).unwrap();
    let generation: serde_json::Value =
        serde_json::from_slice(&extract("generation.json")).unwrap();
    assert_eq!(manifest["files"].as_object().unwrap().len(), 76);
    let rows = plan["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 22804);
    let mut ids = std::collections::BTreeSet::new();
    let order = generation["ordered_corpus"].as_array().unwrap();
    assert_eq!(order.len(), 76);
    for (file_index, source) in order.iter().enumerate() {
        assert!(manifest["files"][source.as_str().unwrap()].is_string());
        for iteration in 0..100 {
            for slot in 1..=3 {
                let row = &rows[file_index * 300 + iteration * 3 + slot - 1];
                let pos = &row["id"]["position"];
                assert_eq!(pos["source"], *source);
                assert_eq!(pos["iteration"], iteration);
                assert_eq!(pos["slot"], slot);
            }
        }
    }
    for row in rows {
        assert!(ids.insert(serde_json::to_string(&row["id"]).unwrap()));
    }
    for index in 1..=3 {
        assert_eq!(
            rows[22800 + index - 1]["id"]["position"],
            serde_json::json!({"kind":"empty","index":index})
        );
    }
    assert_eq!(rows[22803]["id"]["position"]["kind"], "framework");
    let temp = tempfile::tempdir().unwrap();
    let result = Command::new("tar")
        .arg("-xf")
        .arg(evidence.join("delta1-runs.tar.gz"))
        .arg("-C")
        .arg(temp.path())
        .arg("upstream")
        .output()
        .unwrap();
    assert!(result.status.success());
    let output = temp.path().join("generated");
    let result = Command::new("uv")
        .args(["run", "--no-project", "--quiet", "python"])
        .arg(project.join("scripts/delta1-plan.py"))
        .arg(temp.path().join("upstream"))
        .arg(&output)
        .arg("--tcl")
        .arg(project.join(".jlog/bin/tclsh"))
        .arg("--revision")
        .arg(plan["revision"].as_str().unwrap())
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result);
    assert_eq!(
        fs::read(output.join("invocation-plan.json")).unwrap(),
        extract("invocation-plan.json")
    );
    assert_eq!(
        fs::read(output.join("corpus-manifest.json")).unwrap(),
        extract("corpus-manifest.json")
    );
}

/// Enumerate and verify every byte across the retained planned-run qualification archives.
#[test]
fn planned_run_archive_integrity() {
    let evidence = Path::new(env!("CARGO_MANIFEST_DIR")).join("evidence/delta-encoder");
    let summary: serde_json::Value =
        serde_json::from_slice(&fs::read(evidence.join("planned-delta1-results.json")).unwrap())
            .unwrap();
    let index: std::collections::BTreeMap<String, serde_json::Value> =
        serde_json::from_slice(&fs::read(evidence.join("planned-archive-index.json")).unwrap())
            .unwrap();
    assert_eq!(summary["archive_index"], "planned-archive-index.json");
    assert_eq!(index.len(), 30);
    let temp = tempfile::tempdir().unwrap();
    let mut seen = std::collections::BTreeSet::new();
    let mut archive_names = std::collections::BTreeSet::new();
    for archive in summary["archives"].as_array().unwrap() {
        let name = archive["path"].as_str().unwrap();
        assert!(archive_names.insert(name.to_owned()), "Duplicate archive");
        let path = evidence.join(name);
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.len() < 1000 * 1024, "Repository archive size limit");
        assert_eq!(bytes.len() as u64, archive["bytes"]);
        assert_eq!(digest(&bytes), archive["sha256"]);
        // List before extraction so duplicate members cannot silently overwrite evidence.
        let listing = Command::new("tar").arg("-tf").arg(&path).output().unwrap();
        assert!(listing.status.success());
        let listing = String::from_utf8(listing.stdout).unwrap();
        let members: Vec<_> = listing.lines().collect();
        assert_eq!(members.len() as u64, archive["files"]);
        for member in members {
            assert!(seen.insert(member.to_owned()), "Duplicate evidence member");
            assert_eq!(index.get(member).unwrap()["archive"], name);
        }
        assert!(
            Command::new("tar")
                .arg("-xf")
                .arg(&path)
                .arg("-C")
                .arg(temp.path())
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    let present_archives: std::collections::BTreeSet<_> = fs::read_dir(&evidence)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.starts_with("planned-delta1-") && name.ends_with(".tar.gz"))
        .collect();
    assert_eq!(present_archives, archive_names);
    assert_eq!(seen, index.keys().cloned().collect());
    fn names(root: &Path, dir: &Path, all: &mut Vec<String>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                names(root, &p, all)
            } else {
                all.push(p.strip_prefix(root).unwrap().to_str().unwrap().into())
            }
        }
    }
    let mut all = Vec::new();
    names(temp.path(), temp.path(), &mut all);
    all.sort();
    assert_eq!(all, index.keys().cloned().collect::<Vec<_>>());
    for (name, entry) in index {
        let bytes = fs::read(temp.path().join(name)).unwrap();
        assert_eq!(bytes.len() as u64, entry["bytes"]);
        assert_eq!(digest(&bytes), entry["sha256"]);
    }
    let run: gossil::Run =
        serde_json::from_slice(&fs::read(temp.path().join("run-plan-final/report.json")).unwrap())
            .unwrap();
    assert!(run.accepted);
    assert!(!run.timed_out);
    assert_eq!(run.exit_code, Some(0));
    assert_eq!(
        run.config
            .invocation_plan
            .as_ref()
            .unwrap()
            .expected_invocations,
        22804
    );
    assert_eq!(run.report.assertions.len(), 22804);
    assert_eq!(
        run.report
            .assertions
            .iter()
            .filter(|a| !a.framework)
            .count(),
        22803
    );
    let rows = &run.invocation_plan.as_ref().unwrap().rows;
    assert_eq!(rows.len(), 22804);
    let mut ids = std::collections::BTreeSet::new();
    for (actual, expected) in run.report.assertions.iter().zip(rows) {
        assert_eq!(actual.name, expected.name);
        assert_eq!(
            actual.expected_invocation_id.as_ref().unwrap(),
            &expected.id
        );
        assert!(ids.insert(expected.id.clone()));
        assert_eq!(actual.state, "pass");
    }
    assert!(
        parse_report(&run.stdout).malformed,
        "Strict default still rejects the repeated original names"
    );
}
