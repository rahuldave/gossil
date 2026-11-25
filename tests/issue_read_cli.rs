//! Check `issue task list` and `issue task show` through both executables and both
//! group names.
//!
//! Each fixture runs `gos v init`, `gos v open` and `gos i init`, then stores tasks
//! through the library, which sets fixed timestamps and IDs and puts a task in another
//! project. The cases are the CLI-T04 subset of the two commands: the output of each
//! form, every CLI-05 status of a refusal, the help forms, and no change to any byte of
//! the repository.
//!
//! ## Recommendations
//! Each case runs in its own home, so no locator or configuration leaks between cases.
use libsql::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The refusal of a checkout without a bound project.
const NOT_BOUND: &str = "gossil: this checkout has no Gest project; run gos i init first\n";

/// The refusal outside an open checkout.
const NOT_OPEN: &str =
    "gossil: no open checkout holds this directory; run gos v init and gos v open first\n";

/// The project of the fixture tasks that the checkout is not bound to.
const OTHER: &str = "llllllllllllllllllllllllllllllll";

/// The four executable and group forms of CLI-01.
fn forms() -> [(&'static str, &'static str); 4] {
    [
        (env!("CARGO_BIN_EXE_gossil"), "issue"),
        (env!("CARGO_BIN_EXE_gossil"), "i"),
        (env!("CARGO_BIN_EXE_gos"), "issue"),
        (env!("CARGO_BIN_EXE_gos"), "i"),
    ]
}

/// Build one command of an executable in a fixture home, with an isolated environment.
/// # Arguments
/// * `binary` - The executable.
/// * `home` - The fixture home, which is also the configuration and data directory.
/// * `cwd` - The working directory.
/// * `args` - The arguments after the executable.
/// # Returns
/// The command, ready to run.
fn command(binary: &str, home: &Path, cwd: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(binary);
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

/// Run one executable in the checkout root of a fixture home.
/// # Arguments
/// * `binary` - The executable.
/// * `home` - The fixture home, which holds the root `root`.
/// * `args` - The arguments after the executable.
/// # Returns
/// The finished process output.
fn run(binary: &str, home: &Path, args: &[&str]) -> Output {
    command(binary, home, &home.join("root"), args)
        .output()
        .unwrap()
}

/// Run `gos` in the checkout root of a fixture home.
/// # Arguments
/// * `home` - The fixture home.
/// * `args` - The arguments after the executable.
/// # Returns
/// The finished process output.
fn gos(home: &Path, args: &[&str]) -> Output {
    run(env!("CARGO_BIN_EXE_gos"), home, args)
}

/// Require a successful run and return its stdout.
/// # Arguments
/// * `output` - The finished process output.
/// # Returns
/// Standard output, after a check that stderr is empty.
fn success(output: Output) -> String {
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout).unwrap()
}

/// Require a failed run with one status and one diagnostic, and no stdout.
/// # Arguments
/// * `output` - The finished process output.
/// * `status` - The expected exit status.
/// * `stderr` - The expected diagnostic.
fn failure(output: Output, status: i32, stderr: &str) {
    assert_eq!(
        (
            output.status.code(),
            String::from_utf8(output.stderr).unwrap().as_str()
        ),
        (Some(status), stderr)
    );
    assert!(output.stdout.is_empty());
}

/// Make a repository with `gos v init` and open the checkout `work` at `root`.
/// # Arguments
/// * `home` - An empty fixture home.
/// # Returns
/// The repository file.
fn open_checkout(home: &Path) -> PathBuf {
    let root = home.join("root");
    std::fs::create_dir(&root).unwrap();
    let repo = home.join("repo");
    success(gos(home, &["v", "init", repo.to_str().unwrap()]));
    let root = root.canonicalize().unwrap();
    success(gos(
        home,
        &[
            "v",
            "open",
            "--repository",
            repo.to_str().unwrap(),
            "--checkout",
            "work",
            "--root",
            root.to_str().unwrap(),
        ],
    ));
    repo
}

/// Read the text column of the rows of one query on a separate connection.
/// # Arguments
/// * `path` - The repository file.
/// * `sql` - A fixed query of one TEXT column.
/// # Returns
/// The text of each row.
fn texts(path: &Path, sql: &str) -> Vec<String> {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async {
            let database = libsql::Builder::new_local(path).build().await.unwrap();
            let connection = database.connect().unwrap();
            let mut rows = connection.query(sql, ()).await.unwrap();
            let mut values = Vec::new();
            while let Some(row) = rows.next().await.unwrap() {
                match row.get_value(0).unwrap() {
                    Value::Text(text) => values.push(text),
                    other => panic!("not text: {other:?}"),
                }
            }
            values
        })
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

/// One fixture task: the Gest ID, the project, the status and the creation second.
type Fixture = (&'static str, String, &'static str, u32);

/// Store tasks through the library, in the given order.
///
/// A task needs its project, so each project that the repository does not hold is
/// stored first.
/// # Arguments
/// * `repo` - The repository file, with no open owner.
/// * `tasks` - The tasks. Each title is `Task` and the last letter of the ID, and
///   each priority is 1.
fn store_tasks(repo: &Path, tasks: &[Fixture]) {
    use gossil::gest_project_values::{ProjectValueInputs, project_from_values};
    use gossil::gest_task_values::{TaskValueInputs, task_from_values};
    use gossil::storage::{LocalOwnerRepository, PathScope, PrivateIssueRequest};
    use serde_json::json;
    let limits = limits();
    let held = texts(repo, "SELECT id FROM g_gest_projects");
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async {
            let mut owner = LocalOwnerRepository::open(repo, limits.admission)
                .await
                .unwrap();
            let date = gossil::native_date::NativeDate::parse(b"2026-11-18T12:00:00Z").unwrap();
            let mut missing: Vec<&String> = tasks.iter().map(|task| &task.1).collect();
            missing.retain(|project| !held.contains(project));
            missing.dedup();
            for (n, project) in missing.into_iter().enumerate() {
                let project = project_from_values(ProjectValueInputs {
                    gest_id: json!(project),
                    archived_at: json!(null),
                    created_at: json!("2026-11-18T12:00:00Z"),
                    updated_at: json!("2026-11-18T12:00:00Z"),
                })
                .unwrap();
                let k = format!("{:040x}", 1000 + n);
                let artifact = gossil::gest_ticket::project_creation_bytes(
                    &project,
                    k.as_bytes(),
                    b"tester",
                    date,
                    65_536,
                )
                .unwrap();
                let request = PrivateIssueRequest {
                    artifact: &artifact,
                    scope: PathScope::Private,
                };
                owner.create_private_issue(request, limits).await.unwrap();
            }
            for (n, (id, project, status, second)) in tasks.iter().enumerate() {
                let created = format!("2026-11-18T12:00:{second:02}Z");
                let resolved = if matches!(*status, "done" | "cancelled") {
                    json!(created)
                } else {
                    json!(null)
                };
                let task = task_from_values(TaskValueInputs {
                    gest_id: json!(id),
                    project_id: json!(project),
                    title: json!(format!("Task {}", &id[31..])),
                    priority: json!(1),
                    status: json!(status),
                    description: json!("First line\nSecond line"),
                    assigned_to: json!(null),
                    metadata: json!({}),
                    resolved_at: resolved,
                    created_at: json!(created),
                    updated_at: json!(created),
                })
                .unwrap();
                let k = format!("{:040x}", n + 1);
                let artifact = gossil::task_ticket::task_creation_bytes(
                    &task,
                    k.as_bytes(),
                    b"tester",
                    date,
                    65_536,
                )
                .unwrap();
                let request = PrivateIssueRequest {
                    artifact: &artifact,
                    scope: PathScope::Private,
                };
                owner.create_private_issue(request, limits).await.unwrap();
            }
        });
}

/// Supply finite ceilings that every fixture fits.
/// # Returns
/// Generous content, parser and SQL ceilings.
fn limits() -> gossil::storage::IssueLimits {
    gossil::storage::IssueLimits {
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
    }
}

/// Build a full Gest ID from a prefix and a repeated letter.
/// # Arguments
/// * `prefix` - The first letters.
/// * `letter` - The letter that fills the rest.
/// # Returns
/// The 32-letter ID.
fn id(prefix: &str, letter: &str) -> &'static str {
    format!("{prefix}{}", letter.repeat(32 - prefix.len())).leak()
}

/// Build the fixture: an open checkout bound to a project, with four tasks.
///
/// The tasks of the bound project are `lmo` (open, second 1), `lmp` (done, second 2)
/// and `lqr` (in progress, second 3). `lqs` (open, second 4) belongs to [`OTHER`].
/// # Arguments
/// * `home` - An empty fixture home.
/// # Returns
/// The repository file and the Gest ID of the bound project.
fn fixture(home: &Path) -> (PathBuf, String) {
    let repo = open_checkout(home);
    success(gos(home, &["i", "init"]));
    let project = texts(&repo, "SELECT id FROM g_gest_projects").remove(0);
    store_tasks(
        &repo,
        &[
            (id("lmo", "m"), project.clone(), "open", 1),
            (id("lmp", "n"), project.clone(), "done", 2),
            (id("lqr", "o"), project.clone(), "in-progress", 3),
            (id("lqs", "p"), OTHER.into(), "open", 4),
        ],
    );
    (repo, project)
}

/// The text line of `task list` for one fixture task.
/// # Arguments
/// * `short` - The first 8 letters of the ID.
/// * `status` - The status as text prints it.
/// * `letter` - The letter of the title.
/// # Returns
/// The line with its newline.
fn line(short: &str, status: &str, letter: &str) -> String {
    format!("{short}  {status:<11}  P1    Task {letter}\n")
}

#[test]
/// `task list` prints only the open tasks of the bound project, newest first, under
/// all four forms and both names. `--all` adds the done task, `--status` filters as
/// Gest does, and `--json` parses back to the stored fields. No byte changes.
fn task_list_prints_the_tasks_of_the_bound_project() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let (repo, project) = fixture(home);
    let before = file_bytes(&repo);
    let open = line("lqrooooo", "in-progress", "o") + &line("lmommmmm", "open", "m");
    for (binary, group) in forms() {
        for args in [
            &["task", "list"][..],
            &["task", "ls"],
            &["t", "ls"],
            &["t", "list", "--no-color", "--no-pager"],
        ] {
            let args = [&[group][..], args].concat();
            assert_eq!(success(run(binary, home, &args)), open, "{binary} {args:?}");
        }
    }
    let all = line("lqrooooo", "in-progress", "o")
        + &line("lmpnnnnn", "done", "n")
        + &line("lmommmmm", "open", "m");
    for args in [&["--all"][..], &["-a"]] {
        let args = [&["i", "t", "ls"][..], args].concat();
        assert_eq!(success(gos(home, &args)), all);
    }
    for (args, expected) in [
        (&["--status", "open"][..], line("lmommmmm", "open", "m")),
        (&["-s", "done"], String::new()),
        (&["-a", "--status=done"], line("lmpnnnnn", "done", "n")),
        (
            &["-as", "in-progress"],
            line("lqrooooo", "in-progress", "o"),
        ),
    ] {
        let args = [&["i", "t", "ls"][..], args].concat();
        assert_eq!(success(gos(home, &args)), expected, "{args:?}");
    }
    for args in [&["--json"][..], &["-aj"]] {
        let all = args != ["--json"];
        let text = success(gos(home, &[&["i", "t", "ls"][..], args].concat()));
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        let ids: Vec<&str> = parsed
            .as_array()
            .unwrap()
            .iter()
            .map(|task| {
                assert_eq!(task["project_id"], project.as_str());
                assert_eq!(task["description"], "First line\nSecond line");
                assert_eq!(task["relationships"], serde_json::json!([]));
                assert_eq!(task["tags"], serde_json::json!([]));
                assert!(task.get("notes").is_none());
                task["id"].as_str().unwrap()
            })
            .collect();
        let expected: &[&str] = if all {
            &[id("lqr", "o"), id("lmp", "n"), id("lmo", "m")]
        } else {
            &[id("lqr", "o"), id("lmo", "m")]
        };
        assert_eq!(ids, expected);
        assert_eq!(parsed[0]["status"], "inprogress");
    }
    assert_eq!(file_bytes(&repo), before);
}

#[test]
/// `task show` finds a task by its full ID or a unique prefix, in any project, under
/// all four forms and both names, and refuses a missing, an ambiguous and an invalid
/// prefix with 66, 66 and 64. No byte changes.
fn task_show_finds_one_task() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let (repo, project) = fixture(home);
    let before = file_bytes(&repo);
    let full = id("lqr", "o");
    let text = success(gos(home, &["i", "t", "show", full]));
    let rows: Vec<&str> = text.lines().collect();
    assert_eq!(rows[0], format!("id           {full}"));
    assert_eq!(rows[1], "title        Task o");
    assert_eq!(rows[2], "status       in-progress");
    assert_eq!(rows[4], format!("project      {project}"));
    assert_eq!(rows[10], format!("ticket       {:040x}", 3));
    assert_eq!(
        &rows[12..],
        ["description", "    First line", "    Second line"]
    );
    for (binary, group) in forms() {
        for name in ["show", "view"] {
            assert_eq!(success(run(binary, home, &[group, "t", name, full])), text);
        }
    }
    for (prefix, found) in [
        ("lqr", full),
        ("lm", id("lmo", "m")),
        ("lmp", id("lmp", "n")),
        ("lqs", id("lqs", "p")),
    ] {
        let json = success(gos(home, &["i", "task", "show", "--json", prefix]));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["id"], found, "{prefix}");
        assert_eq!(parsed["notes"], serde_json::json!([]));
    }
    assert_eq!(
        success(gos(home, &["i", "t", "show", "-j", "lqs"])),
        success(gos(
            home,
            &["i", "t", "show", "lqs", "--json", "--no-color"]
        ))
    );
    for (prefix, status, message) in [
        ("lz", 66, "gossil: not found: id prefix 'lz'\n"),
        (
            "lq",
            66,
            "gossil: ambiguous id prefix 'lq': matches 2 entities\n",
        ),
        (
            "L",
            64,
            "gossil: Id prefix must contain only characters k-z, found 'L'\n",
        ),
    ] {
        failure(gos(home, &["i", "t", "show", prefix]), status, message);
    }
    assert_eq!(file_bytes(&repo), before);
}

#[test]
/// A parser failure exits 2 with the usage line, and each option of the Gest pin that
/// this build lacks exits 69, before the repository is read.
fn refused_arguments_exit_2_or_69() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let (repo, _) = fixture(home);
    let before = file_bytes(&repo);
    let list = "\nUsage: gossil issue task list [-a|--all] [-s|--status STATUS] [-j|--json]\n";
    let show = "\nUsage: gossil issue task show [-j|--json] ID\n";
    for (args, text) in [
        (
            &["ls", "extra"][..],
            format!("gossil: unexpected argument 'extra'{list}"),
        ),
        (
            &["ls", "--frob"],
            format!("gossil: unexpected argument '--frob'{list}"),
        ),
        (
            &["ls", "-s"],
            format!("gossil: a value is required for '--status'{list}"),
        ),
        (
            &["ls", "-a", "-a"],
            format!("gossil: the argument '--all' cannot be used more than once{list}"),
        ),
        (
            &["ls", "--limit", "x"],
            format!(
                "gossil: invalid value 'x' for '--limit <N>': invalid digit found in string{list}"
            ),
        ),
        (
            &["ls", "-aah"],
            format!("gossil: the argument '--all' cannot be used more than once{list}"),
        ),
        (
            &["show", "lm", "-jjh"],
            format!("gossil: the argument '--json' cannot be used more than once{show}"),
        ),
        (
            &["ls", "-zh"],
            format!("gossil: unexpected argument '-z'{list}"),
        ),
        (
            &["ls", "--", "-h"],
            format!("gossil: unexpected argument '-h'{list}"),
        ),
        (
            &["show"],
            format!("gossil: the required argument ID was not provided{show}"),
        ),
        (
            &["show", "lm", "lq"],
            format!("gossil: unexpected argument 'lq'{show}"),
        ),
    ] {
        failure(gos(home, &[&["i", "t"][..], args].concat()), 2, &text);
    }
    let invalid = gos(home, &["i", "t", "ls", "-sh"]);
    assert_eq!(invalid.status.code(), Some(2));
    let stderr = String::from_utf8(invalid.stderr).unwrap();
    assert!(
        stderr.starts_with("gossil: invalid value 'h' for '--status <STATUS>': "),
        "{stderr}"
    );
    assert!(stderr.ends_with(list), "{stderr}");
    for option in [
        &["--assigned-to", "x"][..],
        &["--limit", "5"],
        &["-t", "x"],
        &["--tag", "x"],
        &["-q"],
        &["--quiet"],
        &["-r"],
        &["--raw"],
        &["-v"],
        &["--verbose"],
    ] {
        let long = match option[0] {
            "-t" => "tag",
            "-q" => "quiet",
            "-r" => "raw",
            "-v" => "verbose",
            other => &other[2..],
        };
        failure(
            gos(home, &[&["i", "t", "ls"][..], option].concat()),
            69,
            &format!("gossil: issue task list option --{long} is not available in this build\n"),
        );
        if matches!(long, "quiet" | "raw" | "verbose") {
            failure(
                gos(home, &[&["i", "t", "show", "lm"][..], option].concat()),
                69,
                &format!(
                    "gossil: issue task show option --{long} is not available in this build\n"
                ),
            );
        }
    }
    assert_eq!(file_bytes(&repo), before);
}

#[test]
/// Before `gos i init`, both commands exit 78 and say to run it. Outside an open
/// checkout, they exit 78 and say to open one. Neither changes a byte.
fn an_unbound_or_missing_checkout_exits_78() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let repo = open_checkout(home);
    let before = file_bytes(&repo);
    let outside = home.join("outside");
    std::fs::create_dir(&outside).unwrap();
    for (binary, group) in forms() {
        for args in [&["task", "list"][..], &["t", "show", "lm"]] {
            let args = [&[group][..], args].concat();
            failure(run(binary, home, &args), 78, NOT_BOUND);
            let out = command(binary, home, &outside, &args).output().unwrap();
            failure(out, 78, NOT_OPEN);
        }
    }
    assert_eq!(file_bytes(&repo), before);
}

#[test]
/// `-h` or `--help` anywhere among the options, or a cluster of flags that reaches
/// `h`, prints the help of the command and exits 0, also before `gos i init` and
/// outside a checkout. Gest does the same after options that it accepts. Gest exits 2
/// when it rejects a letter before `h`, such as the second `a` in `-a -ah`. This build
/// still prints the help there, by the decided rule of the route.
fn help_anywhere_among_the_options_prints_the_help() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    std::fs::create_dir(home.join("root")).unwrap();
    for (command, cases) in [
        (
            "list",
            &[
                &["-h"][..],
                &["--help"],
                &["-a", "-h"],
                &["-ah"],
                &["-s", "open", "--help"],
                &["--no-color", "-sopen", "-jah"],
                &["-a", "-ah"],
            ][..],
        ),
        ("show", &[&["-h"][..], &["lm", "--help"], &["-jh", "lm"]]),
    ] {
        let expected = success(gos(home, &["i", "help", "task", command]));
        assert!(expected.starts_with(&format!("Usage: gossil issue task {command}")));
        assert!(expected.contains("Options:\n"), "{expected}");
        assert!(
            !expected.contains("not available in this build"),
            "{expected}"
        );
        for (binary, group) in forms() {
            for args in cases {
                let args = [&[group, "t", command][..], args].concat();
                assert_eq!(success(run(binary, home, &args)), expected, "{args:?}");
            }
        }
    }
    let init = success(gos(home, &["i", "init", "--help"]));
    assert_eq!(success(gos(home, &["i", "init", "--local", "-vh"])), init);
    assert_eq!(std::fs::read_dir(home.join("root")).unwrap().count(), 0);
}

#[test]
/// When stdout is closed, `task list` and `task show` exit 74 with one diagnostic
/// and change no byte.
fn a_closed_stdout_exits_74() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let (repo, _) = fixture(home);
    let before = file_bytes(&repo);
    for args in [&["i", "t", "ls"][..], &["i", "t", "show", "lm"]] {
        let command = command(env!("CARGO_BIN_EXE_gos"), home, &home.join("root"), args);
        let out = closed_stdout::configure(command).output().unwrap();
        assert_eq!(out.status.code(), Some(74), "{args:?}");
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert!(
            stderr.starts_with("gossil: cannot write the output: "),
            "{stderr}"
        );
        assert_eq!(stderr.lines().count(), 1, "{stderr}");
    }
    assert_eq!(file_bytes(&repo), before);
}

#[path = "support/closed_stdout.rs"]
mod closed_stdout;
