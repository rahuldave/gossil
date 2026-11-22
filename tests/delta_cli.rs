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
        vec!["vcs", "test-delta-apply"],
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
    // A bare root diagnostic is an unknown root command, a parser failure.
    assert_eq!(rejected_root.status.code(), Some(2));
    assert!(rejected_root.stdout.is_empty());
    assert_eq!(
        rejected_root.stderr,
        b"gossil: unknown command \"test-delta-apply\"\nRun \"gossil --help\" to list the commands.\n"
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

/// All finite native create bytes and bidirectional round trips through the public adapter.
#[test]
fn cli_encoder_vectors() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/encoder");
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/encoder/provenance.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let delta = dir.path().join("delta");
    for case in cases.as_array().unwrap() {
        let fixture = root.join(case["id"].as_str().unwrap());
        let source = fixture.join("origin.bin");
        let target = fixture.join("target.bin");
        let result = invoke(&[
            "vcs".as_ref(),
            "test-delta-create".as_ref(),
            source.as_os_str(),
            target.as_os_str(),
            delta.as_os_str(),
        ]);
        assert!(result.status.success(), "{:?}", result);
        assert!(result.stdout.is_empty());
        assert!(result.stderr.is_empty());
        assert_eq!(
            fs::read(&delta).unwrap(),
            fs::read(fixture.join("delta.bin")).unwrap()
        );
        let result = invoke(&[
            "vcs".as_ref(),
            "test-delta".as_ref(),
            source.as_os_str(),
            target.as_os_str(),
        ]);
        assert!(result.status.success());
        assert_eq!(result.stdout, b"ok\n");
        assert!(result.stderr.is_empty());
    }
}

/// Regular-file errors occur before output opening, including either aliased input.
#[test]
fn encoder_file_and_usage_policy() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let target = dir.path().join("target");
    let missing = dir.path().join("missing");
    let output = dir.path().join("output");
    fs::write(&source, b"source").unwrap();
    fs::write(&target, b"target").unwrap();
    fs::write(&output, b"preserve").unwrap();
    for (command, usage) in [
        ("test-delta-create", "ORIGIN TARGET DELTA"),
        ("test-delta", "FILE1 FILE2"),
    ] {
        for extra in [vec![], vec!["one"], vec!["one", "two", "three", "four"]] {
            let mut args = vec!["vcs".as_ref(), command.as_ref()];
            args.extend(extra.iter().map(std::ffi::OsStr::new));
            let result = invoke(&args);
            assert_eq!(result.status.code(), Some(1));
            assert_eq!(
                result.stderr,
                format!("Usage: gossil vcs {command} {usage}\n").as_bytes()
            );
            assert!(result.stdout.is_empty());
        }
    }
    for (a, b) in [(&missing, &target), (&source, &missing)] {
        let result = invoke(&[
            "vcs".as_ref(),
            "test-delta-create".as_ref(),
            a.as_os_str(),
            b.as_os_str(),
            output.as_os_str(),
        ]);
        assert_eq!(result.status.code(), Some(1));
        assert_eq!(
            result.stderr,
            format!("no such file: {}\n", missing.display()).as_bytes()
        );
        assert_eq!(fs::read(&output).unwrap(), b"preserve");
        let result = invoke(&[
            "vcs".as_ref(),
            "test-delta".as_ref(),
            a.as_os_str(),
            b.as_os_str(),
        ]);
        assert_eq!(result.status.code(), Some(1));
    }
    let result = invoke(&[
        "vcs".as_ref(),
        "test-delta-create".as_ref(),
        source.as_os_str(),
        target.as_os_str(),
        dir.path().as_os_str(),
    ]);
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(
        result.stderr,
        format!(
            "unable to open file \"{}\" for writing\n",
            dir.path().display()
        )
        .as_bytes()
    );
    // Output aliasing either input is safe because both inputs have already been read.
    for alias in [&source, &target] {
        fs::write(&source, b"source").unwrap();
        fs::write(&target, b"target").unwrap();
        let expected = gossil::delta::create(b"source", b"target").unwrap();
        let result = invoke(&[
            "vcs".as_ref(),
            "test-delta-create".as_ref(),
            source.as_os_str(),
            target.as_os_str(),
            alias.as_os_str(),
        ]);
        assert!(result.status.success());
        assert_eq!(fs::read(alias).unwrap(), expected);
    }
}
