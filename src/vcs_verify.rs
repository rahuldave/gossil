//! Run the full proof of a repository, or of one issue, and print a short summary.
//!
//! `gos v verify --repository REPO` replays every check-in and every issue
//! artifact, and compares every native and typed table with that replay.
//! `--issue ID` proves one issue instead. Both forms change nothing.
//!
//! ## Recommendations
//! Keep the exit statuses of CLI-05 here. A proof that fails exits with 74. An
//! invalid prefix exits with 64. A prefix that names no issue, or more than one,
//! exits with 66.
use crate::gest_model::primitives::Id;
use crate::storage::{IssueMatch, LocalOwnerRepository, StorageError};
use std::{ffi::OsString, io::Write, path::PathBuf, process::ExitCode};

/// The usage line, which help and every usage error print.
const USAGE: &str = "Usage: gossil vcs verify --repository REPO [--issue ID]";

/// The parsed arguments of one call.
#[derive(Debug, PartialEq)]
enum Request {
    /// A lone `--help` or `-h`.
    Help,
    /// Prove the repository, or the one issue that `issue` names.
    Verify {
        /// The repository file.
        repository: PathBuf,
        /// The Gest ID, or a prefix of it, of the one issue to prove.
        issue: Option<String>,
    },
}

/// Parse the arguments after `verify`, before any file is read.
/// # Arguments
/// * `args` - The arguments after the command name.
/// # Returns
/// Help, or the repository and the optional issue.
/// # Errors
/// Refuses an unknown, repeated or missing option, a missing or empty value, a value
/// that is not UTF-8 or holds NUL, and a value longer than 4096 bytes.
fn request(args: &[OsString]) -> Result<Request, String> {
    if matches!(args, [arg] if arg == "--help" || arg == "-h") {
        return Ok(Request::Help);
    }
    let (mut repository, mut issue) = (None, None);
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let slot = if arg == "--repository" && repository.is_none() {
            &mut repository
        } else if arg == "--issue" && issue.is_none() {
            &mut issue
        } else {
            return Err(USAGE.into());
        };
        let value = args.next().ok_or_else(|| USAGE.to_owned())?;
        let text = value.to_str().ok_or("verify argument must be UTF-8")?;
        if text.is_empty() || text.contains('\0') || text.len() > 4096 {
            return Err("verify argument limit or spelling".into());
        }
        *slot = Some(text.to_owned());
    }
    Ok(Request::Verify {
        repository: PathBuf::from(repository.ok_or_else(|| USAGE.to_owned())?),
        issue,
    })
}

/// A failed call, with its exit status and its diagnostic.
struct Failure {
    /// The exit status of CLI-05.
    status: u8,
    /// The diagnostic line for stderr.
    message: String,
}

/// Make a failure from its exit status and its diagnostic line.
/// # Arguments
/// * `status` - The exit status of CLI-05.
/// * `message` - The diagnostic line for stderr.
/// # Returns
/// The failure.
fn fail(status: u8, message: String) -> Failure {
    Failure { status, message }
}

impl From<StorageError> for Failure {
    /// Map a storage refusal to status 74, or to 78 for an older catalog or lost tables.
    /// # Arguments
    /// * `error` - The refusal.
    /// # Returns
    /// The failure with the refusal as its message.
    fn from(error: StorageError) -> Self {
        Self {
            status: if crate::storage::unusable(&error) {
                78
            } else {
                74
            },
            message: error.to_string(),
        }
    }
}

/// Open the repository and run the proof that the request asks for.
/// # Arguments
/// * `repository` - The repository file.
/// * `issue` - The ID or prefix of one issue, or `None` for the whole repository.
/// # Returns
/// The summary line, with a final newline.
/// # Errors
/// Gives 74 for a failed open or proof, 64 for an invalid prefix and 66 for a
/// prefix that names no issue or more than one.
async fn verify(repository: PathBuf, issue: Option<String>) -> Result<String, Failure> {
    let limits = crate::vcs_open::limits();
    let mut owner = LocalOwnerRepository::open(&repository, limits).await?;
    let Some(prefix) = issue else {
        let summary = owner.verify(limits).await?;
        return Ok(format!(
            "verified: {} check-ins, {} issues, {} tables compared\n",
            summary.checkins, summary.issues, summary.tables
        ));
    };
    Id::validate_prefix(&prefix).map_err(|message| fail(64, format!("gossil: {message}")))?;
    let id = match owner.find_issue(&prefix, limits).await? {
        IssueMatch::One(id) => id,
        IssueMatch::None => {
            return Err(Failure {
                status: 66,
                message: format!("gossil: not found: id prefix '{prefix}'"),
            });
        }
        IssueMatch::Many(n) => {
            return Err(Failure {
                status: 66,
                message: format!("gossil: ambiguous id prefix '{prefix}': matches {n} entities"),
            });
        }
    };
    let ticket = owner.verify_issue(&id, limits).await?;
    Ok(format!(
        "verified issue {id}: ticket {}\n",
        ticket.ticket_id
    ))
}

/// Run `vcs verify` with the arguments after its name.
/// # Arguments
/// * `args` - The arguments after `verify`.
/// # Returns
/// 0 after the summary on stdout. Otherwise the status after one line on stderr: 1
/// for a usage error or a failed write, 64 or 66 for `--issue`, 74 for a failed
/// proof and 78 for an older catalog.
pub(crate) fn run(args: &[OsString]) -> ExitCode {
    let result = match request(args) {
        Err(message) => Err(Failure { status: 1, message }),
        Ok(Request::Help) => Ok(format!("{USAGE}\n")),
        Ok(Request::Verify { repository, issue }) => tokio::runtime::Builder::new_current_thread()
            .build()
            .map_err(|error| fail(74, error.to_string()))
            .and_then(|runtime| {
                runtime.block_on(crate::vcs_open::retry_locked(
                    || verify(repository.clone(), issue.clone()),
                    retryable,
                ))
            }),
    };
    let result = result.and_then(|text| {
        std::io::stdout()
            .lock()
            .write_all(text.as_bytes())
            .map_err(|error| Failure {
                status: 1,
                message: format!("gossil: unable to write the summary: {error}"),
            })
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprintln!("{}", failure.message);
            ExitCode::from(failure.status)
        }
    }
}

/// Say whether a failed attempt of `verify` waited for the lock past its timeout, so
/// that one more attempt may pass.
/// # Arguments
/// * `failure` - The failure of one attempt.
/// # Returns
/// True for a storage failure, status 74, whose message holds `database is locked`.
fn retryable(failure: &Failure) -> bool {
    failure.status == 74 && failure.message.contains(crate::vcs_open::LOCKED)
}

#[cfg(test)]
/// Check the argument grammar without storage.
mod tests {
    use super::*;

    #[test]
    /// Only a storage failure that names the lock timeout is tried again.
    /// # Panics
    /// Panics if another failure is retried, or the lock timeout is not.
    fn only_a_lock_timeout_is_retried() {
        assert!(retryable(&fail(74, "storage: database is locked".into())));
        assert!(!retryable(&fail(74, "storage: corrupt".into())));
        assert!(!retryable(&fail(78, "database is locked".into())));
    }

    /// Turn string slices into arguments.
    fn os(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    /// Help, both options in either order, and each refused form.
    #[test]
    fn the_grammar_takes_one_repository_and_an_optional_issue() {
        assert_eq!(request(&os(&["-h"])), Ok(Request::Help));
        assert_eq!(request(&os(&["--help"])), Ok(Request::Help));
        assert_eq!(
            request(&os(&["--repository", "r"])),
            Ok(Request::Verify {
                repository: "r".into(),
                issue: None
            })
        );
        assert_eq!(
            request(&os(&["--issue", "kk", "--repository", "r"])),
            Ok(Request::Verify {
                repository: "r".into(),
                issue: Some("kk".into())
            })
        );
        for refused in [
            &[][..],
            &["--issue", "kk"],
            &["--repository"],
            &["--repository", "r", "--repository", "s"],
            &["--repository", "r", "--issue", "k", "--issue", "l"],
            &["--repository", "r", "extra"],
            &["--help", "--repository", "r"],
        ] {
            assert_eq!(request(&os(refused)), Err(USAGE.into()), "{refused:?}");
        }
        let limit = "verify argument limit or spelling";
        assert_eq!(request(&os(&["--repository", ""])), Err(limit.into()));
        let longest = "r".repeat(4096);
        assert_eq!(
            request(&os(&["--repository", &longest])),
            Ok(Request::Verify {
                repository: longest.clone().into(),
                issue: None
            })
        );
        let long = "r".repeat(4097);
        assert_eq!(request(&os(&["--repository", &long])), Err(limit.into()));
        assert_eq!(request(&os(&["--repository", "a\0b"])), Err(limit.into()));
        let invalid = <std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(b"\xff");
        let args = vec![OsString::from("--repository"), invalid.to_os_string()];
        assert_eq!(request(&args), Err("verify argument must be UTF-8".into()));
    }
}
