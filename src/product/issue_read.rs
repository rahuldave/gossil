//! Parse, select and print the tasks of `issue task list` and `issue task show`.
//!
//! See the [issue read guide](https://rahuldave.com/gossil/issue-read.html) for the
//! output of each form.
//!
//! The parsers and printers are pure functions. The two handlers take a repository
//! that the caller has already opened, read every issue through
//! [`LocalOwnerRepository::list_private_issues`], and write the output. They do not
//! open a repository, read global flags or print a diagnostic. The caller maps each
//! error to its exit status with [`UsageError::exit_code`] or [`ReadError::exit_code`].
//!
//! [`run_list`] and [`run_show`] are that caller for `gos i task list` and
//! `gos i task show`. They parse the arguments, open the repository of the checkout
//! through [`open_issue_repository`], require its bound project, run the handler on
//! stdout and print each failure on stderr. They do the open and the read inside
//! [`retry_stale`], so they try once more after `database is locked` or a stale
//! admission.
//!
//! The JSON output follows the envelopes of the pinned Gest source. The text output
//! is provisional. It is plain and stable, but it is not Gest's styled table.
//!
//! ## Recommendations
//! Keep the selection rules here equal to Gest `repo::task::all` and
//! `repo::resolve::resolve_id`, so that the IDs and filters mean the same thing.
use super::issue::{IssueFailure, limits, open_issue_repository, retry_stale};
use crate::gest_model::primitives::{Id, TaskStatus};
use crate::gest_model::task::Model;
use crate::gest_task_values::TaskValues;
use crate::storage::{IssueLimits, IssueRecord, IssueTicket, LocalOwnerRepository, StorageError};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use std::ffi::OsString;
use std::io::Write;
use std::process::ExitCode;

/// The usage line of `task list`.
const LIST_USAGE: &str =
    "Usage: gossil issue task list [-a|--all] [-s|--status STATUS] [-j|--json]";

/// The usage line of `task show`.
const SHOW_USAGE: &str = "Usage: gossil issue task show [-j|--json] ID";

/// The options that `task list` accepts in this build.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct TaskListOptions {
    /// Show tasks in every status, as `-a` or `--all`.
    pub(crate) all: bool,
    /// Show only tasks in this status, as `-s` or `--status`.
    pub(crate) status: Option<TaskStatus>,
    /// Print JSON, as `-j` or `--json`.
    pub(crate) json: bool,
}

/// The options that `task show` accepts in this build.
#[derive(Debug, PartialEq)]
pub(crate) struct TaskShowOptions {
    /// The task ID or prefix.
    pub(crate) id: String,
    /// Print JSON, as `-j` or `--json`.
    pub(crate) json: bool,
}

/// A refusal of the arguments of `task list` or `task show`.
#[derive(Debug, PartialEq)]
pub(crate) enum UsageError {
    /// The arguments do not parse. The text names the problem and ends with the usage line.
    Usage(String),
    /// The arguments parse, but they use an option that this build does not support.
    Unsupported(String),
}
impl UsageError {
    /// Give the exit status of the refusal.
    /// # Returns
    /// 2 for a parser failure, as Clap gives, and 69 for an option that this build lacks.
    pub(crate) fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_) => 2,
            Self::Unsupported(_) => 69,
        }
    }
}
impl std::fmt::Display for UsageError {
    /// Write the diagnostic.
    /// # Arguments
    /// * `f` - The formatter.
    /// # Returns
    /// The result of the write.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(text) | Self::Unsupported(text) => f.write_str(text),
        }
    }
}

/// A failure to find exactly one task for an ID or prefix.
#[derive(Debug, PartialEq)]
pub(crate) enum LookupError {
    /// The prefix is empty, longer than 32 characters, or has a character outside `k` to `z`.
    InvalidPrefix(String),
    /// No task ID starts with the prefix.
    NotFound(String),
    /// More than one task ID starts with the prefix, with the count of matches.
    Ambiguous(String, usize),
}
impl std::fmt::Display for LookupError {
    /// Write Gest's message for the failure.
    /// # Arguments
    /// * `f` - The formatter.
    /// # Returns
    /// The result of the write.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPrefix(text) => f.write_str(text),
            Self::NotFound(prefix) => write!(f, "not found: id prefix '{prefix}'"),
            Self::Ambiguous(prefix, n) => {
                write!(f, "ambiguous id prefix '{prefix}': matches {n} entities")
            }
        }
    }
}

/// A failure of a `task list` or `task show` handler.
#[derive(Debug)]
pub(crate) enum ReadError {
    /// The task ID or prefix names no task, or more than one.
    Lookup(LookupError),
    /// The repository refused the read, for example because admission failed.
    Storage(StorageError),
    /// The output could not be written.
    Output(std::io::Error),
}
impl ReadError {
    /// Give the exit status of the failure, as CLI-05 maps it.
    /// # Returns
    /// 64 for an invalid prefix, 66 for a missing or ambiguous task, and 74 for a
    /// storage or output failure.
    pub(crate) fn exit_code(&self) -> u8 {
        match self {
            Self::Lookup(LookupError::InvalidPrefix(_)) => 64,
            Self::Lookup(_) => 66,
            Self::Storage(_) | Self::Output(_) => 74,
        }
    }
}
impl std::fmt::Display for ReadError {
    /// Write the diagnostic of the failure.
    /// # Arguments
    /// * `f` - The formatter.
    /// # Returns
    /// The result of the write.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lookup(error) => write!(f, "{error}"),
            Self::Storage(error) => write!(f, "{error}"),
            Self::Output(error) => write!(f, "cannot write the output: {error}"),
        }
    }
}

/// One stored task, with the ticket that holds it and its place in receipt order.
#[derive(Clone, Copy)]
pub(crate) struct Task<'a> {
    /// The ticket that holds the task.
    pub(crate) ticket: &'a IssueTicket,
    /// The task fields.
    pub(crate) values: &'a TaskValues,
    /// The index of the ticket in receipt order.
    order: usize,
}

/// Keep the tasks of a list of tickets, and drop the projects.
/// # Arguments
/// * `tickets` - Every ticket, in receipt order.
/// # Returns
/// Each task, in receipt order.
pub(crate) fn tasks(tickets: &[IssueTicket]) -> Vec<Task<'_>> {
    tickets
        .iter()
        .enumerate()
        .filter_map(|(order, ticket)| match &ticket.record {
            IssueRecord::Task(values) => Some(Task {
                ticket,
                values,
                order,
            }),
            IssueRecord::Project(_) => None,
        })
        .collect()
}

/// Select and order the tasks that `task list` prints, as Gest `repo::task::all` does.
///
/// Without `--all`, a task in a terminal status (`done` or `cancelled`) is left out,
/// also when `--status` names that status. The newest `created_at` comes first. For
/// equal times, the later receipt comes first.
/// # Arguments
/// * `tasks` - Every task, in receipt order.
/// * `project_id` - Keep only the tasks of this project, when given.
/// * `options` - The parsed options.
/// # Returns
/// The selected tasks, in output order.
pub(crate) fn select<'a>(
    tasks: &[Task<'a>],
    project_id: Option<&Id>,
    options: &TaskListOptions,
) -> Vec<Task<'a>> {
    let mut kept: Vec<Task<'a>> = tasks
        .iter()
        .filter(|task| project_id.is_none_or(|id| task.values.project_id == *id))
        .filter(|task| options.all || !task.values.status.is_terminal())
        .filter(|task| {
            options
                .status
                .is_none_or(|status| task.values.status == status)
        })
        .copied()
        .collect();
    kept.sort_by(|a, b| {
        b.values
            .created_at
            .cmp(&a.values.created_at)
            .then(b.order.cmp(&a.order))
    });
    kept
}

/// Find the one task that a full ID or a prefix names, as Gest `resolve_id` does.
///
/// The prefix must have 1 to 32 characters, each from `k` to `z`, so the match is
/// case-sensitive. The tasks in a status that is not terminal are searched first. When
/// none of them matches, every task is searched. Projects are never matched.
/// # Arguments
/// * `tasks` - Every task.
/// * `prefix` - The ID or prefix that the user typed.
/// # Returns
/// The one matching task.
/// # Errors
/// Refuses an invalid prefix, a prefix that matches no task, and a prefix that matches
/// more than one task in the first set where it has a match.
pub(crate) fn find_task<'a>(tasks: &[Task<'a>], prefix: &str) -> Result<Task<'a>, LookupError> {
    Id::validate_prefix(prefix).map_err(LookupError::InvalidPrefix)?;
    let matching = |active: bool| -> Vec<Task<'a>> {
        tasks
            .iter()
            .filter(|task| !active || !task.values.status.is_terminal())
            .filter(|task| task.values.gest_id.to_string().starts_with(prefix))
            .copied()
            .collect()
    };
    for active in [true, false] {
        match matching(active).as_slice() {
            [] => {}
            [task] => return Ok(*task),
            many => return Err(LookupError::Ambiguous(prefix.into(), many.len())),
        }
    }
    Err(LookupError::NotFound(prefix.into()))
}

/// Read the next value of an option that takes one.
/// # Arguments
/// * `attached` - The text after `=` or after the short letter, when the argument has one.
/// * `rest` - The arguments that follow.
/// * `name` - The option, as the diagnostic names it.
/// * `usage` - The usage line of the command.
/// # Returns
/// The value.
/// # Errors
/// Refuses a missing value and a value that is not UTF-8. As in Clap, an argument
/// that follows the option and starts with `-` is not a value, unless it is `-` alone.
/// The value then counts as missing.
fn value(
    attached: Option<String>,
    rest: &mut std::slice::Iter<'_, OsString>,
    name: &str,
    usage: &str,
) -> Result<String, UsageError> {
    if let Some(text) = attached {
        return Ok(text);
    }
    let missing = || UsageError::Usage(format!("a value is required for '{name}'\n{usage}"));
    let next = rest.as_slice().first().ok_or_else(missing)?;
    if next.len() > 1 && next.as_encoded_bytes().starts_with(b"-") {
        return Err(missing());
    }
    rest.next()
        .ok_or_else(missing)?
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| UsageError::Usage(format!("the value of '{name}' is not UTF-8\n{usage}")))
}

/// One option of a command, with its long and short spellings.
struct Spec {
    /// The long name without `--`.
    long: &'static str,
    /// The short letter, if any.
    short: Option<char>,
    /// True when the option takes a value.
    takes_value: bool,
    /// True when this build supports the option.
    supported: bool,
}

/// The options of `task list` at the Gest pin, and the global options that a command accepts.
const LIST_SPECS: &[Spec] = &[
    Spec {
        long: "all",
        short: Some('a'),
        takes_value: false,
        supported: true,
    },
    Spec {
        long: "status",
        short: Some('s'),
        takes_value: true,
        supported: true,
    },
    Spec {
        long: "json",
        short: Some('j'),
        takes_value: false,
        supported: true,
    },
    Spec {
        long: "assigned-to",
        short: None,
        takes_value: true,
        supported: false,
    },
    Spec {
        long: "limit",
        short: None,
        takes_value: true,
        supported: false,
    },
    Spec {
        long: "tag",
        short: Some('t'),
        takes_value: true,
        supported: false,
    },
    Spec {
        long: "quiet",
        short: Some('q'),
        takes_value: false,
        supported: false,
    },
    Spec {
        long: "raw",
        short: Some('r'),
        takes_value: false,
        supported: false,
    },
    Spec {
        long: "no-color",
        short: None,
        takes_value: false,
        supported: true,
    },
    Spec {
        long: "no-pager",
        short: None,
        takes_value: false,
        supported: true,
    },
    Spec {
        long: "verbose",
        short: Some('v'),
        takes_value: false,
        supported: false,
    },
];

/// The options of `task show` at the Gest pin, and the global options that a command accepts.
const SHOW_SPECS: &[Spec] = &[
    Spec {
        long: "json",
        short: Some('j'),
        takes_value: false,
        supported: true,
    },
    Spec {
        long: "quiet",
        short: Some('q'),
        takes_value: false,
        supported: false,
    },
    Spec {
        long: "raw",
        short: Some('r'),
        takes_value: false,
        supported: false,
    },
    Spec {
        long: "no-color",
        short: None,
        takes_value: false,
        supported: true,
    },
    Spec {
        long: "no-pager",
        short: None,
        takes_value: false,
        supported: true,
    },
    Spec {
        long: "verbose",
        short: Some('v'),
        takes_value: false,
        supported: false,
    },
];

/// Split the arguments of one command into option uses and positional arguments.
///
/// It reads long options with a separate or an `=` value, clusters of short options,
/// a short option with an attached value, and `--`, after which every argument is
/// positional. An option that the command lacks is a parser failure. Each option can
/// appear only once, as Clap requires. Only `--verbose` can repeat, because the parser
/// counts it. A repeated short letter fails at that letter, also when the first copy is
/// in an earlier argument. That failure comes before the failure of a later unknown
/// letter in the same cluster. The value of `--limit` must be a whole number. These failures come before the check for an
/// unsupported option.
/// # Arguments
/// * `args` - The arguments after the command name.
/// * `specs` - The options of the command.
/// * `usage` - The usage line of the command.
/// # Returns
/// The option uses and the positional arguments.
/// # Errors
/// Refuses an unknown option, a missing value, a repeated option, a value given to a
/// flag, and text that is not UTF-8.
fn split(args: &[OsString], specs: &'static [Spec], usage: &str) -> Result<Split, UsageError> {
    let (mut used, mut unsupported, mut positional) = (Vec::new(), Vec::new(), Vec::new());
    let mut seen: Vec<&'static str> = Vec::new();
    let fail = |text: String| UsageError::Usage(format!("{text}\n{usage}"));
    let repeated = |long: &str| {
        fail(format!(
            "the argument '--{long}' cannot be used more than once"
        ))
    };
    let mut rest = args.iter();
    let mut options = true;
    while let Some(arg) = rest.next() {
        let text = arg
            .to_str()
            .ok_or_else(|| fail("an argument is not UTF-8".into()))?;
        let mut found: Vec<(&'static Spec, Option<String>)> = Vec::new();
        if !options || text == "-" || !text.starts_with('-') {
            positional.push(text.to_owned());
            continue;
        } else if text == "--" {
            options = false;
            continue;
        } else if let Some(long) = text.strip_prefix("--") {
            let (name, attached) = match long.split_once('=') {
                Some((name, attached)) => (name, Some(attached.to_owned())),
                None => (long, None),
            };
            let spec = specs
                .iter()
                .find(|spec| spec.long == name)
                .ok_or_else(|| fail(format!("unexpected argument '--{name}'")))?;
            if attached.is_some() && !spec.takes_value {
                return Err(fail(format!("unexpected value for '--{name}'")));
            }
            found.push((spec, attached));
        } else {
            for (at, letter) in text[1..].char_indices() {
                let spec = specs
                    .iter()
                    .find(|spec| spec.short == Some(letter))
                    .ok_or_else(|| fail(format!("unexpected argument '-{letter}'")))?;
                // As in Clap, a second copy fails before a later letter of the cluster.
                if spec.long != "verbose"
                    && (seen.contains(&spec.long)
                        || found.iter().any(|(other, _)| other.long == spec.long))
                {
                    return Err(repeated(spec.long));
                }
                if spec.takes_value {
                    let after = &text[1 + at + letter.len_utf8()..];
                    // `-s=` gives an empty value, and `-s` alone takes the next argument.
                    let attached = match after.strip_prefix('=') {
                        Some(value) => Some(value.to_owned()),
                        None => (!after.is_empty()).then(|| after.to_owned()),
                    };
                    found.push((spec, attached));
                    break;
                }
                found.push((spec, None));
            }
        }
        for (spec, attached) in found {
            let given = if spec.takes_value {
                Some(value(
                    attached,
                    &mut rest,
                    &format!("--{}", spec.long),
                    usage,
                )?)
            } else {
                None
            };
            if seen.contains(&spec.long) && spec.long != "verbose" {
                return Err(repeated(spec.long));
            }
            seen.push(spec.long);
            if let ("limit", Some(text)) = (spec.long, &given)
                && let Err(cause) = text.parse::<usize>()
            {
                return Err(fail(format!(
                    "invalid value '{text}' for '--limit <N>': {cause}"
                )));
            }
            if !spec.supported {
                unsupported.push(spec.long);
            } else if !spec.long.starts_with("no-") {
                used.push((spec.long, given));
            }
        }
    }
    Ok(Split {
        used,
        unsupported,
        positional,
    })
}

/// The arguments of one command, split by [`split`].
struct Split {
    /// Each supported option other than `--no-color` and `--no-pager`, with its value.
    used: Vec<(&'static str, Option<String>)>,
    /// The long name of each unsupported option, in argument order.
    unsupported: Vec<&'static str>,
    /// The positional arguments, in order.
    positional: Vec<String>,
}

/// Refuse the unsupported options that the arguments use.
/// # Arguments
/// * `command` - The command, such as `task list`.
/// * `unsupported` - The long name of each unsupported option, in argument order.
/// # Returns
/// Success when the list is empty.
/// # Errors
/// Names the first unsupported option.
fn refuse(command: &str, unsupported: &[&str]) -> Result<(), UsageError> {
    match unsupported.first() {
        Some(name) => Err(UsageError::Unsupported(format!(
            "gossil: issue {command} option --{name} is {}",
            super::help::UNAVAILABLE
        ))),
        None => Ok(()),
    }
}

/// Parse the arguments of `task list`.
/// # Arguments
/// * `args` - The arguments after `task list`.
/// # Returns
/// The options.
/// # Errors
/// Refuses unknown, repeated or malformed options, an invalid status, and every
/// positional argument as parser failures. After that, refuses `--assigned-to`,
/// `--limit`, `--tag`, `--quiet`, `--raw` and `--verbose` as unsupported.
pub(crate) fn parse_task_list_args(args: &[OsString]) -> Result<TaskListOptions, UsageError> {
    let Split {
        used,
        unsupported,
        positional,
    } = split(args, LIST_SPECS, LIST_USAGE)?;
    if let Some(extra) = positional.first() {
        return Err(UsageError::Usage(format!(
            "unexpected argument '{extra}'\n{LIST_USAGE}"
        )));
    }
    let mut options = TaskListOptions::default();
    for (name, given) in used {
        match (name, given) {
            ("status", Some(text)) => {
                options.status = Some(text.parse().map_err(|cause| {
                    UsageError::Usage(format!(
                        "invalid value '{text}' for '--status <STATUS>': {cause}\n{LIST_USAGE}"
                    ))
                })?);
            }
            ("all", _) => options.all = true,
            _ => options.json = true,
        }
    }
    refuse("task list", &unsupported)?;
    Ok(options)
}

/// Parse the arguments of `task show`.
/// # Arguments
/// * `args` - The arguments after `task show`.
/// # Returns
/// The options and the ID or prefix.
/// # Errors
/// Refuses unknown, repeated or malformed options, a missing ID and more than one
/// positional argument as parser failures. After that, refuses `--quiet`, `--raw` and
/// `--verbose` as unsupported.
pub(crate) fn parse_task_show_args(args: &[OsString]) -> Result<TaskShowOptions, UsageError> {
    let Split {
        used,
        unsupported,
        positional,
    } = split(args, SHOW_SPECS, SHOW_USAGE)?;
    let id = match positional.as_slice() {
        [id] => id.clone(),
        [] => {
            return Err(UsageError::Usage(format!(
                "the required argument ID was not provided\n{SHOW_USAGE}"
            )));
        }
        [_, extra, ..] => {
            return Err(UsageError::Usage(format!(
                "unexpected argument '{extra}'\n{SHOW_USAGE}"
            )));
        }
    };
    refuse("task show", &unsupported)?;
    Ok(TaskShowOptions {
        id,
        json: !used.is_empty(),
    })
}

/// The JSON envelope of one task, with the field order of Gest `ui::envelope::Envelope`.
#[derive(Serialize)]
struct Envelope {
    /// The eleven task fields, in the order of the Gest model.
    #[serde(flatten)]
    entity: Model,
    /// The notes, which `task show` prints and `task list` leaves out. This build has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<[(); 0]>,
    /// The relationships. This build has none.
    relationships: [(); 0],
    /// The tag labels. This build has none.
    tags: [(); 0],
}
impl Envelope {
    /// Build the envelope of one task.
    /// # Arguments
    /// * `task` - The task.
    /// * `notes` - True for `task show`, which prints an empty `notes` list.
    /// # Returns
    /// The envelope.
    fn new(task: &Task<'_>, notes: bool) -> Self {
        Self {
            entity: Model::from(task.values.clone()),
            notes: notes.then_some([]),
            relationships: [],
            tags: [],
        }
    }
}

/// Print the JSON of `task list` as a pretty array of envelopes and a newline.
/// # Arguments
/// * `tasks` - The selected tasks, in output order.
/// # Returns
/// The output bytes. An empty list prints `[]`.
pub(crate) fn task_list_json(tasks: &[Task<'_>]) -> Vec<u8> {
    let envelopes: Vec<Envelope> = tasks
        .iter()
        .map(|task| Envelope::new(task, false))
        .collect();
    json_bytes(&envelopes)
}

/// Print the JSON of `task show` as one pretty envelope with empty notes and a newline.
/// # Arguments
/// * `task` - The task.
/// # Returns
/// The output bytes.
pub(crate) fn task_show_json(task: &Task<'_>) -> Vec<u8> {
    json_bytes(&Envelope::new(task, true))
}

/// Serialize a value as pretty JSON with a final newline, as Gest `println!` prints it.
/// # Arguments
/// * `value` - Envelopes, whose fields serialize without failure.
/// # Returns
/// The bytes.
fn json_bytes(value: &impl Serialize) -> Vec<u8> {
    let mut out = serde_json::to_vec_pretty(value).unwrap_or_default();
    out.push(b'\n');
    out
}

/// Escape each control character, so that one field cannot break the plain layout.
/// # Arguments
/// * `text` - A field value.
/// * `keep_newlines` - True to keep each `\n` as a line break.
/// # Returns
/// The text with each other control character written as a Rust escape, such as `\t`.
fn plain(text: &str, keep_newlines: bool) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() && !(keep_newlines && c == '\n') {
                c.escape_default().to_string()
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// Write a timestamp as Gest's JSON writes it.
/// # Arguments
/// * `time` - The UTC time.
/// # Returns
/// RFC 3339 with `Z` and only the fractional digits that the time needs.
fn timestamp(time: &DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// Print the provisional text of `task list`.
///
/// Each task gives one line, with the 8-letter short ID, the status padded to 11
/// characters, the priority as `P0` to `P255` or `--`, and the title. Two spaces
/// separate the fields. An empty list prints nothing.
/// # Arguments
/// * `tasks` - The selected tasks, in output order.
/// # Returns
/// The output bytes.
pub(crate) fn task_list_text(tasks: &[Task<'_>]) -> Vec<u8> {
    let mut out = String::new();
    for task in tasks {
        let values = task.values;
        let priority = values
            .priority
            .map_or_else(|| "--".to_owned(), |p| format!("P{p}"));
        out.push_str(&format!(
            "{}  {:<11}  {:<4}  {}\n",
            values.gest_id.short(),
            values.status.to_string(),
            priority,
            plain(&values.title, false)
        ));
    }
    out.into_bytes()
}

/// Print the provisional text of `task show`, with one labelled line for each stored field.
///
/// The description comes last, after a `description` line, with each of its lines
/// indented by four spaces. An empty description prints no lines after its label.
/// # Arguments
/// * `task` - The task.
/// # Returns
/// The output bytes.
pub(crate) fn task_show_text(task: &Task<'_>) -> Vec<u8> {
    let values = task.values;
    let none = || "none".to_owned();
    let rows = [
        ("id", values.gest_id.to_string()),
        ("title", plain(&values.title, false)),
        ("status", values.status.to_string()),
        (
            "priority",
            values.priority.map_or_else(none, |p| p.to_string()),
        ),
        ("project", values.project_id.to_string()),
        (
            "assigned to",
            values.assigned_to.as_ref().map_or_else(none, Id::to_string),
        ),
        ("metadata", plain(&values.metadata.to_string(), false)),
        ("created at", timestamp(&values.created_at)),
        ("updated at", timestamp(&values.updated_at)),
        (
            "resolved at",
            values.resolved_at.as_ref().map_or_else(none, timestamp),
        ),
        ("ticket", task.ticket.ticket_id.clone()),
        ("artifact", task.ticket.native_name.clone()),
    ];
    let mut out = String::new();
    for (label, value) in rows {
        out.push_str(&format!("{label:<12} {value}\n"));
    }
    out.push_str("description\n");
    for line in plain(&values.description, true).lines() {
        out.push_str(&format!("    {line}\n"));
    }
    out.into_bytes()
}

/// Run `task list` on an open repository.
/// # Arguments
/// * `repo` - The admitted owner repository.
/// * `project_id` - List only the tasks of this project, when given.
/// * `options` - The parsed options.
/// * `limits` - The ceilings of the list call.
/// * `out` - Where the output goes.
/// # Returns
/// Success after the whole output is written.
/// # Errors
/// Refuses a failed list call and a failed write.
pub(crate) async fn task_list(
    repo: &mut LocalOwnerRepository,
    project_id: Option<&Id>,
    options: &TaskListOptions,
    limits: IssueLimits,
    out: &mut impl Write,
) -> Result<(), ReadError> {
    let tickets = repo
        .list_private_issues(limits)
        .await
        .map_err(ReadError::Storage)?;
    let selected = select(&tasks(&tickets), project_id, options);
    let bytes = if options.json {
        task_list_json(&selected)
    } else {
        task_list_text(&selected)
    };
    out.write_all(&bytes).map_err(ReadError::Output)
}

/// Run `task show` on an open repository.
///
/// As in Gest, the ID or prefix is looked up among the tasks of every project.
/// # Arguments
/// * `repo` - The admitted owner repository.
/// * `options` - The parsed options.
/// * `limits` - The ceilings of the list call.
/// * `out` - Where the output goes.
/// # Returns
/// Success after the whole output is written.
/// # Errors
/// Refuses a failed list call, an invalid, missing or ambiguous ID, and a failed write.
pub(crate) async fn task_show(
    repo: &mut LocalOwnerRepository,
    options: &TaskShowOptions,
    limits: IssueLimits,
    out: &mut impl Write,
) -> Result<(), ReadError> {
    let tickets = repo
        .list_private_issues(limits)
        .await
        .map_err(ReadError::Storage)?;
    let task = find_task(&tasks(&tickets), &options.id).map_err(ReadError::Lookup)?;
    let bytes = if options.json {
        task_show_json(&task)
    } else {
        task_show_text(&task)
    };
    out.write_all(&bytes).map_err(ReadError::Output)
}

/// Run `issue task list` with the arguments after its name.
///
/// The command lists the tasks of the project bound to the open checkout. It opens and
/// reads the repository once more after a refusal that [`retry_stale`] retries.
/// # Arguments
/// * `args` - The arguments after `task list`.
/// # Returns
/// Status 0 after the output on stdout. Otherwise the status of the failure after its
/// diagnostic on stderr: 2 or 69 for the arguments, 78 outside an open checkout or
/// before `issue init`, and 74 for a storage failure or a failed write to stdout. A
/// lock timeout or a stale admission gives 74 only when the second attempt refuses too.
pub(super) fn run_list(args: &[OsString]) -> ExitCode {
    let result = parse_task_list_args(args)
        .map_err(usage_failure)
        .and_then(|options| {
            block_on(retry_stale(|| async {
                let mut found = open_issue_repository().await?;
                let project = Id::from(found.project_id()?);
                let mut out = std::io::stdout().lock();
                task_list(
                    &mut found.owner,
                    Some(&project),
                    &options,
                    limits(),
                    &mut out,
                )
                .await
                .map_err(read_failure)
            }))
        });
    finish(result)
}

/// Run `issue task show` with the arguments after its name.
///
/// As in Gest, the command needs a bound project, and it finds the task among the
/// tasks of every project. It opens and reads the repository once more after a
/// refusal that [`retry_stale`] retries.
/// # Arguments
/// * `args` - The arguments after `task show`.
/// # Returns
/// Status 0 after the output on stdout. Otherwise the status of the failure after its
/// diagnostic on stderr: 2 or 69 for the arguments, 64 for an invalid prefix, 66 for
/// a missing or ambiguous task, 78 outside an open checkout or before `issue init`,
/// and 74 for a storage failure or a failed write to stdout. A lock timeout or a
/// stale admission gives 74 only when the second attempt refuses too.
pub(super) fn run_show(args: &[OsString]) -> ExitCode {
    let result = parse_task_show_args(args)
        .map_err(usage_failure)
        .and_then(|options| {
            block_on(retry_stale(|| async {
                let mut found = open_issue_repository().await?;
                found.project_id()?;
                let mut out = std::io::stdout().lock();
                task_show(&mut found.owner, &options, limits(), &mut out)
                    .await
                    .map_err(read_failure)
            }))
        });
    finish(result)
}

/// Turn a refusal of the arguments into the failure of the command.
/// # Arguments
/// * `error` - The refusal.
/// # Returns
/// Status 2 or 69. A parser failure gets the prefix `gossil: `. The message of an
/// unsupported option already has it.
fn usage_failure(error: UsageError) -> IssueFailure {
    let message = match &error {
        UsageError::Usage(text) => format!("gossil: {text}"),
        UsageError::Unsupported(text) => text.clone(),
    };
    IssueFailure::new(error.exit_code(), message)
}

/// Turn a failure of a handler into the failure of the command.
/// # Arguments
/// * `error` - The failure.
/// # Returns
/// Status 64, 66 or 74. A storage error prints as it is, as `issue init` prints it.
/// Every other failure gets the prefix `gossil: `.
fn read_failure(error: ReadError) -> IssueFailure {
    let message = match &error {
        ReadError::Storage(cause) => cause.to_string(),
        other => format!("gossil: {other}"),
    };
    IssueFailure::new(error.exit_code(), message)
}

/// Run one command on a runtime of the current thread.
/// # Arguments
/// * `command` - The command.
/// # Returns
/// The result of the command, or a status 74 failure when no runtime starts.
fn block_on(
    command: impl std::future::Future<Output = Result<(), IssueFailure>>,
) -> Result<(), IssueFailure> {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|error| IssueFailure::from_error(&error))
        .and_then(|runtime| runtime.block_on(command))
}

/// Give the exit status of a command, after the diagnostic of a failure on stderr.
/// # Arguments
/// * `result` - The result of the command.
/// # Returns
/// Status 0, or the status of the failure.
fn finish(result: Result<(), IssueFailure>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => failure.exit(),
    }
}

#[cfg(test)]
#[path = "../../tests/support/product_issue_read.rs"]
mod tests;
