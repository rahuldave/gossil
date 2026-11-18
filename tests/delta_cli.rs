//! Exact subprocess contracts for the repository-free delta adapter.
use serde_json::Value;
use std::fs;
use std::process::{Command, Output};

/// Convert fixture hex without interpreting UTF-8.
fn bytes(value: &Value) -> Vec<u8> {
    let text = value.as_str().unwrap();
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}
/// Invoke the candidate directly, never a C reference or shell.
fn invoke(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gossil"))
        .args(args)
        .output()
        .unwrap()
}
/// Replay every bounded native decode result, including exact failure framing.
#[test]
fn cli_pinned_observations() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/delta/cases.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("origin");
    let delta = dir.path().join("delta");
    let target = dir.path().join("target");
    for case in cases {
        if case["status"] == "not-run" {
            continue;
        }
        fs::write(&source, bytes(&case["source"])).unwrap();
        fs::write(&delta, bytes(&case["delta"])).unwrap();
        fs::write(&target, b"sentinel\0\xff").unwrap();
        let output = invoke(&[
            "vcs".as_ref(),
            "test-delta-apply".as_ref(),
            source.as_os_str(),
            delta.as_os_str(),
            target.as_os_str(),
        ]);
        assert_eq!(
            output.status.code(),
            case["exit"].as_i64().map(|v| v as i32),
            "{}",
            case["id"]
        );
        assert_eq!(output.stdout, bytes(&case["stdout"]));
        assert_eq!(output.stderr, bytes(&case["stderr"]));
        assert_eq!(
            fs::read(&target).unwrap(),
            if case["status"] == "accepted" {
                bytes(&case["target"])
            } else {
                b"sentinel\0\xff".to_vec()
            },
            "{}",
            case["id"]
        );
    }
}
/// Match characterized usage, paths, aliases and target preservation.
#[test]
fn cli_paths_and_usage() {
    for args in [
        vec![],
        vec!["test-delta-apply"],
        vec!["vcs", "test-delta-apply", "one"],
        vec!["vcs", "test-delta-apply", "one", "two"],
        vec!["vcs", "test-delta-apply", "one", "two", "three", "extra"],
    ] {
        let args: Vec<_> = args.iter().map(std::ffi::OsStr::new).collect();
        let output = invoke(&args);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(
            output.stderr,
            b"Usage: gossil vcs test-delta-apply ORIGIN DELTA TARGET\n"
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("origin with spaces");
    let delta = dir.path().join("delta with spaces");
    let target = dir.path().join("target with spaces");
    for missing_delta in [false, true] {
        if missing_delta {
            fs::write(&source, b"A").unwrap();
        }
        fs::write(&target, b"preserve").unwrap();
        let output = invoke(&[
            "vcs".as_ref(),
            "test-delta-apply".as_ref(),
            source.as_os_str(),
            delta.as_os_str(),
            target.as_os_str(),
        ]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(
            output.stderr,
            format!(
                "no such file: {}\n",
                if missing_delta { &delta } else { &source }.display()
            )
            .as_bytes()
        );
        assert_eq!(fs::read(&target).unwrap(), b"preserve");
    }
    fs::write(&source, b"ABC").unwrap();
    fs::write(&delta, b"3\n3@0,;").unwrap();
    let rejected_root = invoke(&[
        "test-delta-apply".as_ref(),
        source.as_os_str(),
        delta.as_os_str(),
        target.as_os_str(),
    ]);
    assert_eq!(rejected_root.status.code(), Some(1));
    assert!(rejected_root.stdout.is_empty());
    assert_eq!(
        rejected_root.stderr,
        b"Usage: gossil vcs test-delta-apply ORIGIN DELTA TARGET\n"
    );
    assert_eq!(fs::read(&target).unwrap(), b"preserve");
    for alias in [0, 1, 2] {
        fs::write(&source, b"ABC").unwrap();
        fs::write(&delta, b"3\n3@0,;").unwrap();
        let target = [&target, &source, &delta][alias];
        let output = invoke(&[
            "vcs".as_ref(),
            "test-delta-apply".as_ref(),
            source.as_os_str(),
            delta.as_os_str(),
            target.as_os_str(),
        ]);
        assert!(output.status.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        assert_eq!(fs::read(target).unwrap(), b"ABC");
    }
    fs::write(&source, b"ABC").unwrap();
    fs::write(&delta, b"3\n3@0,;").unwrap();
    let output = invoke(&[
        "vcs".as_ref(),
        "test-delta-apply".as_ref(),
        source.as_os_str(),
        delta.as_os_str(),
        dir.path().as_os_str(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        format!(
            "unable to open file \"{}\" for writing\n",
            dir.path().display()
        )
        .as_bytes()
    );
    assert!(dir.path().is_dir());
}
