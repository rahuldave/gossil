//! Route the commands of the `issue` group, and refuse each one.
//!
//! The parser walks the command names of the table in `help.rs`, with the
//! aliases of the pinned Gest source. It checks the names only. The arguments
//! after the last name go to the handler unchecked. Every handler of this build
//! refuses with status 69 before it reads or writes a file.
use super::help::{self, IssueCommand, IssueHandler};
use std::ffi::OsString;

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
/// After the last name, a `--help` or `-h` before any `--` asks for that help too,
/// and the word `help` is an ordinary argument. The group alone prints the group
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
        .any(|arg| help::is_help_flag(arg))
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

/// Run the handler of a recognised issue command.
///
/// A later commit gives `init`, `task create`, `task list` and `task show` their
/// own handlers, each by a change of its arm here.
///
/// # Arguments
/// * `handler` - The handler that the table names.
/// * `path` - The canonical command path, for the diagnostic.
/// * `_args` - The arguments after the last command name, which no handler reads yet.
///
/// # Returns
/// Status 69 after the refusal on stderr.
pub(super) fn run(handler: IssueHandler, path: &str, _args: &[OsString]) -> std::process::ExitCode {
    match handler {
        IssueHandler::Init
        | IssueHandler::TaskCreate
        | IssueHandler::TaskList
        | IssueHandler::TaskShow
        | IssueHandler::Unavailable => refuse(path),
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
                    assert!(text.contains(help::UNAVAILABLE));
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

    /// The refusal of each handler exits 69.
    #[test]
    fn every_handler_refuses() {
        for handler in [
            IssueHandler::Init,
            IssueHandler::TaskCreate,
            IssueHandler::TaskList,
            IssueHandler::TaskShow,
            IssueHandler::Unavailable,
        ] {
            assert_eq!(run(handler, "init", &[]), std::process::ExitCode::from(69));
        }
    }
}
