//! Controls for native pending additions; no recording commit or frozen staging.
//!
//! ## Recommendations
//! Run these controls after the shared native tracked-state admission hooks land.
use gossil::artifact::ParseLimits;
use gossil::storage::{
    AdmissionLimits, InitializeOptions, Limits, LocalOwnerRepository, TrackingLimits,
};
use std::path::{Path, PathBuf};

#[path = "support/proof.rs"]
mod proof;
/// Set explicit finite root and registration ceilings.
fn limits() -> AdmissionLimits {
    AdmissionLimits {
        content: Limits {
            max_stored_bytes: 4096,
            max_output_bytes: 4096,
            max_chain_depth: 4,
            max_total_stored_bytes: 8192,
            // Catalog6 admission charges exact schema/authority and complete operation work.
            max_total_decoded_bytes: 16777216,
        },
        parser: ParseLimits {
            input_bytes: 4096,
            cards: 4096,
            owned_bytes: 1048576,
            entries: 64,
        },
    }
}
/// Initialize through the public native API without allocating a checkout.
/// # Panics
/// Panics if fixture setup or the expected database operation fails.
async fn initialize(path: &Path) -> LocalOwnerRepository {
    let bounds = limits();
    LocalOwnerRepository::initialize(
        path,
        InitializeOptions {
            repository_id: "0123456789abcdef0123456789abcdef01234567",
            owner_id: "owner",
            date: "2026-11-18 12:34:56.125",
            user: b"Owner",
            password: "secret",
            sha1: false,
            content_limits: bounds.content,
            parse_limits: bounds.parser,
        },
    )
    .await
    .unwrap()
}
/// Create a canonical fixture directory to avoid platform temporary-directory aliases.
/// # Panics
/// Panics if fixture setup or the expected database operation fails.
fn checkout_root(temp: &Path) -> PathBuf {
    let root = temp.join("root");
    std::fs::create_dir(&root).unwrap();
    std::fs::canonicalize(root).unwrap()
}

/// Bound the complete explicit batch and the bytes of one captured file.
fn tracking() -> TrackingLimits {
    TrackingLimits {
        paths: 64,
        path_bytes: 4096,
        components: 32,
        retained_bytes: 16384,
        io_buffer_bytes: 1024,
    }
}

#[tokio::test]
/// Adapt donor selected_files.rs cumulative adds/reopen controls to native unstaged tracking.
/// # Panics
/// Panics when persistence, ordering, executable mode or working-byte invariants fail.
async fn cumulative_adds_survive_reopen_and_leave_working_bytes_live() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    std::fs::write(root.join("zeta"), b"first").unwrap();
    std::fs::write(root.join("alpha"), b"#!/bin/sh\n").unwrap();
    std::fs::set_permissions(root.join("alpha"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(root.join("neighbor"), b"untracked").unwrap();
    let mut owner = initialize(&path).await;
    let first = owner
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    let second = owner
        .add_files("work", &["zeta"], &first, limits(), tracking())
        .await
        .unwrap();
    let third = owner
        .add_files("work", &["alpha"], &second, limits(), tracking())
        .await
        .unwrap();
    assert_eq!(third.shared_head, first.shared_head);
    assert_eq!(third.generation, 0);
    assert_eq!(third.pending_version, 2);
    assert_eq!(std::fs::read(root.join("zeta")).unwrap(), b"first");
    drop(owner);
    // A later edit must not require re-add or invalidate admission on reopen.
    std::fs::write(root.join("zeta"), b"later unstaged edit").unwrap();
    let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    let pending = owner
        .pending_adds("work", limits(), tracking())
        .await
        .unwrap();
    assert_eq!(
        pending.iter().map(|p| p.path.as_str()).collect::<Vec<_>>(),
        vec!["alpha", "zeta"]
    );
    assert!(pending[0].executable);
    assert!(!pending[1].executable);
    let fourth = owner
        .add_files("work", &["zeta"], &third, limits(), tracking())
        .await
        .unwrap();
    assert_eq!(fourth, third);
    assert_eq!(
        owner
            .pending_adds("work", limits(), tracking())
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        std::fs::read(root.join("zeta")).unwrap(),
        b"later unstaged edit"
    );
    assert_eq!(std::fs::read(root.join("neighbor")).unwrap(), b"untracked");
}

#[tokio::test]
/// Adapt donor atomic batch/path/budget controls; a later invalid leaf saves no earlier leaf.
/// # Panics
/// Panics if unsupported leaves, aliases or byte exhaustion publish partial tracking state.
async fn invalid_batch_and_aggregate_budget_preserve_exact_pending_state() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    std::fs::write(root.join("one"), b"1234").unwrap();
    std::fs::write(root.join("two"), b"5678").unwrap();
    std::fs::create_dir(root.join("directory")).unwrap();
    symlink("one", root.join("link")).unwrap();
    let mut owner = initialize(&path).await;
    let before = owner
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    for invalid in [
        "missing",
        "directory",
        "link",
        "../one",
        "/one",
        ".gossil/locator",
    ] {
        assert!(
            owner
                .add_files("work", &["one", invalid], &before, limits(), tracking())
                .await
                .is_err()
        );
        drop(owner);
        owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
        assert_eq!(owner.checkout_view("work", limits()).await.unwrap(), before);
        assert!(
            owner
                .pending_adds("work", limits(), tracking())
                .await
                .unwrap()
                .is_empty()
        );
    }
    assert!(
        owner
            .add_files("work", &["one", "one"], &before, limits(), tracking())
            .await
            .is_err()
    );
    // The read ceiling bounds one file: each file has 4 bytes.
    let mut small = tracking();
    small.retained_bytes = 3;
    assert!(
        owner
            .add_files("work", &["one", "two"], &before, limits(), small)
            .await
            .is_err()
    );
    drop(owner);
    let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    assert!(
        owner
            .pending_adds("work", limits(), tracking())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(owner.checkout_view("work", limits()).await.unwrap(), before);
    assert_eq!(std::fs::read(root.join("one")).unwrap(), b"1234");
    assert_eq!(std::fs::read(root.join("two")).unwrap(), b"5678");
    // The ceiling bounds each file, not the sum: two files of 4 bytes pass under 4.
    let mut each = tracking();
    each.retained_bytes = 4;
    let after = owner
        .add_files("work", &["one", "two"], &before, limits(), each)
        .await
        .unwrap();
    assert_eq!(after.pending_version, before.pending_version + 1);
}

#[tokio::test]
/// Refuse stale expectations and selected repository hardlinks without publishing new rows.
/// # Panics
/// Panics if stale snapshots or protected inode aliases are accepted or files change.
async fn stale_snapshot_and_repository_inode_alias_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let mut owner = initialize(&path).await;
    let before = owner
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    std::fs::write(root.join("first"), b"one").unwrap();
    std::fs::write(root.join("second"), b"two").unwrap();
    let after = owner
        .add_files("work", &["first"], &before, limits(), tracking())
        .await
        .unwrap();
    assert!(
        owner
            .add_files("work", &["first"], &before, limits(), tracking())
            .await
            .is_err()
    );
    assert!(
        owner
            .add_files("work", &["second"], &before, limits(), tracking())
            .await
            .is_err()
    );
    drop(owner);
    let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    std::fs::hard_link(&path, root.join("database-alias")).unwrap();
    assert!(
        owner
            .add_files("work", &["database-alias"], &after, limits(), tracking())
            .await
            .is_err()
    );
    drop(owner);
    let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    assert_eq!(owner.checkout_view("work", limits()).await.unwrap(), after);
    assert_eq!(
        owner
            .pending_adds("work", limits(), tracking())
            .await
            .unwrap()
            .iter()
            .map(|p| p.path.as_str())
            .collect::<Vec<_>>(),
        vec!["first"]
    );
    assert_eq!(std::fs::read(root.join("second")).unwrap(), b"two");
}

/// Read canonical/native publication facts independently of local owner authority.
/// # Panics
/// Panics when the fixture connection or bounded scalar query cannot be read.
async fn publication_counts(path: &Path) -> Vec<libsql::Value> {
    let db = libsql::Builder::new_local(path).build().await.unwrap();
    let c = db.connect().unwrap();
    let mut rows=c.query("SELECT (SELECT count(*) FROM blob),(SELECT count(*) FROM event),(SELECT count(*) FROM plink),(SELECT count(*) FROM private),(SELECT count(*) FROM g_pending_path),(SELECT count(*) FROM g_path_scope)",()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    (0..row.column_count())
        .map(|i| row.get_value(i).unwrap())
        .collect()
}

#[tokio::test]
/// Addition publishes native tracking only, and the full proof refuses fabricated row state.
/// # Panics
/// Panics if add creates canonical history/staging or native scalar corruption is admitted.
async fn add_creates_no_checkin_or_stage_and_reopen_checks_native_fields() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let mut owner = initialize(&path).await;
    let before = owner
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    drop(owner);
    let counts = publication_counts(&path).await;
    let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    std::fs::write(root.join("first"), b"native bytes").unwrap();
    owner
        .add_files("work", &["first"], &before, limits(), tracking())
        .await
        .unwrap();
    drop(owner);
    assert_eq!(publication_counts(&path).await, counts);
    let db = libsql::Builder::new_local(&path).build().await.unwrap();
    let c = db.connect().unwrap();
    c.execute("UPDATE g_vfile SET chnged=1", ()).await.unwrap();
    drop(c);
    drop(db);
    // Open admits the changed flag, and the full proof refuses it.
    assert_eq!(
        proof::verify_refusal(&path, limits()).await,
        (true, "recording unsupported tracked state".to_owned())
    );
    // A repeated add of the same path reads only its own row and stays a no-op.
    let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    let view = owner.checkout_view("work", limits()).await.unwrap();
    let files = proof::file_bytes(&path);
    owner
        .add_files("work", &["first"], &view, limits(), tracking())
        .await
        .unwrap();
    drop(owner);
    assert!(proof::file_bytes(&path) == files);
    assert_eq!(std::fs::read(root.join("first")).unwrap(), b"native bytes");
}

#[tokio::test]
/// A checked repeated add is a no-op even at the SQL counter ceiling; new enrollment refuses overflow.
/// # Panics
/// Panics if a no-op changes native facts or an overflowing batch leaves partial rows.
async fn repeated_add_is_noop_and_new_paths_require_counter_capacity() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    let mut owner = initialize(&path).await;
    let before = owner
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    std::fs::write(root.join("first"), b"original").unwrap();
    std::fs::set_permissions(root.join("first"), std::fs::Permissions::from_mode(0o644)).unwrap();
    owner
        .add_files("work", &["first"], &before, limits(), tracking())
        .await
        .unwrap();
    drop(owner);
    let db = libsql::Builder::new_local(&path).build().await.unwrap();
    let c = db.connect().unwrap();
    c.execute(
        "UPDATE g_checkout SET pending_version=9223372036854775807",
        (),
    )
    .await
    .unwrap();
    drop(c);
    drop(db);
    let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    let ceiling = owner.checkout_view("work", limits()).await.unwrap();
    let pending = owner
        .pending_adds("work", limits(), tracking())
        .await
        .unwrap();
    std::fs::write(root.join("first"), b"later bytes").unwrap();
    std::fs::set_permissions(root.join("first"), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        owner
            .add_files("work", &["first"], &ceiling, limits(), tracking())
            .await
            .unwrap(),
        ceiling
    );
    assert_eq!(
        owner
            .pending_adds("work", limits(), tracking())
            .await
            .unwrap(),
        pending
    );
    std::fs::write(root.join("new"), b"new bytes").unwrap();
    assert!(
        owner
            .add_files("work", &["first", "new"], &ceiling, limits(), tracking())
            .await
            .is_err()
    );
    drop(owner);
    let mut owner = LocalOwnerRepository::open(&path, limits()).await.unwrap();
    assert_eq!(
        owner.checkout_view("work", limits()).await.unwrap(),
        ceiling
    );
    assert_eq!(
        owner
            .pending_adds("work", limits(), tracking())
            .await
            .unwrap(),
        pending
    );
    assert_eq!(std::fs::read(root.join("first")).unwrap(), b"later bytes");
    assert_eq!(std::fs::read(root.join("new")).unwrap(), b"new bytes");
}

#[tokio::test]
/// Opening through a stable symlink protects the resolved SQLite WAL namespace and inode aliases.
/// # Panics
/// Panics if resolved sidecars enroll, native/public facts change, or ordinary working bytes change.
async fn symlink_open_protects_actual_sidecars_and_their_hardlinks() {
    use std::os::unix::fs::{MetadataExt, symlink};
    let temp = tempfile::tempdir().unwrap();
    let root = checkout_root(temp.path());
    let actual = root.join("repo");
    let alias = temp.path().join("selected-alias");
    let mut owner = initialize(&actual).await;
    let before = owner
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    drop(owner);
    symlink(&actual, &alias).unwrap();
    // Keep an actual SQLite WAL connection alive so its sidecars are real and retained.
    let db = libsql::Builder::new_local(&actual).build().await.unwrap();
    let c = db.connect().unwrap();
    let mut mode = c.query("PRAGMA journal_mode=WAL", ()).await.unwrap();
    assert_eq!(
        mode.next().await.unwrap().unwrap().get_value(0).unwrap(),
        libsql::Value::Text("wal".into())
    );
    drop(mode);
    c.execute("UPDATE g_checkout SET pending_version=pending_version", ())
        .await
        .unwrap();
    for (source, link) in [
        ("repo", "database-hardlink"),
        ("repo-wal", "wal-hardlink"),
        ("repo-shm", "shm-hardlink"),
    ] {
        std::fs::hard_link(root.join(source), root.join(link)).unwrap();
        let a = std::fs::metadata(root.join(source)).unwrap();
        let b = std::fs::metadata(root.join(link)).unwrap();
        assert_eq!((a.dev(), a.ino()), (b.dev(), b.ino()));
    }
    std::fs::write(root.join("ordinary"), b"untouched ordinary bytes").unwrap();
    let publication = publication_counts(&actual).await;
    for reserved in [
        "repo",
        "database-hardlink",
        "repo-wal",
        "wal-hardlink",
        "repo-shm",
        "shm-hardlink",
    ] {
        let mut owner = LocalOwnerRepository::open(&alias, limits()).await.unwrap();
        assert_eq!(owner.checkout_view("work", limits()).await.unwrap(), before);
        let error = owner
            .add_files(
                "work",
                &["ordinary", reserved],
                &before,
                limits(),
                tracking(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.operation, "selected repository path is reserved");
        drop(owner);
        let mut owner = LocalOwnerRepository::open(&alias, limits()).await.unwrap();
        assert_eq!(owner.checkout_view("work", limits()).await.unwrap(), before);
        assert!(
            owner
                .pending_adds("work", limits(), tracking())
                .await
                .unwrap()
                .is_empty()
        );
        drop(owner);
        assert_eq!(publication_counts(&actual).await, publication);
        assert_eq!(
            std::fs::read(root.join("ordinary")).unwrap(),
            b"untouched ordinary bytes"
        );
    }
    // SQLite can change lock/WAL bytes during a refused transaction. Assert logical
    // preservation above, rather than claiming internal sidecar bytes are immutable.
    drop(c);
    drop(db);
}

/// The whole-command totals do not bound add.
///
/// A batch passes under totals of one byte.
/// # Panics
/// Panics if a total refuses the batch.
#[tokio::test]
async fn totals_do_not_bound_add() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    std::fs::write(root.join("a"), b"a").unwrap();
    std::fs::write(root.join("b"), b"b").unwrap();
    let mut owner = initialize(&path).await;
    let before = owner
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    let mut tiny = limits();
    tiny.content.max_total_stored_bytes = 1;
    tiny.content.max_total_decoded_bytes = 1;
    let after = owner
        .add_files("work", &["a", "b"], &before, tiny, tracking())
        .await
        .unwrap();
    assert_eq!(after.pending_version, before.pending_version + 1);
    // The reads of status ignore the totals too.
    assert_eq!(
        owner
            .pending_adds("work", tiny, tracking())
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        owner.tracked_paths("work", tiny, tracking()).await.unwrap(),
        ["a", "b"]
    );
    assert!(
        owner
            .path_scopes("work", tiny, tracking())
            .await
            .unwrap()
            .is_empty()
    );
}

/// Add refuses a path that differs from a tracked path only by case, or whose
/// folded form is a directory of one, before it reads a file; it accepts the exact
/// tracked path again.
/// # Panics
/// Panics if an alias passes, or the exact path is refused.
#[tokio::test]
async fn add_refuses_folded_aliases_of_tracked_paths() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let root = checkout_root(temp.path());
    std::fs::create_dir(root.join("a")).unwrap();
    std::fs::write(root.join("a/b"), b"ab").unwrap();
    let mut owner = initialize(&path).await;
    let before = owner
        .register_checkout("work", &root, limits())
        .await
        .unwrap();
    let view = owner
        .add_files("work", &["a/b"], &before, limits(), tracking())
        .await
        .unwrap();
    for alias in ["A", "A/B", "a/b/c"] {
        assert_eq!(
            owner
                .add_files("work", &[alias], &view, limits(), tracking())
                .await
                .unwrap_err()
                .operation,
            "aliased tracked path",
            "{alias}"
        );
    }
    assert_eq!(
        owner
            .add_files("work", &["a/b"], &view, limits(), tracking())
            .await
            .unwrap(),
        view
    );
}
