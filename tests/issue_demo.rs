//! Run the issue demo of `docs/issues.md` end to end on the real executables.
//!
//! The demo makes a repository and a checkout, records a file, makes the Gest project,
//! creates three tasks, lists and shows them, and records a second file after the tasks.
//! Every step runs `gos`, and no step calls the library. An ignored test, the oracle,
//! then checks the demo repository with stock Fossil 2.29 and runs `fossil rebuild` on
//! a copy of it.
//!
//! ## Recommendations
//! Run the oracle with `just issue-demo`, which names the pinned Fossil reference.
use serde_json::Value;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Output;

/// The eleven fields of the Gest task model, which `create`, `list` and `show` all print.
const FIELDS: [&str; 11] = [
    "assigned_to",
    "created_at",
    "description",
    "id",
    "metadata",
    "priority",
    "project_id",
    "resolved_at",
    "status",
    "title",
    "updated_at",
];

/// Run `gos` in a directory of a fixture home, with an isolated environment.
/// # Arguments
/// * `home` - The fixture home, which is also the configuration and data directory.
/// * `cwd` - The working directory.
/// * `args` - The arguments after the executable.
/// # Returns
/// The finished process output.
fn gos_in(home: &Path, cwd: &Path, args: &[&str]) -> Output {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_gos"));
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
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap()
}

/// Run `gos` in the checkout root, and require status 0 and an empty stderr.
/// # Arguments
/// * `home` - The fixture home, which holds the root `root`.
/// * `args` - The arguments after the executable.
/// # Returns
/// Standard output.
fn ok(home: &Path, args: &[&str]) -> String {
    let output = gos_in(home, &home.join("root"), args);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "{args:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// Run one `vcs` verb on the demo repository and checkout, and require success.
/// # Arguments
/// * `home` - The fixture home.
/// * `verb` - The verb and its own flags.
/// # Returns
/// Standard output.
fn vcs(home: &Path, verb: &[&str]) -> String {
    let repo = home.join("repo.gossil");
    let mut args = vec![
        "v",
        verb[0],
        "--repository",
        repo.to_str().unwrap(),
        "--checkout",
        "work",
    ];
    args.extend(&verb[1..]);
    ok(home, &args)
}

/// Write one file, then add it, give it the private scope and commit it.
/// # Arguments
/// * `home` - The fixture home.
/// * `file` - The file name in the checkout root.
/// * `comment` - The check-in comment.
/// * `date` - The check-in date.
/// * `recorded` - The files that earlier check-ins recorded.
/// # Returns
/// The full hash of the new check-in.
fn record(home: &Path, file: &str, comment: &str, date: &str, recorded: &[&str]) -> String {
    std::fs::write(home.join("root").join(file), format!("{comment}\n")).unwrap();
    let added = vcs(home, &["add", "--path", file]);
    assert!(
        added.starts_with("Tracked paths (A: pending addition, R: recorded):\n"),
        "{added}"
    );
    assert!(
        added.contains(&format!("A\tunselected\t\"{file}\"\n")),
        "{added}"
    );
    for earlier in recorded {
        assert!(
            added.contains(&format!("R\tprivate\t\"{earlier}\"\n")),
            "{added}"
        );
    }
    assert_eq!(added.lines().count(), 2 + recorded.len(), "{added}");
    assert_eq!(
        vcs(home, &["scope", "--scope", "private", "--path", file]),
        "Selected scopes: 1\n"
    );
    early_commit_is_refused(home);
    let committed = vcs(
        home,
        &[
            "commit",
            "--scope",
            "private",
            "--all-tracked",
            "--user",
            "tester",
            "--comment",
            comment,
            "--date-override",
            date,
        ],
    );
    let hash = committed
        .strip_prefix("Recorded private check-in: ")
        .and_then(|rest| rest.strip_suffix('\n'))
        .unwrap_or_else(|| panic!("{committed}"));
    assert_eq!(hash.len(), 64, "{committed}");
    hash.to_owned()
}

/// Require that `vcs commit` refuses a check-in dated before the empty root check-in.
/// # Arguments
/// * `home` - The fixture home, with one pending private file.
fn early_commit_is_refused(home: &Path) {
    let repo = home.join("repo.gossil");
    let output = gos_in(
        home,
        &home.join("root"),
        &[
            "v",
            "commit",
            "--repository",
            repo.to_str().unwrap(),
            "--checkout",
            "work",
            "--scope",
            "private",
            "--all-tracked",
            "--user",
            "tester",
            "--comment",
            "Too early",
            "--date-override",
            "2000-01-01T00:00:00",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "private manifest date must follow all parents\n"
    );
}

/// Parse a JSON output.
/// # Arguments
/// * `text` - The standard output of a `--json` command.
/// # Returns
/// The parsed value.
fn json(text: &str) -> Value {
    assert!(text.ends_with("}\n") || text.ends_with("]\n"), "{text}");
    serde_json::from_str(text).unwrap()
}

/// Give the eleven model fields of a task object.
/// # Arguments
/// * `task` - A task object of `create`, `list` or `show`.
/// # Returns
/// The fields, in the order of `FIELDS`.
/// # Panics
/// Panics when the task is not an object or lacks one of the fields.
fn model(task: &Value) -> Vec<Value> {
    let object = task.as_object().unwrap_or_else(|| panic!("{task}"));
    FIELDS
        .iter()
        .map(|field| {
            object
                .get(*field)
                .unwrap_or_else(|| panic!("no {field}: {task}"))
                .clone()
        })
        .collect()
}

/// Read the short ID of a text `task create` output.
/// # Arguments
/// * `out` - The standard output.
/// * `title` - The task title.
/// # Returns
/// The 8-letter short ID.
fn short_id(out: &str, title: &str) -> String {
    let id = out
        .strip_prefix("  ✓  created task  ")
        .and_then(|rest| rest.split_once('\n'))
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("{out}"));
    assert_eq!(
        out,
        format!("  ✓  created task  {id}\n\n  title  {title}\n")
    );
    assert_eq!(id.len(), 8);
    id.to_owned()
}

/// Give the shortest prefix of one ID that no other ID starts with.
/// # Arguments
/// * `id` - The full ID.
/// * `others` - The other full IDs.
/// # Returns
/// The prefix.
fn unique_prefix<'a>(id: &'a str, others: &[&str]) -> &'a str {
    (1..=id.len())
        .map(|n| &id[..n])
        .find(|prefix| others.iter().all(|other| !other.starts_with(prefix)))
        .unwrap()
}

/// What the demo made, for the stock Fossil oracle.
struct Demo {
    /// The fixture home, which holds the repository and the root.
    home: tempfile::TempDir,
    /// The full hashes of the two check-ins, newest first.
    checkins: [String; 2],
    /// The three tasks, as `task list --all --json` prints them, newest first.
    tasks: Vec<Value>,
    /// The K card of each task, from `task show`, in the order of `tasks`.
    tickets: Vec<String>,
}

/// Run the demo of `docs/issues.md`, and check the output of each step.
/// # Returns
/// What the demo made.
fn demo() -> Demo {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let root = home.join("root");
    std::fs::create_dir(&root).unwrap();
    let repo = home.join("repo.gossil");

    let init = ok(home, &["v", "init", repo.to_str().unwrap()]);
    assert!(init.starts_with("project-id: "), "{init}");
    let opened = vcs(home, &["open", "--root", root.to_str().unwrap()]);
    let root = root.canonicalize().unwrap();
    assert!(
        opened.starts_with(&format!("Opened checkout work at {}", root.display())),
        "{opened}"
    );
    let first = record(
        home,
        "README.txt",
        "Add the readme",
        "2099-01-01T00:00:01",
        &[],
    );

    let made = ok(home, &["i", "init"]);
    let project = made
        .strip_prefix("  ✓  initialized project  ")
        .and_then(|rest| rest.split_once('\n'))
        .map(|(id, _)| id.to_owned())
        .unwrap_or_else(|| panic!("{made}"));
    assert_eq!(project.len(), 8, "{made}");
    assert_eq!(
        made,
        format!(
            "  ✓  initialized project  {project}\n\n  root  {}\n",
            root.display()
        )
    );

    let guide = short_id(
        &ok(home, &["i", "task", "create", "Write the guide"]),
        "Write the guide",
    );
    let parser = short_id(
        &ok(
            home,
            &[
                "i",
                "task",
                "create",
                "Fix the parser",
                "--description",
                "The parser drops the last line.",
                "--priority",
                "high",
                "--status",
                "done",
            ],
        ),
        "Fix the parser",
    );
    let created = json(&ok(
        home,
        &["i", "task", "create", "Check stock Fossil", "--json"],
    ));
    assert_eq!(created["title"], "Check stock Fossil");
    assert_eq!(created["status"], "open");
    assert_eq!(created["priority"], Value::Null);
    assert_eq!(created["description"], "");
    assert!(
        created["project_id"]
            .as_str()
            .unwrap()
            .starts_with(&project)
    );
    for list in ["notes", "relationships", "tags"] {
        assert_eq!(created[list], serde_json::json!([]), "{list}");
    }
    assert_eq!(created["assigned_to"], Value::Null);
    assert_eq!(created["metadata"], serde_json::json!({}));
    assert_eq!(created["resolved_at"], Value::Null);
    assert_eq!(created["updated_at"], created["created_at"]);
    let check = created["id"].as_str().unwrap()[..8].to_owned();

    // The default list leaves out the done task; --all shows it.
    let open = ok(home, &["i", "task", "list"]);
    assert_eq!(
        open,
        format!(
            "{check}  open         --    Check stock Fossil\n\
             {guide}  open         --    Write the guide\n"
        )
    );
    let all = ok(home, &["i", "task", "list", "--all"]);
    assert_eq!(
        all,
        format!(
            "{check}  open         --    Check stock Fossil\n\
             {parser}  done         P1    Fix the parser\n\
             {guide}  open         --    Write the guide\n"
        )
    );
    let listed = json(&ok(home, &["i", "task", "list", "--json"]));
    let listed = listed.as_array().unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(model(&listed[0]), model(&created));
    // The list leaves out the empty notes, and its relationships and tags match those of `create`.
    let lists_like_create = |task: &Value| {
        assert!(task.get("notes").is_none(), "{task}");
        for list in ["relationships", "tags"] {
            assert_eq!(task[list], created[list], "{list}");
        }
    };
    listed.iter().for_each(lists_like_create);
    let tasks = json(&ok(home, &["i", "task", "list", "--all", "--json"]))
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(tasks.len(), 3);
    assert_eq!(model(&tasks[0]), model(&created));
    tasks.iter().for_each(lists_like_create);
    let ids: Vec<&str> = tasks.iter().map(|t| t["id"].as_str().unwrap()).collect();
    for (id, short) in ids.iter().zip([&check, &parser, &guide]) {
        assert!(id.starts_with(short.as_str()), "{id} {short}");
    }
    let done = &tasks[1];
    assert_eq!(done["status"], "done");
    assert_eq!(done["priority"], 1);
    assert_eq!(done["description"], "The parser drops the last line.");
    assert_eq!(done["resolved_at"], done["created_at"]);
    for task in &tasks {
        assert_eq!(task["project_id"], created["project_id"]);
    }

    // Show each task by its shortest unique prefix, in text and JSON.
    let mut tickets = Vec::new();
    for (n, task) in tasks.iter().enumerate() {
        let id = task["id"].as_str().unwrap();
        let others: Vec<&str> = ids.iter().copied().filter(|other| *other != id).collect();
        let prefix = unique_prefix(id, &others);
        let shown = json(&ok(home, &["i", "task", "show", prefix, "--json"]));
        assert_eq!(model(&shown), model(task), "{prefix}");
        assert_eq!(shown["notes"], serde_json::json!([]));
        for list in ["relationships", "tags"] {
            assert_eq!(shown[list], created[list], "{list}");
        }
        let text = ok(home, &["i", "task", "show", prefix]);
        let line = |label: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(label))
                .map(|rest| rest.trim_start().to_owned())
                .unwrap_or_else(|| panic!("{label}: {text}"))
        };
        assert!(text.starts_with(&format!("id           {id}\n")), "{text}");
        assert_eq!(line("title "), task["title"].as_str().unwrap());
        assert_eq!(line("project "), created["project_id"].as_str().unwrap());
        let ticket = line("ticket ");
        assert_eq!(ticket.len(), 40, "{text}");
        tickets.push(ticket);
        if n == 1 {
            assert_eq!(line("status "), "done");
            assert_eq!(line("priority "), "1");
            assert!(text.ends_with("description\n    The parser drops the last line.\n"));
        }
    }

    let second = record(
        home,
        "NOTES.txt",
        "Add the notes",
        "2099-01-01T00:00:02",
        &["README.txt"],
    );
    let timeline = vcs(home, &["timeline"]);
    let rows: Vec<Value> = timeline
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 3, "{timeline}");
    for (row, (hash, comment, date)) in rows.iter().zip([
        (&second, "Add the notes", "2099-01-01T00:00:02"),
        (&first, "Add the readme", "2099-01-01T00:00:01"),
    ]) {
        assert_eq!(row["hash"], hash.as_str());
        assert_eq!(row["date"], date);
        assert_eq!(row["comment"], comment);
        assert_eq!(row["actor"], "tester");
        assert_eq!(row["private"], true);
    }
    assert_eq!(rows[2]["comment"], "initial empty check-in");
    assert_eq!(rows[2]["private"], false);
    assert_eq!(
        vcs(home, &["status"]),
        "Tracked paths (A: pending addition, R: recorded):\n\
         R\tprivate\t\"NOTES.txt\"\n\
         R\tprivate\t\"README.txt\"\n"
    );
    // The tasks are unchanged after the check-in.
    assert_eq!(ok(home, &["i", "task", "list", "--all"]), all);

    Demo {
        home: temp,
        checkins: [second, first],
        tasks,
        tickets,
    }
}

/// The demo of `docs/issues.md` runs, and `create`, `list` and `show` agree on each task.
#[test]
fn the_issue_demo_runs_end_to_end() {
    demo();
}

/// Run the pinned stock Fossil, and require status 0.
/// # Arguments
/// * `fossil` - The reference executable.
/// * `home` - The fixture home, set as `HOME` and `FOSSIL_HOME`, so that Fossil reads no
///   user configuration.
/// * `args` - The arguments.
/// # Returns
/// Standard output.
fn fossil(fossil: &OsStr, home: &Path, args: &[&OsStr]) -> String {
    let output = std::process::Command::new("jlog")
        .arg("run")
        .arg(fossil)
        .args(args)
        .env("HOME", home)
        .env("FOSSIL_HOME", home)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fossil {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Stock Fossil reads the demo repository, and Gossil refuses a copy that `fossil
/// rebuild` changed, because the rebuild drops every `g_` table.
#[test]
#[ignore = "needs the pinned Fossil reference in GOSSIL_FOSSIL_REFERENCE; run just issue-demo"]
fn stock_fossil_reads_the_demo_repository() {
    let reference =
        std::env::var_os("GOSSIL_FOSSIL_REFERENCE").expect("explicit reference executable");
    let demo = demo();
    let home = demo.home.path();
    let repo = home.join("repo.gossil");
    let run = |args: &[&str], path: &PathBuf| {
        let mut all: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        all.extend([OsStr::new("-R"), path.as_os_str()]);
        fossil(&reference, home, &all)
    };

    let integrity = run(&["test-integrity", "--parse"], &repo);
    assert!(integrity.contains(" 0 errors\n"), "{integrity}");
    assert!(
        integrity.contains("0 control artifacts failed to parse\n"),
        "{integrity}"
    );
    assert!(integrity.contains("  3 manifests\n"), "{integrity}");
    assert!(integrity.contains("  4 tickets\n"), "{integrity}");

    // The timeline lists each check-in and one `New ticket` line for each task.
    // `*UNPUBLISHED*` marks each private line, and only those.
    let lists_the_demo = |timeline: &str| {
        let private_line = |needle: &str| {
            let line = timeline
                .lines()
                .find(|line| line.contains(needle))
                .unwrap_or_else(|| panic!("no {needle}: {timeline}"));
            assert!(line.contains(" *UNPUBLISHED* "), "{line}");
        };
        for hash in &demo.checkins {
            private_line(&format!("[{}]", &hash[..10]));
        }
        for (task, ticket) in demo.tasks.iter().zip(&demo.tickets) {
            let title = task["title"].as_str().unwrap();
            private_line(&format!("New ticket [{}] \"{title}\".", &ticket[..10]));
        }
        let root = timeline
            .lines()
            .find(|line| line.contains("initial empty check-in"))
            .unwrap_or_else(|| panic!("{timeline}"));
        assert!(!root.contains("UNPUBLISHED"), "{root}");
    };
    lists_the_demo(&run(&["timeline"], &repo));

    // `ticket show` prints a header line and one tab-separated row for each task.
    let shown = |path: &PathBuf| {
        let out = run(
            &[
                "ticket",
                "show",
                "0",
                "gossil_kind='task'",
                "--user",
                "tester",
            ],
            path,
        );
        let mut lines = out.lines();
        let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
        // The guide keeps these columns with `cut -f 2,6,8,13`.
        assert_eq!(
            [header[1], header[5], header[7], header[12]],
            ["tkt_uuid", "status", "priority", "title"]
        );
        let mut rows: Vec<std::collections::BTreeMap<String, String>> = lines
            .map(|line| {
                header
                    .iter()
                    .zip(line.split('\t'))
                    .map(|(k, v)| ((*k).to_owned(), v.to_owned()))
                    .collect()
            })
            .collect();
        rows.sort_by(|a, b| a["tkt_uuid"].cmp(&b["tkt_uuid"]));
        rows
    };
    let tickets = shown(&repo);
    assert_eq!(tickets.len(), 3);
    for (task, ticket) in demo.tasks.iter().zip(&demo.tickets) {
        let row = tickets
            .iter()
            .find(|row| row["tkt_uuid"] == *ticket)
            .unwrap_or_else(|| panic!("{ticket}: {tickets:?}"));
        let quoted = |field: &str| serde_json::to_string(&task[field]).unwrap();
        assert_eq!(row["gest_id"], quoted("id"));
        assert_eq!(row["title"], quoted("title"));
        assert_eq!(row["description"], quoted("description"));
        assert_eq!(row["priority"], quoted("priority"));
        assert_eq!(row["project_id"], quoted("project_id"));
        assert_eq!(row["metadata"], quoted("metadata"));
        assert_eq!(row["gossil_kind"], "task");
    }
    let statuses: Vec<&str> = demo
        .tickets
        .iter()
        .map(|ticket| {
            tickets
                .iter()
                .find(|row| row["tkt_uuid"] == *ticket)
                .unwrap()["status"]
                .as_str()
        })
        .collect();
    assert_eq!(statuses, ["\"open\"", "\"done\"", "\"open\""]);

    // `fossil rebuild` runs on a copy only. Stock Fossil still reads the copy.
    let copy = home.join("copy.gossil");
    std::fs::copy(&repo, &copy).unwrap();
    let before = std::fs::read(&repo).unwrap();
    fossil(&reference, home, &[OsStr::new("rebuild"), copy.as_os_str()]);
    assert_eq!(std::fs::read(&repo).unwrap(), before);
    let rebuilt = run(&["test-integrity", "--parse"], &copy);
    assert!(rebuilt.contains(" 0 errors\n"), "{rebuilt}");
    // The rebuild numbers the ticket rows again, so `tkt_id` can differ.
    let without_row_id = |rows: Vec<std::collections::BTreeMap<String, String>>| {
        rows.into_iter()
            .map(|mut row| {
                row.remove("tkt_id");
                row
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        without_row_id(shown(&copy)),
        without_row_id(tickets.clone())
    );
    assert!(rebuilt.contains("  3 manifests\n"), "{rebuilt}");
    assert!(rebuilt.contains("  4 tickets\n"), "{rebuilt}");
    lists_the_demo(&run(&["timeline"], &copy));
    let count = |path: &PathBuf| {
        run(
            &[
                "sql",
                "SELECT count(*) FROM sqlite_schema WHERE name GLOB 'g_*'",
            ],
            path,
        )
    };
    assert_ne!(count(&repo), "0\n");
    assert_eq!(count(&copy), "0\n");

    // Gossil refuses the rebuilt copy, writes no locator and changes no byte of it.
    let other = home.join("other");
    std::fs::create_dir(&other).unwrap();
    let copied = std::fs::read(&copy).unwrap();
    let refused = gos_in(
        home,
        &other,
        &[
            "v",
            "open",
            "--repository",
            copy.to_str().unwrap(),
            "--checkout",
            "work",
            "--root",
            other.to_str().unwrap(),
        ],
    );
    assert_eq!(refused.status.code(), Some(1));
    assert!(refused.stdout.is_empty());
    assert_eq!(
        String::from_utf8(refused.stderr).unwrap(),
        "SQLite failure: `no such table: g_storage`\n"
    );
    assert_eq!(std::fs::read_dir(&other).unwrap().count(), 0);
    assert_eq!(std::fs::read(&copy).unwrap(), copied);
    // The demo repository itself still works.
    assert_eq!(ok(home, &["i", "task", "list", "--all"]).lines().count(), 3);
}
