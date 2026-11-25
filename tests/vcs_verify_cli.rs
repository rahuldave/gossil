//! Check `vcs verify` through both executables and both group names.
//!
//! The cases are the summary of a whole repository, the proof of one issue by its
//! ID or a prefix, the statuses of an invalid, missing or ambiguous prefix, a usage
//! error, and a changed row that the proof refuses with every byte unchanged.
//!
//! ## Recommendations
//! Each case runs in its own home, so no locator or configuration leaks between cases.
use libsql::Value;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Output;

/// The four executable and group forms.
fn forms() -> [(&'static str, &'static str); 4] {
    [
        (env!("CARGO_BIN_EXE_gossil"), "vcs"),
        (env!("CARGO_BIN_EXE_gossil"), "v"),
        (env!("CARGO_BIN_EXE_gos"), "vcs"),
        (env!("CARGO_BIN_EXE_gos"), "v"),
    ]
}

/// Run one executable in the checkout root of a fixture home, with an isolated environment.
/// # Arguments
/// * `binary` - The executable.
/// * `home` - The fixture home, which holds the root `root`.
/// * `args` - The arguments after the executable.
/// # Returns
/// The finished process output.
fn run(binary: &str, home: &Path, args: &[&str]) -> Output {
    let mut command = std::process::Command::new(binary);
    for (key, _) in std::env::vars_os() {
        if key.to_str().is_some_and(|key| key.starts_with("GEST_")) {
            command.env_remove(key);
        }
    }
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("XDG_DATA_HOME", home)
        .env("GOSSIL_USER", "tester")
        .current_dir(home.join("root"))
        .args(args.iter().map(OsStr::new))
        .output()
        .unwrap()
}

/// Run `gos` and require success with an empty stderr.
/// # Arguments
/// * `home` - The fixture home.
/// * `args` - The arguments after the executable.
/// # Returns
/// Standard output.
fn ok(home: &Path, args: &[&str]) -> String {
    let out = run(env!("CARGO_BIN_EXE_gos"), home, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty(), "{args:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// Make a repository with one checkout, one project, `tasks` tasks and, when `commit` is true, one commit.
/// # Arguments
/// * `home` - An empty fixture home.
/// * `tasks` - The number of tasks to create.
/// * `commit` - True to record one private commit.
fn fixture(home: &Path, tasks: usize, commit: bool) {
    let root = home.join("root");
    std::fs::create_dir(&root).unwrap();
    let repo = home.join("repo");
    let repo = repo.to_str().unwrap();
    ok(home, &["v", "init", repo]);
    let root = root.canonicalize().unwrap();
    let select = ["--repository", repo, "--checkout", "work"];
    let with = |verb: &str, rest: &[&str]| {
        let mut args = vec!["v", verb];
        args.extend(select);
        args.extend(rest);
        ok(home, &args);
    };
    with("open", &["--root", root.to_str().unwrap()]);
    ok(home, &["i", "init"]);
    for n in 0..tasks {
        ok(home, &["i", "task", "create", &format!("Task {n}")]);
    }
    if !commit {
        return;
    }
    std::fs::write(root.join("file"), "content").unwrap();
    with("add", &["--path", "file"]);
    with("scope", &["--scope", "private", "--path", "file"]);
    with(
        "commit",
        &[
            "--scope",
            "private",
            "--all-tracked",
            "--user",
            "tester",
            "--comment",
            "one",
            "--date-override",
            "2099-01-01T00:00:01",
        ],
    );
}

/// Read rows from the repository with a plain connection.
/// # Arguments
/// * `path` - The repository file.
/// * `sql` - One query, or one statement when `write` is true.
/// * `write` - True to execute the statement instead.
/// # Returns
/// The rows of the query, or nothing for a statement.
fn sql(path: &Path, sql: &str, write: bool) -> Vec<Vec<Value>> {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async {
            let database = libsql::Builder::new_local(path).build().await.unwrap();
            let connection = database.connect().unwrap();
            if write {
                connection.execute_batch(sql).await.unwrap();
                return Vec::new();
            }
            let mut rows = connection.query(sql, ()).await.unwrap();
            let mut values = Vec::new();
            while let Some(row) = rows.next().await.unwrap() {
                values.push(
                    (0..row.column_count())
                        .map(|index| row.get_value(index).unwrap())
                        .collect(),
                );
            }
            values
        })
}

/// The Gest IDs of the tasks, sorted.
/// # Arguments
/// * `repo` - The repository file.
/// # Returns
/// Each ID.
fn task_ids(repo: &Path) -> Vec<String> {
    sql(repo, "SELECT id FROM g_gest_tasks ORDER BY id", false)
        .into_iter()
        .map(|row| match &row[0] {
            Value::Text(id) => id.clone(),
            other => panic!("{other:?}"),
        })
        .collect()
}

/// The ticket ID of one task or project.
/// # Arguments
/// * `repo` - The repository file.
/// * `id` - The Gest ID.
/// # Returns
/// The K card of its ticket.
fn ticket_of(repo: &Path, id: &str) -> String {
    let rows = sql(
        repo,
        &format!("SELECT tkt_uuid FROM ticket WHERE gest_id='\"{id}\"'"),
        false,
    );
    match &rows[0][0] {
        Value::Text(ticket) => ticket.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
/// In each form, `verify` prints the counts of check-ins, issues and comparisons and
/// exits 0, and `--issue` proves one task by its full ID and by a prefix. Neither
/// changes a byte.
fn verify_prints_a_summary_and_proves_one_issue() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    fixture(home, 3, true);
    let repo = home.join("repo");
    let repo_text = repo.to_str().unwrap();
    let before = std::fs::read(&repo).unwrap();
    let id = task_ids(&repo).remove(0);
    let ticket = ticket_of(&repo, &id);
    let project = match &sql(&repo, "SELECT id FROM g_gest_projects", false)[0][0] {
        Value::Text(id) => id.clone(),
        other => panic!("{other:?}"),
    };
    let project_ticket = ticket_of(&repo, &project);
    for (binary, group) in forms() {
        let out = run(binary, home, &[group, "verify", "--repository", repo_text]);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8(out.stdout).unwrap();
        assert_eq!(
            text,
            "verified: 1 check-ins, 4 issues, 15 tables compared\n"
        );
        for (named, id, ticket) in [
            (id.as_str(), &id, &ticket),
            (&id[..6], &id, &ticket),
            (&project[..6], &project, &project_ticket),
        ] {
            let out = run(
                binary,
                home,
                &[group, "verify", "--repository", repo_text, "--issue", named],
            );
            assert_eq!(
                out.status.code(),
                Some(0),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(
                String::from_utf8(out.stdout).unwrap(),
                format!("verified issue {id}: ticket {ticket}\n")
            );
        }
        let out = run(binary, home, &[group, "verify", "--help"]);
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            "Usage: gossil vcs verify --repository REPO [--issue ID]\n"
        );
    }
    assert_eq!(std::fs::read(&repo).unwrap(), before);
}

#[test]
/// An invalid prefix exits 64, a prefix that names no issue or more than one exits 66,
/// and a usage error exits 1. Each prints one line on stderr and changes no byte. The
/// 17 issues use 16 possible first letters, so two of them share one.
fn verify_refuses_bad_selections() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    fixture(home, 16, true);
    let repo = home.join("repo");
    let repo_text = repo.to_str().unwrap();
    let mut firsts: Vec<String> = sql(&repo, "SELECT substr(id,1,1) FROM g_gest_tasks UNION ALL SELECT substr(id,1,1) FROM g_gest_projects", false)
        .into_iter()
        .map(|row| match &row[0] {
            Value::Text(first) => first.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    firsts.sort();
    let shared = firsts.windows(2).find(|pair| pair[0] == pair[1]).unwrap()[0].clone();
    let count = firsts.iter().filter(|first| **first == shared).count();
    let ambiguous = format!("gossil: ambiguous id prefix '{shared}': matches {count} entities");
    let before = std::fs::read(&repo).unwrap();
    let gos = env!("CARGO_BIN_EXE_gos");
    for (args, status, start) in [
        (
            vec!["v", "verify", "--repository", repo_text, "--issue", "ABC"],
            64,
            "gossil: Id prefix must contain only characters k-z, found 'A'\n",
        ),
        (
            vec![
                "v",
                "verify",
                "--repository",
                repo_text,
                "--issue",
                "kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk",
            ],
            66,
            "gossil: not found: id prefix 'kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk'",
        ),
        (
            vec!["v", "verify", "--repository", repo_text, "--issue", &shared],
            66,
            &ambiguous,
        ),
        (vec!["v", "verify"], 1, "Usage: gossil vcs verify"),
        (
            vec!["v", "verify", "--repository", repo_text, "extra"],
            1,
            "Usage: gossil vcs verify",
        ),
    ] {
        let out = run(gos, home, &args);
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert_eq!(out.status.code(), Some(status), "{args:?}: {stderr}");
        assert!(stderr.starts_with(start), "{args:?}: {stderr}");
        assert_eq!(stderr.lines().count(), 1, "{stderr}");
        assert!(out.stdout.is_empty());
    }
    assert_eq!(std::fs::read(&repo).unwrap(), before);
}

#[test]
/// A changed typed row makes `verify` and `verify --issue` exit 74 with the fixed
/// message, and changes no byte. Open admits the row, so the proof itself refuses it.
fn verify_refuses_a_changed_row() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    fixture(home, 2, true);
    let repo = home.join("repo");
    let repo_text = repo.to_str().unwrap();
    let id = task_ids(&repo).remove(0);
    sql(
        &repo,
        &format!("UPDATE g_gest_tasks SET title='changed' WHERE id='{id}'"),
        true,
    );
    let before = std::fs::read(&repo).unwrap();
    for args in [
        vec!["v", "verify", "--repository", repo_text],
        vec!["v", "verify", "--repository", repo_text, "--issue", &id],
    ] {
        let out = run(env!("CARGO_BIN_EXE_gos"), home, &args);
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert_eq!(out.status.code(), Some(74), "{args:?}: {stderr}");
        assert!(
            stderr.contains("stored issue rows differ from the ticket artifact"),
            "{stderr}"
        );
        assert!(out.stdout.is_empty());
    }
    assert_eq!(std::fs::read(&repo).unwrap(), before);
}

#[test]
/// Without a commit, the summary counts no check-in. A repository at an older catalog
/// exits 78 with the three lines of the catalog refusal, and a failed write of the
/// output exits 1. None of them changes a byte.
fn verify_counts_no_check_in_and_refuses_an_older_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    fixture(home, 2, false);
    let repo = home.join("repo");
    let repo_text = repo.to_str().unwrap();
    let gos = env!("CARGO_BIN_EXE_gos");
    let out = run(gos, home, &["v", "verify", "--repository", repo_text]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "verified: 0 check-ins, 3 issues, 15 tables compared\n"
    );
    let mut command = std::process::Command::new(gos);
    command
        .env("HOME", home)
        .current_dir(home.join("root"))
        .args(["v", "verify", "--repository", repo_text]);
    let out = closed_stdout::configure(command).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.starts_with("gossil: unable to write the summary: "),
        "{stderr}"
    );
    let id = task_ids(&repo).remove(0);
    sql(
        &repo,
        "PRAGMA ignore_check_constraints=ON; UPDATE g_storage SET storage_version=4",
        true,
    );
    let before = std::fs::read(&repo).unwrap();
    for args in [
        vec!["v", "verify", "--repository", repo_text],
        vec!["v", "verify", "--repository", repo_text, "--issue", &id],
    ] {
        let out = run(gos, home, &args);
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert_eq!(out.status.code(), Some(78), "{args:?}: {stderr}");
        assert_eq!(stderr.lines().count(), 3, "{stderr}");
        assert!(
            stderr.contains("is at catalog 4, and this build needs catalog 6."),
            "{stderr}"
        );
        assert!(out.stdout.is_empty());
    }
    assert_eq!(std::fs::read(&repo).unwrap(), before);
}

#[test]
/// Open refuses each changed fact that it reads, before the work of the command.
/// `verify` and `status` each exit with their own status and print the same message,
/// and neither changes a byte. The cases are a wrong marker, a wrong fingerprint, a
/// missing config row, a missing owner row, a wrong checkout count, and lost Gossil
/// tables. The issue commands give lost tables exit 78 and the same message too.
fn open_refuses_each_fact_that_it_reads() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    fixture(home, 1, true);
    let repo = home.join("repo");
    let image = std::fs::read(&repo).unwrap();
    let tables = sql(
        &repo,
        "SELECT name FROM sqlite_schema WHERE type='table' AND name GLOB 'g_*'",
        false,
    );
    let mut lost = String::from("PRAGMA foreign_keys=OFF;");
    for row in tables {
        let Value::Text(name) = &row[0] else { panic!() };
        lost.push_str(&format!("DROP TABLE {name};"));
    }
    let lost_message = "this repository has lost its Gossil tables (was `fossil rebuild` run on it?); restore it from a backup";
    // Each case, with the exit status of `verify` and of `status`, and their message.
    let cases = [
        (
            "PRAGMA ignore_check_constraints=ON; UPDATE g_storage SET storage_version=7".to_owned(),
            74,
            1,
            "unsupported storage version",
        ),
        (
            "UPDATE g_vvar SET value='1/00000000000000000000000000000000' WHERE name='fingerprint'"
                .to_owned(),
            74,
            1,
            "recording fingerprint checksum",
        ),
        (
            "DELETE FROM config WHERE name='server-code'".to_owned(),
            74,
            1,
            "native server identity unavailable",
        ),
        (
            "DELETE FROM config WHERE name='project-code'".to_owned(),
            74,
            1,
            "unsupported or missing native repository markers",
        ),
        (
            "PRAGMA foreign_keys=OFF; DELETE FROM g_local_owner".to_owned(),
            74,
            1,
            "local repository binding unavailable",
        ),
        (
            "PRAGMA ignore_check_constraints=ON; UPDATE g_storage SET checkout_count=0".to_owned(),
            74,
            1,
            "unsupported populated catalog6 checkout scope",
        ),
        (lost, 78, 78, lost_message),
    ];
    let repo_text = repo.to_str().unwrap();
    for (case, verify, status, message) in cases {
        std::fs::write(&repo, &image).unwrap();
        sql(&repo, &case, true);
        let before = std::fs::read(&repo).unwrap();
        for (args, code) in [
            (vec!["v", "verify", "--repository", repo_text], verify),
            (
                vec![
                    "v",
                    "status",
                    "--repository",
                    repo_text,
                    "--checkout",
                    "work",
                ],
                status,
            ),
        ] {
            let out = run(env!("CARGO_BIN_EXE_gos"), home, &args);
            assert!(out.stdout.is_empty(), "{case}");
            assert_eq!(out.status.code(), Some(code), "{args:?} {case}");
            let stderr = String::from_utf8(out.stderr).unwrap();
            assert_eq!(stderr, format!("{message}\n"), "{args:?} {case}");
        }
        // The issue commands open the repository with the same checks, so they give
        // the lost tables the same exit status and message.
        if verify == 78 {
            for args in [&["i", "task", "list"][..], &["i", "task", "show", "a"]] {
                let out = run(env!("CARGO_BIN_EXE_gos"), home, args);
                assert!(out.stdout.is_empty(), "{args:?}");
                assert_eq!(out.status.code(), Some(78), "{args:?}");
                let stderr = String::from_utf8(out.stderr).unwrap();
                assert_eq!(stderr, format!("{message}\n"), "{args:?}");
            }
        }
        assert_eq!(std::fs::read(&repo).unwrap(), before, "{case}");
    }
}

#[path = "support/closed_stdout.rs"]
mod closed_stdout;
