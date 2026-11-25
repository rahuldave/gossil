//! Measure how many small private tasks one repository takes under the CLI ceilings.
//!
//! The ceilings are those of `gos v` and `gos i`. Each write runs the full admission
//! twice, so the charge of one admission bounds how many issues a repository holds.
use gossil::gest_task_values::{TaskValueInputs, task_from_values};
use gossil::native_date::NativeDate;
use gossil::storage::{
    AdmissionLimits, InitializeOptions, IssueLimits, IssueTicket, Limits, LocalOwnerRepository,
    PathScope, PrivateIssueRequest, StorageError,
};
use serde_json::{Value as Json, json};
use std::path::Path;

/// Give the ceilings of one `gos i` command, as `vcs_open::limits` and `issue::limits` set them.
/// # Returns
/// The content, parser and SQL ceilings of the CLI.
fn cli_limits() -> IssueLimits {
    IssueLimits {
        admission: AdmissionLimits {
            content: Limits {
                max_stored_bytes: 1 << 20,
                max_output_bytes: 1 << 20,
                max_chain_depth: 128,
                max_total_stored_bytes: 64 << 20,
                max_total_decoded_bytes: 64 << 20,
            },
            parser: gossil::artifact::ParseLimits {
                input_bytes: 1 << 20,
                cards: 4096,
                owned_bytes: 8 << 20,
                entries: 4096,
            },
        },
        sql_rows: 100_000,
        sql_bytes: 64 << 20,
    }
}

/// Create a new catalog 6 repository under the CLI ceilings.
/// # Arguments
/// * `path` - The absent repository file.
/// # Returns
/// The admitted owner.
async fn repository(path: &Path) -> LocalOwnerRepository {
    let b = cli_limits().admission;
    LocalOwnerRepository::initialize(
        path,
        InitializeOptions {
            repository_id: "0123456789abcdef0123456789abcdef01234567",
            owner_id: "owner",
            date: "2026-11-20T00:00:00",
            user: b"Owner",
            password: "secret",
            sha1: false,
            content_limits: b.content,
            parse_limits: b.parser,
        },
    )
    .await
    .unwrap()
}

/// Spell a number as 32 Gest ID letters.
/// # Arguments
/// * `n` - The number.
/// # Returns
/// A valid Gest ID, unique for each number.
fn gest_id(n: u32) -> String {
    format!("{n:032x}")
        .bytes()
        .map(|b| match b {
            b'0'..=b'9' => char::from(b'k' + (b - b'0')),
            _ => char::from(b'u' + (b - b'a')),
        })
        .collect()
}

/// Emit the creation artifact of task `n`, with a 21-byte title.
/// # Arguments
/// * `n` - The number of the task.
/// # Returns
/// The artifact bytes.
fn task(n: u32) -> Vec<u8> {
    let task = task_from_values(TaskValueInputs {
        gest_id: json!(gest_id(n)),
        project_id: json!("klmnopqrstuvwxyzklmnopqrstuvwxyz"),
        title: json!(format!("Small task {n:>10}")),
        priority: Json::Null,
        status: json!("open"),
        description: json!(""),
        assigned_to: Json::Null,
        metadata: json!({}),
        resolved_at: Json::Null,
        created_at: json!("2026-11-18T12:00:00Z"),
        updated_at: json!("2026-11-18T12:00:00Z"),
    })
    .unwrap();
    let k = format!("{n:040x}");
    let date = NativeDate::parse(b"2026-11-18T12:00:00Z").unwrap();
    gossil::task_ticket::task_creation_bytes(&task, k.as_bytes(), b"gest agent", date, 65_536)
        .unwrap()
}

/// Give the CLI ceilings with a small decoded-byte ceiling, which the old charge met after a
/// few issues.
/// # Returns
/// The CLI ceilings with a decoded-byte ceiling of [`SMALL_DECODED`] and parser
/// ledgers wide enough that they do not bind first.
fn small_limits() -> IssueLimits {
    let mut limits = cli_limits();
    limits.admission.content.max_total_decoded_bytes = SMALL_DECODED;
    limits.admission.parser.cards = 1 << 20;
    limits.admission.parser.entries = 1 << 20;
    limits
}

/// The decoded-byte ceiling of [`small_limits`].
const SMALL_DECODED: u64 = 4 << 20;

/// Store task `n` through the public writer.
/// # Arguments
/// * `owner` - The admitted owner.
/// * `n` - The number of the task.
/// * `limits` - The ceilings of the call.
/// # Returns
/// The writer's result.
async fn create(
    owner: &mut LocalOwnerRepository,
    n: u32,
    limits: IssueLimits,
) -> Result<IssueTicket, StorageError> {
    owner
        .create_private_issue(
            PrivateIssueRequest {
                artifact: &task(n),
                scope: PathScope::Private,
            },
            limits,
        )
        .await
}

/// Read the bytes of every file in a directory, by name.
/// # Arguments
/// * `dir` - The directory.
/// # Returns
/// Each file name with its bytes, sorted by name.
fn files(dir: &Path) -> Vec<(std::ffi::OsString, Vec<u8>)> {
    let mut all: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name(), std::fs::read(entry.path()).unwrap())
        })
        .collect();
    all.sort();
    all
}

#[tokio::test]
/// Writes go on well past the count at which the old charge refused, and each task reads back.
///
/// Under [`small_limits`] the old charge, which took the whole repository text for each
/// ticket, stored 4 tasks and refused the fifth write. The linear charge stores 15. This
/// test writes 12, and an open, a list and a read of each task still fit the same ceilings.
async fn writes_continue_well_past_the_old_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repo.gossil");
    let mut owner = repository(&path).await;
    let mut stored = Vec::new();
    for n in 0..12 {
        stored.push(create(&mut owner, n, small_limits()).await.unwrap());
    }
    drop(owner);
    let mut owner = LocalOwnerRepository::open(&path, small_limits().admission)
        .await
        .unwrap();
    assert_eq!(
        owner.list_private_issues(small_limits()).await.unwrap(),
        stored
    );
    for ticket in &stored {
        let read = owner
            .read_private_issue(&ticket.ticket_id, small_limits())
            .await
            .unwrap();
        assert_eq!(&read, ticket);
    }
}

#[tokio::test]
/// The true ceiling still refuses a write cleanly, and the repository stays readable.
///
/// The test writes until the decoded-byte ceiling refuses a write. That refusal names the
/// ceiling, leaves every byte of the repository files unchanged, and leaves the
/// repository open to reads under the same ceilings.
async fn the_true_ceiling_refuses_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repo.gossil");
    let mut owner = repository(&path).await;
    let mut n = 0;
    let error = loop {
        assert!(n < 64, "no refusal within 64 tasks");
        drop(owner);
        let before = files(dir.path());
        owner = LocalOwnerRepository::open(&path, small_limits().admission)
            .await
            .unwrap();
        match create(&mut owner, n, small_limits()).await {
            Ok(_) => n += 1,
            Err(error) => {
                drop(owner);
                assert_eq!(files(dir.path()), before);
                break error;
            }
        }
    };
    assert_eq!(error.operation, "total decoded content limit");
    assert!(n >= 12, "the ceiling came at {n} tasks");
    let mut owner = LocalOwnerRepository::open(&path, small_limits().admission)
        .await
        .unwrap();
    assert_eq!(
        owner
            .list_private_issues(small_limits())
            .await
            .unwrap()
            .len(),
        n as usize
    );
}

/// Run `gos` in the fixture home, with the checkout root `root` as working directory.
/// # Arguments
/// * `home` - The fixture home.
/// * `args` - The arguments after the executable.
/// # Returns
/// Standard output, after the command succeeds.
/// # Panics
/// Panics if the command fails.
fn gos(home: &Path, args: &[&str]) -> String {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_gos"));
    for (key, _) in std::env::vars_os() {
        if key.to_str().is_some_and(|key| key.starts_with("GEST_")) {
            command.env_remove(key);
        }
    }
    let output = command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("XDG_DATA_HOME", home)
        .current_dir(home.join("root"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[tokio::test]
/// The `gos v` commands still work under the CLI ceilings in a repository with a dozen tasks.
async fn vcs_commands_work_beside_many_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let (repo, root) = (home.join("repo"), home.join("root"));
    std::fs::create_dir(&root).unwrap();
    let repo_arg = repo.to_str().unwrap();
    gos(home, &["v", "init", repo_arg]);
    let vcs = |verb: &[&str]| {
        let mut args = vec!["v", verb[0], "--repository", repo_arg, "--checkout", "work"];
        args.extend_from_slice(&verb[1..]);
        gos(home, &args)
    };
    vcs(&["open", "--root", root.to_str().unwrap()]);
    let mut owner = LocalOwnerRepository::open(&repo, cli_limits().admission)
        .await
        .unwrap();
    for n in 0..12 {
        create(&mut owner, n, cli_limits()).await.unwrap();
    }
    drop(owner);
    std::fs::write(root.join("file"), "content").unwrap();
    vcs(&["add", "--path", "file"]);
    vcs(&["scope", "--scope", "private", "--path", "file"]);
    vcs(&[
        "commit",
        "--scope",
        "private",
        "--all-tracked",
        "--user",
        "CLI Owner",
        "--comment",
        "beside tasks",
        "--date-override",
        "2099-01-01T00:00:00",
    ]);
    vcs(&["status"]);
    assert_eq!(vcs(&["timeline"]).lines().count(), 2);
    let mut owner = LocalOwnerRepository::open(&repo, cli_limits().admission)
        .await
        .unwrap();
    assert_eq!(
        owner.list_private_issues(cli_limits()).await.unwrap().len(),
        12
    );
}
