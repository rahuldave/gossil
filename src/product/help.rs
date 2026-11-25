//! Print the help of the `gossil` and `gos` executables and their `vcs` and `issue` groups.
//!
//! The help lists the groups and commands that this build dispatches. It also
//! names the planned groups, and marks them as not available. Each command of
//! the `issue` group is recognised. The help says which commands this build runs,
//! and marks every other one as not available in this build. This build runs `init`,
//! `task create`, `task list` and `task show`.
//!
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

/// Names the code that runs a recognised issue command.
///
/// [`IssueHandler::Init`], [`IssueHandler::TaskCreate`], [`IssueHandler::TaskList`] and
/// [`IssueHandler::TaskShow`] run their commands. [`IssueHandler::Unavailable`] refuses
/// every other recognised command. A new handler changes only its match arm in
/// `issue::run`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IssueHandler {
    /// `issue init`.
    Init,
    /// `issue task create`, also named `new`.
    TaskCreate,
    /// `issue task list`, also named `ls`.
    TaskList,
    /// `issue task show`, also named `view`.
    TaskShow,
    /// Every other recognised command.
    Unavailable,
}

/// Describes one command of the `issue` group, at any depth.
pub(super) struct IssueCommand {
    /// Holds the canonical name first, then each alias that help shows.
    pub(super) names: &'static [&'static str],
    /// Holds each alias that the parser accepts and help does not show.
    pub(super) hidden: &'static [&'static str],
    /// Holds the usage after the command path, such as `[OPTIONS] <ID>`.
    arguments: &'static str,
    /// Says in one line what the command does.
    summary: &'static str,
    /// Names the handler, or holds `None` for a command that needs a subcommand.
    pub(super) handler: Option<IssueHandler>,
    /// Holds the subcommands, in help order.
    pub(super) children: &'static [IssueCommand],
    /// Holds the help text after the summary of a command that this build runs, or
    /// `None` for a command that this build does not run.
    details: Option<&'static str>,
}

impl IssueCommand {
    /// Say whether this build runs the command.
    ///
    /// # Returns
    /// True when the command has the help text of a command that runs.
    pub(super) const fn runs(&self) -> bool {
        self.details.is_some()
    }
}

/// The help text of `issue init` after its summary.
const INIT_DETAILS: &str = "\nCreate the Gest project of the open checkout that holds the current directory,\n\
     as a private project ticket, and bind the checkout to it in one transaction.\n\
     Run gos v init and gos v open first. When the checkout already has a project,\n\
     init prints that project and changes nothing.\n\n\
     Options:\n\
     \x20 --local           Accepted; the checkout locator already selects the repository\n\
     \x20 --no-color        Accepted; the output has no color\n\
     \x20 --no-pager        Accepted; the output is not paged\n\
     \x20 -v, --verbose...  Accepted; it changes no output\n\
     \x20 -h, --help        Print help\n";

/// The help text of `issue task create` after its summary.
const TASK_CREATE_DETAILS: &str = "\nCreate a private task in the Gest project of the open checkout that holds the\n\
     current directory, as one private task ticket, in one transaction. Run gos i init\n\
     first. This build never reads stdin and never opens an editor, so a missing\n\
     description is empty. A [ in the title, the description or the metadata exits\n\
     with status 64, and so does a task over 65,536 bytes. Tasks of this build have no\n\
     undo history. In this build, task create exits 74 after about 113 small\n\
     tasks, and nothing stored is lost; see docs/cli.md.\n\n\
     Options:\n\
     \x20 -d, --description <DESCRIPTION>  The task description\n\
     \x20 -m, --metadata <KEY=VALUE>       Set a metadata key=value pair (repeatable; supports dot-paths and scalar inference)\n\
     \x20     --metadata-json <JSON>       Merge a JSON object into metadata (repeatable; applied after --metadata pairs)\n\
     \x20 -p, --priority <PRIORITY>        The task priority; either 0-4 (lower is higher) or one of critical|high|medium|low|lowest\n\
     \x20 -s, --status <STATUS>            The initial task status\n\
     \x20 -j, --json                       Emit output as JSON\n\
     \x20 -q, --quiet                      Print only the short task ID\n\
     \x20 -r, --raw                        Accepted; the output has no themed styling\n\
     \x20     --no-color                   Accepted; the output has no color\n\
     \x20     --no-pager                   Accepted; the output is not paged\n\
     \x20 -v, --verbose...                 Accepted; it changes no output\n\
     \x20 -h, --help                       Print help\n\n\
     Not available in this build, and refused with status 69:\n\
     \x20     --assign <ASSIGN>, --batch, -i, --iteration <ITERATION>,\n\
     \x20 -l, --link <LINK>, --phase <PHASE>, --tag <TAG>, and a title read from stdin\n";

/// The help text of `issue task list` after its summary.
const TASK_LIST_DETAILS: &str = "\nList the tasks of the project bound to the open checkout, newest first.\n\
     Without --all, a task that is done or cancelled is left out. The text output\n\
     is provisional; --json prints the JSON of Gest.\n\n\
     Options:\n\
     \x20 -a, --all              Show all tasks, including resolved\n\
     \x20 -s, --status <STATUS>  Filter by status\n\
     \x20 -j, --json             Emit output as JSON\n\
     \x20 --no-color             Accepted; the output has no color\n\
     \x20 --no-pager             Accepted; the output is not paged\n\
     \x20 -h, --help             Print help\n\n\
     Refused in this build with status 69: --assigned-to, --limit, -t/--tag,\n\
     -q/--quiet, -r/--raw and -v/--verbose.\n";

/// The help text of `issue task show` after its summary.
const TASK_SHOW_DETAILS: &str = "\nShow one task of any project, by its ID or a unique prefix of it. The text\n\
     output is provisional; --json prints the JSON of Gest.\n\n\
     Arguments:\n\
     \x20 <ID>  The task ID or prefix\n\n\
     Options:\n\
     \x20 -j, --json   Emit output as JSON\n\
     \x20 --no-color   Accepted; the output has no color\n\
     \x20 --no-pager   Accepted; the output is not paged\n\
     \x20 -h, --help   Print help\n\n\
     Refused in this build with status 69: -q/--quiet, -r/--raw and -v/--verbose.\n";

/// Build a command without subcommands, which this build refuses.
///
/// # Arguments
/// * `names` - The canonical name, then each visible alias.
/// * `arguments` - The usage after the command path.
/// * `summary` - What the command does, in one line.
///
/// # Returns
/// The command, with no hidden alias and the handler [`IssueHandler::Unavailable`].
const fn leaf(
    names: &'static [&'static str],
    arguments: &'static str,
    summary: &'static str,
) -> IssueCommand {
    IssueCommand {
        names,
        hidden: &[],
        arguments,
        summary,
        handler: Some(IssueHandler::Unavailable),
        children: &[],
        details: None,
    }
}

/// Build a command that only selects one of its subcommands.
///
/// # Arguments
/// * `names` - The canonical name.
/// * `hidden` - The aliases that help does not show.
/// * `summary` - What the command does, in one line.
/// * `children` - The subcommands.
///
/// # Returns
/// The command, with the usage `COMMAND` and no handler.
const fn group(
    names: &'static [&'static str],
    hidden: &'static [&'static str],
    summary: &'static str,
    children: &'static [IssueCommand],
) -> IssueCommand {
    IssueCommand {
        names,
        hidden,
        arguments: "COMMAND",
        summary,
        handler: None,
        children,
        details: None,
    }
}

/// The `meta` command of tasks, artifacts and iterations, which shows all the
/// metadata of `ID` when no subcommand follows.
const META: IssueCommand = IssueCommand {
    handler: Some(IssueHandler::Unavailable),
    children: &[
        leaf(
            &["get"],
            "[OPTIONS] <ID> <PATH>",
            "Get a metadata value by dot-delimited path",
        ),
        leaf(
            &["set"],
            "[OPTIONS] <ID> <PATH> <VALUE>",
            "Set a metadata value at a dot-delimited path",
        ),
        IssueCommand {
            hidden: &["delete"],
            ..leaf(
                &["unset"],
                "[OPTIONS] <ID> <PATH>",
                "Remove a metadata value at a dot-delimited path",
            )
        },
    ],
    ..leaf(
        &["meta"],
        "[OPTIONS] [ID] [COMMAND]",
        "Get or set custom metadata",
    )
};

/// Lists each command of the `issue` group that the product entry recognises,
/// in the order and with the names of the pinned Gest source. The `serve`
/// command of Gest is left out, because the root groups `ui` and `serve` take
/// its place (CLI-02).
pub(super) const ISSUE_COMMANDS: &[IssueCommand] = &[
    group(
        &["artifact"],
        &["a"],
        "Manage artifacts",
        &[
            leaf(&["archive"], "[OPTIONS] <ID>", "Archive an artifact"),
            leaf(
                &["create", "new"],
                "[OPTIONS] [TITLE]",
                "Create a new artifact",
            ),
            leaf(
                &["delete", "rm"],
                "[OPTIONS] <ID>",
                "Delete an artifact and its dependent rows",
            ),
            leaf(&["list", "ls"], "[OPTIONS]", "List artifacts"),
            META,
            group(
                &["note"],
                &[],
                "Manage notes on an artifact",
                &[
                    leaf(&["add"], "[OPTIONS] <ID>", "Add a note to an artifact"),
                    leaf(
                        &["delete"],
                        "[OPTIONS] <ID>",
                        "Delete a note from an artifact",
                    ),
                    leaf(&["list"], "[OPTIONS] <ID>", "List notes on an artifact"),
                    leaf(&["show"], "[OPTIONS] <ID>", "Show a single note"),
                    leaf(&["update"], "[OPTIONS] <ID>", "Update a note's body"),
                ],
            ),
            leaf(&["show", "view"], "[OPTIONS] <ID>", "Show an artifact"),
            leaf(
                &["tag"],
                "[OPTIONS] <ID> <LABEL>",
                "Add a tag to an artifact",
            ),
            leaf(
                &["untag"],
                "[OPTIONS] <ID> <LABEL>",
                "Remove a tag from an artifact",
            ),
            leaf(&["update", "edit"], "[OPTIONS] <ID>", "Update an artifact"),
        ],
    ),
    group(
        &["config"],
        &[],
        "View or modify configuration",
        &[
            leaf(&["get"], "[OPTIONS] <KEY>", "Get a configuration value"),
            leaf(
                &["set"],
                "[OPTIONS] <KEY> <VALUE>",
                "Set a configuration value",
            ),
            leaf(&["show"], "[OPTIONS]", "Show the current configuration"),
        ],
    ),
    group(
        &["generate"],
        &[],
        "Generate shell completions and man pages",
        &[
            leaf(
                &["completions"],
                "[OPTIONS] <SHELL>",
                "Generate shell completions",
            ),
            leaf(
                &["man-pages"],
                "[OPTIONS] [OUTPUT_DIR]",
                "Generate man pages",
            ),
        ],
    ),
    IssueCommand {
        handler: Some(IssueHandler::Init),
        details: Some(INIT_DETAILS),
        ..leaf(
            &["init"],
            "[OPTIONS]",
            "Initialize gest for the current directory",
        )
    },
    group(
        &["iteration"],
        &["i"],
        "Manage iterations",
        &[
            leaf(
                &["add"],
                "[OPTIONS] <ITERATION> [TASK]",
                "Add a task to an iteration",
            ),
            leaf(
                &["advance"],
                "[OPTIONS] <ID>",
                "Validate active phase and advance to the next phase",
            ),
            leaf(&["cancel"], "[OPTIONS] <ID>", "Cancel an iteration"),
            leaf(&["complete"], "[OPTIONS] <ID>", "Complete an iteration"),
            leaf(
                &["create", "new"],
                "[OPTIONS] <TITLE>",
                "Create a new iteration",
            ),
            leaf(
                &["delete"],
                "[OPTIONS] <ID>",
                "Delete an iteration and drop its task memberships",
            ),
            leaf(
                &["graph"],
                "[OPTIONS] <ID>",
                "Show phased task dependency graph",
            ),
            leaf(
                &["link"],
                "[OPTIONS] <ID> [[REL] TARGET]...",
                "Link an iteration to another entity",
            ),
            leaf(&["list", "ls"], "[OPTIONS]", "List iterations"),
            META,
            leaf(
                &["next"],
                "[OPTIONS] <ID>",
                "Return the next available task in an iteration",
            ),
            leaf(
                &["remove", "rm"],
                "[OPTIONS] <ITERATION> <TASK>",
                "Remove a task from an iteration",
            ),
            leaf(
                &["reopen"],
                "[OPTIONS] <ID>",
                "Reopen a completed or cancelled iteration",
            ),
            leaf(&["show", "view"], "[OPTIONS] <ID>", "Show an iteration"),
            leaf(
                &["status"],
                "[OPTIONS] <ID>",
                "Show iteration progress summary",
            ),
            leaf(
                &["tag"],
                "[OPTIONS] <ID> <LABEL>",
                "Add a tag to an iteration",
            ),
            leaf(
                &["unlink"],
                "[OPTIONS] <ID> <TARGET>",
                "Remove a relationship between an iteration and another entity",
            ),
            leaf(
                &["untag"],
                "[OPTIONS] <ID> <LABEL>",
                "Remove a tag from an iteration",
            ),
            leaf(&["update", "edit"], "[OPTIONS] <ID>", "Update an iteration"),
        ],
    ),
    leaf(
        &["migrate"],
        "[OPTIONS]",
        "Import v0.4.x flat-file data into the current project store",
    ),
    IssueCommand {
        arguments: "[OPTIONS] [COMMAND]",
        handler: Some(IssueHandler::Unavailable),
        ..group(
            &["project", "p"],
            &[],
            "Show or manage the current project",
            &[
                leaf(
                    &["archive"],
                    "[OPTIONS] <ID>",
                    "Soft-archive a project, hiding it and its entities from default list views",
                ),
                leaf(
                    &["attach"],
                    "[OPTIONS] <ID>",
                    "Attach the current directory to an existing project as a workspace",
                ),
                leaf(
                    &["delete", "rm"],
                    "[OPTIONS] <ID>",
                    "Delete a project and all of its owned entities",
                ),
                leaf(
                    &["detach"],
                    "[OPTIONS]",
                    "Detach the current directory from its project",
                ),
                leaf(&["list", "ls"], "[OPTIONS]", "List all known projects"),
                leaf(
                    &["unarchive"],
                    "[OPTIONS] <ID>",
                    "Restore an archived project to active status",
                ),
            ],
        )
    },
    leaf(
        &["purge"],
        "[OPTIONS]",
        "Purge terminal, archived, and orphaned data from the store",
    ),
    IssueCommand {
        hidden: &["grep"],
        ..leaf(
            &["search"],
            "[OPTIONS] <QUERY>",
            "Search across all entity types",
        )
    },
    leaf(
        &["self-update"],
        "[OPTIONS]",
        "Download and install the latest release from GitHub",
    ),
    IssueCommand {
        arguments: "[COMMAND]",
        handler: Some(IssueHandler::Unavailable),
        ..group(
            &["tag"],
            &[],
            "List all tags",
            &[
                leaf(
                    &["add"],
                    "[OPTIONS] <ID> <TAGS>...",
                    "Add tags to any entity (task, artifact, or iteration)",
                ),
                leaf(&["list"], "[OPTIONS]", "List all tags"),
                leaf(
                    &["remove"],
                    "[OPTIONS] <ID> <TAGS>...",
                    "Remove tags from any entity (task, artifact, or iteration)",
                ),
            ],
        )
    },
    group(
        &["task"],
        &["t"],
        "Manage tasks",
        &[
            leaf(
                &["block"],
                "[OPTIONS] <ID> <BLOCKED>",
                "Mark a task as blocking another task",
            ),
            leaf(&["cancel"], "[OPTIONS] <ID>", "Cancel a task"),
            leaf(
                &["claim"],
                "[OPTIONS] <ID>",
                "Claim a task (assign and mark in-progress)",
            ),
            leaf(&["complete"], "[OPTIONS] <ID>", "Mark a task as done"),
            IssueCommand {
                handler: Some(IssueHandler::TaskCreate),
                details: Some(TASK_CREATE_DETAILS),
                ..leaf(&["create", "new"], "[OPTIONS] [TITLE]", "Create a new task")
            },
            leaf(
                &["delete", "rm"],
                "[OPTIONS] <ID>",
                "Delete a task and its dependent rows",
            ),
            leaf(
                &["link"],
                "[OPTIONS] <ID> <TARGET>",
                "Link a task to another entity",
            ),
            IssueCommand {
                handler: Some(IssueHandler::TaskList),
                details: Some(TASK_LIST_DETAILS),
                ..leaf(&["list", "ls"], "[OPTIONS]", "List tasks")
            },
            META,
            group(
                &["note"],
                &[],
                "Manage notes on a task",
                &[
                    leaf(&["add"], "[OPTIONS] <ID>", "Add a note to a task"),
                    leaf(&["delete"], "[OPTIONS] <ID>", "Delete a note from a task"),
                    leaf(&["list"], "[OPTIONS] <ID>", "List notes on a task"),
                    leaf(&["show"], "[OPTIONS] <ID>", "Show a single note"),
                    leaf(&["update"], "[OPTIONS] <ID>", "Update a note's body"),
                ],
            ),
            IssueCommand {
                handler: Some(IssueHandler::TaskShow),
                details: Some(TASK_SHOW_DETAILS),
                ..leaf(&["show", "view"], "[OPTIONS] <ID>", "Show a task")
            },
            leaf(&["tag"], "[OPTIONS] <ID> <LABEL>", "Add a tag to a task"),
            leaf(
                &["unlink"],
                "[OPTIONS] <ID> <TARGET>",
                "Remove a relationship from a task",
            ),
            leaf(
                &["untag"],
                "[OPTIONS] <ID> <LABEL>",
                "Remove a tag from a task",
            ),
            leaf(&["update", "edit"], "[OPTIONS] <ID>", "Update a task"),
        ],
    ),
    IssueCommand {
        hidden: &["u"],
        ..leaf(&["undo"], "[STEPS]", "Undo the last command")
    },
    leaf(
        &["version"],
        "",
        "Print the current version, platform info, and check for available updates",
    ),
];

/// Lists each group that the specification names and this build does not dispatch.
///
/// Each entry holds the name, the alias and what the group will do.
const PLANNED_GROUPS: &[(&str, &str, &str)] = &[
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
         \x20 vcs, v              Repositories, checkouts, private history and diagnostics\n\
         \x20 issue, i            Gest commands; init, task create, task list and task show run, and the others are not available in this build\n\n\
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
    out.push_str(&format!(
        "\nCommands of the issue group. This build runs {}; every other command is {UNAVAILABLE}:\n",
        runnable()
    ));
    issue_tree(&mut out, ISSUE_COMMANDS, 0);
    out.push_str(ISSUE_FOOTER);
    out
}

/// The phrase that every refusal and help of the issue group uses.
pub(super) const UNAVAILABLE: &str = "not available in this build";

/// The last lines of the issue help, which say how to get the help of one command.
const ISSUE_FOOTER: &str = "\nEach other command refuses to run, and exits with status 69.\n\
     Run \"gossil issue help COMMAND\" or \"gossil issue COMMAND --help\" for the usage of one command.\n";

/// Name the issue commands that this build runs, by their canonical paths.
///
/// # Returns
/// The paths, joined by commas, in help order.
fn runnable() -> String {
    /// Add the paths of the runnable commands of one level and every level below.
    fn walk(commands: &[IssueCommand], prefix: &str, out: &mut Vec<String>) {
        for command in commands {
            let path = format!("{prefix}{}", command.names[0]);
            if command.runs() {
                out.push(path.clone());
            }
            walk(command.children, &format!("{path} "), out);
        }
    }
    let mut out = Vec::new();
    walk(ISSUE_COMMANDS, "", &mut out);
    out.join(", ")
}

/// Find an issue command among the subcommands of one level, by a name or an alias.
///
/// # Arguments
/// * `commands` - The commands of one level of the issue tree.
/// * `name` - The command token.
///
/// # Returns
/// The command, or `None` when the level has no such name. Hidden aliases match too.
pub(super) fn find_issue<'a>(
    commands: &'a [IssueCommand],
    name: &OsStr,
) -> Option<&'a IssueCommand> {
    commands.iter().find(|command| {
        command
            .names
            .iter()
            .chain(command.hidden)
            .any(|alias| name == *alias)
    })
}

/// Write the issue commands of one level and every level below, one line each,
/// indented two spaces for each level.
///
/// # Arguments
/// * `out` - The help text that receives the lines.
/// * `commands` - The commands of the first level to write.
/// * `depth` - The level of `commands`, which sets the indent.
fn issue_tree(out: &mut String, commands: &[IssueCommand], depth: usize) {
    for command in commands {
        let names = format!("{}{}", "  ".repeat(depth), command.names.join(", "));
        out.push_str(&format!("  {names:<23} {}\n", command.summary));
        issue_tree(out, command.children, depth + 1);
    }
}

/// Build the help of the `issue` group.
///
/// The executable prints it for `issue` alone, `issue --help`, `issue -h` and
/// `issue help`, and for forms such as `help issue`.
///
/// # Returns
/// The group help text, with a final newline.
pub(super) fn issue() -> String {
    let mut out = format!(
        "Usage: gossil issue COMMAND [ARGUMENTS]\n\
         \x20      gos i COMMAND [ARGUMENTS]\n\n\
         i is a short name for issue. This build recognises each Gest command below.\n\
         It runs {}; every other command is {UNAVAILABLE}.\n\n\
         Commands:\n",
        runnable()
    );
    issue_tree(&mut out, ISSUE_COMMANDS, 0);
    out.push_str(ISSUE_FOOTER);
    out
}

/// Build the help of one issue command, which gives its usage and what it does.
///
/// # Arguments
/// * `path` - The canonical names from the group down to the command, such as `task create`.
/// * `command` - The command at the end of `path`.
///
/// # Returns
/// The help text, with a final newline. A command with subcommands also lists them.
pub(super) fn issue_command(path: &str, command: &IssueCommand) -> String {
    let mut out = format!("Usage: gossil issue {path}");
    if !command.arguments.is_empty() {
        out.push(' ');
        out.push_str(command.arguments);
    }
    out.push_str(&format!("\n\n{}.\n", command.summary));
    if command.names.len() > 1 {
        out.push_str(&format!("Names: {}.\n", command.names.join(", ")));
    }
    match command.details {
        Some(details) => out.push_str(details),
        None => out.push_str(&format!(
            "This build recognises the command and does not run it: it is {UNAVAILABLE}.\n"
        )),
    }
    if !command.children.is_empty() {
        out.push_str("\nCommands:\n");
        issue_tree(&mut out, command.children, 0);
    }
    out
}

/// Build the error for a token that names no command at one level of the issue group.
///
/// # Arguments
/// * `path` - The canonical path of the level, empty for the group itself.
/// * `arg` - The token that names no command there.
///
/// # Returns
/// The diagnostic lines, without a final newline, with a pointer to the help of the level.
pub(super) fn unknown_issue(path: &str, arg: &OsStr) -> String {
    let name = arg.to_string_lossy();
    if path.is_empty() {
        format!(
            "gossil: unknown issue command \"{name}\"\n\
             Run \"gossil issue --help\" to list the issue commands."
        )
    } else {
        format!(
            "gossil: unknown issue command \"{path} {name}\"\n\
             Run \"gossil issue {path} --help\" to list its commands."
        )
    }
}

/// Build the error for an issue command that needs a subcommand and got none.
///
/// # Arguments
/// * `path` - The canonical path of the command, such as `task`.
///
/// # Returns
/// The diagnostic lines, without a final newline, with a pointer to its help.
pub(super) fn issue_needs_command(path: &str) -> String {
    format!(
        "gossil: issue {path} needs a command\n\
         Run \"gossil issue {path} --help\" to list its commands."
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The builders of the issue table give a leaf that refuses its command, and
    /// a group with no handler, when they run outside a constant.
    #[test]
    fn issue_builders_set_their_defaults() {
        let built = leaf(std::hint::black_box(&["x", "y"]), "[A]", "Do x");
        assert_eq!(built.names, ["x", "y"]);
        assert!(built.hidden.is_empty() && built.children.is_empty());
        assert_eq!((built.arguments, built.summary), ("[A]", "Do x"));
        assert_eq!(built.handler, Some(IssueHandler::Unavailable));
        assert!(!built.runs());
        let parent = group(std::hint::black_box(&["g"]), &["h"], "Group", META.children);
        assert_eq!((parent.names, parent.hidden), (&["g"][..], &["h"][..]));
        assert_eq!((parent.arguments, parent.summary), ("COMMAND", "Group"));
        assert_eq!(parent.handler, None);
        assert_eq!(parent.children.len(), 3);
    }
}
