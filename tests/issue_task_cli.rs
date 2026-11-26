//! Check `issue task create` through both executables, both group names and both
//! command names.
//!
//! The cases are the CLI-01 forms, the CLI-T03 subset of the arguments, the GEST-T02
//! subset of stored values, the CLI-05 statuses of the refusals with every byte
//! unchanged, `vcs` commands beside tasks, the read-back through `task list` and
//! `task show`, a closed stdout, two commands at once and reads that retry after
//! another connection held the write lock.
//! The unit tests of `src/product/issue_task.rs` inject a failure after each write
//! step (GEST-T08).
//!
//! ## Recommendations
//! Each case runs in its own home, so no locator or configuration leaks between cases.
use gossil::storage::{IssueRecord, LocalOwnerRepository};
use libsql::Value;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Output;

#[path = "support/closed_stdout.rs"]
mod closed_stdout;

/// The four executable and group forms of CLI-01.
fn forms() -> [(&'static str, &'static str); 4] {
    [
        (env!("CARGO_BIN_EXE_gossil"), "issue"),
        (env!("CARGO_BIN_EXE_gossil"), "i"),
        (env!("CARGO_BIN_EXE_gos"), "issue"),
        (env!("CARGO_BIN_EXE_gos"), "i"),
    ]
}

/// Build one executable command in a directory of a fixture home, with an isolated environment.
/// # Arguments
/// * `binary` - The executable.
/// * `home` - The fixture home, which is also the configuration and data directory.
/// * `cwd` - The working directory.
/// * `args` - The arguments after the executable.
/// # Returns
/// The command, ready to run.
fn command(binary: &str, home: &Path, cwd: &Path, args: &[&OsStr]) -> std::process::Command {
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
        .current_dir(cwd)
        .args(args);
    command
}

/// Run `gos` in the checkout root of a fixture home.
/// # Arguments
/// * `home` - The fixture home, which holds the root `root`.
/// * `args` - The arguments after the executable.
/// # Returns
/// The finished process output.
fn gos(home: &Path, args: &[&str]) -> Output {
    let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    command(env!("CARGO_BIN_EXE_gos"), home, &home.join("root"), &args)
        .output()
        .unwrap()
}

/// Run `gos` and require success.
/// # Arguments
/// * `home` - The fixture home.
/// * `args` - The arguments after the executable.
/// # Returns
/// Standard output.
fn ok(home: &Path, args: &[&str]) -> String {
    let out = gos(home, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty(), "{args:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// Run one `vcs` verb on the fixture repository and checkout, and require success.
/// # Arguments
/// * `home` - The fixture home.
/// * `verb` - The verb and its own flags.
/// # Returns
/// Standard output.
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
    args.extend(&verb[1..]);
    ok(home, &args)
}

/// Make a repository with `gos v init` and register the checkout `work` at `root`.
/// # Arguments
/// * `home` - An empty fixture home.
/// # Returns
/// The canonical checkout root.
fn open_checkout(home: &Path) -> PathBuf {
    let root = home.join("root");
    std::fs::create_dir(&root).unwrap();
    ok(home, &["v", "init", home.join("repo").to_str().unwrap()]);
    let root = root.canonicalize().unwrap();
    vcs(home, &["open", "--root", root.to_str().unwrap()]);
    root
}

/// Open a checkout and run `gos i init` in it.
/// # Arguments
/// * `home` - An empty fixture home.
/// # Returns
/// The Gest ID of the bound project.
fn bound(home: &Path) -> String {
    open_checkout(home);
    ok(home, &["i", "init"]);
    text(&rows(&home.join("repo"), "SELECT id FROM g_gest_projects")[0][0]).to_owned()
}

/// Add, scope and commit one new file with the CLI.
/// # Arguments
/// * `home` - The fixture home.
/// * `n` - The number of the commit, which names the file and the date.
fn commit(home: &Path, n: usize) {
    let file = format!("file{n}");
    std::fs::write(home.join("root").join(&file), format!("content {n}")).unwrap();
    vcs(home, &["add", "--path", &file]);
    vcs(home, &["scope", "--scope", "private", "--path", &file]);
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
            &format!("2099-01-01T00:00:{n:02}"),
        ],
    );
}

/// Read the rows of one query on a separate connection.
/// # Arguments
/// * `path` - The repository file.
/// * `sql` - A fixed query.
/// # Returns
/// Every row.
fn rows(path: &Path, sql: &str) -> Vec<Vec<Value>> {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async {
            let database = libsql::Builder::new_local(path).build().await.unwrap();
            let connection = database.connect().unwrap();
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

/// The text of one value.
/// # Arguments
/// * `value` - A TEXT value.
/// # Returns
/// Its text.
fn text(value: &Value) -> &str {
    match value {
        Value::Text(text) => text,
        other => panic!("not text: {other:?}"),
    }
}

/// Read the bytes of a repository file and its write-ahead log.
/// # Arguments
/// * `path` - The repository file.
/// # Returns
/// The bytes of both files. An absent log reads as empty.
fn file_bytes(path: &Path) -> (Vec<u8>, Vec<u8>) {
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    (
        std::fs::read(path).unwrap(),
        std::fs::read(wal).unwrap_or_default(),
    )
}

/// Return the Gest IDs of every stored task, in key order.
/// # Arguments
/// * `repo` - The repository file.
/// # Returns
/// The 32-letter IDs.
fn task_ids(repo: &Path) -> Vec<String> {
    rows(repo, "SELECT id FROM g_gest_tasks ORDER BY id")
        .iter()
        .map(|row| text(&row[0]).to_owned())
        .collect()
}

/// Read one task back through the library reader, after the checks of open.
/// # Arguments
/// * `repo` - The repository file, with no open owner.
/// * `id` - The Gest ID of the task.
/// # Returns
/// The eleven fields of the task, with the canonical spellings of the reader.
fn read_back(repo: &Path, id: &str) -> gossil::gest_task_values::TaskValueInputs {
    let k = rows(
        repo,
        &format!("SELECT tkt_uuid FROM ticket WHERE gest_id='\"{id}\"'"),
    );
    let k = text(&k[0][0]).to_owned();
    let limits = gossil::storage::IssueLimits {
        admission: gossil::storage::AdmissionLimits {
            content: gossil::storage::Limits {
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
    };
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async {
            let mut owner = LocalOwnerRepository::open(repo, limits.admission)
                .await
                .unwrap();
            let ticket = owner.read_private_issue(&k, limits).await.unwrap();
            let IssueRecord::Task(task) = ticket.record else {
                panic!("not a task");
            };
            gossil::gest_task_values::task_to_values(&task)
        })
}

/// The success message of `task create`, as Gest prints it without color.
/// # Arguments
/// * `id` - The Gest ID of the task.
/// * `title` - The title.
/// # Returns
/// The expected standard output.
fn created(id: &str, title: &str) -> String {
    format!("  ✓  created task  {}\n\n  title  {title}\n", &id[..8])
}

#[test]
/// A task with a description of 120,000 bytes, over the old ceiling of 65,536 bytes
/// and under the 131,072 bytes that Linux allows for one argument, is created, shown
/// and listed.
fn a_task_of_120_kb_is_created_and_read_back() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    bound(home);
    let description = "word ".repeat(24_000);
    ok(home, &["i", "task", "create", "Large", "-d", &description]);
    let ids = task_ids(&home.join("repo"));
    assert_eq!(ids.len(), 1);
    let shown = ok(home, &["i", "task", "show", &ids[0][..8], "--json"]);
    assert!(shown.contains(&description), "{}", &shown[..200]);
    assert!(ok(home, &["i", "task", "list"]).contains("Large"));
}

#[test]
/// Tasks that `task create` stores, in each of the four forms, are listed by `task list`
/// and shown by `task show` in each form, in JSON, with the stored ID, title and priority.
fn created_tasks_read_back_through_list_and_show() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let project = bound(home);
    let repo = home.join("repo");
    let root = home.join("root");
    let run = |binary: &str, args: &[&str]| {
        let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        let out = command(binary, home, &root, &args).output().unwrap();
        assert_eq!(out.status.code(), Some(0), "{binary} {args:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    for (n, (binary, group)) in forms().into_iter().enumerate() {
        run(
            binary,
            &[group, "task", "create", &format!("Task {n}"), "-p", "1"],
        );
    }
    let ids = task_ids(&repo);
    assert_eq!(ids.len(), 4);
    for (binary, group) in forms() {
        let listed: serde_json::Value =
            serde_json::from_str(&run(binary, &[group, "task", "list", "--json"])).unwrap();
        let listed = listed.as_array().unwrap();
        let mut listed_ids: Vec<_> = listed
            .iter()
            .map(|task| task["id"].as_str().unwrap())
            .collect();
        listed_ids.sort_unstable();
        let mut stored_ids: Vec<_> = ids.iter().map(String::as_str).collect();
        stored_ids.sort_unstable();
        assert_eq!(listed_ids, stored_ids);
        for task in listed {
            let id = task["id"].as_str().unwrap();
            assert!(ids.contains(&id.to_owned()));
            assert_eq!(task["project_id"], serde_json::json!(project));
            assert_eq!(task["title"], read_back(&repo, id).title);
        }
        for id in &ids {
            let shown: serde_json::Value =
                serde_json::from_str(&run(binary, &[group, "task", "show", &id[..8], "--json"]))
                    .unwrap();
            let stored = read_back(&repo, id);
            assert_eq!(shown["id"], serde_json::json!(id));
            assert_eq!(shown["title"], stored.title);
            assert_eq!(shown["priority"], stored.priority);
            assert_eq!(shown["priority"], serde_json::json!(1));
        }
    }
}

#[test]
/// A `task list` and a `task show` that start while another connection holds the write
/// lock for 7 seconds both succeed. Each waits within the busy timeout of 15 seconds.
/// A `task create` beside a `task list` also both succeed.
fn reads_retry_after_a_held_write_lock() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    bound(home);
    let root = home.join("root");
    ok(home, &["i", "task", "create", "Kept"]);
    let id = task_ids(&home.join("repo")).remove(0);
    let spawn = |args: &[&str]| {
        let args: Vec<&OsStr> = args.iter().copied().map(OsStr::new).collect();
        command(env!("CARGO_BIN_EXE_gos"), home, &root, &args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let database = runtime
        .block_on(libsql::Builder::new_local(home.join("repo")).build())
        .unwrap();
    let holder = database.connect().unwrap();
    runtime
        .block_on(holder.execute("BEGIN IMMEDIATE", ()))
        .unwrap();
    let mut children = [
        spawn(&["i", "task", "list", "--json"]),
        spawn(&["i", "task", "show", &id[..8], "--json"]),
    ];
    std::thread::sleep(std::time::Duration::from_secs(7));
    // Both reads are still waiting for the lock.
    for child in &mut children {
        assert!(child.try_wait().unwrap().is_none());
    }
    runtime.block_on(holder.execute("ROLLBACK", ())).unwrap();
    drop(holder);
    for child in children {
        let out = child.wait_with_output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(json.to_string().contains(&id));
    }
    let children = [
        spawn(&["i", "task", "create", "Beside"]),
        spawn(&["i", "task", "list", "--json"]),
    ];
    for child in children {
        let out = child.wait_with_output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert_eq!(task_ids(&home.join("repo")).len(), 2);
}

#[test]
/// CLI-01: each of the four forms, with `create` and `new` and the alias `t`, creates
/// one private task in the bound project and prints Gest's success message.
fn each_form_creates_a_task() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let project = bound(home);
    let repo = home.join("repo");
    let root = home.join("root");
    let mut count = 0;
    for (binary, group) in forms() {
        for path in [
            ["task", "create"],
            ["task", "new"],
            ["t", "create"],
            ["t", "new"],
        ] {
            let before = task_ids(&repo);
            let title = format!("{group} {} {}", path[0], path[1]);
            let args = [group, path[0], path[1], &title].map(OsStr::new);
            let out = command(binary, home, &root, &args).output().unwrap();
            assert_eq!(out.status.code(), Some(0), "{binary} {args:?}");
            assert!(out.stderr.is_empty(), "{args:?}");
            let after = task_ids(&repo);
            let new: Vec<_> = after.iter().filter(|id| !before.contains(id)).collect();
            assert_eq!(new.len(), 1);
            assert_eq!(
                String::from_utf8(out.stdout).unwrap(),
                created(new[0], &title)
            );
            let stored = read_back(&repo, new[0]);
            assert_eq!(stored.title, serde_json::json!(title));
            assert_eq!(stored.project_id, serde_json::json!(project));
            count += 1;
        }
    }
    assert_eq!(task_ids(&repo).len(), count);
    assert_eq!(
        rows(&repo, "SELECT count(*) FROM private"),
        [vec![Value::Integer(count as i64 + 1)]]
    );
    assert_eq!(
        rows(
            &repo,
            "SELECT DISTINCT c.tkt_user, t.gossil_kind FROM ticket t JOIN ticketchng c USING(tkt_id) WHERE t.gossil_kind='task'"
        ),
        [vec![
            Value::Text("tester".into()),
            Value::Text("task".into())
        ]]
    );
}

#[test]
/// GEST-T02 subset and CLI-T03: each accepted option is stored, and the reader gives
/// the same values as the JSON output, in Gest's field order. `--quiet` prints the
/// short ID, `--json` wins over `--quiet`, and `-r`, `--no-color`, `--no-pager` and
/// `-v` change nothing.
fn each_option_is_stored_and_read_back() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let project = bound(home);
    let repo = home.join("repo");
    for (args, status, priority, metadata) in [
        (
            &["Plain", "--json"][..],
            "open",
            serde_json::Value::Null,
            serde_json::json!({}),
        ),
        (
            &[
                "Second",
                "-d",
                "Body\nmore",
                "-p",
                "high",
                "-s",
                "in_progress",
                "-m",
                "a.b=1",
                "-m",
                "c=true",
                "--metadata-json",
                "{\"d\":{\"e\":null}}",
                "-jq",
                "-r",
                "--no-color",
                "--no-pager",
                "-vv",
            ],
            "in-progress",
            serde_json::json!(1),
            serde_json::json!({"a": {"b": 1}, "c": true, "d": {"e": null}}),
        ),
        (
            &[
                "Third",
                "--priority=4",
                "--status=DONE",
                "--metadata=x=y",
                "-j",
            ],
            "done",
            serde_json::json!(4),
            serde_json::json!({"x": "y"}),
        ),
        (
            &["Fourth", "-p0", "-scancelled", "--json"],
            "cancelled",
            serde_json::json!(0),
            serde_json::json!({}),
        ),
    ] {
        let mut full = vec!["i", "task", "create"];
        full.extend_from_slice(args);
        let output = ok(home, &full);
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        let keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "assigned_to",
                "created_at",
                "description",
                "id",
                "metadata",
                "notes",
                "priority",
                "project_id",
                "relationships",
                "resolved_at",
                "status",
                "tags",
                "title",
                "updated_at"
            ],
            "serde_json sorts the keys of a parsed object"
        );
        let order: Vec<usize> = [
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
            "notes",
            "relationships",
            "tags",
        ]
        .iter()
        .map(|key| output.find(&format!("\n  \"{key}\":")).unwrap())
        .collect();
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{output}");
        let id = json["id"].as_str().unwrap();
        let stored = read_back(&repo, id);
        assert_eq!(stored.status, serde_json::json!(status));
        assert_eq!(json["status"], serde_json::json!(status.replace('-', "")));
        assert_eq!(stored.priority, priority);
        assert_eq!(json["priority"], priority);
        assert_eq!(stored.metadata, metadata);
        assert_eq!(json["metadata"], metadata);
        assert_eq!(stored.title, json["title"]);
        assert_eq!(stored.description, json["description"]);
        assert_eq!(stored.project_id, serde_json::json!(project));
        assert_eq!(json["project_id"], serde_json::json!(project));
        assert_eq!(json["assigned_to"], serde_json::Value::Null);
        assert_eq!(stored.assigned_to, serde_json::Value::Null);
        for key in ["created_at", "updated_at", "resolved_at"] {
            let shown = json[key]
                .as_str()
                .map(|text| chrono::DateTime::parse_from_rfc3339(text).unwrap());
            let kept = stamp(&stored, key)
                .as_str()
                .map(|text| chrono::DateTime::parse_from_rfc3339(text).unwrap());
            assert_eq!(shown, kept, "{key}");
        }
        assert_eq!(stored.created_at, stored.updated_at);
        assert_eq!(
            stored.resolved_at.is_null(),
            !matches!(status, "done" | "cancelled")
        );
        for key in ["notes", "relationships", "tags"] {
            assert_eq!(json[key], serde_json::json!([]), "{key}");
        }
    }
    let before = task_ids(&repo);
    let quiet = ok(home, &["i", "t", "new", "Quiet", "-q", "--no-color"]);
    let new: Vec<_> = task_ids(&repo)
        .into_iter()
        .filter(|id| !before.contains(id))
        .collect();
    assert_eq!(quiet, format!("{}\n", &new[0][..8]));
    let raw = ok(home, &["i", "t", "new", "Raw", "-r", "-v", "--no-pager"]);
    assert!(raw.starts_with("  ✓  created task  "), "{raw}");
    assert!(raw.ends_with("\n\n  title  Raw\n"), "{raw}");
}

/// Return one timestamp field of a task.
/// # Arguments
/// * `task` - The fields of the task.
/// * `key` - `created_at`, `updated_at` or `resolved_at`.
/// # Returns
/// The value of the field.
fn stamp<'a>(
    task: &'a gossil::gest_task_values::TaskValueInputs,
    key: &str,
) -> &'a serde_json::Value {
    match key {
        "created_at" => &task.created_at,
        "updated_at" => &task.updated_at,
        _ => &task.resolved_at,
    }
}

#[test]
/// CLI-05 and CLI-T03: usage errors give 2 before any refusal; refused options and a
/// missing title give 69; bad metadata and a `[` give 64; a checkout with no project
/// gives 78. None of them changes a byte of the repository.
fn refusals_keep_every_byte() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    bound(home);
    let repo = home.join("repo");
    let usage = "\nUsage: gossil issue task create [OPTIONS] [TITLE]\n";
    let unavailable =
        |what: &str| format!("gossil: issue task create: {what} is not available in this build\n");
    let cases: Vec<(Vec<&str>, i32, String)> = vec![
        (vec!["T", "--frob"], 2, format!("gossil: issue task create: unexpected argument \"--frob\"{usage}")),
        (vec!["T", "U"], 2, format!("gossil: issue task create: unexpected argument \"U\"{usage}")),
        (vec!["T", "-p", "-1"], 2, format!("gossil: issue task create: \"--priority\" needs a value{usage}")),
        (
            vec!["T", "-p", "256"],
            2,
            format!("gossil: issue task create: invalid value for \"--priority\": invalid priority \"256\" (accepted: 0-4, critical, high, medium, low, lowest){usage}"),
        ),
        (vec!["T", "-s", "closed"], 2, format!("gossil: issue task create: invalid value for \"--status\": invalid task status: closed{usage}")),
        (vec!["T", "-d", "a", "-d", "b"], 2, format!("gossil: issue task create: \"--description\" cannot be used more than once{usage}")),
        (vec!["T", "--assign", "me", "--frob"], 2, format!("gossil: issue task create: unexpected argument \"--frob\"{usage}")),
        (vec!["T", "--phase", "1"], 2, format!("gossil: issue task create: \"--phase\" needs \"--iteration\"{usage}")),
        (vec!["T", "--assign", "me"], 69, unavailable("--assign")),
        (vec!["T", "--batch"], 69, unavailable("--batch")),
        (vec!["T", "-i", "x", "--phase", "2"], 69, unavailable("--iteration")),
        (vec!["T", "-l", "blocks:x"], 69, unavailable("--link")),
        (vec!["T", "--tag", "a,b"], 69, unavailable("--tag")),
        (
            vec!["-d", "D"],
            69,
            "gossil: issue task create: reading the title from stdin is not available in this build; give TITLE\n".into(),
        ),
        (vec!["T", "-m", "bad"], 64, "gossil: issue task create: invalid --metadata pair (expected key=value): bad\n".into()),
        (vec!["T", "--metadata-json", "[1]"], 64, "gossil: issue task create: --metadata-json must be a JSON object\n".into()),
        (vec!["See [[x]]"], 64, "gossil: issue task create: the title contains \"[\", and this build refuses it, because Markdown backlinks are not ported; a JSON array in the metadata holds \"[\" too\n".into()),
        (vec!["T", "-d", "a [link]"], 64, "gossil: issue task create: the description contains \"[\", and this build refuses it, because Markdown backlinks are not ported; a JSON array in the metadata holds \"[\" too\n".into()),
        (vec!["T", "--metadata-json", "{\"a\":[1]}"], 64, "gossil: issue task create: the metadata contains \"[\", and this build refuses it, because Markdown backlinks are not ported; a JSON array in the metadata holds \"[\" too\n".into()),
    ];
    let files = file_bytes(&repo);
    for (args, status, message) in cases {
        let mut full = vec!["i", "task", "create"];
        full.extend(&args);
        let out = gos(home, &full);
        assert_eq!(out.status.code(), Some(status), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        assert_eq!(String::from_utf8(out.stderr).unwrap(), message, "{args:?}");
        assert_eq!(file_bytes(&repo), files, "{args:?}");
    }
    assert!(task_ids(&repo).is_empty());
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    open_checkout(home);
    let files = file_bytes(&home.join("repo"));
    let out = gos(home, &["i", "task", "create", "T"]);
    assert_eq!(out.status.code(), Some(78));
    assert_eq!(
        String::from_utf8(out.stderr).unwrap(),
        "gossil: this checkout has no Gest project; run gos i init first\n"
    );
    assert_eq!(file_bytes(&home.join("repo")), files);
    let empty = home.join("empty");
    std::fs::create_dir(&empty).unwrap();
    let out = command(
        env!("CARGO_BIN_EXE_gos"),
        home,
        &empty,
        &["i", "t", "new", "T"].map(OsStr::new),
    )
    .output()
    .unwrap();
    assert_eq!(out.status.code(), Some(78));
    assert_eq!(
        String::from_utf8(out.stderr).unwrap(),
        "gossil: no open checkout holds this directory; run gos v init and gos v open first\n"
    );
    assert_eq!(std::fs::read_dir(&empty).unwrap().count(), 0);
}

#[test]
/// A login that is not UTF-8 gives 78 and changes no byte.
fn a_login_that_is_not_utf8_gives_78() {
    use std::os::unix::ffi::OsStringExt;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    bound(home);
    let files = file_bytes(&home.join("repo"));
    let out = command(
        env!("CARGO_BIN_EXE_gos"),
        home,
        &home.join("root"),
        &["i", "task", "create", "T"].map(OsStr::new),
    )
    .env("GOSSIL_USER", OsString::from_vec(vec![0xff]))
    .output()
    .unwrap();
    assert_eq!(out.status.code(), Some(78));
    assert_eq!(
        String::from_utf8(out.stderr).unwrap(),
        "gossil: the local login is not UTF-8; set GOSSIL_USER\n"
    );
    assert_eq!(file_bytes(&home.join("repo")), files);
}

#[test]
/// Tasks can be created before and after a commit, in the root and below it, and every
/// `vcs` command and `issue init` still work after tasks exist.
fn vcs_commands_work_beside_tasks() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    bound(home);
    ok(home, &["i", "task", "create", "Before"]);
    commit(home, 1);
    let below = home.join("root/a/b");
    std::fs::create_dir_all(&below).unwrap();
    let out = command(
        env!("CARGO_BIN_EXE_gossil"),
        home,
        &below,
        &["issue", "task", "create", "Below"].map(OsStr::new),
    )
    .output()
    .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    commit(home, 2);
    ok(home, &["i", "task", "create", "After"]);
    let status = vcs(home, &["status"]);
    assert!(status.contains("R\tprivate\t\"file2\""), "{status}");
    assert_eq!(vcs(home, &["timeline"]).lines().count(), 3);
    assert_eq!(vcs(home, &["log"]).lines().count(), 3);
    vcs(home, &["info"]);
    vcs(home, &["open"]);
    assert!(ok(home, &["i", "init"]).contains("initialized project"));
    assert_eq!(task_ids(&home.join("repo")).len(), 3);
}

#[test]
/// `task list` and `task show` read back each task as `task create` wrote it: no
/// priority, a priority, metadata, a done status and the clock time of the write.
fn list_and_show_read_back_created_tasks() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let project = bound(home);
    let plain: serde_json::Value =
        serde_json::from_str(&ok(home, &["i", "task", "create", "Plain", "-j"])).unwrap();
    let done: serde_json::Value = serde_json::from_str(&ok(
        home,
        &[
            "i", "t", "new", "Done", "-d", "Body", "-p", "low", "-s", "done", "-m", "a.b=1",
            "--json",
        ],
    ))
    .unwrap();
    assert_eq!(plain["priority"], serde_json::Value::Null);
    assert_eq!(done["metadata"], serde_json::json!({"a": {"b": 1}}));
    assert_eq!(done["resolved_at"], done["created_at"]);
    let created = [&done, &plain];
    let fields = |task: &serde_json::Value| {
        let mut task = task.clone();
        task.as_object_mut().unwrap().remove("notes");
        task
    };
    let listed: Vec<serde_json::Value> =
        serde_json::from_str(&ok(home, &["i", "task", "list", "--all", "--json"])).unwrap();
    assert_eq!(listed, created.map(fields));
    let open: Vec<serde_json::Value> =
        serde_json::from_str(&ok(home, &["i", "task", "list", "--json"])).unwrap();
    assert_eq!(open, [fields(&plain)]);
    for task in created {
        let id = task["id"].as_str().unwrap();
        assert_eq!(task["project_id"], project.as_str());
        let shown: serde_json::Value =
            serde_json::from_str(&ok(home, &["i", "task", "show", id, "--json"])).unwrap();
        assert_eq!(&shown, task);
        let text = ok(home, &["i", "task", "show", &id[..8]]);
        assert!(text.starts_with(&format!("id           {id}\n")), "{text}");
        let created_at = task["created_at"].as_str().unwrap();
        assert!(
            text.contains(&format!("created at   {created_at}\n")),
            "{text}"
        );
    }
    assert_eq!(
        ok(home, &["i", "task", "list", "--all"]),
        format!(
            "{}  done         P3    Done\n{}  open         --    Plain\n",
            &done["id"].as_str().unwrap()[..8],
            &plain["id"].as_str().unwrap()[..8]
        )
    );
}

#[test]
/// When stdout is closed, `task create` keeps the committed task, says so on stderr
/// and exits 74, without a panic.
fn a_closed_stdout_keeps_the_task_and_exits_74() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    bound(home);
    let command = command(
        env!("CARGO_BIN_EXE_gos"),
        home,
        &home.join("root"),
        &["i", "task", "create", "Kept"].map(OsStr::new),
    );
    let out = closed_stdout::configure(command).output().unwrap();
    assert_eq!(out.status.code(), Some(74));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.starts_with("gossil: issue task create kept the task, and stdout failed: "),
        "{stderr}"
    );
    let ids = task_ids(&home.join("repo"));
    assert_eq!(ids.len(), 1);
    assert_eq!(
        read_back(&home.join("repo"), &ids[0]).title,
        serde_json::json!("Kept")
    );
}

#[test]
/// Two commands at once both succeed. A write that waited for the write lock past its
/// timeout runs once more.
fn two_commands_at_once_both_succeed() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    bound(home);
    let root = home.join("root");
    for round in 0..4 {
        let children: Vec<_> = (0..2)
            .map(|n| {
                let title = format!("Task {round} {n}");
                command(
                    env!("CARGO_BIN_EXE_gos"),
                    home,
                    &root,
                    &["i", "task", "create", &title].map(OsStr::new),
                )
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
            })
            .collect();
        for child in children {
            let out = child.wait_with_output().unwrap();
            assert_eq!(
                out.status.code(),
                Some(0),
                "{round}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
    assert_eq!(task_ids(&home.join("repo")).len(), 8);
}
