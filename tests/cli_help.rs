//! Both executables print real help for the product, the vcs group and each vcs command.
use std::process::{Command, Output};

/// Both product executables, which share one entry.
fn binaries() -> [&'static str; 2] {
    [env!("CARGO_BIN_EXE_gossil"), env!("CARGO_BIN_EXE_gos")]
}

/// Every vcs command and alias that the help lists and the entry dispatches.
const COMMANDS: [&str; 14] = [
    "init",
    "new",
    "open",
    "info",
    "add",
    "status",
    "scope",
    "commit",
    "timeline",
    "log",
    "test-delta-create",
    "test-delta-apply",
    "test-delta",
    "test-glob",
];

/// Run one executable in an empty directory, with a private home and configuration.
///
/// # Arguments
/// * `binary` - The executable to run.
/// * `dir` - The empty working, home and configuration directory.
/// * `args` - The arguments after the executable name.
///
/// # Returns
/// The finished process output.
fn run(binary: &str, dir: &std::path::Path, args: &[&str]) -> Output {
    Command::new(binary)
        .args(args)
        .current_dir(dir)
        .env("HOME", dir)
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("XDG_DATA_HOME", dir.join("data"))
        .env_remove("GEST_CONFIG")
        .output()
        .unwrap()
}

/// No arguments, `--help`, `-h` and `help` print the product help on stdout, exit 0
/// and leave the directory empty, for both executables.
#[test]
fn root_help_lists_groups_and_commands() {
    for binary in binaries() {
        let temp = tempfile::tempdir().unwrap();
        let mut outputs = Vec::new();
        for args in [&[][..], &["--help"], &["-h"], &["help"]] {
            let out = run(binary, temp.path(), args);
            assert_eq!(out.status.code(), Some(0), "{args:?}");
            assert!(out.stderr.is_empty(), "{args:?}");
            outputs.push(String::from_utf8(out.stdout).unwrap());
        }
        assert!(outputs.iter().all(|help| help == &outputs[0]));
        let help = &outputs[0];
        assert!(help.starts_with(&format!("gossil {}:", env!("CARGO_PKG_VERSION"))));
        assert!(help.contains("Usage: gossil GROUP COMMAND"));
        assert!(help.contains("gos GROUP COMMAND"));
        assert!(help.contains("\n  vcs, v "));
        for planned in ["issue, i", "ui, u", "serve, s"] {
            assert!(help.contains(&format!("\n  {planned} ")), "{planned}");
        }
        assert!(help.contains("Planned groups, not available in this build:"));
        for command in COMMANDS {
            assert!(help.contains(command), "{command}");
        }
        assert!(help.contains("  init, new "));
        assert!(help.contains("  timeline, log "));
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}

/// The `vcs` group prints its help alone, with `--help`, `-h` and `help`, under both
/// group names.
#[test]
fn vcs_group_help_for_every_alias() {
    let temp = tempfile::tempdir().unwrap();
    for binary in binaries() {
        let mut outputs = Vec::new();
        for group in ["vcs", "v"] {
            for tail in [&[][..], &["--help"], &["-h"], &["help"]] {
                let mut args = vec![group];
                args.extend_from_slice(tail);
                let out = run(binary, temp.path(), &args);
                assert_eq!(out.status.code(), Some(0), "{args:?}");
                assert!(out.stderr.is_empty());
                outputs.push(String::from_utf8(out.stdout).unwrap());
            }
        }
        assert!(outputs.iter().all(|help| help == &outputs[0]));
        let help = &outputs[0];
        assert!(help.starts_with("Usage: gossil vcs COMMAND"));
        for command in COMMANDS {
            assert!(help.contains(command), "{command}");
        }
        assert!(!help.contains("issue"));
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

/// Each listed command prints its usage on stdout and exits 0, through
/// `COMMAND --help`, `COMMAND -h`, `vcs help COMMAND` and `help vcs COMMAND`.
#[test]
fn every_listed_command_prints_its_usage() {
    let temp = tempfile::tempdir().unwrap();
    for binary in binaries() {
        for group in ["vcs", "v"] {
            for command in COMMANDS {
                let mut outputs = Vec::new();
                for args in [
                    vec![group, command, "--help"],
                    vec![group, command, "-h"],
                    vec![group, "help", command],
                    vec!["help", group, command],
                ] {
                    let out = run(binary, temp.path(), &args);
                    assert_eq!(out.status.code(), Some(0), "{args:?}");
                    assert!(out.stderr.is_empty(), "{args:?}");
                    let help = String::from_utf8(out.stdout).unwrap();
                    assert!(help.starts_with("Usage: gossil vcs "), "{args:?}: {help}");
                    outputs.push(help);
                }
                assert!(outputs.iter().all(|help| help == &outputs[0]), "{command}");
            }
        }
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

/// Help that a command module already owns keeps its text, and the table
/// gives the exact usage line of each command that has no help of its own.
#[test]
fn command_help_text_matches_each_grammar() {
    let temp = tempfile::tempdir().unwrap();
    for (command, first) in [
        ("init", "Usage: gossil vcs init ?OPTIONS? FILENAME\n"),
        ("new", "Usage: gossil vcs init ?OPTIONS? FILENAME\n"),
        (
            "info",
            "Usage: gossil vcs info --repository REPO --checkout ID\n",
        ),
        (
            "log",
            "Usage: gossil vcs timeline|log --repository REPO --checkout ID\n",
        ),
        (
            "open",
            "Usage: gossil vcs open --repository PATH --checkout ID [--root ROOT]\n",
        ),
        (
            "add",
            "Usage: gossil vcs add --repository REPO --checkout ID --path PATH [--path PATH ...]\n",
        ),
        (
            "status",
            "Usage: gossil vcs status --repository REPO --checkout ID\n",
        ),
        (
            "scope",
            "Usage: gossil vcs scope --repository REPO --checkout ID --scope shared|private --path PATH [--path PATH ...]\n",
        ),
        (
            "commit",
            "Usage: gossil vcs commit --repository REPO --checkout ID --scope private --all-tracked --user ACTOR --comment TEXT --date-override DATE\n",
        ),
        (
            "test-delta-apply",
            "Usage: gossil vcs test-delta-apply ORIGIN DELTA TARGET\n",
        ),
        (
            "test-glob",
            "Usage: gossil vcs test-glob PATTERN STRING ...\n",
        ),
    ] {
        let out = run(
            env!("CARGO_BIN_EXE_gos"),
            temp.path(),
            &["v", command, "--help"],
        );
        let help = String::from_utf8(out.stdout).unwrap();
        assert!(help.starts_with(first), "{command}: {help}");
    }
}

/// Several help words in a row count as one, before the group or between the group
/// and a command name. The entry reports none of them as an unknown name.
#[test]
fn help_after_help_gives_help() {
    let temp = tempfile::tempdir().unwrap();
    let gos = env!("CARGO_BIN_EXE_gos");
    let root = run(gos, temp.path(), &[]).stdout;
    let vcs = run(gos, temp.path(), &["v"]).stdout;
    let open = run(gos, temp.path(), &["v", "open", "--help"]).stdout;
    for (args, expected) in [
        (&["help", "help"][..], &root),
        (&["help", "--help"], &root),
        (&["help", "-h"], &root),
        (&["vcs", "help", "help"], &vcs),
        (&["help", "vcs", "help"], &vcs),
        (&["help", "v", "--help"], &vcs),
        (&["v", "--help", "-h"], &vcs),
        (&["v", "--help", "open"], &open),
        (&["help", "help", "v", "open"], &open),
        (&["--help", "v", "open"], &open),
        (&["-h", "help"], &root),
        (&["vcs", "help", "help", "open"], &open),
        (&["help", "vcs", "help", "open"], &open),
        (&["help", "v", "--help", "open"], &open),
    ] {
        let out = run(gos, temp.path(), args);
        assert_eq!(out.status.code(), Some(0), "{args:?}");
        assert!(out.stderr.is_empty(), "{args:?}");
        assert_eq!(&out.stdout, expected, "{args:?}");
    }
    let out = run(gos, temp.path(), &["v", "--help", "extra"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        out.stderr,
        b"gossil: unknown vcs command \"extra\"\nRun \"gossil vcs --help\" to list the vcs commands.\n"
    );
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

/// An unknown command, a planned group and a diagnostic without its group fail with
/// status 2 and a pointer to the help, with nothing on stdout.
#[test]
fn unknown_commands_point_to_help() {
    let temp = tempfile::tempdir().unwrap();
    let root = "Run \"gossil --help\" to list the commands.\n";
    let vcs = "Run \"gossil vcs --help\" to list the vcs commands.\n";
    for binary in binaries() {
        for (args, expected) in [
            (
                vec!["frob"],
                format!("gossil: unknown command \"frob\"\n{root}"),
            ),
            (
                vec!["--frob"],
                format!("gossil: unknown command \"--frob\"\n{root}"),
            ),
            (
                vec!["test-delta-apply", "a", "b", "c"],
                format!("gossil: unknown command \"test-delta-apply\"\n{root}"),
            ),
            (
                vec!["help", "frob"],
                format!("gossil: unknown command \"frob\"\n{root}"),
            ),
            (
                vec!["issue", "task", "list"],
                format!(
                    "gossil: the group \"issue\" is planned and not available in this build\n{root}"
                ),
            ),
            (
                vec!["u"],
                format!(
                    "gossil: the group \"u\" is planned and not available in this build\n{root}"
                ),
            ),
            (
                vec!["v", "frob"],
                format!("gossil: unknown vcs command \"frob\"\n{vcs}"),
            ),
            (
                vec!["vcs", "help", "frob"],
                format!("gossil: unknown vcs command \"frob\"\n{vcs}"),
            ),
            (
                vec!["help", "v", "frob"],
                format!("gossil: unknown vcs command \"frob\"\n{vcs}"),
            ),
        ] {
            let out = run(binary, temp.path(), &args);
            assert_eq!(out.status.code(), Some(2), "{args:?}");
            assert!(out.stdout.is_empty(), "{args:?}");
            assert_eq!(String::from_utf8(out.stderr).unwrap(), expected, "{args:?}");
        }
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

/// A failed write of the help to stdout exits 1 with one diagnostic line.
#[test]
fn help_write_failure_exits_one() {
    for args in [&["--help"][..], &["v", "--help"], &["v", "open", "--help"]] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_gos"));
        command.args(args);
        let out = closed_stdout::configure(command).output().unwrap();
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert_eq!(out.stderr, b"gossil: unable to write the help\n");
    }
}

#[path = "support/closed_stdout.rs"]
mod closed_stdout;
