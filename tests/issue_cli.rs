//! Both executables recognise every command of the `issue` group under both group
//! names, refuse it with status 69 and change no file (CLI-01, CLI-05).
use std::path::Path;
use std::process::{Command, Output};

/// Both product executables, which share one entry.
fn binaries() -> [&'static str; 2] {
    [env!("CARGO_BIN_EXE_gossil"), env!("CARGO_BIN_EXE_gos")]
}

/// Every command path that runs a handler, as the command table of the CLI page of
/// the specification lists it under `issue`/`i`.
const RUNNABLE: [&str; 84] = [
    "artifact archive",
    "artifact create",
    "artifact delete",
    "artifact list",
    "artifact meta",
    "artifact meta get",
    "artifact meta set",
    "artifact meta unset",
    "artifact note add",
    "artifact note delete",
    "artifact note list",
    "artifact note show",
    "artifact note update",
    "artifact show",
    "artifact tag",
    "artifact untag",
    "artifact update",
    "config get",
    "config set",
    "config show",
    "generate completions",
    "generate man-pages",
    "init",
    "iteration add",
    "iteration advance",
    "iteration cancel",
    "iteration complete",
    "iteration create",
    "iteration delete",
    "iteration graph",
    "iteration link",
    "iteration list",
    "iteration meta",
    "iteration meta get",
    "iteration meta set",
    "iteration meta unset",
    "iteration next",
    "iteration remove",
    "iteration reopen",
    "iteration show",
    "iteration status",
    "iteration tag",
    "iteration unlink",
    "iteration untag",
    "iteration update",
    "migrate",
    "project",
    "project archive",
    "project attach",
    "project delete",
    "project detach",
    "project list",
    "project unarchive",
    "purge",
    "search",
    "self-update",
    "tag",
    "tag add",
    "tag list",
    "tag remove",
    "task block",
    "task cancel",
    "task claim",
    "task complete",
    "task create",
    "task delete",
    "task link",
    "task list",
    "task meta",
    "task meta get",
    "task meta set",
    "task meta unset",
    "task note add",
    "task note delete",
    "task note list",
    "task note show",
    "task note update",
    "task show",
    "task tag",
    "task untag",
    "task unlink",
    "task update",
    "undo",
    "version",
];

/// Every command path of the specification that only selects a subcommand.
const CONTAINERS: [&str; 7] = [
    "artifact",
    "artifact note",
    "config",
    "generate",
    "iteration",
    "task",
    "task note",
];

/// Each alias of the pinned Gest source, written as a path, with its canonical path.
const ALIASES: [(&str, &str); 28] = [
    ("a list", "artifact list"),
    ("artifact new", "artifact create"),
    ("artifact rm", "artifact delete"),
    ("artifact ls", "artifact list"),
    ("artifact view", "artifact show"),
    ("artifact edit", "artifact update"),
    ("artifact meta delete", "artifact meta unset"),
    ("i list", "iteration list"),
    ("iteration new", "iteration create"),
    ("iteration ls", "iteration list"),
    ("iteration rm", "iteration remove"),
    ("iteration view", "iteration show"),
    ("iteration edit", "iteration update"),
    ("iteration meta delete", "iteration meta unset"),
    ("p", "project"),
    ("project rm", "project delete"),
    ("project ls", "project list"),
    ("grep", "search"),
    ("t list", "task list"),
    ("task new", "task create"),
    ("task rm", "task delete"),
    ("task ls", "task list"),
    ("task view", "task show"),
    ("task edit", "task update"),
    ("task meta delete", "task meta unset"),
    ("u", "undo"),
    ("t meta delete", "task meta unset"),
    ("a ls", "artifact list"),
];

/// Run one executable in a directory, with that directory as home and configuration.
///
/// # Arguments
/// * `binary` - The executable to run.
/// * `dir` - The working, home and configuration directory.
/// * `args` - The arguments after the executable name.
///
/// # Returns
/// The finished process output.
fn run(binary: &str, dir: &Path, args: &[&str]) -> Output {
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

/// Run `GROUP PATH ARGS` for one executable and group name.
fn run_path(binary: &str, dir: &Path, group: &str, path: &str, extra: &[&str]) -> Output {
    let mut args = vec![group];
    args.extend(path.split(' '));
    args.extend_from_slice(extra);
    run(binary, dir, &args)
}

/// Each recognised command exits 69 with one refusal line on stderr, under the four
/// executable and group pairs, with or without arguments, and writes no file.
#[test]
fn every_recognised_command_refuses_with_69() {
    let temp = tempfile::tempdir().unwrap();
    for binary in binaries() {
        for group in ["issue", "i"] {
            for path in RUNNABLE {
                for extra in [&[][..], &["x", "--json", "help"]] {
                    let out = run_path(binary, temp.path(), group, path, extra);
                    assert_eq!(out.status.code(), Some(69), "{group} {path} {extra:?}");
                    assert!(out.stdout.is_empty(), "{group} {path}");
                    assert_eq!(
                        String::from_utf8(out.stderr).unwrap(),
                        format!("gossil: issue {path} is not available in this build\n"),
                        "{group} {path}"
                    );
                }
            }
            for (alias, path) in ALIASES {
                let out = run_path(binary, temp.path(), group, alias, &[]);
                assert_eq!(out.status.code(), Some(69), "{alias}");
                assert_eq!(
                    String::from_utf8(out.stderr).unwrap(),
                    format!("gossil: issue {path} is not available in this build\n"),
                    "{alias}"
                );
            }
        }
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

/// Commands whose real forms would write, such as `init` and `task create`, write
/// nothing in a directory that already holds a file, and leave its bytes unchanged.
#[test]
fn refused_writers_leave_files_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let kept = temp.path().join("kept.db");
    std::fs::write(&kept, b"unchanged").unwrap();
    for binary in binaries() {
        for args in [
            &["i", "init", "--local"][..],
            &["issue", "task", "create", "Title", "-d", "text"],
            &["i", "config", "set", "-g", "log.level", "debug"],
            &["i", "generate", "man-pages", "man"],
            &["i", "undo"],
        ] {
            let out = run(binary, temp.path(), args);
            assert_eq!(out.status.code(), Some(69), "{args:?}");
        }
    }
    let names: Vec<_> = std::fs::read_dir(temp.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["kept.db"]);
    assert_eq!(std::fs::read(kept).unwrap(), b"unchanged");
}

/// A command that only selects a subcommand fails with status 2 without one.
#[test]
fn containers_need_a_command() {
    let temp = tempfile::tempdir().unwrap();
    for binary in binaries() {
        for group in ["issue", "i"] {
            for path in CONTAINERS {
                let out = run_path(binary, temp.path(), group, path, &[]);
                assert_eq!(out.status.code(), Some(2), "{path}");
                assert!(out.stdout.is_empty());
                assert_eq!(
                    String::from_utf8(out.stderr).unwrap(),
                    format!(
                        "gossil: issue {path} needs a command\n\
                         Run \"gossil issue {path} --help\" to list its commands.\n"
                    )
                );
            }
        }
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

/// Unknown names exit 2 with a pointer to the help, at the group and below it. The
/// Gest `serve` command and its alias `s` are not issue commands (CLI-02).
#[test]
fn unknown_issue_names_exit_2() {
    let temp = tempfile::tempdir().unwrap();
    let group_help = "Run \"gossil issue --help\" to list the issue commands.\n";
    for binary in binaries() {
        for group in ["issue", "i"] {
            for (path, expected) in [
                (
                    "frob",
                    format!("gossil: unknown issue command \"frob\"\n{group_help}"),
                ),
                (
                    "serve",
                    format!("gossil: unknown issue command \"serve\"\n{group_help}"),
                ),
                (
                    "s",
                    format!("gossil: unknown issue command \"s\"\n{group_help}"),
                ),
                (
                    "--frob",
                    format!("gossil: unknown issue command \"--frob\"\n{group_help}"),
                ),
                (
                    "task frob",
                    "gossil: unknown issue command \"task frob\"\n\
                     Run \"gossil issue task --help\" to list its commands.\n"
                        .into(),
                ),
                (
                    "a note frob",
                    "gossil: unknown issue command \"artifact note frob\"\n\
                     Run \"gossil issue artifact note --help\" to list its commands.\n"
                        .into(),
                ),
            ] {
                let out = run_path(binary, temp.path(), group, path, &[]);
                assert_eq!(out.status.code(), Some(2), "{path}");
                assert!(out.stdout.is_empty());
                assert_eq!(String::from_utf8(out.stderr).unwrap(), expected, "{path}");
            }
        }
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

/// The group help and the product help list every command of the specification,
/// for both executables and both group names, and mark each as not available.
#[test]
fn group_and_product_help_list_every_command() {
    let temp = tempfile::tempdir().unwrap();
    for binary in binaries() {
        let mut outputs = Vec::new();
        for group in ["issue", "i"] {
            for args in [
                vec![group],
                vec![group, "--help"],
                vec![group, "-h"],
                vec![group, "help"],
                vec!["help", group],
            ] {
                let out = run(binary, temp.path(), &args);
                assert_eq!(out.status.code(), Some(0), "{args:?}");
                assert!(out.stderr.is_empty());
                outputs.push(String::from_utf8(out.stdout).unwrap());
            }
        }
        assert!(outputs.iter().all(|help| help == &outputs[0]));
        let root = String::from_utf8(run(binary, temp.path(), &["help"]).stdout).unwrap();
        for help in [&outputs[0], &root] {
            assert!(help.contains("not available in this build"));
            for path in RUNNABLE.iter().chain(&CONTAINERS) {
                let last = path.rsplit(' ').next().unwrap();
                assert!(help.contains(&format!(" {last}")), "{path}");
            }
            assert!(!help.contains("serve "), "{help}");
        }
        assert!(outputs[0].starts_with("Usage: gossil issue COMMAND"));
        assert!(root.contains("\n  issue, i "));
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

/// Each command prints the same help through `--help`, `-h`, `help` before it and
/// `help` before the group, with its usage line and the mark of this build.
#[test]
fn every_command_prints_its_help() {
    let temp = tempfile::tempdir().unwrap();
    let gos = env!("CARGO_BIN_EXE_gos");
    for path in RUNNABLE.iter().chain(&CONTAINERS) {
        let words: Vec<&str> = path.split(' ').collect();
        let mut outputs = Vec::new();
        for args in [
            [&["i"][..], &words, &["--help"]].concat(),
            [&["issue"][..], &words, &["-h"]].concat(),
            [&["i", "help"][..], &words].concat(),
            [&["help", "issue"][..], &words].concat(),
        ] {
            let out = run(gos, temp.path(), &args);
            assert_eq!(out.status.code(), Some(0), "{args:?}");
            assert!(out.stderr.is_empty(), "{args:?}");
            outputs.push(String::from_utf8(out.stdout).unwrap());
        }
        assert!(outputs.iter().all(|help| help == &outputs[0]), "{path}");
        assert!(
            outputs[0].starts_with(&format!("Usage: gossil issue {path}")),
            "{path}"
        );
        assert!(outputs[0].contains("not available in this build"), "{path}");
    }
    let out = run(gos, temp.path(), &["i", "t", "new", "--help"]);
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "Usage: gossil issue task create [OPTIONS] [TITLE]\n\n\
         Create a new task.\n\
         Names: create, new.\n\
         This build recognises the command and does not run it: it is not available in this build.\n"
    );
    let out = run(gos, temp.path(), &["help", "i", "task", "create", "extra"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(out.stderr).unwrap(),
        "Usage: gossil issue help COMMAND\n\
         Run \"gossil issue --help\" to list the issue commands.\n"
    );
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}
