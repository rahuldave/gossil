//! Public native filename boundary regressions.

use gossil::storage::LocalStore;

#[tokio::test]
/// Reject embedded NUL filenames through the public opening API without panicking.
async fn embedded_nul_path_returns_error_with_original_cause() {
    use std::error::Error;
    let result = LocalStore::open("bad\0name.db").await;
    let error = result
        .err()
        .expect("embedded NUL path must return an error");
    assert!(error.operation.contains("path"));
    let cause = error
        .source()
        .unwrap()
        .downcast_ref::<std::ffi::NulError>()
        .unwrap();
    assert_eq!(cause.nul_position(), 3);
    assert!(error.cleanup.is_none());
}

#[tokio::test]
/// Preserve native filename bytes and SQLite temporary-file semantics.
async fn path_validation_preserves_empty_invalid_utf8_and_valid_file_behavior() {
    use std::{error::Error, os::unix::ffi::OsStrExt};
    let error = LocalStore::open("")
        .await
        .err()
        .expect("temporary database cannot satisfy required WAL mode");
    assert_eq!(
        error.operation,
        "configuration disagreement: PRAGMA journal_mode"
    );
    assert!(error.source().is_none());
    let temp = tempfile::tempdir().unwrap();
    let native = temp
        .path()
        .join(std::ffi::OsStr::from_bytes(b"native-\xff?#%.db"));
    match std::fs::File::create(&native) {
        Ok(file) => {
            drop(file);
            let mut store = LocalStore::open(&native).await.unwrap();
            store.begin_write().await.unwrap().commit().await.unwrap();
            drop(store);
            assert!(native.is_file());
            LocalStore::open(&native).await.unwrap();
        }
        Err(_) => {
            // macOS rejects these raw filename bytes at the filesystem boundary;
            // pinned C Fossil also fails CANTOPEN. The URI bridge must not invent a name.
            let error = LocalStore::open(&native).await.err().unwrap();
            assert!(matches!(
                error.source().unwrap().downcast_ref::<libsql::Error>(),
                Some(libsql::Error::ConnectionFailed(_))
            ));
            assert!(!native.exists());
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
        }
    }
    let path = temp.path().join("valid.db");
    let mut store = LocalStore::open(&path).await.unwrap();
    store.begin_write().await.unwrap().commit().await.unwrap();
    assert!(path.is_file());
}

/// A relative filename beginning with file: remains a literal native path.
#[tokio::test]
async fn relative_uri_shaped_filename_is_literal_and_reopens() {
    let cwd = std::env::current_dir().unwrap();
    let directory = tempfile::Builder::new()
        .prefix("file:native-")
        .tempdir_in(&cwd)
        .unwrap();
    let absolute = directory.path().join("selected?#%.db");
    let relative = absolute.strip_prefix(&cwd).unwrap();
    assert!(relative.to_str().unwrap().starts_with("file:"));
    let mut store = LocalStore::open(relative).await.unwrap();
    store.begin_write().await.unwrap().commit().await.unwrap();
    drop(store);
    assert!(absolute.is_file());
    let mut reopened = LocalStore::open(&absolute).await.unwrap();
    reopened
        .begin_write()
        .await
        .unwrap()
        .rollback()
        .await
        .unwrap();
}

/// Raw Unix names retain their identity when a leading double slash is supplied.
#[tokio::test]
async fn raw_byte_path_with_double_slash_keeps_empty_uri_authority() {
    use std::{
        error::Error,
        ffi::OsString,
        os::unix::ffi::{OsStrExt, OsStringExt},
    };
    let directory = tempfile::tempdir().unwrap();
    let native = directory
        .path()
        .join(std::ffi::OsStr::from_bytes(b"double-\xff?#%.db"));
    let mut bytes = vec![b'/'];
    bytes.extend_from_slice(native.as_os_str().as_bytes());
    let double = std::path::PathBuf::from(OsString::from_vec(bytes));
    match std::fs::File::create(&native) {
        Ok(file) => {
            drop(file);
            let mut store = LocalStore::open(&double).await.unwrap();
            store.begin_write().await.unwrap().commit().await.unwrap();
            drop(store);
            assert!(native.is_file());
            let reopened = LocalStore::open(&native).await.unwrap();
            drop(reopened);
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        }
        Err(_) => {
            let error = LocalStore::open(&double).await.err().unwrap();
            assert!(matches!(
                error.source().unwrap().downcast_ref::<libsql::Error>(),
                Some(libsql::Error::ConnectionFailed(_))
            ));
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
        }
    }
}
