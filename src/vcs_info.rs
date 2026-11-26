//! Show the authenticated current recorded checkout identity without changing it.
//!
//! ## Recommendations
//! Use explicit repository and checkout selections from a complete opened checkout.
use std::{ffi::OsString, io::Write, path::PathBuf};

/// Preserve original admission, serialization and output errors.
type Error = Box<dyn std::error::Error>;
/// Exact bounded command grammar shared by help and usage errors.
const USAGE: &str = "Usage: gossil vcs info --repository REPO --checkout ID";
/// Explicit selections, retaining the supplied UTF-8 repository spelling.
struct Options {
    /// Supplied repository path, before canonical admission.
    repository: String,
    /// Exact checkout registration selection.
    checkout: String,
}
/// Parse required unique selectors without reading the filesystem.
/// # Arguments
/// * `args` - Arguments after the info subcommand.
/// # Returns
/// Explicit UTF-8 repository and checkout selections.
/// # Errors
/// Reject missing values, duplicate or unsupported options and non-UTF-8 selections.
fn options(args: &[OsString]) -> Result<Options, Error> {
    let mut repository = None;
    let mut checkout = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--repository" && repository.is_none() {
            repository = Some(
                args.next()
                    .ok_or("--repository requires REPO")?
                    .to_str()
                    .ok_or("repository path must be UTF-8")?
                    .to_owned(),
            );
        } else if arg == "--checkout" && checkout.is_none() {
            checkout = Some(
                args.next()
                    .ok_or("--checkout requires ID")?
                    .to_str()
                    .ok_or("checkout ID must be UTF-8")?
                    .to_owned(),
            );
        } else {
            return Err(USAGE.into());
        }
    }
    Ok(Options {
        repository: repository.ok_or("info requires --repository REPO")?,
        checkout: checkout.ok_or("info requires --checkout ID")?,
    })
}
/// Admit the existing checkout before writing its five current identity fields.
/// # Arguments
/// * `args` - Arguments after the info subcommand; standalone help needs no checkout.
/// # Returns
/// Success after writing the complete LF-terminated response.
/// # Errors
/// Retain admission and output errors. An output failure can leave partial stdout.
/// # Panics
/// Panics if called from an already running Tokio runtime.
pub(crate) fn run(args: &[OsString]) -> Result<(), Error> {
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        writeln!(std::io::stdout().lock(), "{USAGE}")?;
        return Ok(());
    }
    let options = options(args)?;
    let (_, view) = tokio::runtime::Builder::new_current_thread()
        .build()?
        .block_on(crate::vcs_open::retry_locked(
            || {
                crate::vcs_open::admit_existing(
                    PathBuf::from(&options.repository),
                    options.checkout.clone(),
                )
            },
            |error: &Error| crate::vcs_open::is_locked(&**error),
        ))?;
    let root = view
        .registered_root
        .to_str()
        .ok_or("registered root must be UTF-8")?;
    let effective = view.combined_head.as_deref().unwrap_or(&view.shared_head);
    let private = view.combined_head.as_deref().unwrap_or("none");
    let output = format!(
        "repository: {}\nlocal-root: {}\ncheckout: {effective}\nshared-head: {}\nprivate-head: {private}\n",
        serde_json::to_string(&options.repository)?,
        serde_json::to_string(root)?,
        view.shared_head,
    );
    std::io::stdout().lock().write_all(output.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Exact options retain spelling and reject ambiguity before storage admission.
    #[test]
    fn explicit_options() {
        let args = |values: &[&str]| values.iter().map(OsString::from).collect::<Vec<_>>();
        let parsed = options(&args(&[
            "--checkout",
            "work",
            "--repository",
            "../alias.db",
        ]))
        .unwrap();
        assert_eq!(parsed.repository, "../alias.db");
        assert_eq!(parsed.checkout, "work");
        assert!(options(&args(&["--repository", "repo", "--checkout", ""])).is_ok());
        for values in [
            vec![],
            vec!["--repository"],
            vec!["--checkout"],
            vec!["--repository", "repo"],
            vec!["--checkout", "work"],
            vec!["--repository", "repo", "--repository", "other"],
            vec!["--checkout", "work", "--checkout", "other"],
            vec!["tip"],
            vec!["--json"],
        ] {
            assert!(options(&args(&values)).is_err());
        }
        use std::os::unix::ffi::OsStringExt;
        for flag in ["--repository", "--checkout"] {
            let error = options(&[OsString::from(flag), OsString::from_vec(vec![0xff])])
                .err()
                .unwrap();
            assert!(error.to_string().contains("UTF-8"));
        }
    }
}
