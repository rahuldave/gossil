//! Run `issue task create`, which stores one private task in the bound project of an open checkout.
//!
//! The command follows `src/cli/commands/task/create.rs` of the pinned Gest source
//! (eaf821b). It takes the title as its one positional argument and the options of
//! Gest. The parser follows the rules of `clap`, the parser of arguments that
//! the source uses. An unknown option, a second positional argument, a repeated
//! option that does not repeat, a missing value and a malformed priority or status
//! are usage errors with status 2. A separate value that starts with `-` counts as
//! missing, as `clap` counts it.
//!
//! This build refuses `--assign`, `--batch`, `--iteration`, `--link`, `--phase`,
//! `--tag` and a title read from stdin with status 69. It never reads stdin and
//! never opens an editor, so a missing description is empty. A `[` in the title,
//! the description or the metadata is refused with status 64, because Markdown
//! backlinks are not ported. A task artifact over 65,536 bytes is refused with
//! status 64. The task is private, and the command writes no audit row and no undo
//! history.
use super::{IssueFailure, IssueRepository, open_issue_repository};
use crate::gest_model::primitives::{Id, Priority, TaskStatus};
use crate::gest_model::task::Model;
use crate::gest_task_values::{TaskValueInputs, TaskValues, task_from_values};
use crate::storage::{IssueLimits, PathScope, PrivateIssueRequest};
use serde_json::{Map, Value};
use std::ffi::OsString;

/// The usage line of `task create`, for each usage error.
const USAGE: &str = "Usage: gossil issue task create [OPTIONS] [TITLE]";

/// The largest number of dot-delimited segments in one `--metadata` key, as in Gest's `store::meta`.
const MAX_DEPTH: usize = 32;

/// One option of `task create`, as the pinned Clap declaration gives it.
struct TaskOption {
    /// The long name, without `--`.
    long: &'static str,
    /// The short name, if any.
    short: Option<char>,
    /// True when the option takes a value.
    takes_value: bool,
    /// True when the option may be given more than once.
    repeats: bool,
    /// True when this build refuses the option with status 69.
    refused: bool,
}

/// Build one option of the table.
/// # Arguments
/// * `long` - The long name.
/// * `short` - The short name, if any.
/// * `takes_value` - True when the option takes a value.
/// # Returns
/// An option that may not repeat and that this build accepts.
const fn option(long: &'static str, short: Option<char>, takes_value: bool) -> TaskOption {
    TaskOption {
        long,
        short,
        takes_value,
        repeats: false,
        refused: false,
    }
}

/// Every option of `task create` and the global options, in the order of the Gest help.
const OPTIONS: &[TaskOption] = &[
    TaskOption {
        refused: true,
        ..option("assign", None, true)
    },
    TaskOption {
        refused: true,
        ..option("batch", None, false)
    },
    option("description", Some('d'), true),
    TaskOption {
        refused: true,
        ..option("iteration", Some('i'), true)
    },
    TaskOption {
        repeats: true,
        refused: true,
        ..option("link", Some('l'), true)
    },
    TaskOption {
        repeats: true,
        ..option("metadata", Some('m'), true)
    },
    TaskOption {
        repeats: true,
        ..option("metadata-json", None, true)
    },
    TaskOption {
        refused: true,
        ..option("phase", None, true)
    },
    option("priority", Some('p'), true),
    option("status", Some('s'), true),
    TaskOption {
        repeats: true,
        refused: true,
        ..option("tag", None, true)
    },
    option("json", Some('j'), false),
    option("quiet", Some('q'), false),
    option("raw", Some('r'), false),
    option("no-color", None, false),
    option("no-pager", None, false),
    TaskOption {
        repeats: true,
        ..option("verbose", Some('v'), false)
    },
];

/// The output form of a created task.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Output {
    /// Gest's success message without color.
    #[default]
    Text,
    /// The task in Gest's JSON envelope.
    Json,
    /// The short ID alone.
    Quiet,
}

/// The parsed arguments of one `task create`.
#[derive(Debug, Default, PartialEq)]
struct Request {
    /// The title.
    title: String,
    /// The description, empty when none was given.
    description: String,
    /// The priority from 0 through 4, if one was given.
    priority: Option<u8>,
    /// The initial status, `open` when none was given.
    status: TaskStatus,
    /// Each `--metadata` pair, in order.
    metadata: Vec<String>,
    /// Each `--metadata-json` object, in order.
    metadata_json: Vec<String>,
    /// The output form.
    output: Output,
}

/// The state of the parser between arguments.
#[derive(Default)]
struct Parser {
    /// The request so far, without its title.
    request: Request,
    /// The title, once the positional argument came.
    title: Option<String>,
    /// Each option given, in order.
    seen: Vec<&'static TaskOption>,
    /// True after `--json`.
    json: bool,
    /// True after `--quiet`.
    quiet: bool,
}

/// Build a usage error with status 2.
/// # Arguments
/// * `problem` - What is wrong with the arguments.
/// # Returns
/// The failure, with the usage line.
fn usage(problem: &str) -> IssueFailure {
    IssueFailure::new(2, format!("gossil: issue task create: {problem}\n{USAGE}"))
}

/// Build the usage error of an argument that `task create` does not take.
/// # Arguments
/// * `arg` - The argument as typed.
/// # Returns
/// The failure with status 2.
fn unexpected(arg: &str) -> IssueFailure {
    usage(&format!("unexpected argument \"{arg}\""))
}

/// Parse a priority as Gest's `parse_priority` does.
/// # Arguments
/// * `value` - A name in any case, or a number.
/// # Returns
/// The rank from 0 for critical through 4 for lowest.
/// # Errors
/// Gives the message of a value that is neither a name nor a number from 0 through 4.
fn parse_priority(value: &str) -> Result<u8, String> {
    let rank = match value.parse::<Priority>() {
        Ok(rank) => Some(rank),
        Err(_) => value
            .parse::<u8>()
            .ok()
            .and_then(|n| Priority::try_from(n).ok()),
    };
    rank.map(u8::from).ok_or_else(|| {
        format!("invalid priority \"{value}\" (accepted: 0-4, critical, high, medium, low, lowest)")
    })
}

impl Parser {
    /// Take the value of one option.
    /// # Arguments
    /// * `option` - The option.
    /// * `value` - Its value, or `None` for a flag.
    /// # Returns
    /// Nothing; the parser keeps the value.
    /// # Errors
    /// Refuses a repeated option that does not repeat, and a malformed priority,
    /// status or phase, with status 2.
    fn apply(
        &mut self,
        option: &'static TaskOption,
        value: Option<String>,
    ) -> Result<(), IssueFailure> {
        let name = option.long;
        if !option.repeats && self.seen.iter().any(|seen| seen.long == name) {
            return Err(usage(&format!(
                "\"--{name}\" cannot be used more than once"
            )));
        }
        self.seen.push(option);
        let value = value.unwrap_or_default();
        let invalid = |reason: String| usage(&format!("invalid value for \"--{name}\": {reason}"));
        match name {
            "description" => self.request.description = value,
            "metadata" => self.request.metadata.push(value),
            "metadata-json" => self.request.metadata_json.push(value),
            "priority" => self.request.priority = Some(parse_priority(&value).map_err(invalid)?),
            "status" => self.request.status = value.parse().map_err(invalid)?,
            "phase" => {
                value
                    .parse::<u32>()
                    .map_err(|error| invalid(error.to_string()))?;
            }
            "json" => self.json = true,
            "quiet" => self.quiet = true,
            _ => {}
        }
        Ok(())
    }

    /// Take one positional argument, which is the title.
    /// # Arguments
    /// * `text` - The argument.
    /// # Returns
    /// Nothing; the parser keeps the value.
    /// # Errors
    /// Refuses a second positional argument with status 2.
    fn positional(&mut self, text: &str) -> Result<(), IssueFailure> {
        if self.title.is_some() {
            return Err(unexpected(text));
        }
        self.title = Some(text.to_owned());
        Ok(())
    }

    /// Check the whole set of arguments and give the request.
    /// # Returns
    /// The request.
    /// # Errors
    /// Refuses `--phase` without `--iteration` with status 2. Refuses the first
    /// option that this build does not run, and a missing title, with status 69.
    fn finish(mut self) -> Result<Request, IssueFailure> {
        let given = |name: &str| self.seen.iter().any(|seen| seen.long == name);
        if given("phase") && !given("iteration") {
            return Err(usage("\"--phase\" needs \"--iteration\""));
        }
        if let Some(option) = self.seen.iter().find(|seen| seen.refused) {
            return Err(IssueFailure::new(
                69,
                format!(
                    "gossil: issue task create: --{} is {}",
                    option.long,
                    super::help::UNAVAILABLE
                ),
            ));
        }
        let Some(title) = self.title else {
            return Err(IssueFailure::new(
                69,
                format!(
                    "gossil: issue task create: reading the title from stdin is {}; give TITLE",
                    super::help::UNAVAILABLE
                ),
            ));
        };
        self.request.title = title;
        self.request.output = match (self.json, self.quiet) {
            (true, _) => Output::Json,
            (false, true) => Output::Quiet,
            (false, false) => Output::Text,
        };
        Ok(self.request)
    }
}

/// Take the separate value of an option from the next argument.
/// # Arguments
/// * `words` - The arguments after the option.
/// * `option` - The option that needs the value.
/// # Returns
/// The value.
/// # Errors
/// Refuses a missing value, and a value that starts with `-` other than `-` itself,
/// with status 2. Refuses a value that is not UTF-8 with status 2.
fn next_value<'a>(
    words: &mut impl Iterator<Item = &'a OsString>,
    option: &TaskOption,
) -> Result<String, IssueFailure> {
    match words.next().map(|word| word.to_str()) {
        Some(Some(value)) if value == "-" || !value.starts_with('-') => Ok(value.to_owned()),
        Some(None) => Err(usage("an argument is not valid UTF-8")),
        _ => Err(usage(&format!("\"--{}\" needs a value", option.long))),
    }
}

/// Parse the arguments after `task create`, as Clap parses the pinned declaration.
/// # Arguments
/// * `args` - The arguments after the command name.
/// # Returns
/// The request.
/// # Errors
/// Each usage error gives status 2. Then an option that this build refuses, or a
/// missing title, gives status 69.
fn parse(args: &[OsString]) -> Result<Request, IssueFailure> {
    let mut parser = Parser::default();
    let mut words = args.iter();
    let mut positional_only = false;
    while let Some(word) = words.next() {
        let text = word
            .to_str()
            .ok_or_else(|| usage("an argument is not valid UTF-8"))?;
        if positional_only || text == "-" || !text.starts_with('-') {
            parser.positional(text)?;
        } else if text == "--" {
            positional_only = true;
        } else if let Some(long) = text.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value.to_owned())),
                None => (long, None),
            };
            let option = OPTIONS
                .iter()
                .find(|option| option.long == name)
                .ok_or_else(|| unexpected(text))?;
            let value = match (option.takes_value, inline) {
                (false, Some(_)) => return Err(usage(&format!("\"--{name}\" takes no value"))),
                (false, None) => None,
                (true, Some(value)) => Some(value),
                (true, None) => Some(next_value(&mut words, option)?),
            };
            parser.apply(option, value)?;
        } else {
            let mut rest = &text[1..];
            while let Some(short) = rest.chars().next() {
                rest = &rest[short.len_utf8()..];
                let option = OPTIONS
                    .iter()
                    .find(|option| option.short == Some(short))
                    .ok_or_else(|| unexpected(&format!("-{short}")))?;
                let value = if !option.takes_value {
                    None
                } else if rest.is_empty() {
                    Some(next_value(&mut words, option)?)
                } else {
                    let value = rest.strip_prefix('=').unwrap_or(rest).to_owned();
                    rest = "";
                    Some(value)
                };
                parser.apply(option, value)?;
            }
        }
    }
    parser.finish()
}

/// Build an argument error with status 64.
/// # Arguments
/// * `problem` - What is wrong with a value.
/// # Returns
/// The failure.
fn argument(problem: &str) -> IssueFailure {
    IssueFailure::new(64, format!("gossil: issue task create: {problem}"))
}

/// Parse a metadata value into the most specific JSON scalar, as Gest's `parse_scalar` does.
/// # Arguments
/// * `text` - The value after `=`.
/// # Returns
/// A boolean for `true` or `false`, a number for an `i64` or a finite `f64`, else a string.
fn parse_scalar(text: &str) -> Value {
    match text {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => text
            .parse::<i64>()
            .map(Value::from)
            .ok()
            .or_else(|| {
                text.parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .map(Value::from)
            })
            .unwrap_or_else(|| Value::String(text.to_owned())),
    }
}

/// Store a value under a dot-delimited key path, as Gest's `set_nested` does.
/// # Arguments
/// * `map` - The object at the current depth.
/// * `segments` - The keys of the path from this depth, at least one.
/// * `value` - The value for the last key.
fn set_nested(map: &mut Map<String, Value>, segments: &[&str], value: Value) {
    let (first, rest) = (segments[0], &segments[1..]);
    if rest.is_empty() {
        map.insert(first.to_owned(), value);
        return;
    }
    let entry = map
        .entry(first.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    if let Value::Object(inner) = entry {
        set_nested(inner, rest, value);
    }
}

/// Build the metadata of a new task, as Gest's `build_metadata` does with no base.
/// # Arguments
/// * `pairs` - Each `--metadata` value.
/// * `blobs` - Each `--metadata-json` value.
/// # Returns
/// An empty object when both are empty. Otherwise the pairs, with dot paths and
/// scalar inference, and then each JSON object merged on top, key by key.
/// # Errors
/// Refuses a pair without `=`, a key path of more than 32 segments, a value that is
/// not JSON and JSON that is not an object, with status 64.
fn metadata(pairs: &[String], blobs: &[String]) -> Result<Value, IssueFailure> {
    let mut map = Map::new();
    for pair in pairs {
        let (key, raw) = pair.split_once('=').ok_or_else(|| {
            argument(&format!(
                "invalid --metadata pair (expected key=value): {pair}"
            ))
        })?;
        let segments: Vec<&str> = key.split('.').collect();
        if segments.len() > MAX_DEPTH {
            return Err(argument(&format!("invalid --metadata key path: {key}")));
        }
        set_nested(&mut map, &segments, parse_scalar(raw));
    }
    for blob in blobs {
        let parsed: Value = serde_json::from_str(blob)
            .map_err(|error| argument(&format!("invalid --metadata-json: {error}")))?;
        let Value::Object(object) = parsed else {
            return Err(argument("--metadata-json must be a JSON object"));
        };
        map.extend(object);
    }
    Ok(Value::Object(map))
}

/// Refuse a `[` in a text field, which Markdown backlinks would read.
/// # Arguments
/// * `request` - The parsed request.
/// * `metadata` - The built metadata.
/// # Returns
/// Nothing when no field holds `[`.
/// # Errors
/// Refuses a `[` in the title, the description or the JSON text of the metadata,
/// with status 64. A JSON array in the metadata holds `[` too.
fn refuse_brackets(request: &Request, metadata: &Value) -> Result<(), IssueFailure> {
    let fields = [
        ("title", request.title.clone()),
        ("description", request.description.clone()),
        ("metadata", metadata.to_string()),
    ];
    match fields.iter().find(|(_, text)| text.contains('[')) {
        Some((field, _)) => Err(argument(&format!(
            "the {field} contains \"[\", and this build refuses it, because Markdown backlinks are not ported; a JSON array in the metadata holds \"[\" too"
        ))),
        None => Ok(()),
    }
}

/// The largest size, in bytes, of a task artifact that `task create` stores.
///
/// The cap keeps each task well below the parser ceilings of the one artifact that
/// a write parses, and of the proof of `task show` and `gos v verify`. A write
/// parses only its own artifact, so the cap does not bound the number of tasks.
const TASK_BYTES: usize = 65_536;

/// Refuse a task that is larger than [`TASK_BYTES`].
/// # Arguments
/// * `bytes` - The size of the artifact, or of its text when the text alone is too large.
/// # Returns
/// The failure, with status 64.
fn too_large(bytes: usize) -> IssueFailure {
    IssueFailure::new(
        64,
        format!(
            "gossil: issue task create: the task has at least {bytes} bytes, more than the {TASK_BYTES} that this build stores"
        ),
    )
}

/// Store the task of one request in the bound project, in one transaction.
/// # Arguments
/// * `found` - The admitted repository and checkout.
/// * `request` - The parsed request.
/// # Returns
/// The ticket ID and the stored task.
/// # Errors
/// Refuses a checkout with no bound project with status 78, and bad metadata, a
/// `[` or an artifact over [`TASK_BYTES`] with status 64. A login that is not UTF-8
/// gives 78. A value or storage failure gives 74. Every refusal leaves the repository unchanged.
async fn create_in(
    found: &mut IssueRepository,
    request: &Request,
) -> Result<(String, TaskValues), IssueFailure> {
    create_bounded(found, request, super::limits()).await
}

/// Store the task of one request under explicit storage limits.
/// # Arguments
/// * `found` - The admitted repository and checkout.
/// * `request` - The parsed request.
/// * `limits` - The limits of the write, admission included.
/// # Returns
/// The ticket ID and the stored task.
/// # Errors
/// The refusals of [`create_in`]. A write whose admission uses up a budget of
/// `limits` refuses with 74 and changes no byte. A lock timeout refuses with 74,
/// and its message starts with `gossil: issue task create:`.
async fn create_bounded(
    found: &mut IssueRepository,
    request: &Request,
    limits: IssueLimits,
) -> Result<(String, TaskValues), IssueFailure> {
    let project = found.project_id()?.to_owned();
    let metadata = metadata(&request.metadata, &request.metadata_json)?;
    refuse_brackets(request, &metadata)?;
    let text = request.title.len() + request.description.len() + metadata.to_string().len();
    if text > TASK_BYTES {
        return Err(too_large(text));
    }
    let login = super::login()?;
    let now = super::creation("task create")?;
    let value = |error: &dyn std::fmt::Display| {
        IssueFailure::new(74, format!("gossil: issue task create: {error}"))
    };
    let resolved_at = if request.status.is_terminal() {
        now.stamp.clone()
    } else {
        Value::Null
    };
    let task = task_from_values(TaskValueInputs {
        gest_id: Value::from(Id::new().to_string()),
        project_id: Value::from(project),
        title: Value::from(request.title.clone()),
        priority: request.priority.map_or(Value::Null, Value::from),
        status: Value::from(request.status.to_string()),
        description: Value::from(request.description.clone()),
        assigned_to: Value::Null,
        metadata,
        resolved_at,
        created_at: now.stamp.clone(),
        updated_at: now.stamp,
    })
    .map_err(|error| value(&error))?;
    let artifact = crate::task_ticket::task_creation_bytes(
        &task,
        now.ticket_id.as_bytes(),
        login.as_bytes(),
        now.date,
        limits.admission.parser.input_bytes,
    )
    .map_err(|error| value(&error))?;
    if artifact.len() > TASK_BYTES {
        return Err(too_large(artifact.len()));
    }
    let ticket = found
        .owner
        .create_private_issue(
            PrivateIssueRequest {
                artifact: &artifact,
                scope: PathScope::Private,
            },
            limits,
        )
        .await
        .map_err(|error| {
            let failure = IssueFailure::from_error(&error);
            if failure.message.contains(super::LOCKED) {
                IssueFailure::new(
                    74,
                    format!("gossil: issue task create: {}", failure.message),
                )
            } else {
                failure
            }
        })?;
    Ok((ticket.ticket_id, task))
}

/// The JSON form of a task, as Gest's `Envelope` writes it after `task create`.
#[derive(serde::Serialize)]
struct Envelope<'a> {
    /// The eleven fields of the task, in Gest's order.
    #[serde(flatten)]
    task: &'a Model,
    /// The notes, which a new task does not have.
    notes: &'a [String],
    /// The relationships, which this build does not write.
    relationships: &'a [String],
    /// The tags, which this build does not write.
    tags: &'a [String],
}

/// Format the output of a created task.
/// # Arguments
/// * `task` - The stored task.
/// * `output` - The output form.
/// # Returns
/// The text for stdout, with a final newline.
/// # Errors
/// Gives the error of the JSON serializer.
fn output(task: &TaskValues, output: Output) -> Result<String, serde_json::Error> {
    let model = Model::from(task.clone());
    let short = model.id().short();
    Ok(match output {
        Output::Text => format!("  ✓  created task  {short}\n\n  title  {}\n", model.title()),
        Output::Quiet => format!("{short}\n"),
        Output::Json => {
            let envelope = Envelope {
                task: &model,
                notes: &[],
                relationships: &[],
                tags: &[],
            };
            format!("{}\n", serde_json::to_string_pretty(&envelope)?)
        }
    })
}

/// Map a failure of the JSON serializer to status 65, the code of a serialization error.
/// # Arguments
/// * `error` - The error.
/// # Returns
/// The failure, which says that the task was kept.
fn json_failure(error: impl std::fmt::Display) -> IssueFailure {
    IssueFailure::new(
        65,
        format!("gossil: issue task create kept the task, and JSON output failed: {error}"),
    )
}

/// Open the repository and create the task of one request, and do both once more
/// after a refusal that [`super::retry_locked`] retries.
/// # Arguments
/// * `open` - Opens and admits the repository, each time it is called.
/// * `request` - The parsed request.
/// # Returns
/// The text for stdout.
/// # Errors
/// Each refusal of `open` and of the write, after the retry.
async fn create_with<O, F>(mut open: O, request: &Request) -> Result<String, IssueFailure>
where
    O: FnMut() -> F,
    F: std::future::Future<Output = Result<IssueRepository, IssueFailure>>,
{
    super::retry_locked(|| {
        let opening = open();
        async move {
            let mut found = opening.await?;
            let (_, task) = create_in(&mut found, request).await?;
            output(&task, request.output).map_err(json_failure)
        }
    })
    .await
}

/// Run `issue task create` with the arguments after its name.
/// # Arguments
/// * `args` - The arguments after `create` or `new`.
/// # Returns
/// Status 0 after the output on stdout. Otherwise the status of [`IssueFailure`]
/// after its diagnostic on stderr: 2 for a usage error, 69 for a refused option or
/// a missing title, 64 for bad metadata, a `[` or an artifact over [`TASK_BYTES`],
/// 78 outside an open checkout or before `issue init`, and 74 for a storage or I/O
/// failure. A failed JSON output after the commit gives 65, a failed write to stdout
/// after the commit gives 74, and in both cases the task stays.
pub(super) fn run(args: &[OsString]) -> std::process::ExitCode {
    let result = parse(args)
        .and_then(|request| {
            tokio::runtime::Builder::new_current_thread()
                .build()
                .map_err(|error| IssueFailure::from_error(&error))
                .and_then(|runtime| runtime.block_on(create_with(open_issue_repository, &request)))
        })
        .and_then(|text| {
            use std::io::Write;
            let mut out = std::io::stdout().lock();
            out.write_all(text.as_bytes())
                .and_then(|()| out.flush())
                .map_err(|error| {
                    IssueFailure::new(
                        74,
                        format!(
                            "gossil: issue task create kept the task, and stdout failed: {error}"
                        ),
                    )
                })
        });
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(failure) => failure.exit(),
    }
}

#[cfg(test)]
#[path = "../../tests/support/product_issue_task.rs"]
mod tests;
