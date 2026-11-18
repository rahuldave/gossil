//! Compatibility adapter for the bounded delta decoder; no repository is opened.
use gossil::delta::{DeltaError, Limits, apply};
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
            Self::Decode(source) => Some(source),
            Self::Usage => None,
        }
    }
}

/// Read chunks with explicit fallible growth rather than infallible `read_to_end`.
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
fn read_file(path: &Path) -> Result<Vec<u8>, CommandError> {
    let file = File::open(path).map_err(|source| CommandError::Read {
        path: path.into(),
        source,
    })?;
    read_buffer(path, file, Vec::try_reserve)
}

/// Read and decode both inputs before opening the target, allowing input aliases.
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

/// Print the adapter error once, with the native newline and exit status.
fn main() -> std::process::ExitCode {
    match run_with(&std::env::args_os().skip(1).collect::<Vec<_>>(), apply) {
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
}
