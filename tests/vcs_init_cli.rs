//! Exercise the initializer executables against native repositories.
use gossil::{
    artifact::ParseLimits,
    storage::{AdmissionLimits, Limits, LocalOwnerRepository},
};
use std::os::unix::ffi::OsStringExt;
use std::{
    ffi::OsString,
    path::Path,
    process::{Command, Output, Stdio},
};

/// Isolate login and preference locations for every subprocess.
fn command(binary: &str, home: &Path) -> Command {
    let mut command = Command::new(binary);
    for key in ["GOSSIL_USER", "USER", "LOGNAME", "USERNAME"] {
        command.env_remove(key);
    }
    command
        .env("HOME", home)
        .env("FOSSIL_HOME", home)
        .env("XDG_CONFIG_HOME", home);
    command
}
/// Open the initializer's admitted empty-root contract.
async fn reopen(path: &Path) -> LocalOwnerRepository {
    LocalOwnerRepository::open(
        path,
        AdmissionLimits {
            content: Limits {
                max_stored_bytes: 1048576,
                max_output_bytes: 1048576,
                max_chain_depth: 1,
                max_total_stored_bytes: 1048576,
                max_total_decoded_bytes: 1048576,
            },
            parser: ParseLimits {
                input_bytes: 1048576,
                cards: 32,
                owned_bytes: 4194304,
                entries: 32,
            },
        },
    )
    .await
    .unwrap()
}
/// Execute an isolated normal initialization.
fn initialize(home: &Path, path: &Path, flags: &[&str]) -> Output {
    command(env!("CARGO_BIN_EXE_gossil"), home)
        .args(["vcs", "init"])
        .args(flags)
        .arg(path)
        .output()
        .unwrap()
}
/// Query independent native facts after the CLI releases its owner.
async fn facts(path: &Path) -> (String, f64, i64, Option<String>) {
    let db = libsql::Builder::new_local(path).build().await.unwrap();
    let c = db.connect().unwrap();
    let mut rows = c.query("SELECT b.uuid,e.mtime,g.checkout_count,CAST((SELECT value FROM config WHERE name='hash-policy') AS TEXT) FROM blob b JOIN event e ON e.objid=b.rid CROSS JOIN g_storage g", ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    (
        row.get(0).unwrap(),
        row.get(1).unwrap(),
        row.get(2).unwrap(),
        row.get(3).unwrap(),
    )
}

#[tokio::test]
/// Every binary, group and command alias creates an admitted repository with real identities.
async fn every_alias_initializes_and_reopens_without_checkout() {
    let temp = tempfile::tempdir().unwrap();
    for (i, binary) in [env!("CARGO_BIN_EXE_gossil"), env!("CARGO_BIN_EXE_gos")]
        .iter()
        .enumerate()
    {
        for group in ["vcs", "v"] {
            for verb in ["init", "new"] {
                let path = temp.path().join(format!("{i}-{group}-{verb}"));
                let result = command(binary, temp.path())
                    .args([group, verb])
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                assert!(result.stderr.is_empty());
                let mut owner = reopen(&path).await;
                let (project, server) = owner.initialization_identity().await.unwrap();
                assert_eq!(project.len(), 40);
                assert_eq!(server.len(), 40);
                let receipt = String::from_utf8(result.stdout).unwrap();
                assert!(receipt.contains(&format!(
                    "project-id: {project}\nserver-id:  {server}\nadmin-user: root"
                )));
                drop(owner);
                let (hash, _, checkouts, policy) = facts(&path).await;
                assert_eq!(hash.len(), 64);
                assert_eq!(checkouts, 0);
                assert_eq!(policy, None);
            }
        }
    }
}

#[tokio::test]
/// Explicit date and SHA1 select actual native event time, hash and policy.
async fn implemented_flags_change_native_facts() {
    let temp = tempfile::tempdir().unwrap();
    for (i, admin) in ["--admin-user=Explicit Owner", "-A"].iter().enumerate() {
        let path = temp.path().join(format!("repo-{i}"));
        let mut args = vec![*admin];
        if *admin == "-A" {
            args.push("Explicit Owner");
        }
        args.extend(["--date-override=2026-11-18 12:34:56.125", "--sha1"]);
        let result = initialize(temp.path(), &path, &args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(String::from_utf8_lossy(&result.stdout).contains("admin-user: Explicit Owner"));
        drop(reopen(&path).await);
        let (hash, time, checkouts, policy) = facts(&path).await;
        assert_eq!(hash.len(), 40);
        assert_eq!(checkouts, 0);
        assert_eq!(policy.as_deref(), Some("0"));
        let db = libsql::Builder::new_local(&path).build().await.unwrap();
        let c = db.connect().unwrap();
        let mut rows = c
            .query(
                "SELECT abs(CAST(?1 AS REAL)-julianday('2026-11-18 12:34:56.125')) < 0.00000001",
                vec![libsql::Value::Real(time)],
            )
            .await
            .unwrap();
        assert_eq!(
            rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
            1
        );
    }
}

#[test]
/// Unsupported and malformed options are refused before creating a destination or preferences.
fn rejected_options_and_arity_have_no_effects() {
    for (flags, rejected) in [
        (vec!["--template", "source"], Some("--template")),
        (vec!["--project-name=x"], Some("--project-name=x")),
        (vec!["--project-desc", "x"], Some("--project-desc")),
        (vec!["--globalprefs"], Some("--globalprefs")),
        (vec!["--unknown"], Some("--unknown")),
        (vec!["--sha1", "--sha1"], Some("--sha1")),
        (vec!["--date-override"], Some("--date-override")),
        (vec!["-A"], Some("-A")),
        (vec!["extra"], None),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        // Value-taking missing arguments are tested without a destination below.
        let mut c = command(env!("CARGO_BIN_EXE_gossil"), temp.path());
        c.args(["vcs", "init"]).args(&flags);
        if flags != ["--date-override"] && flags != ["-A"] {
            c.arg(&path);
        }
        let result = c.output().unwrap();
        assert_eq!(result.status.code(), Some(1), "{flags:?}");
        assert!(result.stdout.is_empty());
        let expected = match rejected {
            Some(option) => {
                format!("unrecognized command-line option or missing argument: {option}\n")
            }
            None => "Usage: gossil vcs init ?OPTIONS? FILENAME\n".to_string(),
        };
        assert_eq!(result.stderr, expected.as_bytes(), "{flags:?}");
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }
    let temp = tempfile::tempdir().unwrap();
    let result = command(env!("CARGO_BIN_EXE_gossil"), temp.path())
        .args(["vcs", "init"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(
        result.stderr,
        b"Usage: gossil vcs init ?OPTIONS? FILENAME\n"
    );
    assert!(result.stdout.is_empty());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
/// Help is successful, advertises only implemented flags, and separator creates a literal filename.
fn help_and_literal_separator() {
    let temp = tempfile::tempdir().unwrap();
    for flag in ["--help", "-h"] {
        let result = command(env!("CARGO_BIN_EXE_gossil"), temp.path())
            .args(["v", "new", flag])
            .output()
            .unwrap();
        assert!(result.status.success());
        assert!(result.stderr.is_empty());
        let help = String::from_utf8(result.stdout).unwrap();
        for absent in ["--template", "--project-name", "--project-desc", "vcs open"] {
            assert!(!help.contains(absent));
        }
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }
    let result = command(env!("CARGO_BIN_EXE_gossil"), temp.path())
        .current_dir(temp.path())
        .args(["vcs", "init", "--", "--help"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(temp.path().join("--help").is_file());
}

#[tokio::test]
/// Login fallback uses only the specified environment precedence and never accesses preference files.
async fn environment_precedence_without_preferences() {
    let temp = tempfile::tempdir().unwrap();
    // Invalid preference bytes would fail any attempt to open these as SQLite databases.
    for name in [".fossil", ".fossil.db", ".gossil", ".gossil.db"] {
        std::fs::write(temp.path().join(name), b"sentinel").unwrap();
    }
    let keys = ["GOSSIL_USER", "USER", "LOGNAME", "USERNAME"];
    for start in 0..=keys.len() {
        let path = temp.path().join(format!("repo-{start}"));
        let mut c = command(env!("CARGO_BIN_EXE_gossil"), temp.path());
        for (i, key) in keys.iter().enumerate() {
            c.env(key, if i < start { "" } else { key });
        }
        let result = c.args(["vcs", "init"]).arg(&path).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let expected = keys.get(start).copied().unwrap_or("root");
        assert!(
            String::from_utf8_lossy(&result.stdout).contains(&format!("admin-user: {expected} "))
        );
        drop(reopen(&path).await);
    }
    for name in [".fossil", ".fossil.db", ".gossil", ".gossil.db"] {
        assert_eq!(std::fs::read(temp.path().join(name)).unwrap(), b"sentinel");
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 9);
}

#[tokio::test]
/// URI-looking and SQLite-special spellings are mandatory literal filesystem names.
async fn literal_uri_and_memory_names_initialize_and_reopen() {
    let temp = tempfile::tempdir().unwrap();
    for name in ["file:literal?mode=memory", ":memory:"] {
        let path = temp.path().join(name);
        let result = command(env!("CARGO_BIN_EXE_gossil"), temp.path())
            .current_dir(temp.path())
            .args(["vcs", "init"])
            .arg(name)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(path.is_file());
        drop(reopen(&path).await);
    }
}

#[tokio::test]
/// Preserve exact raw filenames or the host's explicit invalid-byte rejection without inventing a name.
async fn raw_filename_creation_matches_host_capability() {
    let temp = tempfile::tempdir().unwrap();
    let name = OsString::from_vec(b"repo-\xff".to_vec());
    let path = temp.path().join(&name);
    let probe = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path);
    let expected_failure = match probe {
        Ok(file) => {
            // Only the exclusively created probe belongs to this test.
            drop(file);
            std::fs::remove_file(&path).unwrap();
            None
        }
        Err(error) => {
            // APFS rejects invalid UTF-8 with EILSEQ. Other probe failures are test failures.
            assert_eq!(
                error.raw_os_error(),
                Some(libc::EILSEQ),
                "unexpected raw filename probe failure: {error}"
            );
            let mut operation = "exclusive repository creation failed".to_string();
            for suffix in ["-journal", "-wal", "-shm"] {
                let mut sidecar = path.as_os_str().to_os_string();
                sidecar.push(suffix);
                match std::fs::symlink_metadata(&sidecar) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        assert_eq!(
                            error.raw_os_error(),
                            Some(libc::EILSEQ),
                            "unexpected sidecar probe failure: {error}"
                        );
                        operation = format!("repository sidecar {suffix} observation failed");
                        break;
                    }
                    Ok(_) => panic!("unexpected sidecar exists in isolated raw filename fixture"),
                }
            }
            Some(operation)
        }
    };
    let result = command(env!("CARGO_BIN_EXE_gossil"), temp.path())
        .current_dir(temp.path())
        .args(["vcs", "init"])
        .arg(&name)
        .output()
        .unwrap();
    match expected_failure {
        None => {
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(result.stderr.is_empty());
            assert!(path.is_file());
            drop(reopen(&path).await);
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
            assert_eq!(
                std::fs::read_dir(temp.path())
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .file_name(),
                name
            );
        }
        Some(operation) => {
            // StorageError::Display prints the operation, and product::entry adds one newline;
            // the original EILSEQ cause is retained internally rather than printed by the CLI.
            assert_eq!(result.status.code(), Some(1));
            assert!(result.stdout.is_empty());
            assert_eq!(result.stderr, format!("{operation}\n").as_bytes());
            assert!(!path.exists());
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
        }
    }
}

#[test]
/// Invalid dates, login bytes, existing destinations and sidecars cannot become successful repositories.
fn invalid_inputs_and_existing_names_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    for (i, flags) in [vec!["--date-override=not a date"], vec!["--admin-user="]]
        .iter()
        .enumerate()
    {
        let path = temp.path().join(format!("invalid-{i}"));
        let result = initialize(temp.path(), &path, flags);
        assert_eq!(result.status.code(), Some(1));
        assert!(result.stdout.is_empty());
        if i == 0 {
            assert!(result.stderr.starts_with(
                b"unrecognized date format (not a date): use \"YYYY-MM-DD HH:MM:SS.SSS\""
            ));
        } else {
            assert_eq!(result.stderr, b"invalid native administrator or password\n");
            assert!(!path.exists());
        }
    }
    // A failed engine attempt may conservatively retain its exclusively created file.
    let path = temp.path().join("existing");
    std::fs::write(&path, b"retained").unwrap();
    let result = initialize(temp.path(), &path, &[]);
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(
        result.stderr,
        format!("file already exists: {}\n", path.display()).as_bytes()
    );
    assert!(result.stdout.is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), b"retained");
    for suffix in ["-wal", "-shm", "-journal"] {
        let path = temp.path().join(format!("sidecar{suffix}"));
        let sidecar = temp.path().join(format!("sidecar{suffix}{suffix}"));
        std::fs::write(&sidecar, b"retained").unwrap();
        let result = initialize(temp.path(), &path, &[]);
        assert_eq!(result.status.code(), Some(1));
        assert_eq!(
            result.stderr,
            format!("repository sidecar {suffix} exists; retained\n").as_bytes()
        );
        assert!(result.stdout.is_empty());
        assert!(!path.exists());
        assert_eq!(std::fs::read(sidecar).unwrap(), b"retained");
    }
    let adjacent = temp.path().join("adjacent");
    assert!(initialize(temp.path(), &adjacent, &[]).status.success());
}

#[test]
/// Native invalid login/date bytes fail before creation and symlinks and regular parents stay intact.
fn raw_invalid_values_and_namespace_refusals() {
    let temp = tempfile::tempdir().unwrap();
    for (flag, diagnostic) in [
        (
            b"--admin-user=raw\xff".as_slice(),
            "local native user is not UTF-8\n",
        ),
        (
            b"--date-override=raw\xff".as_slice(),
            "date-override must be UTF-8\n",
        ),
    ] {
        let path = temp.path().join("invalid");
        let result = command(env!("CARGO_BIN_EXE_gossil"), temp.path())
            .args(["vcs", "init"])
            .arg(OsString::from_vec(flag.to_vec()))
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert_eq!(result.stderr, diagnostic.as_bytes());
        assert!(result.stdout.is_empty());
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }
    let target = temp.path().join("target");
    std::fs::write(&target, b"retained").unwrap();
    let link = temp.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let result = initialize(temp.path(), &link, &[]);
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(
        result.stderr,
        format!("file already exists: {}\n", link.display()).as_bytes()
    );
    assert!(result.stdout.is_empty());
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"retained");
    let result = initialize(temp.path(), &target.join("child"), &[]);
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(result.stderr, b"repository parent is not a directory\n");
    assert!(result.stdout.is_empty());
    assert_eq!(std::fs::read(&target).unwrap(), b"retained");
}

#[tokio::test]
/// A real pipe with its read peer closed reports EPIPE after initialization has committed.
async fn closed_stdout_peer_retains_committed_repository() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let mut descriptors = [0; 2];
    // SAFETY: pipe initializes exactly the two descriptors in this writable array.
    assert_eq!(unsafe { libc::pipe(descriptors.as_mut_ptr()) }, 0);
    use std::os::fd::FromRawFd;
    // SAFETY: each newly allocated descriptor is adopted once and closed by File.
    let reader = unsafe { std::fs::File::from_raw_fd(descriptors[0]) };
    let writer = unsafe { std::fs::File::from_raw_fd(descriptors[1]) };
    drop(reader);
    let result = command(env!("CARGO_BIN_EXE_gossil"), temp.path())
        .args(["vcs", "init"])
        .arg(&path)
        .stdout(Stdio::from(writer))
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
        .wait_with_output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(
        error.contains("Broken pipe") || error.contains("broken pipe"),
        "{error}"
    );
    let mut owner = reopen(&path).await;
    assert_eq!(owner.initialization_identity().await.unwrap().0.len(), 40);
}

#[test]
/// Help output retains the real closed-peer EPIPE error without creating any files.
fn closed_stdout_peer_rejects_help_without_filesystem_effects() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let mut descriptors = [0; 2];
    // SAFETY: pipe initializes exactly the two descriptors in this writable array.
    assert_eq!(unsafe { libc::pipe(descriptors.as_mut_ptr()) }, 0);
    use std::os::fd::FromRawFd;
    // SAFETY: each newly allocated descriptor is adopted once and closed by File.
    let reader = unsafe { std::fs::File::from_raw_fd(descriptors[0]) };
    let writer = unsafe { std::fs::File::from_raw_fd(descriptors[1]) };
    drop(reader);
    let result = command(env!("CARGO_BIN_EXE_gossil"), temp.path())
        .current_dir(temp.path())
        .args(["vcs", "init", "--help"])
        .arg(&path)
        .stdout(Stdio::from(writer))
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
        .wait_with_output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(
        error.contains("Broken pipe") || error.contains("broken pipe"),
        "{error}"
    );
    assert!(!path.exists());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}
