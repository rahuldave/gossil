//! Route the commands of the `issue` group, open the repository of an issue command,
//! and refuse each command that this build does not run.
//!
//! The parser walks the command names of the table in `help.rs`, with the
//! aliases of the pinned Gest source. It checks the names only. The arguments
//! after the last name go to the handler unchecked. The handlers of `init`,
//! `task create`, `task list` and `task show` run. Every other handler of this
//! build refuses with status 69 before it reads or writes a file.
//!
//! [`open_issue_repository`] finds the repository of an issue command through the
//! checkout locator that `gos v open` published, as GEST-ROOT-01 asks. Each
//! handler that reads or writes issues calls it, inside [`retry_stale`].
use super::help::{self, IssueCommand, IssueHandler};
use crate::native_date::NativeDate;
use crate::storage::{CheckoutView, IssueLimits, LocalOwnerRepository};
use std::ffi::{OsStr, OsString};

/// Run `issue init`. The file is `src/product/issue_init.rs`.
#[path = "issue_init.rs"]
mod init;

/// Run `issue task create`. The file is `src/product/issue_task.rs`.
#[path = "issue_task.rs"]
mod task;

/// The refusal of an issue command outside an open checkout.
pub(crate) const NOT_OPEN: &str =
    "gossil: no open checkout holds this directory; run gos v init and gos v open first";

/// The refusal of an issue command on a checkout that has no bound project.
pub(crate) const NOT_BOUND: &str =
    "gossil: this checkout has no Gest project; run gos i init first";

/// A failed issue command, with its diagnostic and its exit status.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct IssueFailure {
    /// The sysexits status of CLI-05, such as 78 for an uninitialized project.
    pub(crate) status: u8,
    /// The diagnostic for stderr, without a final newline.
    pub(crate) message: String,
}

impl IssueFailure {
    /// Build a failure from a status and a diagnostic.
    /// # Arguments
    /// * `status` - The exit status.
    /// * `message` - The diagnostic.
    /// # Returns
    /// The failure.
    pub(crate) fn new(status: u8, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    /// Map an error of the locator, the storage boundary or the file system to a CLI-05 status.
    ///
    /// An older Gossil catalog anywhere in the chain gives 78 (CAT-07). A locator
    /// error gives its own category, 78 or 74. A storage or I/O error in the chain
    /// gives 74. Every other error is a refusal of the selection policy, such as a
    /// configured data directory that conflicts with the locator, and gives 78.
    /// # Arguments
    /// * `error` - The failure.
    /// # Returns
    /// The failure with the error's text and its status.
    pub(crate) fn from_error(error: &(dyn std::error::Error + 'static)) -> Self {
        let chain = || std::iter::successors(Some(error), |current| current.source());
        let status = if chain().any(|e| e.is::<crate::storage::OlderCatalog>()) {
            78
        } else if let Some(locator) = error.downcast_ref::<crate::checkout_locator::Error>() {
            locator.code()
        } else if chain()
            .any(|e| e.is::<crate::storage::StorageError>() || e.is::<std::io::Error>())
        {
            74
        } else {
            78
        };
        Self::new(status, error.to_string())
    }

    /// Write the diagnostic to stderr and return the exit status.
    /// # Returns
    /// The exit status of the failure.
    pub(crate) fn exit(&self) -> std::process::ExitCode {
        eprintln!("{}", self.message);
        std::process::ExitCode::from(self.status)
    }
}

/// The repository, checkout and bound project of one issue command, after full admission.
pub(crate) struct IssueRepository {
    /// The admitted owner of the repository.
    pub(crate) owner: LocalOwnerRepository,
    /// The registered checkout that holds the current directory.
    pub(crate) view: CheckoutView,
    /// The Gest ID of the project bound to the checkout, or `None` before `issue init`.
    pub(crate) project: Option<String>,
}

impl IssueRepository {
    /// Return the Gest ID of the bound project, for a command that needs one.
    /// # Returns
    /// The 32-letter Gest ID of the project.
    /// # Errors
    /// Refuses a checkout without a bound project with status 78 and [`NOT_BOUND`].
    pub(crate) fn project_id(&self) -> Result<&str, IssueFailure> {
        self.project
            .as_deref()
            .ok_or_else(|| IssueFailure::new(78, NOT_BOUND))
    }
}

/// Give the finite ceilings of one issue command.
/// # Returns
/// The admission ceilings of the `vcs` commands, and SQL ceilings for the call.
pub(crate) fn limits() -> IssueLimits {
    IssueLimits {
        admission: crate::vcs_open::limits(),
        sql_rows: 100_000,
        sql_bytes: 64 << 20,
    }
}

/// The storage refusal of a write whose admission proof another process made stale.
const STALE: &str = "repository admission stale; reopen with explicit admission limits";

/// The text inside the storage refusal of a write that waited for the write lock past its timeout.
const LOCKED: &str = "database is locked";

/// Say whether a failure is one that a second attempt may get past.
/// # Arguments
/// * `failure` - The failure of one attempt.
/// # Returns
/// True for status 74 with the stale refusal [`STALE`], or with a message that holds
/// [`LOCKED`]. Both refusals come before the commit, so the attempt wrote nothing.
fn retryable(failure: &IssueFailure) -> bool {
    failure.status == 74 && (failure.message == STALE || failure.message.contains(LOCKED))
}

/// Run one attempt of an issue command, and run it once more after a stale admission or a lock timeout.
///
/// Two commands at once can race. The second one admits the repository, and then the
/// first one commits. The write of the second one then refuses with [`STALE`] and
/// writes nothing. With more commands at once, a write can also wait for the write
/// lock past its timeout and refuse with [`LOCKED`]. A read of `task list` or
/// `task show` can meet the same two refusals while other commands write. A second
/// attempt opens and admits the repository again.
///
/// # Arguments
/// * `attempt` - Opens the repository and runs the command, each time it is called.
/// # Returns
/// The result of the first attempt, or of the second one after one of these refusals.
/// # Errors
/// Gives the failure of the first attempt when it is not one of these refusals, and
/// the failure of the second attempt otherwise.
pub(crate) async fn retry_stale<T, F, A>(mut attempt: A) -> Result<T, IssueFailure>
where
    A: FnMut() -> F,
    F: std::future::Future<Output = Result<T, IssueFailure>>,
{
    match attempt().await {
        Err(failure) if retryable(&failure) => attempt().await,
        result => result,
    }
}

/// Return the local login, for the U card of a new ticket.
/// # Returns
/// The first nonempty value of `GOSSIL_USER`, `USER`, `LOGNAME` and `USERNAME`, or `root`.
/// # Errors
/// Refuses a login that is not UTF-8 with status 78.
pub(crate) fn login() -> Result<String, IssueFailure> {
    String::from_utf8(crate::vcs_init::default_administrator())
        .map_err(|_| IssueFailure::new(78, "gossil: the local login is not UTF-8; set GOSSIL_USER"))
}

/// The clock and the ticket ID of one new ticket.
pub(crate) struct Creation {
    /// The current instant, for the D card.
    pub(crate) date: NativeDate,
    /// The same instant as a JSON RFC3339 string, for the timestamps of the record.
    pub(crate) stamp: serde_json::Value,
    /// Forty random lowercase hexadecimal digits, for the K card.
    pub(crate) ticket_id: String,
}

/// Read the clock and draw a random ticket ID for a new ticket.
/// # Arguments
/// * `command` - The command path, such as `init`, for the diagnostic.
/// # Returns
/// The instant, its RFC3339 string and the ticket ID.
/// # Errors
/// Refuses with status 74 when the clock is before 1970 or out of range, or when no
/// random bytes come.
pub(crate) fn creation(command: &str) -> Result<Creation, IssueFailure> {
    let value = |what: &str| IssueFailure::new(74, format!("gossil: issue {command}: {what}"));
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| value("the clock is before 1970"))?
        .as_nanos();
    let date = NativeDate::from_epoch_nanos(nanos as i128)
        .map_err(|_| value("the clock is out of range"))?;
    let mut bytes = [0u8; 20];
    getrandom::fill(&mut bytes).map_err(|_| value("the system gave no random bytes"))?;
    Ok(Creation {
        date,
        stamp: serde_json::json!(format!("{date}Z")),
        ticket_id: bytes.iter().map(|b| format!("{b:02x}")).collect(),
    })
}

/// Open the repository of an issue command through the checkout locator.
///
/// The command takes no repository or checkout selection. The nearest `.gossil`
/// marker at or above the current directory must hold the locator that
/// `gos v open` published. The locator pins the repository and the checkout, and
/// [`crate::vcs_open::admit_existing`] then checks them with the same rules as the
/// `vcs` commands, configuration included. The repository passes full admission,
/// and the call reads the project bound to the checkout. It writes nothing.
/// # Returns
/// The admitted owner, the checkout view and the bound project, if any.
/// # Errors
/// Refuses a directory outside an open checkout with status 78 and [`NOT_OPEN`].
/// Maps every other failure with [`IssueFailure::from_error`].
pub(crate) async fn open_issue_repository() -> Result<IssueRepository, IssueFailure> {
    let fail = |error: Box<dyn std::error::Error>| IssueFailure::from_error(&*error);
    let cwd = std::env::current_dir()
        .and_then(std::fs::canonicalize)
        .map_err(|error| fail(error.into()))?;
    let Some((_, Some(locator))) = crate::vcs_open::marker(&cwd).map_err(fail)? else {
        return Err(IssueFailure::new(78, NOT_OPEN));
    };
    let repository = locator.path().map_err(|error| fail(error.into()))?;
    let (mut owner, view) =
        crate::vcs_open::admit_existing(repository, locator.checkout_id.clone())
            .await
            .map_err(fail)?;
    let project = owner
        .bound_project(&view.checkout_id, limits())
        .await
        .map_err(|error| fail(error.into()))?;
    Ok(IssueRepository {
        owner,
        view,
        project,
    })
}

/// Holds the decision of the issue parser.
#[derive(Debug, PartialEq)]
pub(super) enum IssueRoute {
    /// Write this help text to stdout and exit 0.
    Print(String),
    /// Write this diagnostic to stderr and exit 2, as a parser failure.
    Fail(String),
    /// Run the handler of a recognised command.
    Run {
        /// The handler that the table names for the command.
        handler: IssueHandler,
        /// The canonical names from the group down to the command, such as `task create`.
        path: String,
        /// The arguments after the last command name.
        args: Vec<OsString>,
    },
}

/// Parse the arguments after `issue` or `i` into help, a failure or a command.
///
/// Help words (`--help`, `-h` and `help`) before a command name ask for the help
/// of the command they lead to, as in `help task create` or `task --help create`.
/// After the last name, a `--help`, a `-h` or a cluster of short flags that reaches
/// `h`, such as `-ah`, before any `--` asks for that help too. The route checks each
/// argument on its own, by the rules that Clap applies to one cluster. Clap also stops
/// at a letter that repeats a flag of an earlier argument, such as the second `a` in
/// `-a -ah`. The route still prints the help there. The word `help`
/// is an ordinary argument after the last name. The group alone prints the group
/// help (CLI-04). An unknown name, or a command that needs a subcommand and gets
/// none, is a parser failure.
///
/// # Arguments
/// * `args` - The arguments after the group name.
/// * `asked` - True when help words came before the group name.
///
/// # Returns
/// The help to print, the diagnostic to fail with, or the handler to run.
pub(super) fn route(args: &[OsString], asked: bool) -> IssueRoute {
    let mut asked = asked;
    let mut level = help::ISSUE_COMMANDS;
    let mut found: Option<&IssueCommand> = None;
    let mut path: Vec<&str> = Vec::new();
    let mut rest = args;
    while !level.is_empty() {
        let words = rest.iter().take_while(|arg| help::is_help(arg)).count();
        asked |= words > 0;
        rest = &rest[words..];
        let Some(name) = rest.first() else { break };
        let Some(command) = help::find_issue(level, name) else {
            if found.is_some_and(|command| command.handler.is_some()) && !asked {
                break;
            }
            return IssueRoute::Fail(help::unknown_issue(&path.join(" "), name));
        };
        found = Some(command);
        path.push(command.names[0]);
        level = command.children;
        rest = &rest[1..];
    }
    let path = path.join(" ");
    let Some(command) = found else {
        return IssueRoute::Print(help::issue());
    };
    if asked {
        return if rest.is_empty() {
            IssueRoute::Print(help::issue_command(&path, command))
        } else {
            IssueRoute::Fail(
                "Usage: gossil issue help COMMAND\n\
                 Run \"gossil issue --help\" to list the issue commands."
                    .into(),
            )
        };
    }
    if rest
        .iter()
        .take_while(|arg| *arg != "--")
        .any(|arg| asks_help(arg, command.handler.map_or("", short_flags)))
    {
        return IssueRoute::Print(help::issue_command(&path, command));
    }
    match command.handler {
        Some(handler) => IssueRoute::Run {
            handler,
            path,
            args: rest.to_vec(),
        },
        None => IssueRoute::Fail(help::issue_needs_command(&path)),
    }
}

/// Say whether one argument after the last command name asks for help, as Clap reads it.
///
/// The check sees this one argument only. So a letter that repeats a flag of an
/// earlier argument, such as the second `a` in `-a -ah`, does not stop it.
///
/// Clap reads a cluster such as `-ajh` letter by letter, and stops at the first letter
/// that is not a flag that takes no value. When that letter is `h`, the cluster asks for
/// help. A letter of an option with a value takes the rest of the cluster as its value,
/// as in `-sh`, and an unknown letter is a parser failure. So is a second copy of a
/// flag, as in `-aah`, except the counted `-v`.
/// # Arguments
/// * `arg` - One argument after the last command name.
/// * `flags` - The short letters of the command's flags that take no value.
/// # Returns
/// True for `--help`, `-h` and a cluster that reaches `h` through `flags` only.
fn asks_help(arg: &OsStr, flags: &str) -> bool {
    help::is_help_flag(arg)
        || arg
            .to_str()
            .and_then(|text| text.strip_prefix('-'))
            .is_some_and(|cluster| {
                let mut seen = String::new();
                let stop = cluster.chars().find(|c| {
                    let repeated = *c != 'v' && seen.contains(*c);
                    seen.push(*c);
                    repeated || !flags.contains(*c)
                });
                stop == Some('h')
            })
}

/// Give the short letters of the flags of a command that take no value, at the Gest pin.
///
/// The global `-v` counts for each command. A command without a planned handler,
/// such as `undo`, gets no letters, so only `-h` and `--help` print its help.
/// # Arguments
/// * `handler` - The handler of the command.
/// # Returns
/// The letters, such as `ajqrv` for `task list`.
fn short_flags(handler: IssueHandler) -> &'static str {
    match handler {
        IssueHandler::Init => "v",
        IssueHandler::TaskList => "ajqrv",
        IssueHandler::TaskCreate | IssueHandler::TaskShow => "jqrv",
        IssueHandler::Unavailable => "",
    }
}

/// Run the handler of a recognised issue command.
///
/// `init`, `task create`, `task list` and `task show` run, and each other handler
/// refuses with status 69.
///
/// # Arguments
/// * `handler` - The handler that the table names.
/// * `path` - The canonical command path, for the diagnostic.
/// * `args` - The arguments after the last command name.
///
/// # Returns
/// The status of the command that runs, or status 69 after the refusal on stderr.
pub(super) fn run(handler: IssueHandler, path: &str, args: &[OsString]) -> std::process::ExitCode {
    match handler {
        IssueHandler::Init => init::run(args),
        IssueHandler::TaskCreate => task::run(args),
        IssueHandler::TaskList => super::issue_read::run_list(args),
        IssueHandler::TaskShow => super::issue_read::run_show(args),
        IssueHandler::Unavailable => refuse(path),
    }
}

/// Refuse a recognised command that this build does not run.
///
/// # Arguments
/// * `path` - The canonical command path, such as `task create`.
///
/// # Returns
/// Status 69, the code of an invalid state (CLI-05), after one diagnostic line on stderr.
fn refuse(path: &str) -> std::process::ExitCode {
    eprintln!("gossil: issue {path} is {}", help::UNAVAILABLE);
    std::process::ExitCode::from(69)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Turn string slices into the argument vector that `route` receives.
    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    /// Find a command of the table by its canonical path.
    fn at(path: &[&str]) -> &'static IssueCommand {
        let mut level = help::ISSUE_COMMANDS;
        let mut found = None;
        for name in path {
            let command = help::find_issue(level, std::ffi::OsStr::new(name)).unwrap();
            level = command.children;
            found = Some(command);
        }
        found.unwrap()
    }

    /// The help of one command, by its canonical path.
    fn help_of(path: &[&str]) -> IssueRoute {
        IssueRoute::Print(help::issue_command(&path.join(" "), at(path)))
    }

    /// Each help form gives the help of the command it leads to.
    #[test]
    fn help_words_lead_to_the_help_of_a_command() {
        for (args, asked) in [
            (&[][..], false),
            (&["help"], false),
            (&["--help", "-h"], false),
            (&[], true),
        ] {
            assert_eq!(route(&os(args), asked), IssueRoute::Print(help::issue()));
        }
        let create = help_of(&["task", "create"]);
        for (args, asked) in [
            (&["task", "create", "--help"][..], false),
            (&["t", "new", "x", "-h"], false),
            (&["help", "task", "create"], false),
            (&["task", "--help", "new"], false),
            (&["task", "create"], true),
        ] {
            assert_eq!(route(&os(args), asked), create, "{args:?}");
        }
        assert_eq!(route(&os(&["task", "help"]), false), help_of(&["task"]));
        assert_eq!(
            route(&os(&["help", "t", "meta", "help", "delete"]), false),
            help_of(&["task", "meta", "unset"])
        );
        assert_eq!(route(&os(&["p", "-h"]), false), help_of(&["project"]));
        let usage = IssueRoute::Fail(
            "Usage: gossil issue help COMMAND\nRun \"gossil issue --help\" to list the issue commands."
                .into(),
        );
        assert_eq!(route(&os(&["help", "task", "create", "x"]), false), usage);
        assert_eq!(route(&os(&["task", "create", "x"]), true), usage);
    }

    /// A cluster of short flags asks for help when Clap would reach its `h` through
    /// flags of the command that take no value, before any `--`.
    #[test]
    fn a_cluster_of_flags_can_ask_for_help() {
        let list = help_of(&["task", "list"]);
        for args in [
            &["t", "ls", "-ah"][..],
            &["t", "ls", "-a", "-h"],
            &["t", "ls", "-jaqrvh"],
            &["t", "ls", "-hz"],
            &["t", "ls", "-s", "open", "-ah"],
            &["t", "ls", "-a", "-ah"],
        ] {
            assert_eq!(route(&os(args), false), list, "{args:?}");
        }
        assert_eq!(
            route(&os(&["t", "show", "lm", "-jh"]), false),
            help_of(&["task", "show"])
        );
        assert_eq!(route(&os(&["init", "-vvh"]), false), help_of(&["init"]));
        for args in [&["t", "new", "-jh"][..], &["t", "new", "-j", "-jh", "T"]] {
            assert_eq!(
                route(&os(args), false),
                help_of(&["task", "create"]),
                "{args:?}"
            );
        }
        for (args, handler, path) in [
            (&["t", "ls", "-sh"][..], IssueHandler::TaskList, "task list"),
            (&["t", "ls", "-zh"], IssueHandler::TaskList, "task list"),
            (&["t", "ls", "-aah"], IssueHandler::TaskList, "task list"),
            (&["t", "ls", "-ajah"], IssueHandler::TaskList, "task list"),
            (
                &["t", "show", "lm", "-jjh"],
                IssueHandler::TaskShow,
                "task show",
            ),
            (
                &["t", "ls", "--", "-ah"],
                IssueHandler::TaskList,
                "task list",
            ),
            (&["t", "ls", "--hz"], IssueHandler::TaskList, "task list"),
            (&["t", "show", "-ah"], IssueHandler::TaskShow, "task show"),
            (&["init", "-ah"], IssueHandler::Init, "init"),
            (
                &["t", "new", "-dh"],
                IssueHandler::TaskCreate,
                "task create",
            ),
            (&["undo", "-vh"], IssueHandler::Unavailable, "undo"),
        ] {
            assert_eq!(
                route(&os(args), false),
                IssueRoute::Run {
                    handler,
                    path: path.into(),
                    args: os(&args[path.split(' ').count()..]),
                },
                "{args:?}"
            );
        }
        assert!(!asks_help(OsStr::new("-"), "a"));
        assert_eq!(short_flags(IssueHandler::Unavailable), "");
    }

    /// A recognised command runs its handler with the arguments after its last
    /// name, and the word `help` there is an ordinary argument.
    #[test]
    fn commands_run_their_handlers() {
        for (args, handler, path, rest) in [
            (
                &["task", "create", "help"][..],
                IssueHandler::TaskCreate,
                "task create",
                &["help"][..],
            ),
            (&["t", "ls"], IssueHandler::TaskList, "task list", &[]),
            (
                &["t", "view", "ab"],
                IssueHandler::TaskShow,
                "task show",
                &["ab"],
            ),
            (
                &["init", "--local"],
                IssueHandler::Init,
                "init",
                &["--local"],
            ),
            (&["u", "2"], IssueHandler::Unavailable, "undo", &["2"]),
            (&["grep", "q"], IssueHandler::Unavailable, "search", &["q"]),
            (&["p"], IssueHandler::Unavailable, "project", &[]),
            (
                &["p", "--json"],
                IssueHandler::Unavailable,
                "project",
                &["--json"],
            ),
            (&["tag"], IssueHandler::Unavailable, "tag", &[]),
            (
                &["task", "create", "--", "-h"],
                IssueHandler::TaskCreate,
                "task create",
                &["--", "-h"],
            ),
            (
                &["a", "meta", "ID"],
                IssueHandler::Unavailable,
                "artifact meta",
                &["ID"],
            ),
            (
                &["i", "rm", "it", "t"],
                IssueHandler::Unavailable,
                "iteration remove",
                &["it", "t"],
            ),
        ] {
            assert_eq!(
                route(&os(args), false),
                IssueRoute::Run {
                    handler,
                    path: path.into(),
                    args: os(rest),
                },
                "{args:?}"
            );
        }
    }

    /// Unknown names and missing subcommands are parser failures that point to help.
    #[test]
    fn unknown_names_and_missing_commands_fail() {
        for (args, asked, path, name) in [
            (&["frob"][..], false, "", "frob"),
            (&["serve"], false, "", "serve"),
            (&["s"], false, "", "s"),
            (&["author"], false, "", "author"),
            (&["task", "frob"], false, "task", "frob"),
            (&["t", "meta", "x", "y"], true, "task meta", "x"),
            (
                &["help", "task", "note", "frob"],
                false,
                "task note",
                "frob",
            ),
        ] {
            assert_eq!(
                route(&os(args), asked),
                IssueRoute::Fail(help::unknown_issue(path, std::ffi::OsStr::new(name))),
                "{args:?}"
            );
        }
        for (args, path) in [
            (&["task"][..], "task"),
            (&["a", "note"], "artifact note"),
            (&["config"], "config"),
        ] {
            assert_eq!(
                route(&os(args), false),
                IssueRoute::Fail(help::issue_needs_command(path)),
                "{args:?}"
            );
        }
        assert_eq!(
            help::unknown_issue("", std::ffi::OsStr::new("x")),
            "gossil: unknown issue command \"x\"\nRun \"gossil issue --help\" to list the issue commands."
        );
        assert_eq!(
            help::unknown_issue("task", std::ffi::OsStr::new("x")),
            "gossil: unknown issue command \"task x\"\nRun \"gossil issue task --help\" to list its commands."
        );
        assert_eq!(
            help::issue_needs_command("task"),
            "gossil: issue task needs a command\nRun \"gossil issue task --help\" to list its commands."
        );
    }

    /// Every command of the table routes to its handler, or to the failure of a
    /// command that needs a subcommand, under each of its names. The group help
    /// and the help of the command name it.
    #[test]
    fn every_table_command_routes() {
        /// Walk one level of the table below `path`, and count its commands.
        fn walk(level: &'static [IssueCommand], path: &[&'static str], count: &mut usize) {
            for command in level {
                for name in command.names.iter().chain(command.hidden) {
                    let mut args: Vec<&str> = path.to_vec();
                    args.push(name);
                    let mut canonical = path.to_vec();
                    canonical.push(command.names[0]);
                    let joined = canonical.join(" ");
                    let expected = command.handler.map_or_else(
                        || IssueRoute::Fail(help::issue_needs_command(&joined)),
                        |handler| IssueRoute::Run {
                            handler,
                            path: joined.clone(),
                            args: Vec::new(),
                        },
                    );
                    assert_eq!(route(&os(&args), false), expected, "{args:?}");
                    let text = help::issue_command(&joined, command);
                    assert!(text.starts_with(&format!("Usage: gossil issue {joined}")));
                    assert_eq!(
                        text.contains(help::UNAVAILABLE),
                        !command.runs(),
                        "{joined}"
                    );
                }
                assert!(help::issue().contains(&format!(" {}", command.names.join(", "))));
                *count += 1;
                let mut below = path.to_vec();
                below.push(command.names[0]);
                walk(command.children, &below, count);
            }
        }
        let mut count = 0;
        walk(help::ISSUE_COMMANDS, &[], &mut count);
        assert_eq!(count, 91);
        assert_eq!(
            help::issue_command("version", at(&["version"])),
            "Usage: gossil issue version\n\n\
             Print the current version, platform info, and check for available updates.\n\
             This build recognises the command and does not run it: it is not available in this build.\n"
        );
    }

    /// Commands that run without a subcommand show it as optional in their usage,
    /// and the summaries keep the wording of the pinned Gest help.
    #[test]
    fn usage_lines_match_gest() {
        for (path, first) in [
            (
                &["project"][..],
                "Usage: gossil issue project [OPTIONS] [COMMAND]\n",
            ),
            (&["tag"], "Usage: gossil issue tag [COMMAND]\n"),
            (&["task"], "Usage: gossil issue task COMMAND\n"),
            (
                &["task", "meta"],
                "Usage: gossil issue task meta [OPTIONS] [ID] [COMMAND]\n",
            ),
        ] {
            let text = help::issue_command(&path.join(" "), at(path));
            assert!(text.starts_with(first), "{text}");
        }
        assert!(help::issue().contains(
            "\n  self-update             Download and install the latest release from GitHub\n"
        ));
    }

    /// Each kind of error maps to its CLI-05 status, with its text as the message.
    #[tokio::test]
    async fn errors_map_to_their_cli_05_status() {
        let io = || std::io::Error::other("disk");
        let storage = LocalOwnerRepository::open(
            std::path::Path::new("/nonexistent/gossil-test/repo"),
            crate::vcs_open::limits(),
        )
        .await
        .err()
        .unwrap();
        let cases: [(Box<dyn std::error::Error>, u8); 6] = [
            (
                Box::new(crate::checkout_locator::Error::config_source("bad", io())),
                78,
            ),
            (Box::new(crate::checkout_locator::Error::from(io())), 74),
            (Box::new(storage), 74),
            (Box::new(io()), 74),
            ("a selection conflict".into(), 78),
            (Box::new(crate::storage::OlderCatalog { found: 4 }), 78),
        ];
        for (error, status) in cases {
            let failure = IssueFailure::from_error(&*error);
            assert_eq!(
                failure,
                IssueFailure::new(status, error.to_string()),
                "{error}"
            );
        }
    }

    /// A command that needs the bound project gets it, and refuses with status 78
    /// and [`NOT_BOUND`] before `init`.
    #[tokio::test]
    async fn a_task_command_needs_the_bound_project() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let bounds = crate::vcs_open::limits();
        let mut owner = LocalOwnerRepository::initialize(
            &temp.path().join("repo"),
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
        let mut found = IssueRepository {
            owner,
            view,
            project: None,
        };
        assert_eq!(found.project_id(), Err(IssueFailure::new(78, NOT_BOUND)));
        found.project = Some("k".repeat(32));
        assert_eq!(found.project_id(), Ok("k".repeat(32).as_str()));
    }

    /// A stale admission runs the attempt once more. Success, any other failure and a
    /// second stale refusal are the result.
    #[tokio::test]
    async fn a_stale_admission_is_retried_once() {
        let stale = || IssueFailure::new(74, STALE);
        let other = || IssueFailure::new(74, "other");
        let locked = || IssueFailure::new(74, "SQLite failure: `database is locked`");
        type Case = (
            Vec<Result<u8, IssueFailure>>,
            Result<u8, IssueFailure>,
            usize,
        );
        let cases: [Case; 7] = [
            (vec![Err(stale()), Ok(1)], Ok(1), 2),
            (vec![Err(locked()), Ok(1)], Ok(1), 2),
            (vec![Err(locked()), Err(stale())], Err(stale()), 2),
            (vec![Err(stale()), Err(stale())], Err(stale()), 2),
            (vec![Ok(1)], Ok(1), 1),
            (vec![Err(other())], Err(other()), 1),
            (
                vec![Err(IssueFailure::new(78, STALE))],
                Err(IssueFailure::new(78, STALE)),
                1,
            ),
        ];
        for (results, expected, count) in cases {
            let mut queue = results.into_iter();
            let mut calls = 0;
            let result = retry_stale(|| {
                calls += 1;
                let next = queue.next().unwrap();
                async move { next }
            })
            .await;
            assert_eq!((result, calls), (expected, count));
        }
    }

    /// The text of [`STALE`] is the refusal that storage gives an owner after another
    /// owner of the same repository committed.
    #[tokio::test]
    async fn the_stale_text_is_the_storage_refusal() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let bounds = crate::vcs_open::limits();
        let mut first = LocalOwnerRepository::initialize(
            &path,
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
        let mut second = LocalOwnerRepository::open(&path, bounds).await.unwrap();
        first
            .register_checkout("work", &root.canonicalize().unwrap(), bounds)
            .await
            .unwrap();
        let error = second.bound_project("work", limits()).await.unwrap_err();
        assert_eq!(
            IssueFailure::from_error(&error),
            IssueFailure::new(74, STALE)
        );
    }

    /// A new ticket gets the current instant, its RFC3339 string and 40 random
    /// lowercase hexadecimal digits, and the local login is UTF-8 here.
    #[test]
    fn a_creation_has_a_date_and_a_ticket_id() {
        let now = creation("x").unwrap();
        assert_eq!(now.stamp, serde_json::json!(format!("{}Z", now.date)));
        assert_eq!(now.ticket_id.len(), 40);
        assert!(
            now.ticket_id
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        );
        assert_ne!(creation("x").unwrap().ticket_id, now.ticket_id);
        assert!(!login().unwrap().is_empty());
    }

    /// The handler of a command that this build does not run exits with status 69.
    #[test]
    fn every_handler_refuses() {
        assert_eq!(
            run(IssueHandler::Unavailable, "init", &[]),
            std::process::ExitCode::from(69)
        );
    }
}
