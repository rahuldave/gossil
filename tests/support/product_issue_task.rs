//! Unit tests of `issue task create`: the parser, the metadata, the output and the
//! write, with GEST-T08 failure injection after each write step.
use super::*;
use crate::storage::{IssuePhase, IssueRecord, LocalOwnerRepository, inject_issue_failure};
use serde_json::json;
use std::path::{Path, PathBuf};

/// Turn string slices into the argument vector that `parse` receives.
fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

/// The request of `task create TITLE` with no option.
fn titled(title: &str) -> Request {
    Request {
        title: title.into(),
        ..Request::default()
    }
}

/// The builder of the option table gives an option that does not repeat and that runs.
#[test]
fn the_option_builder_sets_its_defaults() {
    let built = option(std::hint::black_box("x"), Some('x'), true);
    assert_eq!(
        (built.long, built.short, built.takes_value),
        ("x", Some('x'), true)
    );
    assert!(!built.repeats && !built.refused);
}

/// Each accepted option, in its long, short, attached and `=` forms, sets its field.
#[test]
fn parse_accepts_each_option() {
    let full = Request {
        title: "T".into(),
        description: "D".into(),
        priority: Some(1),
        status: TaskStatus::InProgress,
        metadata: vec!["a=1".into(), "b.c=x".into()],
        metadata_json: vec!["{}".into(), "{\"k\":2}".into()],
        output: Output::Json,
    };
    for args in [
        &[
            "T",
            "-d",
            "D",
            "-p",
            "high",
            "-s",
            "IN_PROGRESS",
            "-m",
            "a=1",
            "--metadata",
            "b.c=x",
            "--metadata-json",
            "{}",
            "--metadata-json={\"k\":2}",
            "-j",
        ][..],
        &[
            "--description=D",
            "-p1",
            "-s=inprogress",
            "-ma=1",
            "-m=b.c=x",
            "--metadata-json",
            "{}",
            "--metadata-json",
            "{\"k\":2}",
            "-jq",
            "T",
            "-r",
            "--no-color",
            "--no-pager",
            "-vvv",
            "--verbose",
        ],
    ] {
        assert_eq!(parse(&os(args)), Ok(full_clone(&full)), "{args:?}");
    }
    for (args, expected) in [
        (&["T"][..], titled("T")),
        (&["-"], titled("-")),
        (&["--", "-x"], titled("-x")),
        (
            &["-q", "T"],
            Request {
                output: Output::Quiet,
                ..titled("T")
            },
        ),
        (
            &["T", "-d", "-"],
            Request {
                description: "-".into(),
                ..titled("T")
            },
        ),
        (&["T", "--description="], titled("T")),
        (
            &["T", "-p", "CRITICAL"],
            Request {
                priority: Some(0),
                ..titled("T")
            },
        ),
        (
            &["T", "-p", "4"],
            Request {
                priority: Some(4),
                ..titled("T")
            },
        ),
        (
            &["T", "-s", "done"],
            Request {
                status: TaskStatus::Done,
                ..titled("T")
            },
        ),
        (
            &["T", "-s", "in_progress"],
            Request {
                status: TaskStatus::InProgress,
                ..titled("T")
            },
        ),
        (
            &["T", "-s", "Cancelled"],
            Request {
                status: TaskStatus::Cancelled,
                ..titled("T")
            },
        ),
    ] {
        assert_eq!(parse(&os(args)), Ok(expected), "{args:?}");
    }
}

/// Copy a request, which has no `Clone`.
fn full_clone(request: &Request) -> Request {
    Request {
        title: request.title.clone(),
        description: request.description.clone(),
        priority: request.priority,
        status: request.status,
        metadata: request.metadata.clone(),
        metadata_json: request.metadata_json.clone(),
        output: request.output,
    }
}

/// The usage error of `problem`, as the parser gives it.
fn usage_of(problem: &str) -> Result<Request, IssueFailure> {
    Err(IssueFailure::new(
        2,
        format!(
            "gossil: issue task create: {problem}\nUsage: gossil issue task create [OPTIONS] [TITLE]"
        ),
    ))
}

/// CLI-T03: unknown options, a second title, repeated options, missing and malformed
/// values give status 2, before any refusal with status 69.
#[test]
fn parse_refuses_usage_errors_with_2() {
    for (args, problem) in [
        (&["T", "--frob"][..], "unexpected argument \"--frob\""),
        (&["T", "-x"], "unexpected argument \"-x\""),
        (&["T", "-jx"], "unexpected argument \"-x\""),
        (&["T", "U"], "unexpected argument \"U\""),
        (&["--", "T", "-d"], "unexpected argument \"-d\""),
        (&["T", "-d"], "\"--description\" needs a value"),
        (&["T", "-d", "-x"], "\"--description\" needs a value"),
        (&["T", "-p", "-1"], "\"--priority\" needs a value"),
        (&["T", "--assign"], "\"--assign\" needs a value"),
        (
            &["T", "-d", "a", "--description", "b"],
            "\"--description\" cannot be used more than once",
        ),
        (
            &["T", "-j", "--json"],
            "\"--json\" cannot be used more than once",
        ),
        (&["T", "-jj"], "\"--json\" cannot be used more than once"),
        (
            &["T", "--no-color", "--no-color"],
            "\"--no-color\" cannot be used more than once",
        ),
        (&["T", "--json=yes"], "\"--json\" takes no value"),
        (&["T", "--batch=1"], "\"--batch\" takes no value"),
        (
            &["T", "-p", "5"],
            "invalid value for \"--priority\": invalid priority \"5\" (accepted: 0-4, critical, high, medium, low, lowest)",
        ),
        (
            &["T", "-p", "256"],
            "invalid value for \"--priority\": invalid priority \"256\" (accepted: 0-4, critical, high, medium, low, lowest)",
        ),
        (
            &["T", "-p", "urgent"],
            "invalid value for \"--priority\": invalid priority \"urgent\" (accepted: 0-4, critical, high, medium, low, lowest)",
        ),
        (
            &["T", "-s", "Closed"],
            "invalid value for \"--status\": invalid task status: closed",
        ),
        (
            &["T", "-i", "x", "--phase", "one"],
            "invalid value for \"--phase\": invalid digit found in string",
        ),
        (&["T", "--phase", "1"], "\"--phase\" needs \"--iteration\""),
        (&["--assign", "a", "T", "U"], "unexpected argument \"U\""),
        (&["--tag", "a", "--frob"], "unexpected argument \"--frob\""),
    ] {
        assert_eq!(parse(&os(args)), usage_of(problem), "{args:?}");
    }
    use std::os::unix::ffi::OsStringExt;
    let bad = OsString::from_vec(vec![0xff]);
    for args in [
        vec![bad.clone()],
        vec![OsString::from("T"), OsString::from("-d"), bad],
    ] {
        assert_eq!(parse(&args), usage_of("an argument is not valid UTF-8"));
    }
}

/// The options that this build does not run, and a missing title, give status 69
/// after the usage checks. The first refused option in argument order is named.
#[test]
fn parse_refuses_unavailable_options_with_69() {
    let refused = |what: &str| {
        Err(IssueFailure::new(
            69,
            format!("gossil: issue task create: {what} is not available in this build"),
        ))
    };
    for (args, what) in [
        (&["T", "--assign", "me"][..], "--assign"),
        (&["T", "--batch"], "--batch"),
        (&["T", "-i", "it"], "--iteration"),
        (&["T", "--iteration=it", "--phase", "2"], "--iteration"),
        (&["T", "-l", "blocks:x", "--link", "y"], "--link"),
        (&["T", "--tag", "a,b", "--tag", "c"], "--tag"),
        (&["--tag", "a", "--assign", "me", "T"], "--tag"),
    ] {
        assert_eq!(parse(&os(args)), refused(what), "{args:?}");
    }
    for args in [&[][..], &["-d", "D"], &["--"]] {
        assert_eq!(
            parse(&os(args)),
            Err(IssueFailure::new(
                69,
                "gossil: issue task create: reading the title from stdin is not available in this build; give TITLE"
            )),
            "{args:?}"
        );
    }
}

/// GEST-T02 subset: metadata is an empty object when omitted, follows dot paths and
/// scalar inference, merges JSON objects on top, and refuses malformed input with 64.
#[test]
fn metadata_follows_gest() {
    let strings = |items: &[&str]| {
        items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(metadata(&[], &[]), Ok(json!({})));
    assert_eq!(metadata(&[], &strings(&["{}"])), Ok(json!({})));
    assert_eq!(
        metadata(
            &strings(&[
                "a.b=deep", "c=42", "d=true", "e=false", "f=1.5", "g=1e400", "h=null", "i=", "=j",
                "a=flat", "k.l=1", "k.m=2", "n=-7", "o=NaN",
            ]),
            &strings(&["{\"c\":{\"x\":null},\"p\":\"q\"}", "{\"p\":0}"]),
        ),
        Ok(json!({
            "a": "flat", "c": {"x": null}, "d": true, "e": false, "f": 1.5, "g": "1e400",
            "h": "null", "i": "", "": "j", "k": {"l": 1, "m": 2}, "n": -7, "o": "NaN", "p": 0
        }))
    );
    assert_eq!(
        metadata(&strings(&["a=1", "a.b=2"]), &[]),
        Ok(json!({"a": {"b": 2}}))
    );
    let deep = format!("{}=1", vec!["k"; 32].join("."));
    assert!(metadata(&[deep], &[]).is_ok());
    let deeper = vec!["k"; 33].join(".");
    for (pairs, blobs, problem) in [
        (
            strings(&["bad"]),
            vec![],
            "invalid --metadata pair (expected key=value): bad".to_string(),
        ),
        (
            vec![format!("{deeper}=1")],
            vec![],
            format!("invalid --metadata key path: {deeper}"),
        ),
        (
            vec![],
            strings(&["{"]),
            "invalid --metadata-json: EOF while parsing an object at line 1 column 1".into(),
        ),
        (
            vec![],
            strings(&["1"]),
            "--metadata-json must be a JSON object".into(),
        ),
        (
            vec![],
            strings(&["null"]),
            "--metadata-json must be a JSON object".into(),
        ),
    ] {
        assert_eq!(
            metadata(&pairs, &blobs),
            Err(IssueFailure::new(
                64,
                format!("gossil: issue task create: {problem}")
            )),
            "{problem}"
        );
    }
}

/// A `[` in the title, the description or the metadata, including a JSON array, gives 64.
#[test]
fn brackets_are_refused_with_64() {
    let refusal = |field: &str| {
        Err(IssueFailure::new(
            64,
            format!(
                "gossil: issue task create: the {field} contains \"[\", and this build refuses it, because Markdown backlinks are not ported; a JSON array in the metadata holds \"[\" too"
            ),
        ))
    };
    assert_eq!(
        refuse_brackets(&titled("a [b]"), &json!({})),
        refusal("title")
    );
    let described = Request {
        description: "see [[x]]".into(),
        ..titled("T")
    };
    assert_eq!(
        refuse_brackets(&described, &json!({})),
        refusal("description")
    );
    for metadata in [json!({"a": "["}), json!({"[": 1}), json!({"a": [1]})] {
        assert_eq!(
            refuse_brackets(&titled("T"), &metadata),
            refusal("metadata")
        );
    }
    assert_eq!(
        refuse_brackets(&titled("a ] (b) {c}"), &json!({"k": "]"})),
        Ok(())
    );
}

/// A complete task with fixed values.
fn fixed_task(status: &str, resolved: Value) -> TaskValues {
    task_from_values(TaskValueInputs {
        gest_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
        project_id: json!("k".repeat(32)),
        title: json!("Write the docs"),
        priority: json!(2),
        status: json!(status),
        description: json!("line one\nline two"),
        assigned_to: Value::Null,
        metadata: json!({"b": 1, "a": {"c": true}}),
        resolved_at: resolved,
        created_at: json!("2026-11-25T10:00:00.123456789Z"),
        updated_at: json!("2026-11-25T10:00:00.123456789Z"),
    })
    .unwrap()
}

/// The text form follows Gest's success message without color, the quiet form is the
/// short ID, and the JSON form follows Gest's envelope and field order.
#[test]
fn each_output_form_follows_gest() {
    let task = fixed_task("in-progress", Value::Null);
    assert_eq!(
        output(&task, Output::Text).unwrap(),
        "  ✓  created task  zyxwvuts\n\n  title  Write the docs\n"
    );
    assert_eq!(output(&task, Output::Quiet).unwrap(), "zyxwvuts\n");
    assert_eq!(
        output(&task, Output::Json).unwrap(),
        "{\n  \"assigned_to\": null,\n  \"created_at\": \"2026-11-25T10:00:00.123456789Z\",\n  \
         \"description\": \"line one\\nline two\",\n  \"id\": \"zyxwvutsrqponmlkzyxwvutsrqponmlk\",\n  \
         \"metadata\": {\n    \"a\": {\n      \"c\": true\n    },\n    \"b\": 1\n  },\n  \
         \"priority\": 2,\n  \"project_id\": \"kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk\",\n  \
         \"resolved_at\": null,\n  \"status\": \"inprogress\",\n  \"title\": \"Write the docs\",\n  \
         \"updated_at\": \"2026-11-25T10:00:00.123456789Z\",\n  \"notes\": [],\n  \
         \"relationships\": [],\n  \"tags\": []\n}\n"
    );
    let done = fixed_task("done", json!("2026-11-25T10:00:00Z"));
    let text = output(&done, Output::Json).unwrap();
    assert!(
        text.contains("\"resolved_at\": \"2026-11-25T10:00:00Z\",\n  \"status\": \"done\""),
        "{text}"
    );
}

/// A failure of the JSON serializer gives status 65 and says that the task stays.
#[test]
fn a_json_failure_gives_65() {
    assert_eq!(
        json_failure("boom"),
        IssueFailure::new(
            65,
            "gossil: issue task create kept the task, and JSON output failed: boom"
        )
    );
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

/// Make a repository with one registered checkout, and bind it to a new project
/// when `bound` is true.
/// # Arguments
/// * `dir` - An empty directory.
/// * `bound` - True to bind the checkout to a project.
/// # Returns
/// The repository as `open_issue_repository` gives it.
async fn repository(dir: &Path, bound: bool) -> IssueRepository {
    let root = dir.join("root");
    std::fs::create_dir(&root).unwrap();
    let bounds = crate::vcs_open::limits();
    let mut owner = LocalOwnerRepository::initialize(
        &dir.join("repo"),
        crate::storage::InitializeOptions {
            repository_id: "0123456789abcdef0123456789abcdef01234567",
            owner_id: "owner",
            date: "2026-11-20 12:34:56",
            user: b"owner",
            password: "secret",
            sha1: false,
            content_limits: bounds.content,
            parse_limits: bounds.parser,
        },
    )
    .await
    .unwrap();
    let view = owner
        .register_checkout("work", &root.canonicalize().unwrap(), bounds)
        .await
        .unwrap();
    let project = "p".repeat(32);
    if bound {
        let stamp = json!("2026-11-20T12:34:56Z");
        let values = crate::gest_project_values::project_from_values(
            crate::gest_project_values::ProjectValueInputs {
                gest_id: json!(project),
                archived_at: Value::Null,
                created_at: stamp.clone(),
                updated_at: stamp,
            },
        )
        .unwrap();
        let date = crate::native_date::NativeDate::parse(b"2026-11-20T12:34:56Z").unwrap();
        let artifact = crate::gest_ticket::project_creation_bytes(
            &values,
            &[b'a'; 40],
            b"owner",
            date,
            65_536,
        )
        .unwrap();
        owner
            .init_private_project(
                crate::storage::ProjectInitRequest {
                    artifact: &artifact,
                    checkout_id: "work",
                },
                super::super::limits(),
            )
            .await
            .unwrap();
    }
    IssueRepository {
        owner,
        view,
        project: bound.then_some(project),
    }
}

/// GEST-T02 subset: each accepted value is stored, and the reader gives back the same
/// task. A terminal status sets `resolved_at` to the creation time; any spelling of in
/// progress is stored as `in-progress`. A checkout with no project gives 78.
#[tokio::test]
async fn each_value_is_stored_and_read_back() {
    let temp = tempfile::tempdir().unwrap();
    let mut found = repository(temp.path(), true).await;
    for (args, status, terminal) in [
        (&["Plain"][..], "open", false),
        (
            &[
                "T",
                "-p",
                "0",
                "-s",
                "IN_PROGRESS",
                "-d",
                "D",
                "-m",
                "a.b=1",
            ],
            "in-progress",
            false,
        ),
        (
            &[
                "T",
                "-p",
                "lowest",
                "-s",
                "done",
                "--metadata-json",
                "{\"x\":{}}",
            ],
            "done",
            true,
        ),
        (
            &["T", "-s", "cancelled", "-d", "ünïcode ✓"],
            "cancelled",
            true,
        ),
    ] {
        let request = parse(&os(args)).unwrap();
        let (ticket_id, task) = create_in(&mut found, &request).await.unwrap();
        let stored = found
            .owner
            .read_private_issue(&ticket_id, super::super::limits())
            .await
            .unwrap();
        assert_eq!(stored.record, IssueRecord::Task(task.clone()), "{args:?}");
        let values = crate::gest_task_values::task_to_values(&task);
        assert_eq!(values.status, json!(status));
        assert_eq!(values.project_id, json!("p".repeat(32)));
        assert_eq!(values.title, json!(request.title));
        assert_eq!(values.description, json!(request.description));
        assert_eq!(
            values.priority,
            request.priority.map_or(Value::Null, Value::from)
        );
        assert_eq!(values.assigned_to, Value::Null);
        assert_eq!(values.created_at, values.updated_at);
        assert_eq!(
            values.resolved_at,
            if terminal {
                values.created_at.clone()
            } else {
                Value::Null
            }
        );
        assert_eq!(
            values.metadata,
            metadata(&request.metadata, &request.metadata_json).unwrap()
        );
        assert_eq!(ticket_id.len(), 40);
        assert!(
            ticket_id
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
    }
    let temp = tempfile::tempdir().unwrap();
    let mut unbound = repository(temp.path(), false).await;
    assert_eq!(
        create_in(&mut unbound, &titled("T")).await.unwrap_err(),
        IssueFailure::new(78, super::super::NOT_BOUND)
    );
}

/// GEST-T08: a failure after each write step leaves every byte of the repository
/// unchanged and gives 74. The same owner then stores the task.
#[tokio::test]
async fn a_failure_after_each_write_step_changes_no_byte() {
    let temp = tempfile::tempdir().unwrap();
    let mut found = repository(temp.path(), true).await;
    let path = temp.path().join("repo");
    let request = parse(&os(&["T", "-d", "D", "-p", "1", "-m", "k=v"])).unwrap();
    for phase in [
        IssuePhase::Receipt,
        IssuePhase::Content,
        IssuePhase::Tag,
        IssuePhase::Ticket,
        IssuePhase::Event,
        IssuePhase::Typed,
        IssuePhase::Admission,
    ] {
        let files = file_bytes(&path);
        inject_issue_failure(Some(phase));
        let failure = create_in(&mut found, &request).await.unwrap_err();
        inject_issue_failure(None);
        assert_eq!(
            failure,
            IssueFailure::new(74, "injected issue ticket failure"),
            "{phase:?}"
        );
        assert_eq!(file_bytes(&path), files, "{phase:?}");
    }
    let (ticket_id, task) = create_in(&mut found, &request).await.unwrap();
    let stored = found
        .owner
        .read_private_issue(&ticket_id, super::super::limits())
        .await
        .unwrap();
    assert_eq!(stored.record, IssueRecord::Task(task));
}

/// Each of these refuses before the write and changes no byte: bad metadata, a `[`,
/// an artifact just over the task limit, text over that limit, text over the parser
/// ceiling, and a project ID that is not a Gest ID. A task just under the limit is
/// stored, and so is a later small task.
#[tokio::test]
async fn refusals_before_the_write_change_no_byte() {
    let temp = tempfile::tempdir().unwrap();
    let mut found = repository(temp.path(), true).await;
    let path = temp.path().join("repo");
    let large = Request {
        description: "x".repeat(super::super::limits().admission.parser.input_bytes),
        ..titled("T")
    };
    let sized = |bytes: usize| Request {
        description: "x".repeat(bytes),
        ..titled("T")
    };
    let base = crate::task_ticket::task_creation_bytes(
        &create_in(&mut found, &titled("T")).await.unwrap().1,
        &[b'a'; 40],
        b"owner",
        crate::native_date::NativeDate::parse(b"2026-11-20T12:34:56Z").unwrap(),
        65_536,
    )
    .unwrap()
    .len();
    let room = TASK_BYTES - base;
    let files = file_bytes(&path);
    for (request, status) in [
        (parse(&os(&["T", "-m", "bad"])).unwrap(), 64),
        (titled("[x]"), 64),
        (sized(room + 200), 64),
        (sized(TASK_BYTES), 64),
        (large, 64),
    ] {
        let failure = create_in(&mut found, &request).await.unwrap_err();
        assert_eq!(failure.status, status, "{}", failure.message);
        assert!(
            failure.message.starts_with("gossil: issue task create: "),
            "{}",
            failure.message
        );
        assert_eq!(file_bytes(&path), files);
    }
    let project = found.project.replace("bad".to_string());
    let failure = create_in(&mut found, &titled("T")).await.unwrap_err();
    assert_eq!(failure.status, 74, "{}", failure.message);
    assert!(
        failure.message.contains("project_id"),
        "{}",
        failure.message
    );
    assert_eq!(file_bytes(&path), files);
    found.project = project;
    create_in(&mut found, &sized(room - 200)).await.unwrap();
    create_in(&mut found, &titled("small")).await.unwrap();
}

/// Open a second owner of the repository in `dir`, admitted now, as
/// `open_issue_repository` opens it after the locator check.
/// # Arguments
/// * `dir` - The directory that holds `repo` and the checkout root `root`.
/// # Returns
/// The second owner, with the project that the checkout is bound to now.
async fn reopen(dir: PathBuf) -> Result<IssueRepository, IssueFailure> {
    let bounds = crate::vcs_open::limits();
    let mut owner = LocalOwnerRepository::open(dir.join("repo"), bounds)
        .await
        .unwrap();
    let view = owner
        .checkout_view_for_root(&dir.join("root").canonicalize().unwrap(), bounds)
        .await
        .unwrap();
    let project = owner
        .bound_project(&view.checkout_id, super::super::limits())
        .await
        .unwrap();
    Ok(IssueRepository {
        owner,
        view,
        project,
    })
}

/// The race of two commands at once. A second owner opens the repository, and the
/// first owner commits. The second owner checks what it reads in its own
/// transaction, so it needs no second open. Task create writes its task, and init
/// finds the project that the first owner made. An init whose write refuses while
/// no project is bound gives the refusal of the write and stores no project.
#[tokio::test]
async fn an_owner_opened_before_another_commit_needs_no_second_open() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().to_path_buf();
    let mut first = repository(&dir, true).await;
    let mut stale = Some(reopen(dir.clone()).await.unwrap());
    create_in(&mut first, &parse(&[OsString::from("Before")]).unwrap())
        .await
        .unwrap();
    let request = parse(&[OsString::from("After"), OsString::from("-j")]).unwrap();
    let mut opened = 0;
    let text = create_with(
        || {
            opened += 1;
            let given = stale.take();
            let dir = dir.clone();
            async move {
                match given {
                    Some(found) => Ok(found),
                    None => reopen(dir).await,
                }
            }
        },
        &request,
    )
    .await
    .unwrap();
    assert_eq!(opened, 1);
    assert!(text.contains("\"title\": \"After\""), "{text}");

    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().to_path_buf();
    let mut first = Some(repository(&dir, false).await);
    let mut stale = Some(reopen(dir.clone()).await.unwrap());
    let made = super::super::init::init_with(|| std::future::ready(Ok(first.take().unwrap())))
        .await
        .unwrap();
    let mut opened = 0;
    let found = super::super::init::init_with(|| {
        opened += 1;
        let given = stale.take();
        let dir = dir.clone();
        async move {
            match given {
                Some(found) => Ok(found),
                None => reopen(dir).await,
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(opened, 1);
    assert_eq!(found, made);

    // A write that refuses while no project is bound gives its own refusal, and
    // stores nothing.
    let temp = tempfile::tempdir().unwrap();
    let mut other = repository(temp.path(), false).await;
    other.view.checkout_id = "other".into();
    let mut given = Some(other);
    let refused = super::super::init::init_with(|| std::future::ready(Ok(given.take().unwrap())))
        .await
        .unwrap_err();
    assert_eq!(
        (refused.status, refused.message.as_str()),
        (
            74,
            "issue init needs one project creation, a registered checkout with no bound project, and a repository with no project; binding an existing project is not available in this build"
        )
    );
    let reopened = reopen(temp.path().to_path_buf()).await.unwrap();
    assert_eq!(reopened.project, None);
}

/// While another connection holds the write lock, the write waits for its busy
/// timeout and refuses with the real storage text, which the retry takes as a lock
/// timeout. The refusal changes no byte and leaves the owner unusable, so the retry
/// opens a new owner, whose write succeeds after the lock goes.
#[tokio::test]
async fn a_held_write_lock_gives_a_retryable_refusal() {
    let temp = tempfile::tempdir().unwrap();
    let mut found = repository(temp.path(), true).await;
    let path = temp.path().join("repo");
    let db = libsql::Builder::new_local(&path).build().await.unwrap();
    let holder = db.connect().unwrap();
    holder.execute("BEGIN IMMEDIATE", ()).await.unwrap();
    let files = file_bytes(&path);
    let failure = create_in(&mut found, &titled("T")).await.unwrap_err();
    assert_eq!(failure.status, 74, "{}", failure.message);
    assert!(super::super::retryable(&failure), "{}", failure.message);
    assert!(
        failure.message.starts_with("gossil: issue task create: "),
        "{}",
        failure.message
    );
    assert_eq!(file_bytes(&path), files);
    holder.execute("ROLLBACK", ()).await.unwrap();
    drop(holder);
    assert_eq!(
        create_in(&mut found, &titled("T"))
            .await
            .unwrap_err()
            .status,
        74
    );
    let mut fresh = reopen(temp.path().to_path_buf()).await.unwrap();
    create_in(&mut fresh, &titled("T")).await.unwrap();
}

/// When the write uses up the budget of decoded content, the write refuses with 74
/// and changes no byte. The stored tasks still read back, and a write under the
/// usual limits still succeeds. The budget here is halved until a write refuses, so
/// the test pins no task count.
#[tokio::test]
async fn a_used_up_decoded_budget_refuses_cleanly() {
    let temp = tempfile::tempdir().unwrap();
    let mut found = repository(temp.path(), true).await;
    let path = temp.path().join("repo");
    let (ticket_id, task) = create_in(&mut found, &titled("Kept")).await.unwrap();
    let mut limits = super::super::limits();
    let (failure, files) = loop {
        limits.admission.content.max_total_decoded_bytes /= 2;
        assert!(limits.admission.content.max_total_decoded_bytes > 0);
        let files = file_bytes(&path);
        match create_bounded(&mut found, &titled("Next"), limits).await {
            Ok(_) => continue,
            Err(failure) => break (failure, files),
        }
    };
    assert!(
        failure.message.contains("total decoded content limit"),
        "{}",
        failure.message
    );
    assert_eq!(failure.status, 74, "{}", failure.message);
    assert_eq!(file_bytes(&path), files);
    let mut fresh = reopen(temp.path().to_path_buf()).await.unwrap();
    let stored = fresh
        .owner
        .read_private_issue(&ticket_id, super::super::limits())
        .await
        .unwrap();
    assert_eq!(stored.record, IssueRecord::Task(task));
    create_in(&mut fresh, &titled("After")).await.unwrap();
}
