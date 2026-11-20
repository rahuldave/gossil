//! Test recovery sidecars during public repository creation.
use gossil::artifact::ParseLimits;
use gossil::storage::{AdmissionLimits, CreateOptions, Limits, LocalOwnerRepository};
use std::path::{Path, PathBuf};

/// Return finite content bounds sufficient for the empty native root.
fn content_limits() -> Limits {
    Limits {
        max_stored_bytes: 4096,
        max_output_bytes: 4096,
        max_chain_depth: 4,
        max_total_stored_bytes: 8192,
        max_total_decoded_bytes: 16384,
    }
}
/// Return finite parser bounds for the constructor's canonical root.
fn parse_limits() -> ParseLimits {
    ParseLimits {
        input_bytes: 4096,
        cards: 16,
        owned_bytes: 16384,
        entries: 64,
    }
}
/// Bind a real local checkout without fixture SQL or hidden initialization.
fn options(root: &Path) -> CreateOptions<'_> {
    CreateOptions {
        repository_id: "0123456789abcdef0123456789abcdef01234567",
        owner_id: "local-owner",
        checkout_id: "default",
        checkout_root: root,
        date: b"2026-11-18T00:00:00.000",
        user: b"Local Owner",
        content_limits: content_limits(),
        parse_limits: parse_limits(),
    }
}
/// Construct native SQLite neighbor names without interpreting URI punctuation.
fn neighbor(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    name.into()
}

#[tokio::test]
/// Refuse each stale recovery file before engine entry and preserve its bytes.
async fn create_refuses_each_existing_sidecar_without_destination_effects() {
    for suffix in ["-journal", "-wal", "-shm"] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("file:repo?#%.db");
        let sidecar = neighbor(&path, suffix);
        let bytes = b"retained stale recovery bytes\0\xff";
        std::fs::write(&sidecar, bytes).unwrap();
        let error = LocalOwnerRepository::create(&path, options(temp.path()))
            .await
            .err()
            .unwrap();
        assert_eq!(
            error.operation,
            format!("repository sidecar {suffix} exists; retained")
        );
        assert!(error.cleanup.is_none());
        assert_eq!(std::fs::read(&sidecar).unwrap(), bytes);
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
        std::fs::remove_file(sidecar).unwrap();
        drop(
            LocalOwnerRepository::create(&path, options(temp.path()))
                .await
                .unwrap(),
        );
        drop(
            LocalOwnerRepository::open(
                &path,
                AdmissionLimits {
                    content: content_limits(),
                    parser: parse_limits(),
                },
            )
            .await
            .unwrap(),
        );
    }
}

#[tokio::test]
/// Do not follow dangling recovery symlinks or replace recovery directories.
async fn create_refuses_nonregular_sidecars_and_retains_first_failure() {
    // Earlier binding validation remains the first error even with a stale journal.
    {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let sidecar = neighbor(&path, "-journal");
        std::fs::write(&sidecar, b"untouched").unwrap();
        let mut invalid = options(temp.path());
        invalid.owner_id = "";
        let error = LocalOwnerRepository::create(&path, invalid)
            .await
            .err()
            .unwrap();
        assert_eq!(error.operation, "invalid local repository identity binding");
        assert_eq!(std::fs::read(sidecar).unwrap(), b"untouched");
        assert!(!path.exists());
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let journal = neighbor(&path, "-journal");
    let wal = neighbor(&path, "-wal");
    std::os::unix::fs::symlink("missing-target", &journal).unwrap();
    std::fs::create_dir(&wal).unwrap();
    let error = LocalOwnerRepository::create(&path, options(temp.path()))
        .await
        .err()
        .unwrap();
    assert_eq!(
        error.operation,
        "repository sidecar -journal exists; retained"
    );
    assert!(error.cleanup.is_none());
    assert_eq!(
        std::fs::read_link(&journal).unwrap(),
        Path::new("missing-target")
    );
    assert!(wal.is_dir());
    assert!(!path.exists());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 2);

    std::fs::remove_file(&journal).unwrap();
    let error = LocalOwnerRepository::create(&path, options(temp.path()))
        .await
        .err()
        .unwrap();
    assert_eq!(error.operation, "repository sidecar -wal exists; retained");
    assert!(error.cleanup.is_none());
    assert!(wal.is_dir());
    assert!(!path.exists());
    let entries: Vec<_> = std::fs::read_dir(temp.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(entries, vec![wal]);
}

#[tokio::test]
/// Preserve a real sidecar observation failure and its original OS cause.
async fn sidecar_observation_failure_retains_original_io_error() {
    use std::error::Error;
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("regular-parent");
    std::fs::write(&parent, b"retained parent bytes").unwrap();
    let path = parent.join("repo");
    let expected = std::fs::symlink_metadata(neighbor(&path, "-journal")).unwrap_err();
    let error = LocalOwnerRepository::create(&path, options(temp.path()))
        .await
        .err()
        .unwrap();
    assert_eq!(
        error.operation,
        "repository sidecar -journal observation failed"
    );
    let cause = error
        .source()
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .unwrap();
    assert_eq!(cause.kind(), expected.kind());
    assert_eq!(cause.raw_os_error(), expected.raw_os_error());
    assert!(error.cleanup.is_none());
    assert_eq!(std::fs::read(parent).unwrap(), b"retained parent bytes");
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
}
