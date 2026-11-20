//! Route native repository initialization and repository-free Fossil delta diagnostics.
use crate::delta::{DeltaError, Limits, apply, create};
use std::collections::TryReserveError;
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Adapter failures retain their original I/O or allocation cause.
#[derive(Debug)]
enum CommandError {
    /// The argument count or command does not match `vcs test-delta-apply`.
    Usage,
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

/// Dispatch repository-free diagnostics while preserving the existing apply adapter.
///
/// # Arguments
/// - `args` - Arguments after the executable name, with the command group normalized to `vcs`.
///
/// # Returns
/// Success after the requested diagnostic.
///
/// # Errors
/// Returns a usage, file or delta error.
fn dispatch(args: &[OsString]) -> Result<(), CommandError> {
    if args.first().is_some_and(|arg| arg == "vcs") {
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

/// Print the adapter error once, with the native newline and exit status.
///
/// # Returns
/// The success exit code after initialization or a completed diagnostic, or the failure exit code
/// after printing its error to standard error. The first argument `v` is accepted
/// as an alias for `vcs`.
pub fn entry() -> std::process::ExitCode {
    let mut args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg| arg == "v") {
        args[0] = OsString::from("vcs");
    }
    if args.first().is_some_and(|arg| arg == "vcs")
        && args.get(1).is_some_and(|arg| arg == "init" || arg == "new")
    {
        return match crate::vcs_init::run(&args[2..]) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                std::process::ExitCode::FAILURE
            }
        };
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
