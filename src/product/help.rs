//! Print the help of the `gossil` and `gos` executables and their `vcs` group.
//!
//! The help lists the groups and commands that this build dispatches. It also
//! names the planned groups, and marks them as not available.
//! Usage lines keep the fixed `gossil vcs` spelling for every executable and
//! group alias. Most usage errors of the commands use the same spelling. The
//! usage error of `add` and `status` uses `gos v` instead.
use std::ffi::OsStr;

/// Describes one command of the `vcs` group by its names, its usage and what it does.
struct VcsCommand {
    /// Holds the canonical name first, then each alias that dispatch accepts.
    names: &'static [&'static str],
    /// Holds the usage lines of the command, without a final newline.
    usage: &'static str,
    /// Says in one line what the command does.
    summary: &'static str,
    /// Says whether the command module prints its own help for a lone `--help` or `-h`.
    own_help: bool,
}

/// Lists each command of the `vcs` group that the product entry dispatches, in help order.
const VCS_COMMANDS: &[VcsCommand] = &[
    VcsCommand {
        names: &["init", "new"],
        usage: "Usage: gossil vcs init ?OPTIONS? FILENAME",
        summary: "Create an empty repository without a checkout",
        own_help: true,
    },
    VcsCommand {
        names: &["open"],
        usage: "Usage: gossil vcs open --repository PATH --checkout ID [--root ROOT]",
        summary: "Register a checkout at an existing directory, or reopen it",
        own_help: false,
    },
    VcsCommand {
        names: &["info"],
        usage: "Usage: gossil vcs info --repository REPO --checkout ID",
        summary: "Print the recorded identity of a checkout",
        own_help: true,
    },
    VcsCommand {
        names: &["add"],
        usage: "Usage: gossil vcs add --repository REPO --checkout ID --path PATH [--path PATH ...]",
        summary: "Track explicit files of a checkout",
        own_help: false,
    },
    VcsCommand {
        names: &["status"],
        usage: "Usage: gossil vcs status --repository REPO --checkout ID",
        summary: "List the tracked files, their state and their scope",
        own_help: false,
    },
    VcsCommand {
        names: &["scope"],
        usage: "Usage: gossil vcs scope --repository REPO --checkout ID --scope shared|private --path PATH [--path PATH ...]",
        summary: "Choose the shared or private scope of tracked files",
        own_help: false,
    },
    VcsCommand {
        names: &["commit"],
        usage: "Usage: gossil vcs commit --repository REPO --checkout ID --scope private --all-tracked --user ACTOR --comment TEXT --date-override DATE",
        summary: "Record every tracked file in private history",
        own_help: false,
    },
    VcsCommand {
        names: &["timeline", "log"],
        usage: "Usage: gossil vcs timeline|log --repository REPO --checkout ID",
        summary: "Print the complete recorded ancestry as NDJSON",
        own_help: true,
    },
    VcsCommand {
        names: &["test-delta-create"],
        usage: "Usage: gossil vcs test-delta-create ORIGIN TARGET DELTA",
        summary: "Write the delta from ORIGIN to TARGET into DELTA",
        own_help: false,
    },
    VcsCommand {
        names: &["test-delta-apply"],
        usage: "Usage: gossil vcs test-delta-apply ORIGIN DELTA TARGET",
        summary: "Apply DELTA to ORIGIN and write the result into TARGET",
        own_help: false,
    },
    VcsCommand {
        names: &["test-delta"],
        usage: "Usage: gossil vcs test-delta FILE1 FILE2",
        summary: "Check a delta round trip in both directions, then print ok",
        own_help: false,
    },
    VcsCommand {
        names: &["test-glob"],
        usage: "Usage: gossil vcs test-glob PATTERN STRING ...",
        summary: "Print the SQL, patterns and match indices of a Fossil glob list",
        own_help: false,
    },
];

/// Lists each group that the specification names and this build does not dispatch.
///
/// Each entry holds the name, the alias and what the group will do.
const PLANNED_GROUPS: &[(&str, &str, &str)] = &[
    ("issue", "i", "Gest issue and task commands"),
    ("ui", "u", "Web interface that opens a browser"),
    ("serve", "s", "Web server that opens no browser"),
];

/// Say whether an argument asks for help, as `--help`, `-h` or `help`.
///
/// # Arguments
/// * `arg` - One command-line argument.
///
/// # Returns
/// True for `--help`, `-h` and `help`.
pub(super) fn is_help(arg: &OsStr) -> bool {
    arg == "--help" || arg == "-h" || arg == "help"
}

/// Say whether an argument after a command asks for its help, as `--help` or `-h`.
///
/// # Arguments
/// * `arg` - The only argument after the command name.
///
/// # Returns
/// True for `--help` and `-h`. The word `help` is not a flag here, because
/// `vcs init help` names a new repository file.
pub(super) fn is_help_flag(arg: &OsStr) -> bool {
    arg == "--help" || arg == "-h"
}

/// Find the help entry of a `vcs` command by its name or alias.
///
/// # Arguments
/// * `name` - The command token after the group.
///
/// # Returns
/// The entry, or `None` when the group has no such command.
fn find(name: &OsStr) -> Option<&'static VcsCommand> {
    VCS_COMMANDS
        .iter()
        .find(|command| command.names.iter().any(|alias| name == *alias))
}

/// Say whether the `vcs` group dispatches a command of this name.
///
/// # Arguments
/// * `name` - The command token after the group.
///
/// # Returns
/// True for each canonical name and alias in the help table.
pub(super) fn is_vcs_command(name: &OsStr) -> bool {
    find(name).is_some()
}

/// Say whether a known `vcs` command module prints its own help.
///
/// # Arguments
/// * `name` - A command name or alias that [`is_vcs_command`] accepts.
///
/// # Returns
/// True for `init`, `new`, `info`, `timeline` and `log`, whose modules answer
/// `--help` and `-h` themselves; false for every other name.
pub(super) fn owns_help(name: &OsStr) -> bool {
    find(name).is_some_and(|command| command.own_help)
}

/// Write the table of `vcs` commands, one line each, with aligned summaries.
///
/// # Arguments
/// * `out` - The help text that receives the table.
fn command_table(out: &mut String) {
    for command in VCS_COMMANDS {
        let names = command.names.join(", ");
        out.push_str(&format!("  {names:<19} {}\n", command.summary));
    }
}

/// Build the product help.
///
/// The executable prints it for no arguments, `--help`, `-h` and `help`, and
/// for a help word before another, as in `help help`.
///
/// # Returns
/// The complete help text, with a final newline.
pub(super) fn root() -> String {
    let mut out = format!(
        "gossil {}: Fossil version-control mechanisms on libSQL\n\n\
         Usage: gossil GROUP COMMAND [ARGUMENTS]\n\
         \x20      gos GROUP COMMAND [ARGUMENTS]\n\n\
         gos is a short name for gossil. Both run the same program.\n\n\
         Groups:\n\
         \x20 vcs, v              Repositories, checkouts, private history and diagnostics\n\n\
         Planned groups, not available in this build:\n",
        env!("CARGO_PKG_VERSION")
    );
    for (group, alias, summary) in PLANNED_GROUPS {
        out.push_str(&format!(
            "  {:<19} {summary}\n",
            format!("{group}, {alias}")
        ));
    }
    out.push_str("\nCommands of the vcs group:\n");
    command_table(&mut out);
    out.push_str(
        "\nRun \"gossil vcs help COMMAND\" or \"gossil vcs COMMAND --help\" for the usage of one command.\n",
    );
    out
}

/// Build the help of the `vcs` group.
///
/// The executable prints it for `vcs` alone, `vcs --help`, `vcs -h` and
/// `vcs help`, and for forms such as `help vcs` and `vcs help help`.
///
/// # Returns
/// The group help text, with a final newline.
pub(super) fn vcs() -> String {
    let mut out = String::from(
        "Usage: gossil vcs COMMAND [ARGUMENTS]\n\
         \x20      gos v COMMAND [ARGUMENTS]\n\n\
         v is a short name for vcs.\n\nCommands:\n",
    );
    command_table(&mut out);
    out.push_str(
        "\nRun \"gossil vcs help COMMAND\" or \"gossil vcs COMMAND --help\" for the usage of one command.\n",
    );
    out
}

/// Build the help of one `vcs` command, which gives its usage and what it does.
///
/// # Arguments
/// * `name` - The command name or alias.
///
/// # Returns
/// The help text with a final newline, or `None` for an unknown command.
pub(super) fn command(name: &OsStr) -> Option<String> {
    find(name).map(|command| {
        let mut out = format!("{}\n\n{}.\n", command.usage, command.summary);
        if command.names.len() > 1 {
            out.push_str(&format!("Names: {}.\n", command.names.join(", ")));
        }
        out
    })
}

/// Build the error for a token that names no group of this build.
///
/// # Arguments
/// * `arg` - The first argument.
///
/// # Returns
/// The diagnostic lines, without a final newline, with a pointer to the help.
pub(super) fn unknown_group(arg: &OsStr) -> String {
    let name = arg.to_string_lossy();
    let planned = PLANNED_GROUPS
        .iter()
        .any(|(group, alias, _)| arg == *group || arg == *alias);
    if planned {
        format!(
            "gossil: the group \"{name}\" is planned and not available in this build\n\
             Run \"gossil --help\" to list the commands."
        )
    } else {
        format!("gossil: unknown command \"{name}\"\nRun \"gossil --help\" to list the commands.")
    }
}

/// Build the error for a token that names no command of the `vcs` group.
///
/// # Arguments
/// * `arg` - The command token after the group.
///
/// # Returns
/// The diagnostic lines, without a final newline, with a pointer to the group help.
pub(super) fn unknown_vcs(arg: &OsStr) -> String {
    format!(
        "gossil: unknown vcs command \"{}\"\nRun \"gossil vcs --help\" to list the vcs commands.",
        arg.to_string_lossy()
    )
}
