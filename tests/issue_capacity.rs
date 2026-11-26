//! Check that each command charges only what it touches, beside many tasks.
//!
//! The ceilings are those of `gos v` and `gos i`. No command reads the whole
//! repository, so no count of tasks makes the writes refuse. `task list` and
//! `gos v verify` read every task, and their cost grows with the count. The ordinary
//! test stores a few hundred tasks. The ignored test stores 5,000 and prints the time
//! of each command at 100, 1,000 and 5,000 tasks. `just issue-capacity` runs it in a
//! release build.
use gossil::gest_task_values::{TaskValueInputs, task_from_values};
use gossil::native_date::NativeDate;
use gossil::storage::{
    AdmissionLimits, IssueLimits, Limits, LocalOwnerRepository, PathScope, PrivateIssueRequest,
};
use serde_json::{Value as Json, json};
use std::path::Path;
use std::time::{Duration, Instant};

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

/// Emit the creation artifact of task `n` of a project, with a 21-byte title.
/// # Arguments
/// * `n` - The number of the task.
/// * `project` - The Gest ID of the project.
/// # Returns
/// The artifact bytes.
fn task(n: u32, project: &str) -> Vec<u8> {
    let task = task_from_values(TaskValueInputs {
        gest_id: json!(gest_id(n)),
        project_id: json!(project),
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
        .env("GOSSIL_USER", "tester")
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

/// Run one `vcs` verb on the fixture repository and checkout.
/// # Arguments
/// * `home` - The fixture home.
/// * `verb` - The verb and its own flags.
/// # Returns
/// Standard output, after the command succeeds.
fn vcs(home: &Path, verb: &[&str]) -> String {
    let repo = home.join("repo");
    let mut args = vec![
        "v",
        verb[0],
        "--repository",
        repo.to_str().unwrap(),
        "--checkout",
        "work",
    ];
    args.extend_from_slice(&verb[1..]);
    gos(home, &args)
}

/// Make a repository, open the checkout `work`, run `gos i init`, and track one file.
/// # Arguments
/// * `home` - An empty fixture home.
/// # Returns
/// The Gest ID of the bound project.
async fn fixture(home: &Path) -> String {
    let root = home.join("root");
    std::fs::create_dir(&root).unwrap();
    gos(home, &["v", "init", home.join("repo").to_str().unwrap()]);
    let root = root.canonicalize().unwrap();
    vcs(home, &["open", "--root", root.to_str().unwrap()]);
    gos(home, &["i", "init"]);
    std::fs::write(root.join("file"), "content").unwrap();
    vcs(home, &["add", "--path", "file"]);
    vcs(home, &["scope", "--scope", "private", "--path", "file"]);
    let mut owner = LocalOwnerRepository::open(home.join("repo"), cli_limits().admission)
        .await
        .unwrap();
    owner
        .bound_project("work", cli_limits())
        .await
        .unwrap()
        .unwrap()
}

/// Store tasks `from` to `to`, not included, through the public writer.
/// # Arguments
/// * `home` - The fixture home.
/// * `project` - The Gest ID of the project.
/// * `from` - The number of the first task.
/// * `to` - The number after the last task.
async fn create(home: &Path, project: &str, from: u32, to: u32) {
    let mut owner = LocalOwnerRepository::open(home.join("repo"), cli_limits().admission)
        .await
        .unwrap();
    for n in from..to {
        let request = PrivateIssueRequest {
            artifact: &task(n, project),
            scope: PathScope::Private,
        };
        owner
            .create_private_issue(request, cli_limits())
            .await
            .unwrap();
    }
}

/// Commit the tracked file with new content.
/// # Arguments
/// * `home` - The fixture home.
/// * `n` - The number of the commit, which gives the content and the date.
fn commit(home: &Path, n: u32) {
    std::fs::write(home.join("root").join("file"), format!("content {n}")).unwrap();
    vcs(
        home,
        &[
            "commit",
            "--scope",
            "private",
            "--all-tracked",
            "--user",
            "tester",
            "--comment",
            &format!("commit {n}"),
            "--date-override",
            &format!("2099-01-01T00:{:02}:{:02}", n / 60, n % 60),
        ],
    );
}

/// The times of the commands at one count of tasks.
struct Times {
    /// `gos i task create`.
    create: Duration,
    /// `gos i task list --json`.
    list: Duration,
    /// `gos i task show`.
    show: Duration,
    /// `gos v status`.
    status: Duration,
    /// `gos v commit`.
    commit: Duration,
    /// `gos v verify`.
    verify: Duration,
}

/// Time one run of a command.
/// # Arguments
/// * `run` - Runs the command and gives its output.
/// # Returns
/// The time and the output.
fn timed(run: impl FnOnce() -> String) -> (Duration, String) {
    let start = Instant::now();
    let out = run();
    (start.elapsed(), out)
}

/// Run each command once at the current count, check its output, and time it.
/// # Arguments
/// * `home` - The fixture home.
/// * `tasks` - The count of tasks before `task create` runs.
/// * `commits` - The count of check-ins before `commit` runs.
/// # Returns
/// The time of each command.
fn run_each(home: &Path, tasks: u32, commits: u32) -> Times {
    let (create, out) = timed(|| gos(home, &["i", "task", "create", "Timed task"]));
    assert!(!out.is_empty());
    let (list, out) = timed(|| gos(home, &["i", "task", "list", "--json"]));
    let listed: Vec<Json> = serde_json::from_str(&out).unwrap();
    assert_eq!(listed.len(), tasks as usize + 1);
    let id = gest_id(tasks / 2);
    let (show, out) = timed(|| gos(home, &["i", "task", "show", &id, "--json"]));
    let shown: Json = serde_json::from_str(&out).unwrap();
    assert_eq!(shown["id"], json!(id));
    let (status, _) = timed(|| vcs(home, &["status"]));
    let (commit, _) = timed(|| {
        commit(home, commits);
        String::new()
    });
    let repo = home.join("repo");
    let (verify, out) = timed(|| {
        gos(
            home,
            &["v", "verify", "--repository", repo.to_str().unwrap()],
        )
    });
    let expected = format!(
        "verified: {} check-ins, {} issues, ",
        commits + 1,
        tasks + 2
    );
    assert!(out.starts_with(&expected), "{out}");
    Times {
        create,
        list,
        show,
        status,
        commit,
        verify,
    }
}

#[tokio::test]
/// With 300 tasks, each command runs under the CLI ceilings, and its output is right.
async fn three_hundred_tasks_fit_the_cli_ceilings() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let project = fixture(home).await;
    create(home, &project, 0, 300).await;
    run_each(home, 300, 0);
    assert_eq!(vcs(home, &["timeline"]).lines().count(), 2);
}

#[tokio::test]
#[ignore = "stores 5,000 tasks; run it with just issue-capacity"]
/// With 5,000 tasks, each command still runs under the CLI ceilings. The test prints
/// the time of each command at 100, 1,000 and 5,000 tasks, and the load of the machine.
async fn five_thousand_tasks_fit_the_cli_ceilings() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let project = fixture(home).await;
    let load = std::process::Command::new("uptime").output().unwrap();
    println!(
        "load before: {}",
        String::from_utf8_lossy(&load.stdout).trim()
    );
    println!("tasks  create  list    show    status  commit  verify  (ms)");
    let (mut stored, mut commits) = (0, 0);
    for count in [100, 1_000, 5_000] {
        create(home, &project, stored, count).await;
        let times = run_each(home, count + commits, commits);
        stored = count;
        commits += 1;
        let ms = |time: Duration| time.as_millis();
        println!(
            "{count:<6} {:<7} {:<7} {:<7} {:<7} {:<7} {}",
            ms(times.create),
            ms(times.list),
            ms(times.show),
            ms(times.status),
            ms(times.commit),
            ms(times.verify)
        );
    }
    let load = std::process::Command::new("uptime").output().unwrap();
    println!(
        "load after: {}",
        String::from_utf8_lossy(&load.stdout).trim()
    );
}
