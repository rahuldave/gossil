//! Run `issue init`, which creates the Gest project of an open checkout and binds the checkout to it.
//!
//! The command follows `src/cli/commands/init.rs` of the pinned Gest source
//! (eaf821b). Gest finds the project of the current directory, or creates one, and
//! prints `initialized project` with the short project ID and the root. Here the
//! checkout locator that `gos v open` published selects the repository and the
//! checkout. A checkout that already has a project keeps it, and the command
//! changes nothing. Otherwise one transaction writes a private project ticket and
//! the two binding rows, as CAT-08 and GEST-ROOT-01 ask.
//!
//! This build does not create a repository, a checkout registration or a locator.
//! Run `gos v init` and `gos v open` first.
use super::{IssueFailure, IssueRepository, open_issue_repository};
use crate::gest_project_values::{ProjectValueInputs, project_from_values};
use crate::storage::ProjectInitRequest;
use std::ffi::OsString;

/// Say whether `issue init` accepts one argument.
///
/// Gest's `init` takes `--local` and the global `--no-color`, `--no-pager` and
/// counted `-v`/`--verbose`. Each one changes nothing here. The locator already
/// selects the repository, the output has no color and no pager, and the command writes no log.
/// # Arguments
/// * `arg` - One argument after `init`.
/// # Returns
/// True for an accepted option.
fn accepted(arg: &OsString) -> bool {
    let text = arg.to_string_lossy();
    matches!(
        text.as_ref(),
        "--local" | "--no-color" | "--no-pager" | "--verbose"
    ) || text
        .strip_prefix('-')
        .is_some_and(|flags| !flags.is_empty() && flags.bytes().all(|b| b == b'v'))
}

/// Run `issue init` with the arguments after its name.
/// # Arguments
/// * `args` - The arguments after `init`.
/// # Returns
/// Status 0 after the success message on stdout. Status 2 for an argument that
/// `init` does not take. Otherwise the status of [`IssueFailure`] after its
/// diagnostic on stderr. That status is 78 outside an open checkout or for a
/// configuration conflict, and 74 for a storage or I/O failure. A failed write of
/// the message to stdout also gives 74, and the committed project stays.
pub(super) fn run(args: &[OsString]) -> std::process::ExitCode {
    if let Some(arg) = args.iter().find(|arg| !accepted(arg)) {
        eprintln!(
            "gossil: unexpected argument \"{}\" for issue init\nUsage: gossil issue init [OPTIONS]",
            arg.to_string_lossy()
        );
        return std::process::ExitCode::from(2);
    }
    let result = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|error| IssueFailure::from_error(&error))
        .and_then(|runtime| runtime.block_on(init_with(open_issue_repository)))
        .and_then(|message| {
            use std::io::Write;
            let mut out = std::io::stdout().lock();
            out.write_all(message.as_bytes())
                .and_then(|()| out.flush())
                .map_err(|error| {
                    IssueFailure::new(
                        74,
                        format!("gossil: issue init kept the project, and stdout failed: {error}"),
                    )
                })
        });
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(failure) => failure.exit(),
    }
}

/// Open the repository, and find or create the project of its checkout. Do both
/// once more after a refusal that [`super::retry_locked`] retries.
/// # Arguments
/// * `open` - Opens the repository, each time it is called.
/// # Returns
/// The success message, with the short project ID and the checkout root.
/// # Errors
/// Each refusal of `open`, a login that is not UTF-8, and each refusal of the
/// write, after the retry.
pub(super) async fn init_with<O, F>(mut open: O) -> Result<String, IssueFailure>
where
    O: FnMut() -> F,
    F: std::future::Future<Output = Result<IssueRepository, IssueFailure>>,
{
    super::retry_locked(|| {
        let opening = open();
        async move { init(opening.await?).await }
    })
    .await
}

/// Find or create the project of one opened checkout.
///
/// When the write refuses, the binding is read again, since another command may
/// have bound a project after the open. That project is then the result.
/// # Arguments
/// * `found` - The opened repository.
/// # Returns
/// The success message, with the short project ID and the checkout root.
/// # Errors
/// A login that is not UTF-8, and each refusal of the write.
async fn init(mut found: IssueRepository) -> Result<String, IssueFailure> {
    let root = found.view.registered_root.display().to_string();
    if let Ok(project) = found.project_id() {
        return Ok(message(project, &root));
    }
    let login = super::login()?;
    let id = crate::gest_model::primitives::Id::new().to_string();
    let artifact = project_artifact(&id, &login)?;
    let written = found
        .owner
        .init_private_project(
            ProjectInitRequest {
                artifact: &artifact,
                checkout_id: &found.view.checkout_id,
            },
            super::limits(),
        )
        .await;
    let Err(error) = written else {
        return Ok(message(&id, &root));
    };
    // Another command may have bound a project since this one opened the repository.
    let bound = found
        .owner
        .bound_project(&found.view.checkout_id, super::limits())
        .await;
    match bound {
        Ok(Some(project)) => Ok(message(&project, &root)),
        _ => Err(IssueFailure::from_error(&error)),
    }
}

/// Emit the creation artifact of a new project, dated now.
///
/// The D card and both timestamps hold the same instant. The K card is 40 random
/// lowercase hexadecimal digits, and the U card is the local login.
/// # Arguments
/// * `id` - The Gest ID of the project.
/// * `login` - The local login.
/// # Returns
/// The artifact bytes.
/// # Errors
/// Refuses with status 74 when the clock is before 1970, when no random bytes
/// come, or when the artifact cannot hold the login.
fn project_artifact(id: &str, login: &str) -> Result<Vec<u8>, IssueFailure> {
    let value = |what: &str| IssueFailure::new(74, format!("gossil: issue init: {what}"));
    let now = super::creation("init")?;
    let project = project_from_values(ProjectValueInputs {
        gest_id: serde_json::json!(id),
        archived_at: serde_json::Value::Null,
        created_at: now.stamp.clone(),
        updated_at: now.stamp,
    })
    .map_err(|error| value(&error.to_string()))?;
    crate::gest_ticket::project_creation_bytes(
        &project,
        now.ticket_id.as_bytes(),
        login.as_bytes(),
        now.date,
        65_536,
    )
    .map_err(|error| value(&error.to_string()))
}

/// Format the success message of Gest's `init`, without color.
/// # Arguments
/// * `id` - The Gest ID of the project.
/// * `root` - The checkout root.
/// # Returns
/// The action line with the first 8 letters of the ID, or the whole ID when it is
/// shorter, a blank line and the root field.
fn message(id: &str, root: &str) -> String {
    let short = id.get(..8).unwrap_or(id);
    format!("  ✓  initialized project  {short}\n\n  root  {root}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `init` accepts its own option and the global options, and nothing else.
    #[test]
    fn init_accepts_its_options() {
        for arg in [
            "--local",
            "--no-color",
            "--no-pager",
            "--verbose",
            "-v",
            "-vvv",
        ] {
            assert!(accepted(&OsString::from(arg)), "{arg}");
        }
        for arg in ["-", "-x", "-vx", "--json", "local", "--local=x"] {
            assert!(!accepted(&OsString::from(arg)), "{arg}");
        }
    }

    /// The artifact of a new project is one canonical project creation with the login
    /// in its U card, and the message gives the short ID and the root.
    #[test]
    fn the_project_artifact_and_message_follow_gest() {
        let id = "k".repeat(32);
        let bytes = project_artifact(&id, "alice").unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains(&format!("J gest_id \"{id}\"\n")), "{text}");
        assert!(text.contains("\nU alice\n"), "{text}");
        assert_eq!(
            message(&id, "/r"),
            "  ✓  initialized project  kkkkkkkk\n\n  root  /r\n"
        );
        assert_eq!(
            message("short", "/r"),
            "  ✓  initialized project  short\n\n  root  /r\n"
        );
        let long = "x".repeat(70_000);
        assert_eq!(project_artifact(&id, &long).unwrap_err().status, 74);
        assert_eq!(project_artifact("bad", "alice").unwrap_err().status, 74);
    }
}
