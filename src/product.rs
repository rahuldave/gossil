//! Route native initialization, checkout open, information, tracking, recording, timeline, delta and glob diagnostics,
//! and the recognised commands of the `issue` group.
use crate::delta::{DeltaError, Limits, apply, create};
use std::collections::TryReserveError;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

mod help;
mod issue;

/// Adapter failures retain their original I/O or allocation cause.
#[derive(Debug)]
enum CommandError {
    /// The argument count does not match `vcs test-delta-apply`.
    ///
    /// The `route` function rejects unknown commands before this check.
    Usage,
    /// Invalid glob diagnostic arguments.
    UsageGlob,
    /// Glob budget or allocation failure.
    Glob(crate::glob::GlobError),
    /// Settings need repository integration.
    GlobSetting,
    /// A non-Unix argument cannot be represented as Unicode.
    #[cfg(any(not(unix), test))]
    GlobEncoding,
    /// Invalid create arguments.
    UsageCreate,
    /// Invalid round-trip arguments.
    UsageTest,
    /// Creation or round-trip verification failed.
    Create(DeltaError),
    /// Decoded bytes differ from the requested target.
    TestFailed,
    /// Input could not be read.
    Read {
        /// File whose operation failed.
        path: PathBuf,
        /// Original filesystem or stream error.
        source: std::io::Error,
    },
    /// A file buffer could not grow.
    ReadAllocation {
        /// Original fallible file-buffer reservation error.
        source: TryReserveError,
    },
    /// Delta validation or reservation failed.
    Decode(DeltaError),
    /// Target could not be opened or written.
    Write {
        /// File whose operation failed.
        path: PathBuf,
        /// Original filesystem or stream error.
        source: std::io::Error,
    },
}
impl std::fmt::Display for CommandError {
    /// Preserve the characterized diagnostic framing and program-name substitution.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UsageGlob => write!(f, "Usage: gossil vcs test-glob PATTERN STRING ..."),
            Self::Glob(e) => write!(f, "{e}"),
            Self::GlobSetting => write!(f, "test-glob setting lookup is not implemented"),
            #[cfg(any(not(unix), test))]
            Self::GlobEncoding => write!(f, "test-glob argument is not valid Unicode"),
            Self::UsageCreate => {
                write!(f, "Usage: gossil vcs test-delta-create ORIGIN TARGET DELTA")
            }
            Self::UsageTest => write!(f, "Usage: gossil vcs test-delta FILE1 FILE2"),
            Self::Create(_) | Self::TestFailed => write!(f, "delta test failed"),
            Self::Usage => write!(f, "Usage: gossil vcs test-delta-apply ORIGIN DELTA TARGET"),
            Self::Read { path, source } if source.kind() == std::io::ErrorKind::NotFound => {
                write!(f, "no such file: {}", path.display())
            }
            Self::Read { path, source } => {
                write!(f, "unable to read file \"{}\": {source}", path.display())
            }
            Self::Decode(_) | Self::ReadAllocation { .. } => write!(f, "Delta application failed."),
            Self::Write { path, .. } => {
                write!(f, "unable to open file \"{}\" for writing", path.display())
            }
        }
    }
}
impl std::error::Error for CommandError {
    /// Return the original error, including typed nested decoder failures.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            #[cfg(any(not(unix), test))]
            Self::GlobEncoding => None,
            Self::Glob(e) => Some(e),
            Self::UsageGlob | Self::GlobSetting => None,
            Self::Read { source, .. } | Self::Write { source, .. } => Some(source),
            Self::ReadAllocation { source } => Some(source),
            Self::Decode(source) | Self::Create(source) => Some(source),
            Self::Usage | Self::UsageCreate | Self::UsageTest | Self::TestFailed => None,
        }
    }
}

/// Read chunks with explicit fallible growth rather than infallible `read_to_end`.
///
/// # Arguments
/// - `path` - Input path included in read errors.
/// - `reader` - Stream read until EOF, retrying interrupted reads.
/// - `reserve` - Fallible buffer-growth operation called before appending each chunk.
///
/// # Returns
/// The input bytes.
///
/// # Errors
/// Returns a read or allocation error with its original cause.
fn read_buffer(
    path: &Path,
    mut reader: impl Read,
    mut reserve: impl FnMut(&mut Vec<u8>, usize) -> Result<(), TryReserveError>,
) -> Result<Vec<u8>, CommandError> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let count = match reader.read(&mut chunk) {
            Ok(0) => return Ok(bytes),
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(source) => {
                return Err(CommandError::Read {
                    path: path.into(),
                    source,
                });
            }
        };
        reserve(&mut bytes, count).map_err(|source| CommandError::ReadAllocation { source })?;
        bytes.extend_from_slice(&chunk[..count]);
    }
}

/// Attach the actual input path to file-open and stream errors.
///
/// # Arguments
/// - `path` - File to open and read.
///
/// # Returns
/// The file bytes.
///
/// # Errors
/// Returns an open, read or allocation error.
fn read_file(path: &Path) -> Result<Vec<u8>, CommandError> {
    let file = File::open(path).map_err(|source| CommandError::Read {
        path: path.into(),
        source,
    })?;
    read_buffer(path, file, Vec::try_reserve)
}

/// Read and decode both inputs before opening the target, allowing input aliases.
///
/// # Arguments
/// - `args` - Command group, apply command, origin path, delta path and target path.
/// - `decode` - Decoder called with the input bytes and default resource limits.
///
/// # Returns
/// Success after writing the decoded bytes.
///
/// # Errors
/// Returns a usage, input, decode or write error.
fn run_with(
    args: &[OsString],
    decode: impl FnOnce(&[u8], &[u8], Limits) -> Result<Vec<u8>, DeltaError>,
) -> Result<(), CommandError> {
    if args.len() != 5 || args[0] != "vcs" || args[1] != "test-delta-apply" {
        return Err(CommandError::Usage);
    }
    let source = read_file(Path::new(&args[2]))?;
    let delta = read_file(Path::new(&args[3]))?;
    let output = decode(&source, &delta, Limits::default()).map_err(CommandError::Decode)?;
    let path = Path::new(&args[4]);
    File::create(path)
        .and_then(|mut file| file.write_all(&output))
        .map_err(|source| CommandError::Write {
            path: path.into(),
            source,
        })
}

/// Execute both directions before reporting a successful round trip.
///
/// # Arguments
/// - `source` - Bytes used as the origin in the first direction.
/// - `target` - Bytes used as the origin in the reverse direction.
///
/// # Returns
/// Success if both delta round trips reproduce their targets.
///
/// # Errors
/// Returns the first creation, decode or mismatch error.
fn round_trip(source: &[u8], target: &[u8]) -> Result<(), CommandError> {
    round_trip_with(source, target, create, apply)
}

/// Keep both directions observable to failure-injection tests.
///
/// # Arguments
/// - `source` - Bytes used as the origin in the first direction.
/// - `target` - Bytes used as the origin in the reverse direction.
/// - `encode` - Delta creator called once per direction until a failure occurs.
/// - `decode` - Delta applier called with default resource limits after each creation.
///
/// # Returns
/// Success if both decoded results match.
///
/// # Errors
/// Returns the first creation, decode or mismatch error.
fn round_trip_with(
    source: &[u8],
    target: &[u8],
    mut encode: impl FnMut(&[u8], &[u8]) -> Result<Vec<u8>, DeltaError>,
    mut decode: impl FnMut(&[u8], &[u8], Limits) -> Result<Vec<u8>, DeltaError>,
) -> Result<(), CommandError> {
    for (from, to) in [(source, target), (target, source)] {
        let delta = encode(from, to).map_err(CommandError::Create)?;
        let output = decode(from, &delta, Limits::default()).map_err(CommandError::Create)?;
        if output != to {
            return Err(CommandError::TestFailed);
        }
    }
    Ok(())
}

/// Accept Unicode without substitution for platforms without Unix byte arguments.
/// # Errors
/// Returns the encoding error when the argument is not representable as Unicode.
#[cfg(any(not(unix), test))]
fn unicode_glob_argument(arg: &OsStr) -> Result<&[u8], CommandError> {
    arg.to_str()
        .map(str::as_bytes)
        .ok_or(CommandError::GlobEncoding)
}

/// Preserve Unix argument bytes; elsewhere accept only lossless Unicode as UTF-8.
/// # Arguments
/// * `arg` - Operating-system argument to borrow without character substitution.
/// # Returns
/// Exact argument bytes on Unix, or UTF-8 bytes of a valid Unicode argument elsewhere.
/// # Errors
/// On non-Unix platforms, returns the encoding error for an unrepresentable argument.
fn glob_argument(arg: &OsStr) -> Result<&[u8], CommandError> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(arg.as_bytes())
    }
    #[cfg(not(unix))]
    {
        unicode_glob_argument(arg)
    }
}

/// Run the repository-free glob diagnostic with a caller-supplied output stream.
/// Accept one literal pattern list and at least one candidate; all aliases share
/// fixed usage spelling. Leading `@` lists reject settings lookup. Unix arguments
/// retain raw bytes, while other platforms require Unicode. Output contains raw
/// pattern/candidate bytes, zero-based pattern labels and one-based match indices.
/// The fixed byte/count budgets do not bound engine matching time or stack use.
///
/// # Arguments
/// * `args` - Normalized group, command, pattern list and candidates.
/// * `out` - Destination for exact diagnostic bytes.
/// # Returns
/// Success after writing all pattern and candidate rows.
/// # Errors
/// Returns invalid arguments, glob failures or an original output I/O cause.
fn run_glob(args: &[OsString], mut out: impl Write) -> Result<(), CommandError> {
    use crate::glob::{glob_create, glob_expr, glob_multi_match};
    if args.len() < 4 {
        return Err(CommandError::UsageGlob);
    }
    let list = glob_argument(&args[2])?;
    if list.starts_with(b"@") {
        return Err(CommandError::GlobSetting);
    }
    let limits = crate::glob::Limits {
        list_bytes: 1_048_576,
        candidate_bytes: 1_048_576,
        patterns: 65_536,
        expression_bytes: 16_777_216,
    };
    let glob = glob_create(Some(list), limits).map_err(CommandError::Glob)?;
    let expr = glob_expr(b"x", Some(list), limits).map_err(CommandError::Glob)?;
    let write_error = |source| CommandError::Write {
        path: PathBuf::from("<stdout>"),
        source,
    };
    out.write_all(b"SQL expression: ").map_err(write_error)?;
    out.write_all(&expr).map_err(write_error)?;
    out.write_all(b"\n").map_err(write_error)?;
    for (i, pattern) in glob.patterns().enumerate() {
        write!(out, "pattern[{i}] = [").map_err(write_error)?;
        out.write_all(pattern).map_err(write_error)?;
        out.write_all(b"]\n").map_err(write_error)?;
    }
    for value in &args[3..] {
        let value = glob_argument(value)?;
        let first = glob.glob_match(value).map_err(CommandError::Glob)?;
        let second = glob_multi_match(Some(list), value, limits).map_err(CommandError::Glob)?;
        write!(out, "{first} {second} ").map_err(write_error)?;
        out.write_all(value).map_err(write_error)?;
        out.write_all(b"\n").map_err(write_error)?;
    }
    Ok(())
}

/// Dispatch repository-free diagnostics while preserving the existing apply adapter.
///
/// # Arguments
/// - `args` - Arguments after the executable name, with the command group normalized to `vcs`.
///
/// # Returns
/// Success after the requested diagnostic.
///
/// # Errors
/// Returns a usage, file, delta, glob or output error.
fn dispatch(args: &[OsString]) -> Result<(), CommandError> {
    if args.first().is_some_and(|arg| arg == "vcs") {
        if args.get(1).is_some_and(|arg| arg == "test-glob") {
            return run_glob(args, std::io::stdout().lock());
        }

        if args.get(1).is_some_and(|arg| arg == "test-delta-create") {
            if args.len() != 5 {
                return Err(CommandError::UsageCreate);
            }
            let source = read_file(Path::new(&args[2]))?;
            let target = read_file(Path::new(&args[3]))?;
            let output = create(&source, &target).map_err(CommandError::Create)?;
            let path = Path::new(&args[4]);
            return File::create(path)
                .and_then(|mut file| file.write_all(&output))
                .map_err(|source| CommandError::Write {
                    path: path.into(),
                    source,
                });
        }
        if args.get(1).is_some_and(|arg| arg == "test-delta") {
            if args.len() != 4 {
                return Err(CommandError::UsageTest);
            }
            let source = read_file(Path::new(&args[2]))?;
            let target = read_file(Path::new(&args[3]))?;
            round_trip(&source, &target)?;
            println!("ok");
            return Ok(());
        }
    }
    run_with(args, apply)
}

/// Holds the first decision of the product entry, which is to print help, refuse
/// the arguments or run a command.
#[derive(Debug, PartialEq)]
enum Route {
    /// Write this help text to stdout and exit 0.
    Print(String),
    /// Write this diagnostic to stderr and exit 2, as a parser failure.
    Fail(String),
    /// Run the command, with the group normalized to `vcs`.
    Dispatch(Vec<OsString>),
    /// Run the handler of a recognised issue command, with its canonical path
    /// and the arguments after its last name.
    Issue(help::IssueHandler, String, Vec<OsString>),
}

/// Answer help requests and unknown names before any command runs.
///
/// A help word is `--help`, `-h` or `help`. Help words before the group, or
/// between the group and a command name, ask for help, and several of them in a
/// row count as one. So no arguments, `help` and `help help` give the product
/// help, `vcs` and `vcs --help` give the group help, and `help vcs open`,
/// `vcs help open` and `vcs --help -h open` give the help of `open`. A lone
/// `--help` or `-h` after a command also gives its help.
///
/// A command module that prints its own help gets the request as
/// `vcs COMMAND --help`, or as `vcs COMMAND -h` when a lone `-h` follows the
/// command.
///
/// A help request with an argument after the command name is a parser
/// failure. So is a name that is
/// neither a group nor a command of this build. Each diagnostic points to the help.
///
/// The group `issue`, or its alias `i`, hands the rest to the issue parser,
/// which applies the same help rules at each level of its command tree.
///
/// # Arguments
/// * `args` - The arguments after the executable name.
///
/// # Returns
/// The help to print, the diagnostic to fail with, or the arguments to dispatch.
fn route(args: Vec<OsString>) -> Route {
    let vcs = || OsString::from("vcs");
    let skip = |words: &[OsString]| words.iter().take_while(|arg| help::is_help(arg)).count();
    let leading = skip(&args);
    let Some(group) = args.get(leading) else {
        return Route::Print(help::root());
    };
    if group == "issue" || group == "i" {
        return match issue::route(&args[leading + 1..], leading > 0) {
            issue::IssueRoute::Print(text) => Route::Print(text),
            issue::IssueRoute::Fail(message) => Route::Fail(message),
            issue::IssueRoute::Run {
                handler,
                path,
                args,
            } => Route::Issue(handler, path, args),
        };
    }
    if group != "vcs" && group != "v" {
        return Route::Fail(help::unknown_group(group));
    }
    let after = &args[leading + 1..];
    let between = skip(after);
    let command = &after[between..];
    let Some(name) = command.first() else {
        return Route::Print(help::vcs());
    };
    if !help::is_vcs_command(name) {
        return Route::Fail(help::unknown_vcs(name));
    }
    let asked = leading + between > 0;
    if asked && command.len() > 1 {
        return Route::Fail(
            "Usage: gossil vcs help COMMAND\nRun \"gossil vcs --help\" to list the vcs commands."
                .into(),
        );
    }
    let flagged = command.len() == 2 && help::is_help_flag(&command[1]);
    if !asked && !flagged {
        return Route::Dispatch([vcs()].into_iter().chain(command.iter().cloned()).collect());
    }
    match help::command(name) {
        Some(text) if !help::owns_help(name) => Route::Print(text),
        _ => Route::Dispatch(vec![
            vcs(),
            name.clone(),
            if flagged {
                command[1].clone()
            } else {
                OsString::from("--help")
            },
        ]),
    }
}

/// Print a failed command's diagnostic once and choose its exit status.
///
/// # Arguments
/// * `error` - The failure that the command returned.
///
/// # Returns
/// 78, the code of an unusable configuration, when the error chain holds an older
/// Gossil catalog (CAT-07), and 1 for every other failure.
fn failed(error: &(dyn std::error::Error + 'static)) -> std::process::ExitCode {
    eprintln!("{error}");
    let mut cause = Some(error);
    while let Some(current) = cause {
        if current.is::<crate::storage::OlderCatalog>() {
            return std::process::ExitCode::from(78);
        }
        cause = current.source();
    }
    std::process::ExitCode::FAILURE
}

/// Print the adapter error once, with the native newline and exit status.
///
/// # Returns
/// The success exit code after initialization, native checkout open, tracking,
/// scope selection, private recording, checkout timeline/information or a completed diagnostic. Returns the failure exit code
/// after printing an adapter error to standard error. A repository at an older Gossil catalog
/// returns 78. The first argument `v` is accepted
/// as an alias for `vcs`. In the `issue` group, or its alias `i`, `init` runs
/// and returns its own status, 0 on success. Every other recognised issue
/// command writes a refusal to stderr and returns 69, because this build does
/// not run it. A help request prints the help on stdout and returns 0.
/// If the write of the help fails, the entry returns 1. An unknown group or command
/// returns 2, as a parser failure, with a pointer to the help on stderr.
/// # Panics
/// Panics if writing an error diagnostic to standard error fails.
pub fn entry() -> std::process::ExitCode {
    let args = match route(std::env::args_os().skip(1).collect()) {
        Route::Dispatch(args) => args,
        Route::Print(text) => {
            return if std::io::stdout().lock().write_all(text.as_bytes()).is_ok() {
                std::process::ExitCode::SUCCESS
            } else {
                eprintln!("gossil: unable to write the help");
                std::process::ExitCode::FAILURE
            };
        }
        Route::Fail(message) => {
            eprintln!("{message}");
            return std::process::ExitCode::from(2);
        }
        Route::Issue(handler, path, args) => return issue::run(handler, &path, &args),
    };
    if args.first().is_some_and(|arg| arg == "vcs")
        && args.get(1).is_some_and(|arg| arg == "init" || arg == "new")
    {
        return match crate::vcs_init::run(&args[2..]) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(error) => failed(&*error),
        };
    }
    if args.first().is_some_and(|arg| arg == "vcs") && args.get(1).is_some_and(|arg| arg == "info")
    {
        return crate::vcs_info::run(&args[2..]).map_or_else(
            |error| failed(&*error),
            |()| std::process::ExitCode::SUCCESS,
        );
    }
    if args.first().is_some_and(|arg| arg == "vcs") && args.get(1).is_some_and(|arg| arg == "open")
    {
        return crate::vcs_open::run(&args[2..]).map_or_else(
            |error| failed(&*error),
            |()| std::process::ExitCode::SUCCESS,
        );
    }
    if args.first().is_some_and(|arg| arg == "vcs")
        && args
            .get(1)
            .is_some_and(|arg| arg == "add" || arg == "status")
    {
        return crate::vcs_tracking::run(&args[2..], args[1] == "add").map_or_else(
            |error| failed(&*error),
            |()| std::process::ExitCode::SUCCESS,
        );
    }
    if args.first().is_some_and(|arg| arg == "vcs")
        && args
            .get(1)
            .is_some_and(|arg| arg == "scope" || arg == "commit")
    {
        return crate::vcs_recording::run(&args[2..], args[1] == "commit").map_or_else(
            |error| failed(&*error),
            |()| std::process::ExitCode::SUCCESS,
        );
    }
    if args.first().is_some_and(|arg| arg == "vcs")
        && args
            .get(1)
            .is_some_and(|arg| arg == "timeline" || arg == "log")
    {
        return crate::vcs_timeline::run(&args[2..]).map_or_else(
            |error| failed(&*error),
            |()| std::process::ExitCode::SUCCESS,
        );
    }
    match dispatch(&args) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    /// Turn string slices into the argument vector that `route` receives.
    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    /// Help forms route to the matching help, and every other form keeps its dispatch.
    #[test]
    fn route_answers_help_and_rejects_unknown_names() {
        for args in [
            &[][..],
            &["help"],
            &["--help"],
            &["-h"],
            &["help", "help"],
            &["help", "--help"],
            &["help", "-h", "help"],
        ] {
            assert_eq!(route(os(args)), Route::Print(help::root()));
        }
        for args in [
            &["v"][..],
            &["vcs", "help"],
            &["help", "v"],
            &["v", "-h"],
            &["vcs", "help", "help"],
            &["help", "vcs", "help"],
            &["help", "v", "--help"],
            &["v", "--help", "-h"],
        ] {
            assert_eq!(route(os(args)), Route::Print(help::vcs()));
        }
        let open = help::command(OsStr::new("open")).unwrap();
        for args in [
            &["v", "open", "--help"][..],
            &["v", "help", "open"],
            &["help", "vcs", "open"],
            &["help", "help", "vcs", "open"],
            &["v", "--help", "open"],
            &["v", "-h", "open"],
            &["--help", "v", "open"],
        ] {
            assert_eq!(route(os(args)), Route::Print(open.clone()));
        }
        assert_eq!(
            route(os(&["help", "v", "init"])),
            Route::Dispatch(os(&["vcs", "init", "--help"]))
        );
        assert_eq!(
            route(os(&["v", "init", "help"])),
            Route::Dispatch(os(&["vcs", "init", "help"]))
        );
        assert_eq!(
            route(os(&["v", "open", "--help", "x"])),
            Route::Dispatch(os(&["vcs", "open", "--help", "x"]))
        );
        assert_eq!(
            route(os(&["v", "help", "open", "extra"])),
            Route::Fail(
                "Usage: gossil vcs help COMMAND\nRun \"gossil vcs --help\" to list the vcs commands."
                    .into()
            )
        );
        for args in [
            &["help", "issue"][..],
            &["i"],
            &["issue", "--help"],
            &["-h", "i"],
        ] {
            assert_eq!(route(os(args)), Route::Print(help::issue()), "{args:?}");
        }
        assert_eq!(
            route(os(&["help", "i", "frob"])),
            Route::Fail(help::unknown_issue("", OsStr::new("frob")))
        );
        assert_eq!(
            route(os(&["help", "ui"])),
            Route::Fail(help::unknown_group(OsStr::new("ui")))
        );
        assert_eq!(
            route(vec![OsString::from("help"); 100_000]),
            Route::Print(help::root())
        );
        let mut deep = vec![OsString::from("v")];
        deep.extend(vec![OsString::from("-h"); 100_000]);
        deep.push(OsString::from("open"));
        assert_eq!(route(deep), Route::Print(open.clone()));
        for args in [
            &["vcs", "help", "help", "open"][..],
            &["v", "--help", "-h", "open"],
            &["help", "vcs", "help", "open"],
            &["help", "v", "--help", "open"],
        ] {
            assert_eq!(route(os(args)), Route::Print(open.clone()), "{args:?}");
        }
        assert_eq!(
            route(os(&["v", "init", "-h"])),
            Route::Dispatch(os(&["vcs", "init", "-h"]))
        );
        assert_eq!(
            route(os(&["v", "test-delta", "a", "b"])),
            Route::Dispatch(os(&["vcs", "test-delta", "a", "b"]))
        );
        assert_eq!(
            route(os(&["vcs", "--help", "extra"])),
            Route::Fail(help::unknown_vcs(OsStr::new("extra")))
        );
        assert_eq!(
            route(os(&["vcs", "frob"])),
            Route::Fail(help::unknown_vcs(OsStr::new("frob")))
        );
    }

    /// The help table names every command that the entry dispatches, and rejects
    /// other names such as `help` and `frob`.
    #[test]
    fn help_table_matches_dispatch() {
        for name in [
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
        ] {
            assert!(help::is_vcs_command(OsStr::new(name)), "{name}");
            let text = help::command(OsStr::new(name)).unwrap();
            assert!(text.starts_with("Usage: gossil vcs "), "{name}");
        }
        for name in ["help", "frob", "diff", "push", "-h", ""] {
            assert!(!help::is_vcs_command(OsStr::new(name)), "{name}");
            assert!(help::command(OsStr::new(name)).is_none());
        }
        let root = help::root();
        for command in help::ISSUE_COMMANDS {
            assert!(root.contains(&format!("\n  {} ", command.names.join(", "))));
            for group in ["issue", "i"] {
                let routed = route(os(&[group, command.names[0], "--help"]));
                assert!(
                    matches!(routed, Route::Print(ref text) if text.lines().next()
                        .is_some_and(|line| line.split(' ').nth(3) == Some(command.names[0]))),
                    "{routed:?}"
                );
            }
        }
        for (args, handler, path) in [
            (&["i", "init"][..], help::IssueHandler::Init, "init"),
            (
                &["issue", "task", "create"],
                help::IssueHandler::TaskCreate,
                "task create",
            ),
            (&["i", "t", "ls"], help::IssueHandler::TaskList, "task list"),
            (
                &["issue", "task", "show"],
                help::IssueHandler::TaskShow,
                "task show",
            ),
            (&["i", "u"], help::IssueHandler::Unavailable, "undo"),
        ] {
            assert_eq!(
                route(os(args)),
                Route::Issue(handler, path.into(), Vec::new())
            );
        }
        assert_eq!(
            help::command(OsStr::new("log")).unwrap(),
            "Usage: gossil vcs timeline|log --repository REPO --checkout ID\n\n\
             Print the complete recorded ancestry as NDJSON.\nNames: timeline, log.\n"
        );
    }

    /// A failed glob output retains the original typed stream error.
    #[test]
    fn glob_stdout_error_preserves_cause() {
        /// Output stream that deterministically returns a broken-pipe cause.
        struct Denied;
        impl Write for Denied {
            /// Reject every write with the original broken-pipe error.
            /// # Errors
            /// Always returns the broken-pipe error used by this control.
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
            }
            /// Complete a flush without changing the deterministic write failure.
            /// # Errors
            /// This control returns no flush error.
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let args = ["vcs", "test-glob", "a", "a"].map(OsString::from);
        let mut out = Denied;
        assert!(out.flush().is_ok());
        let error = run_glob(&args, &mut out).unwrap_err();
        assert_eq!(
            error.to_string(),
            "unable to open file \"<stdout>\" for writing"
        );
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .kind(),
            std::io::ErrorKind::BrokenPipe
        );
    }

    /// Unsupported CLI forms carry usage diagnostics without an invented cause.
    #[test]
    fn glob_cli_usage_diagnostics() {
        for (args, expected) in [
            (
                vec!["vcs", "test-glob"],
                "Usage: gossil vcs test-glob PATTERN STRING ...",
            ),
            (
                vec!["vcs", "test-glob", "@ignore-glob", "x"],
                "test-glob setting lookup is not implemented",
            ),
        ] {
            let args: Vec<OsString> = args.into_iter().map(OsString::from).collect();
            let error = dispatch(&args).unwrap_err();
            assert_eq!(error.to_string(), expected);
            assert!(error.source().is_none());
        }
    }

    /// CLI budgets reject oversized lists before writing output and retain the cause.
    #[test]
    fn glob_cli_budget_diagnostic() {
        let args = [
            OsString::from("vcs"),
            OsString::from("test-glob"),
            OsString::from("a".repeat(1_048_577)),
            OsString::from("a"),
        ];
        let error = dispatch(&args).unwrap_err();
        assert_eq!(error.to_string(), "glob list limit exceeded");
        let cause = error.source().unwrap();
        assert!(matches!(
            cause.downcast_ref::<crate::glob::GlobError>(),
            Some(crate::glob::GlobError::Limit("list"))
        ));
    }

    /// Preserve Unicode UTF-8 exactly and reject unrepresentable portable arguments.
    #[test]
    fn glob_argument_encoding_policy() {
        let unicode = OsStr::new("é😀");
        assert_eq!(unicode_glob_argument(unicode).unwrap(), "é😀".as_bytes());
        assert_eq!(glob_argument(unicode).unwrap(), "é😀".as_bytes());
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let invalid = OsStr::from_bytes(&[0xff, b'a']);
            assert_eq!(glob_argument(invalid).unwrap(), &[0xff, b'a']);
            let error = unicode_glob_argument(invalid).unwrap_err();
            assert_eq!(error.to_string(), "test-glob argument is not valid Unicode");
            assert!(error.source().is_none());
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            let invalid = OsString::from_wide(&[0xd800]);
            assert!(unicode_glob_argument(&invalid).is_err());
            assert!(glob_argument(&invalid).is_err());
        }
    }

    use super::*;
    use std::error::Error;

    /// Exercise recoverable file-buffer allocation and interrupted/error reads.
    #[test]
    fn read_causes() {
        let error = read_buffer(Path::new("input"), b"A".as_slice(), |_, _| {
            Vec::<u8>::new().try_reserve(usize::MAX)
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "Delta application failed.");
        assert!(
            error
                .source()
                .unwrap()
                .downcast_ref::<TryReserveError>()
                .is_some()
        );
        let mut calls = 0;
        let mut reserve = |bytes: &mut Vec<u8>, count| {
            calls += 1;
            bytes.try_reserve(count)
        };
        assert!(
            read_buffer(Path::new("input"), std::io::empty(), &mut reserve)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            read_buffer(Path::new("input"), b"A".as_slice(), &mut reserve).unwrap(),
            b"A"
        );
        assert_eq!(calls, 1);
        struct InterruptedThenDenied(bool);
        impl Read for InterruptedThenDenied {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                let kind = if self.0 {
                    std::io::ErrorKind::PermissionDenied
                } else {
                    self.0 = true;
                    std::io::ErrorKind::Interrupted
                };
                Err(std::io::Error::from(kind))
            }
        }
        let reader = InterruptedThenDenied(false);
        let error = read_buffer(Path::new("input"), reader, Vec::try_reserve).unwrap_err();
        assert!(error.to_string().contains("unable to read file"));
        assert!(
            error
                .source()
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .is_some()
        );
    }

    /// Adopt decode-failure framing for allocation failures while preserving target bytes.
    #[test]
    fn allocation_preserves_target_and_error_chains() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let delta = dir.path().join("delta");
        let target = dir.path().join("target");
        std::fs::write(&source, b"").unwrap();
        std::fs::write(&delta, b"1\n1:A;").unwrap();
        std::fs::write(&target, b"preserve").unwrap();
        let args = [
            OsString::from("vcs"),
            OsString::from("test-delta-apply"),
            source.into_os_string(),
            delta.into_os_string(),
            target.clone().into_os_string(),
        ];
        let error = run_with(&args, |_, _, _| {
            Err(DeltaError::AllocationFailed {
                requested: 1,
                source: Vec::<u8>::new().try_reserve_exact(usize::MAX).unwrap_err(),
            })
        })
        .unwrap_err();
        assert_eq!(format!("{error}\n"), "Delta application failed.\n");
        assert!(
            error
                .source()
                .unwrap()
                .downcast_ref::<DeltaError>()
                .unwrap()
                .source()
                .unwrap()
                .downcast_ref::<TryReserveError>()
                .is_some()
        );
        assert_eq!(std::fs::read(target).unwrap(), b"preserve");
        assert!(CommandError::Usage.source().is_none());
        let error = CommandError::Write {
            path: dir.path().into(),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        };
        assert!(
            error
                .source()
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .is_some()
        );
    }
    /// Reject second-direction mismatches and preserve nested encode/decode causes.
    #[test]
    fn both_directions_are_verified() {
        let mut calls = Vec::new();
        let mut decodes = 0;
        let error = round_trip_with(
            b"first",
            b"second",
            |from, to| {
                calls.push((from.to_vec(), to.to_vec()));
                create(from, to)
            },
            |from, delta, limits| {
                decodes += 1;
                if decodes == 2 {
                    Ok(b"mismatch".to_vec())
                } else {
                    apply(from, delta, limits)
                }
            },
        )
        .unwrap_err();
        assert!(matches!(error, CommandError::TestFailed));
        assert_eq!(
            calls,
            [
                (b"first".to_vec(), b"second".to_vec()),
                (b"second".to_vec(), b"first".to_vec())
            ]
        );
        assert_eq!(decodes, 2);
        assert_eq!(error.to_string(), "delta test failed");
        assert!(error.source().is_none());
        let error =
            round_trip_with(b"a", b"b", |_, _| Err(DeltaError::Unterminated), apply).unwrap_err();
        assert!(
            error
                .source()
                .unwrap()
                .downcast_ref::<DeltaError>()
                .is_some()
        );
        let error = round_trip_with(b"a", b"b", create, |_, _, _| Err(DeltaError::Unterminated))
            .unwrap_err();
        assert!(
            error
                .source()
                .unwrap()
                .downcast_ref::<DeltaError>()
                .is_some()
        );
    }
}
