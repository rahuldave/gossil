//! Hold the unit controls of the `task list` and `task show` parsers, printers and
//! handlers. The handler controls build catalog 6 repositories through the library,
//! with tasks and a project before and after a private check-in.
use super::*;
use crate::gest_task_values::{TaskValueInputs, task_from_values};
use crate::native_date::NativeDate;
use crate::storage::{
    AdmissionLimits, CheckoutView, InitializeOptions, Limits, PathScope, PathScopeSelection,
    PrivateIssueRequest, PrivateRecordingOutcome, PrivateRecordingRequest, TrackingLimits,
};
use serde_json::{Value as Json, json};

/// Convert literal arguments into operating-system arguments.
/// # Arguments
/// * `args` - The arguments.
/// # Returns
/// The owned arguments.
fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

/// Build the inputs of one fixture task.
/// # Arguments
/// * `letter` - The letter that the Gest ID repeats after `prefix`.
/// * `prefix` - The first letters of the Gest ID.
/// * `status` - The status. A terminal status gets a resolution time.
/// * `created` - The creation time.
/// # Returns
/// Every field, with project `k...k`, title `Task <letter>` and priority 1.
fn inputs(letter: &str, prefix: &str, status: &str, created: &str) -> TaskValueInputs {
    let resolved = if matches!(status, "done" | "cancelled") {
        json!(created)
    } else {
        json!(null)
    };
    TaskValueInputs {
        gest_id: json!(format!("{prefix}{}", letter.repeat(32 - prefix.len()))),
        project_id: json!("k".repeat(32)),
        title: json!(format!("Task {letter}")),
        priority: json!(1),
        status: json!(status),
        description: json!(""),
        assigned_to: json!(null),
        metadata: json!({}),
        resolved_at: resolved,
        created_at: json!(created),
        updated_at: json!(created),
    }
}

/// Wrap task inputs in a ticket, as the list call returns it.
/// # Arguments
/// * `inputs` - The task fields.
/// # Returns
/// A ticket with a fixed K and native name.
fn ticket(inputs: TaskValueInputs) -> IssueTicket {
    IssueTicket {
        ticket_id: "a".repeat(40),
        native_name: "b".repeat(64),
        record: IssueRecord::Task(task_from_values(inputs).unwrap()),
    }
}

/// Build a fixture project record.
/// # Returns
/// The project `z...z`.
fn project() -> crate::gest_project_values::ProjectValues {
    crate::gest_project_values::project_from_values(
        crate::gest_project_values::ProjectValueInputs {
            gest_id: json!("z".repeat(32)),
            archived_at: json!(null),
            created_at: json!("2026-11-18T12:00:00Z"),
            updated_at: json!("2026-11-18T12:00:00Z"),
        },
    )
    .unwrap()
}

/// Give the Gest IDs of tasks, in order.
/// # Arguments
/// * `tasks` - The tasks.
/// # Returns
/// Each full ID.
fn ids(tasks: &[Task<'_>]) -> Vec<String> {
    tasks
        .iter()
        .map(|task| task.values.gest_id.to_string())
        .collect()
}

#[test]
/// `task list` accepts `--all`, `--status` and `--json` in every Clap spelling, and
/// the global `--no-color` and `--no-pager`.
fn the_list_parser_accepts_the_supported_options() {
    let all = |status, json| TaskListOptions {
        all: true,
        status,
        json,
    };
    for (args, expected) in [
        (&[][..], TaskListOptions::default()),
        (&["-a"], all(None, false)),
        (&["--all", "--json"], all(None, true)),
        (&["-aj"], all(None, true)),
        (&["-ajsdone"], all(Some(TaskStatus::Done), true)),
        (&["-as", "Done"], all(Some(TaskStatus::Done), false)),
        (&["-a", "-s=open"], all(Some(TaskStatus::Open), false)),
        (
            &["--all", "--status=in_progress"],
            all(Some(TaskStatus::InProgress), false),
        ),
        (
            &["--no-color", "-a", "--no-pager", "--status", "cancelled"],
            all(Some(TaskStatus::Cancelled), false),
        ),
    ] {
        assert_eq!(parse_task_list_args(&os(args)), Ok(expected), "{args:?}");
    }
}

#[test]
/// A parser failure exits with 2, and its message ends with the usage line. It comes
/// before the check for an unsupported option.
fn the_list_parser_refuses_malformed_arguments() {
    for (args, first) in [
        (&["--bogus"][..], "unexpected argument '--bogus'"),
        (&["-x"], "unexpected argument '-x'"),
        (&["-ax"], "unexpected argument '-x'"),
        (&["open"], "unexpected argument 'open'"),
        (&["-"], "unexpected argument '-'"),
        (&["--", "-a"], "unexpected argument '-a'"),
        (&["-s"], "a value is required for '--status'"),
        (&["--status"], "a value is required for '--status'"),
        (&["--tag"], "a value is required for '--tag'"),
        (&["--all=yes"], "unexpected value for '--all'"),
        (
            &["--all", "-a"],
            "the argument '--all' cannot be used more than once",
        ),
        (
            &["-jj"],
            "the argument '--json' cannot be used more than once",
        ),
        (
            &["-s", "open", "--status", "done"],
            "the argument '--status' cannot be used more than once",
        ),
        (&["--tag", "x", "--bogus"], "unexpected argument '--bogus'"),
        (
            &["--no-color", "--no-color"],
            "the argument '--no-color' cannot be used more than once",
        ),
        (
            &["-q", "-q"],
            "the argument '--quiet' cannot be used more than once",
        ),
        (
            &["--tag", "x", "-ty"],
            "the argument '--tag' cannot be used more than once",
        ),
        (&["--tag", "-q"], "a value is required for '--tag'"),
        (&["-s", "-a"], "a value is required for '--status'"),
        (&["-s=", "open"], "unexpected argument 'open'"),
        (
            &["-s="],
            "invalid value '' for '--status <STATUS>': invalid task status: ",
        ),
        (
            &["--status="],
            "invalid value '' for '--status <STATUS>': invalid task status: ",
        ),
        (
            &["--limit", "abc"],
            "invalid value 'abc' for '--limit <N>': invalid digit found in string",
        ),
        (
            &["--status", "nope"],
            "invalid value 'nope' for '--status <STATUS>': invalid task status: nope",
        ),
    ] {
        let error = parse_task_list_args(&os(args)).unwrap_err();
        let text = format!("{first}\n{LIST_USAGE}");
        assert_eq!(error, UsageError::Usage(text.clone()), "{args:?}");
        assert_eq!(error.exit_code(), 2);
        assert_eq!(error.to_string(), text);
    }
}

#[cfg(unix)]
#[test]
/// An argument or a value that is not UTF-8 is a parser failure.
fn the_parsers_refuse_text_that_is_not_utf8() {
    use std::os::unix::ffi::OsStringExt;
    let bad = OsString::from_vec(vec![0xff]);
    assert_eq!(
        parse_task_list_args(std::slice::from_ref(&bad)),
        Err(UsageError::Usage(format!(
            "an argument is not UTF-8\n{LIST_USAGE}"
        )))
    );
    assert_eq!(
        parse_task_list_args(&[OsString::from("-s"), bad.clone()]),
        Err(UsageError::Usage(format!(
            "the value of '--status' is not UTF-8\n{LIST_USAGE}"
        )))
    );
    assert_eq!(
        parse_task_show_args(&[bad]),
        Err(UsageError::Usage(format!(
            "an argument is not UTF-8\n{SHOW_USAGE}"
        )))
    );
}

#[test]
/// Every other option of `task list` at the Gest pin parses. The command then exits
/// with 69 and names the first one.
fn the_list_parser_refuses_unsupported_options() {
    for (args, name) in [
        (&["--assigned-to", "me"][..], "assigned-to"),
        (&["--limit=3"], "limit"),
        (&["-a", "-t", "x"], "tag"),
        (&["-tx", "-q"], "tag"),
        (&["--tag", "x"], "tag"),
        (&["-q"], "quiet"),
        (&["--raw", "--tag", "x"], "raw"),
        (&["-v"], "verbose"),
        (&["-vv", "-v"], "verbose"),
        (&["--assigned-to", "-"], "assigned-to"),
        (&["--verbose"], "verbose"),
    ] {
        let error = parse_task_list_args(&os(args)).unwrap_err();
        let text =
            format!("gossil: issue task list option --{name} is not available in this build");
        assert_eq!(error, UsageError::Unsupported(text), "{args:?}");
        assert_eq!(error.exit_code(), 69);
    }
}

#[test]
/// `task show` takes one ID, before or after `--json`, or after `--`.
fn the_show_parser_takes_one_id() {
    for (args, id, json) in [
        (&["mmmm"][..], "mmmm", false),
        (&["-j", "mmmm"], "mmmm", true),
        (&["mmmm", "--json", "--no-pager"], "mmmm", true),
        (&["--", "-m"], "-m", false),
        (&["--no-color", "X"], "X", false),
    ] {
        let expected = TaskShowOptions {
            id: id.into(),
            json,
        };
        assert_eq!(parse_task_show_args(&os(args)), Ok(expected), "{args:?}");
    }
    for (args, first) in [
        (&[][..], "the required argument ID was not provided"),
        (&["--json"], "the required argument ID was not provided"),
        (&["a", "b"], "unexpected argument 'b'"),
        (&["-a", "x"], "unexpected argument '-a'"),
        (
            &["-j", "-j", "x"],
            "the argument '--json' cannot be used more than once",
        ),
        (
            &["-q", "x", "--quiet"],
            "the argument '--quiet' cannot be used more than once",
        ),
    ] {
        let error = parse_task_show_args(&os(args)).unwrap_err();
        let text = format!("{first}\n{SHOW_USAGE}");
        assert_eq!(error, UsageError::Usage(text), "{args:?}");
    }
    for (args, name) in [
        (&["-q", "x"][..], "quiet"),
        (&["x", "--raw"], "raw"),
        (&["-jv", "x"], "verbose"),
    ] {
        let error = parse_task_show_args(&os(args)).unwrap_err();
        let text =
            format!("gossil: issue task show option --{name} is not available in this build");
        assert_eq!(error, UsageError::Unsupported(text), "{args:?}");
    }
}

#[test]
/// The lookup follows Gest `resolve_id`. It searches the tasks in an open status first,
/// then every task, with a case-sensitive prefix of 1 to 32 letters from `k` to `z`.
fn the_lookup_finds_one_task_by_prefix() {
    let time = "2026-11-18T12:00:00Z";
    let tickets = vec![
        ticket(inputs("m", "lmo", "open", time)),
        ticket(inputs("n", "lmp", "done", time)),
        ticket(inputs("o", "lqr", "done", time)),
        ticket(inputs("p", "lqs", "cancelled", time)),
        ticket(inputs("q", "ltu", "in-progress", time)),
        IssueTicket {
            record: IssueRecord::Project(project()),
            ..ticket(inputs("m", "", "open", time))
        },
    ];
    let all = tasks(&tickets);
    assert_eq!(all.len(), 5);
    let found = |prefix: &str| find_task(&all, prefix).map(|task| task.values.gest_id.to_string());
    let full = format!("lmo{}", "m".repeat(29));
    assert_eq!(found(&full), Ok(full.clone()));
    assert_eq!(found("lmo"), Ok(full.clone()));
    // Two tasks start with lm, but only one is open, so the open one wins.
    assert_eq!(found("lm"), Ok(full.clone()));
    assert_eq!(found("lt"), Ok(format!("ltu{}", "q".repeat(29))));
    // With no open match, every task is searched.
    assert_eq!(found("lmp"), Ok(format!("lmp{}", "n".repeat(29))));
    assert_eq!(found("l"), Err(LookupError::Ambiguous("l".into(), 2)));
    assert_eq!(found("lq"), Err(LookupError::Ambiguous("lq".into(), 2)));
    assert_eq!(found("lz"), Err(LookupError::NotFound("lz".into())));
    // The project's Gest ID is not a task ID.
    assert_eq!(found("zz"), Err(LookupError::NotFound("zz".into())));
    let long = "m".repeat(33);
    for (prefix, text) in [
        ("", "Id prefix must not be empty"),
        (
            "LMO",
            "Id prefix must contain only characters k-z, found 'L'",
        ),
        (
            "lm1",
            "Id prefix must contain only characters k-z, found '1'",
        ),
        (&long, "Id prefix must be at most 32 characters, got 33"),
    ] {
        let error = LookupError::InvalidPrefix(text.into());
        assert_eq!(found(prefix), Err(error), "{prefix}");
    }
    assert_eq!(
        LookupError::NotFound("lz".into()).to_string(),
        "not found: id prefix 'lz'"
    );
    assert_eq!(
        LookupError::Ambiguous("l".into(), 2).to_string(),
        "ambiguous id prefix 'l': matches 2 entities"
    );
    assert_eq!(LookupError::InvalidPrefix("bad".into()).to_string(), "bad");
}

#[test]
/// The default list leaves out done and cancelled tasks, also for `--status done`.
/// The newest `created_at` comes first, and the later receipt breaks a tie.
fn the_selection_filters_and_orders_as_gest() {
    let tickets = vec![
        ticket(inputs("m", "", "open", "2026-11-18T12:00:00Z")),
        ticket(inputs("n", "", "done", "2026-11-19T12:00:00Z")),
        ticket(inputs("o", "", "in-progress", "2026-11-18T12:00:00Z")),
        ticket(inputs("p", "", "cancelled", "2026-11-17T12:00:00Z")),
        ticket(TaskValueInputs {
            project_id: json!("l".repeat(32)),
            ..inputs("q", "", "open", "2026-11-20T12:00:00Z")
        }),
    ];
    let all = tasks(&tickets);
    let pick =
        |project: Option<&Id>, options: TaskListOptions| ids(&select(&all, project, &options));
    let id = |letter: &str| letter.repeat(32);
    assert_eq!(
        pick(None, TaskListOptions::default()),
        [id("q"), id("o"), id("m")]
    );
    let everything = TaskListOptions {
        all: true,
        ..TaskListOptions::default()
    };
    assert_eq!(
        pick(None, everything),
        [id("q"), id("n"), id("o"), id("m"), id("p")]
    );
    let status = |status, all| TaskListOptions {
        all,
        status: Some(status),
        json: false,
    };
    assert!(pick(None, status(TaskStatus::Done, false)).is_empty());
    assert_eq!(pick(None, status(TaskStatus::Done, true)), [id("n")]);
    assert_eq!(
        pick(None, status(TaskStatus::Open, false)),
        [id("q"), id("m")]
    );
    let project: Id = "k".repeat(32).parse().unwrap();
    assert_eq!(
        pick(Some(&project), TaskListOptions::default()),
        [id("o"), id("m")]
    );
}

/// A task with every kind of field value, for the printers.
/// # Returns
/// A ticket with a description of two lines and a title with a tab.
fn rich() -> IssueTicket {
    ticket(TaskValueInputs {
        title: json!("Alpha <b>\tx"),
        description: json!("line1\nline2\u{1b}"),
        priority: json!(2),
        metadata: json!({"n": 3, "area": "cli"}),
        assigned_to: json!("l".repeat(32)),
        created_at: json!("2026-11-25T05:34:42.239223Z"),
        updated_at: json!("2026-11-25T05:34:42.239223+00:00"),
        ..inputs("m", "", "open", "2026-11-25T05:34:42.239223Z")
    })
}

#[test]
/// The provisional text of `task list` has one plain line per task.
fn the_list_text_has_one_plain_line_per_task() {
    let tickets = vec![
        rich(),
        ticket(TaskValueInputs {
            priority: json!(null),
            ..inputs("n", "", "in-progress", "2026-11-18T12:00:00Z")
        }),
    ];
    let text = String::from_utf8(task_list_text(&tasks(&tickets))).unwrap();
    assert_eq!(
        text,
        "mmmmmmmm  open         P2    Alpha <b>\\tx\n\
         nnnnnnnn  in-progress  --    Task n\n"
    );
    assert!(task_list_text(&[]).is_empty());
}

#[test]
/// The provisional text of `task show` labels every stored field.
fn the_show_text_labels_every_field() {
    let tickets = [rich()];
    let text = String::from_utf8(task_show_text(&tasks(&tickets)[0])).unwrap();
    let expected = format!(
        "id           {m}\n\
         title        Alpha <b>\\tx\n\
         status       open\n\
         priority     2\n\
         project      {k}\n\
         assigned to  {l}\n\
         metadata     {{\"area\":\"cli\",\"n\":3}}\n\
         created at   2026-11-25T05:34:42.239223Z\n\
         updated at   2026-11-25T05:34:42.239223Z\n\
         resolved at  none\n\
         ticket       {a}\n\
         artifact     {b}\n\
         description\n    line1\n    line2\\u{{1b}}\n",
        m = "m".repeat(32),
        k = "k".repeat(32),
        l = "l".repeat(32),
        a = "a".repeat(40),
        b = "b".repeat(64),
    );
    assert_eq!(text, expected);
    let done = [ticket(TaskValueInputs {
        priority: json!(null),
        ..inputs("n", "", "done", "2026-11-18T12:00:00Z")
    })];
    let text = String::from_utf8(task_show_text(&tasks(&done)[0])).unwrap();
    assert!(text.contains("\npriority     none\n"), "{text}");
    assert!(text.contains("\nassigned to  none\n"), "{text}");
    assert!(
        text.contains("\nresolved at  2026-11-18T12:00:00Z\n"),
        "{text}"
    );
    assert!(text.ends_with("\ndescription\n"), "{text}");
}

/// The JSON lines of the rich task's model fields, as Gest prints them.
/// # Arguments
/// * `indent` - The indent of each field line.
/// # Returns
/// The pretty JSON lines of the fields, without the braces and the sidecars.
fn rich_fields(indent: &str) -> String {
    let lines = [
        format!("\"assigned_to\": \"{}\",", "l".repeat(32)),
        "\"created_at\": \"2026-11-25T05:34:42.239223Z\",".to_owned(),
        "\"description\": \"line1\\nline2\\u001b\",".to_owned(),
        format!("\"id\": \"{}\",", "m".repeat(32)),
        "\"metadata\": {".to_owned(),
        "  \"area\": \"cli\",".to_owned(),
        "  \"n\": 3".to_owned(),
        "},".to_owned(),
        "\"priority\": 2,".to_owned(),
        format!("\"project_id\": \"{}\",", "k".repeat(32)),
        "\"resolved_at\": null,".to_owned(),
        "\"status\": \"open\",".to_owned(),
        "\"title\": \"Alpha <b>\\tx\",".to_owned(),
        "\"updated_at\": \"2026-11-25T05:34:42.239223Z\",".to_owned(),
    ];
    lines
        .iter()
        .map(|line| format!("{indent}{line}\n"))
        .collect()
}

#[test]
/// The JSON follows Gest's envelopes, with the model fields, then `notes` for show only,
/// then empty `relationships` and `tags`. It is pretty printed with a final newline.
fn the_json_follows_gest_envelopes() {
    let tickets = [rich()];
    let all = tasks(&tickets);
    let list = String::from_utf8(task_list_json(&all)).unwrap();
    assert_eq!(
        list,
        format!(
            "[\n  {{\n{}    \"relationships\": [],\n    \"tags\": []\n  }}\n]\n",
            rich_fields("    ")
        )
    );
    let show = String::from_utf8(task_show_json(&all[0])).unwrap();
    assert_eq!(
        show,
        format!(
            "{{\n{}  \"notes\": [],\n  \"relationships\": [],\n  \"tags\": []\n}}\n",
            rich_fields("  ")
        )
    );
    assert_eq!(task_list_json(&[]), b"[]\n");
    let status = [ticket(inputs(
        "n",
        "",
        "in-progress",
        "2026-11-18T12:00:00Z",
    ))];
    let show = String::from_utf8(task_show_json(&tasks(&status)[0])).unwrap();
    // Gest's serde spelling of `in-progress` has no hyphen.
    assert!(show.contains("\"status\": \"inprogress\","), "{show}");
}

#[test]
/// Each failure maps to its CLI-05 exit status.
fn the_errors_map_to_their_exit_codes() {
    let output = ReadError::Output(std::io::Error::other("closed"));
    assert_eq!(output.exit_code(), 74);
    assert_eq!(output.to_string(), "cannot write the output: closed");
    for (error, code) in [
        (LookupError::InvalidPrefix("x".into()), 64),
        (LookupError::NotFound("x".into()), 66),
        (LookupError::Ambiguous("x".into(), 2), 66),
    ] {
        let text = error.to_string();
        let error = ReadError::Lookup(error);
        assert_eq!(error.exit_code(), code);
        assert_eq!(error.to_string(), text);
    }
}

/// Supply finite ceilings that every fixture fits.
/// # Returns
/// Generous content, parser and SQL ceilings.
fn limits() -> IssueLimits {
    IssueLimits {
        admission: AdmissionLimits {
            content: Limits {
                max_stored_bytes: 1 << 20,
                max_output_bytes: 1 << 20,
                max_chain_depth: 128,
                max_total_stored_bytes: 64 << 20,
                max_total_decoded_bytes: 256 << 20,
            },
            parser: crate::artifact::ParseLimits {
                input_bytes: 4 << 20,
                cards: 65536,
                owned_bytes: 16 << 20,
                entries: 8192,
            },
        },
        sql_rows: 100_000,
        sql_bytes: 64 << 20,
    }
}

/// The tracking ceilings of the fixture check-in.
/// # Returns
/// Small finite ceilings.
fn tracking() -> TrackingLimits {
    TrackingLimits {
        paths: 16,
        path_bytes: 4096,
        components: 16,
        retained_bytes: 1 << 20,
        io_buffer_bytes: 1024,
    }
}

/// Create a new catalog 6 repository through the library.
/// # Arguments
/// * `path` - The absent repository file.
/// # Returns
/// The admitted owner.
async fn repository(path: std::path::PathBuf) -> LocalOwnerRepository {
    let a = limits().admission;
    let options = InitializeOptions {
        repository_id: "0123456789abcdef0123456789abcdef01234567",
        owner_id: "owner",
        date: "2026-11-20T00:00:00",
        user: b"Owner",
        password: "secret",
        sha1: false,
        content_limits: a.content,
        parse_limits: a.parser,
    };
    LocalOwnerRepository::initialize(path, options)
        .await
        .unwrap()
}

/// Store one creation artifact through the library.
/// # Arguments
/// * `owner` - The admitted owner.
/// * `artifact` - The artifact.
/// # Returns
/// The stored ticket.
async fn put(owner: &mut LocalOwnerRepository, artifact: &[u8]) -> IssueTicket {
    let request = PrivateIssueRequest {
        artifact,
        scope: PathScope::Private,
    };
    owner.create_private_issue(request, limits()).await.unwrap()
}

/// Store one task through the library.
/// # Arguments
/// * `owner` - The admitted owner.
/// * `inputs` - The task fields.
/// * `n` - The number that the K card spells in hexadecimal.
/// # Returns
/// The stored ticket.
async fn store(owner: &mut LocalOwnerRepository, inputs: TaskValueInputs, n: usize) -> IssueTicket {
    let task = task_from_values(inputs).unwrap();
    let date = NativeDate::parse(b"2026-11-18T12:00:00Z").unwrap();
    let k = format!("{n:040x}");
    let artifact =
        crate::task_ticket::task_creation_bytes(&task, k.as_bytes(), b"agent", date, 65_536)
            .unwrap();
    put(owner, &artifact).await
}

/// Record one private check-in of the file `a` in the registered checkout.
/// # Arguments
/// * `owner` - The admitted owner.
/// * `root` - The checkout root.
/// * `view` - The current checkout view.
async fn check_in(owner: &mut LocalOwnerRepository, root: &std::path::Path, view: CheckoutView) {
    std::fs::write(root.join("a"), b"first").unwrap();
    let a = limits().admission;
    let view = owner
        .add_files("work", &["a"], &view, a, tracking())
        .await
        .unwrap();
    let scopes = [PathScopeSelection {
        path: "a".into(),
        scope: PathScope::Private,
    }];
    let view = owner
        .select_path_scopes("work", &view, &scopes, a, tracking())
        .await
        .unwrap();
    let request = PrivateRecordingRequest {
        expected: &view,
        paths: &["a"],
        scope: PathScope::Private,
        actor: b"Owner",
        comment: b"first",
        date: b"2026-11-20T00:00:01",
    };
    let outcome = owner
        .commit_private_tracked_files("work", request, a, tracking())
        .await
        .unwrap();
    assert!(matches!(outcome, PrivateRecordingOutcome::Recorded { .. }));
}

/// Build a catalog 6 repository with a project and four tasks, two of them after a
/// private check-in.
///
/// The tasks are `lmo…` (open), `lmp…` (done), `lqr…` (open, after the check-in) and
/// `lqs…` (in progress in project `l…`, after the check-in). The project is `z…`.
/// # Returns
/// The directory, the owner and the stored tickets in receipt order.
async fn fixture() -> (tempfile::TempDir, LocalOwnerRepository, Vec<IssueTicket>) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let mut owner = repository(temp.path().join("repo")).await;
    let mut stored = Vec::new();
    let open = inputs("m", "lmo", "open", "2026-11-18T12:00:01Z");
    stored.push(store(&mut owner, open, 1).await);
    let done = inputs("n", "lmp", "done", "2026-11-18T12:00:02Z");
    stored.push(store(&mut owner, done, 2).await);
    let date = NativeDate::parse(b"2026-11-18T12:00:00Z").unwrap();
    let k = format!("{:040x}", 3);
    let bytes = crate::gest_ticket::project_creation_bytes(
        &project(),
        k.as_bytes(),
        b"agent",
        date,
        65_536,
    )
    .unwrap();
    stored.push(put(&mut owner, &bytes).await);
    let view = owner
        .register_checkout("work", &root, limits().admission)
        .await
        .unwrap();
    check_in(&mut owner, &root, view).await;
    let later = inputs("o", "lqr", "open", "2026-11-18T12:00:03Z");
    stored.push(store(&mut owner, later, 4).await);
    let other = TaskValueInputs {
        project_id: json!("l".repeat(32)),
        ..inputs("p", "lqs", "in-progress", "2026-11-18T12:00:04Z")
    };
    stored.push(store(&mut owner, other, 5).await);
    (temp, owner, stored)
}

/// Run the list handler and return its output.
/// # Arguments
/// * `owner` - The admitted owner.
/// * `project` - The project filter.
/// * `args` - The arguments after `task list`.
/// # Returns
/// The output text.
async fn list(owner: &mut LocalOwnerRepository, project: Option<&Id>, args: &[&str]) -> String {
    let options = parse_task_list_args(&os(args)).unwrap();
    let mut out = Vec::new();
    task_list(owner, project, &options, limits(), &mut out)
        .await
        .unwrap();
    String::from_utf8(out).unwrap()
}

/// Run the show handler.
/// # Arguments
/// * `owner` - The admitted owner.
/// * `args` - The arguments after `task show`.
/// # Returns
/// The output text, or the handler's failure.
async fn show(owner: &mut LocalOwnerRepository, args: &[&str]) -> Result<String, ReadError> {
    let options = parse_task_show_args(&os(args)).unwrap();
    let mut out = Vec::new();
    task_show(owner, &options, limits(), &mut out).await?;
    Ok(String::from_utf8(out).unwrap())
}

/// Give the short ID of a stored task.
/// # Arguments
/// * `ticket` - A task ticket.
/// # Returns
/// The first 8 letters of its Gest ID.
fn short(ticket: &IssueTicket) -> String {
    let IssueRecord::Task(values) = &ticket.record else {
        panic!("a task is required")
    };
    values.gest_id.short()
}

#[tokio::test]
/// CLI-T04 subset: on a repository with check-ins and issues, `task list` prints the
/// open tasks newest first, `--all` adds the done task, a project filter keeps its
/// tasks, and the JSON parses back to the stored fields.
async fn the_list_handler_reads_a_repository_with_check_ins() {
    let (_temp, mut owner, stored) = fixture().await;
    let (m, n, o, p) = (
        short(&stored[0]),
        short(&stored[1]),
        short(&stored[3]),
        short(&stored[4]),
    );
    assert_eq!(
        list(&mut owner, None, &[]).await,
        format!(
            "{p}  in-progress  P1    Task p\n{o}  open         P1    Task o\n{m}  open         P1    Task m\n"
        )
    );
    let firsts =
        |text: &str| -> Vec<String> { text.lines().map(|line| line[..8].to_owned()).collect() };
    let all = list(&mut owner, None, &["-a"]).await;
    assert_eq!(firsts(&all), [p.clone(), o.clone(), n, m.clone()]);
    let project: Id = "k".repeat(32).parse().unwrap();
    let mine = list(&mut owner, Some(&project), &["--all", "-s", "open"]).await;
    assert_eq!(firsts(&mine), [o, m]);
    let text = list(&mut owner, None, &["--all", "--json"]).await;
    let json: Json = serde_json::from_str(&text).unwrap();
    let expected: Vec<Json> = [4, 3, 1, 0]
        .iter()
        .map(|&index| {
            let IssueRecord::Task(values) = &stored[index].record else {
                panic!("a task is required")
            };
            let mut object = serde_json::to_value(Model::from(values.clone())).unwrap();
            object["relationships"] = json!([]);
            object["tags"] = json!([]);
            object
        })
        .collect();
    assert_eq!(json, Json::Array(expected));
    let missing: Id = "y".repeat(32).parse().unwrap();
    assert_eq!(list(&mut owner, Some(&missing), &["--json"]).await, "[]\n");
}

#[tokio::test]
/// `task show` finds a task by a unique prefix, and refuses a missing or ambiguous
/// prefix with 66 and an invalid one with 64.
async fn the_show_handler_looks_up_by_prefix() {
    let (_temp, mut owner, stored) = fixture().await;
    let text = show(&mut owner, &["lqr"]).await.unwrap();
    let head = format!("id           lqr{}\n", "o".repeat(29));
    assert!(text.starts_with(&head), "{text}");
    let ticket = format!("\nticket       {}\n", stored[3].ticket_id);
    assert!(text.contains(&ticket), "{text}");
    let artifact = format!("\nartifact     {}\n", stored[3].native_name);
    assert!(text.contains(&artifact), "{text}");
    let text = show(&mut owner, &["--json", "lmp"]).await.unwrap();
    let json: Json = serde_json::from_str(&text).unwrap();
    assert_eq!(json["id"], json!(format!("lmp{}", "n".repeat(29))));
    assert_eq!(json["status"], json!("done"));
    assert_eq!(json["notes"], json!([]));
    // lm matches an open and a done task, so the open one wins.
    let text = show(&mut owner, &["lm"]).await.unwrap();
    let head = format!("id           lmo{}\n", "m".repeat(29));
    assert!(text.starts_with(&head), "{text}");
    for (prefix, code, message) in [
        ("lq", 66, "ambiguous id prefix 'lq': matches 2 entities"),
        ("l", 66, "ambiguous id prefix 'l': matches 3 entities"),
        ("lz", 66, "not found: id prefix 'lz'"),
        // The project's Gest ID is not a task ID.
        ("z", 66, "not found: id prefix 'z'"),
        (
            "LQ",
            64,
            "Id prefix must contain only characters k-z, found 'L'",
        ),
    ] {
        let error = show(&mut owner, &[prefix]).await.unwrap_err();
        let got = (error.exit_code(), error.to_string());
        assert_eq!(got, (code, message.to_owned()), "{prefix}");
    }
}

/// A writer that refuses every write.
struct Closed;
impl Write for Closed {
    /// Refuse the write.
    /// # Arguments
    /// * `_buf` - The bytes.
    /// # Returns
    /// Always an error.
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("closed"))
    }
    /// Do nothing.
    /// # Returns
    /// Success.
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
/// A failed write exits with 74. When another connection changes a row, both handlers
/// refuse with 74 before any output, and a new open refuses the repository.
async fn the_handlers_refuse_a_changed_repository_and_a_failed_write() {
    let (temp, mut owner, _) = fixture().await;
    let options = TaskListOptions::default();
    let error = task_list(&mut owner, None, &options, limits(), &mut Closed)
        .await
        .unwrap_err();
    assert_eq!(error.exit_code(), 74);
    assert_eq!(error.to_string(), "cannot write the output: closed");
    let show_options = TaskShowOptions {
        id: "lqr".into(),
        json: true,
    };
    let error = task_show(&mut owner, &show_options, limits(), &mut Closed)
        .await
        .unwrap_err();
    assert_eq!(error.exit_code(), 74);
    let database = libsql::Builder::new_local(temp.path().join("repo"))
        .build()
        .await
        .unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute("UPDATE g_gest_tasks SET title='other'", ())
        .await
        .unwrap();
    drop(connection);
    let mut out = Vec::new();
    let error = task_list(&mut owner, None, &options, limits(), &mut out)
        .await
        .unwrap_err();
    assert!(matches!(error, ReadError::Storage(_)), "{error}");
    assert_eq!(error.exit_code(), 74);
    let stale = "repository admission stale; reopen with explicit admission limits";
    assert_eq!(error.to_string(), stale);
    let error = task_show(&mut owner, &show_options, limits(), &mut out)
        .await
        .unwrap_err();
    assert!(matches!(error, ReadError::Storage(_)), "{error}");
    assert!(out.is_empty());
    drop(owner);
    let path = temp.path().join("repo");
    let refused = LocalOwnerRepository::open(path, limits().admission).await;
    assert_eq!(
        refused.err().unwrap().to_string(),
        "stored issue rows differ from the ticket artifact"
    );
}

/// Run the reference `gest` in a fixture home and return its standard output.
/// # Arguments
/// * `gest` - The reference executable.
/// * `home` - The fixture home, which holds the project directory `proj`.
/// * `args` - The arguments.
/// # Returns
/// The output text.
fn gest(gest: &std::ffi::OsStr, home: &std::path::Path, args: &[&str]) -> String {
    let mut command = std::process::Command::new(gest);
    for (key, _) in std::env::vars_os() {
        if key.to_str().is_some_and(|key| key.starts_with("GEST_")) {
            command.env_remove(key);
        }
    }
    let output = command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("XDG_DATA_HOME", home)
        .env("EDITOR", "true")
        .env("NO_COLOR", "1")
        .current_dir(home.join("proj"))
        .stdin(std::process::Stdio::null())
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
#[ignore = "needs a reference gest executable; run just issue-read-oracle"]
/// Gest and this build print the same JSON bytes for the same tasks: `task show
/// --json`, `task list --json` and `task list --all --json` for an open and a done
/// task, after this build stores each task as Gest's JSON gives it.
async fn gest_reference_json_matches() {
    let reference = std::env::var_os("GOSSIL_GEST_REFERENCE").expect("GOSSIL_GEST_REFERENCE");
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir(home.path().join("proj")).unwrap();
    let run = |args: &[&str]| gest(&reference, home.path(), args);
    run(&["init"]);
    let first = run(&[
        "task",
        "create",
        "Alpha <b> & \"q\"",
        "-d",
        "line1\nline2",
        "-p",
        "2",
        "-m",
        "area=cli",
        "-m",
        "n=3",
        "-q",
    ]);
    let second = run(&["task", "create", "Beta", "-d", "", "-s", "done", "-q"]);
    let temp = tempfile::tempdir().unwrap();
    let mut owner = repository(temp.path().join("repo")).await;
    for (index, short) in [first.trim(), second.trim()].into_iter().enumerate() {
        let reference = run(&["task", "show", short, "--json"]);
        let shown: Json = serde_json::from_str(&reference).unwrap();
        let field = |name: &str| shown[name].clone();
        let inputs = TaskValueInputs {
            gest_id: field("id"),
            project_id: field("project_id"),
            title: field("title"),
            priority: field("priority"),
            status: field("status"),
            description: field("description"),
            assigned_to: field("assigned_to"),
            metadata: field("metadata"),
            resolved_at: field("resolved_at"),
            created_at: field("created_at"),
            updated_at: field("updated_at"),
        };
        store(&mut owner, inputs, index + 1).await;
        let ours = show(&mut owner, &["--json", short]).await.unwrap();
        assert_eq!(ours, reference, "{short}");
    }
    assert_eq!(
        list(&mut owner, None, &["--all", "--json"]).await,
        run(&["task", "list", "--all", "--json"])
    );
    assert_eq!(
        list(&mut owner, None, &["--json"]).await,
        run(&["task", "list", "--json"])
    );
}
