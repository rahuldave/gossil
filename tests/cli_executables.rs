//! Both executable targets share diagnostics, exact errors and OS paths.
use std::os::unix::ffi::OsStringExt;
use std::{ffi::OsString, fs, process::Command};
fn binaries() -> [&'static str; 2] {
    [env!("CARGO_BIN_EXE_gossil"), env!("CARGO_BIN_EXE_gos")]
}
#[test]
fn every_executable_group_pair_runs_all_three_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    let origin = temp.path().join("origin");
    let target = temp.path().join("target");
    let delta = temp.path().join("delta");
    let output = temp.path().join("output");
    fs::write(&origin, b"abcabcabc").unwrap();
    fs::write(&target, b"abcabcxyz").unwrap();
    for binary in binaries() {
        for group in ["vcs", "v"] {
            let result = Command::new(binary)
                .arg(group)
                .arg("test-delta-create")
                .args([&origin, &target, &delta])
                .output()
                .unwrap();
            assert!(result.status.success());
            assert!(result.stdout.is_empty() && result.stderr.is_empty());
            let result = Command::new(binary)
                .arg(group)
                .arg("test-delta-apply")
                .args([&origin, &delta, &output])
                .output()
                .unwrap();
            assert!(result.status.success());
            assert_eq!(fs::read(&output).unwrap(), fs::read(&target).unwrap());
            assert!(result.stdout.is_empty() && result.stderr.is_empty());
            let result = Command::new(binary)
                .arg(group)
                .arg("test-delta")
                .args([&origin, &target])
                .output()
                .unwrap();
            assert!(result.status.success());
            assert_eq!(result.stdout, b"ok\n");
            assert!(result.stderr.is_empty());
            for (cmd, usage) in [
                ("test-delta-apply", "ORIGIN DELTA TARGET"),
                ("test-delta-create", "ORIGIN TARGET DELTA"),
                ("test-delta", "FILE1 FILE2"),
            ] {
                let result = Command::new(binary).args([group, cmd]).output().unwrap();
                assert_eq!(result.status.code(), Some(1));
                assert_eq!(
                    result.stderr,
                    format!("Usage: gossil vcs {cmd} {usage}\n").as_bytes()
                );
                assert!(result.stdout.is_empty());
            }
            fs::write(&delta, b"invalid").unwrap();
            fs::write(&output, b"preserve").unwrap();
            let result = Command::new(binary)
                .arg(group)
                .arg("test-delta-apply")
                .args([&origin, &delta, &output])
                .output()
                .unwrap();
            assert_eq!(result.status.code(), Some(1));
            assert_eq!(result.stderr, b"Delta application failed.\n");
            assert!(result.stdout.is_empty());
            assert_eq!(fs::read(&output).unwrap(), b"preserve");
        }
    }
}
#[test]
fn non_utf8_missing_path_is_preserved_in_error() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp
        .path()
        .join(OsString::from_vec(b"missing-\xff".to_vec()));
    for binary in binaries() {
        for group in ["vcs", "v"] {
            let result = Command::new(binary)
                .arg(group)
                .arg("test-delta")
                .arg(&path)
                .arg("other")
                .output()
                .unwrap();
            assert_eq!(result.status.code(), Some(1));
            assert!(result.stdout.is_empty());
            assert_eq!(
                result.stderr,
                format!("no such file: {}\n", path.display()).as_bytes()
            );
        }
    }
}

/// Every shared entry alias preserves exact glob output and diagnostics.
#[test]
fn every_executable_group_pair_runs_glob() {
    for binary in binaries() {
        for group in ["vcs", "v"] {
            let out = Command::new(binary)
                .args([group, "test-glob", "a*,abc,z", "abc", "z", "no"])
                .output()
                .unwrap();
            assert!(out.status.success());
            assert!(out.stderr.is_empty());
            assert_eq!(out.stdout, b"SQL expression: (x GLOB 'a*' OR x GLOB 'abc' OR x GLOB 'z')\npattern[0] = [a*]\npattern[1] = [abc]\npattern[2] = [z]\n1 1 abc\n3 3 z\n0 0 no\n");
            for (tail, expected) in [
                (vec![], "Usage: gossil vcs test-glob PATTERN STRING ...\n"),
                (
                    vec!["a"],
                    "Usage: gossil vcs test-glob PATTERN STRING ...\n",
                ),
                (
                    vec!["@ignore-glob", "x"],
                    "test-glob setting lookup is not implemented\n",
                ),
            ] {
                let out = Command::new(binary)
                    .args([group, "test-glob"])
                    .args(tail)
                    .output()
                    .unwrap();
                assert_eq!(out.status.code(), Some(1));
                assert!(out.stdout.is_empty());
                assert_eq!(out.stderr, expected.as_bytes());
            }
            let raw = OsString::from_vec(vec![0xff]);
            let out = Command::new(binary)
                .args([group, "test-glob"])
                .arg(&raw)
                .arg(&raw)
                .output()
                .unwrap();
            assert!(out.status.success());
            assert!(out.stderr.is_empty());
            assert_eq!(
                out.stdout,
                b"SQL expression: (x GLOB '\xff')\npattern[0] = [\xff]\n1 1 \xff\n"
            );
            let mut command = Command::new(binary);
            command.args([group, "test-glob", "a", "a"]);
            let out = glob_closed_stdout::configure(command).output().unwrap();
            assert_eq!(out.status.code(), Some(1));
            assert_eq!(
                out.stderr,
                b"unable to open file \"<stdout>\" for writing\n"
            );
        }
    }
}

#[path = "support/closed_stdout.rs"]
mod glob_closed_stdout;
