//! Adapt native add and tracked status to bounded explicit commands.
//!
//! ## Recommendations
//! Run `vcs open` to establish or recover the locator first. Add explicit regular
//! files, choose scopes explicitly through the library, and inspect recorded versus pending paths.
use crate::storage::TrackingLimits;
use crate::vcs_open::tracking_limits as limits;
use std::{ffi::OsString, path::PathBuf};
/// Preserve storage, locator and I/O causes through the command boundary.
type Error = Box<dyn std::error::Error>;
/// Explicit selections and bounded UTF-8 relative paths.
#[derive(Clone)]
struct Options {
    /// Explicit existing database path.
    repository: PathBuf,
    /// Explicit exact registered identity.
    checkout: String,
    /// Exact caller paths, retained only within finite ceilings.
    paths: Vec<String>,
}
/// Parse exact long options; reject unsupported status paths and ambiguous selections.
/// # Arguments
/// * `args` - Command suffix after the selected add/status verb.
/// * `add` - Whether explicit repeated path options are required and permitted.
/// * `limits` - The path ceilings.
/// # Returns
/// Explicit selectors with bounded owned path strings.
/// # Errors
/// Refuses unknown/duplicate selectors, missing values, non-UTF-8 identities/paths and bounds.
fn options(args: &[OsString], add: bool, limits: TrackingLimits) -> Result<Options, Error> {
    let mut repository = None;
    let mut checkout = None;
    let mut paths = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let value = args.next().ok_or("tracking option requires a value")?;
        if arg == "--repository" && repository.is_none() {
            repository = Some(PathBuf::from(value));
        } else if arg == "--checkout" && checkout.is_none() {
            let text = value.to_str().ok_or("checkout ID must be UTF-8")?;
            if text.is_empty() || text.len() > 1024 || text.contains('\0') {
                return Err("checkout identity limit or spelling".into());
            }
            checkout = Some(text.to_owned());
        } else if arg == "--path" && add {
            let text = value.to_str().ok_or("tracking path must be UTF-8")?;
            if text.len() > limits.path_bytes {
                return Err("tracking path limit".into());
            }
            if paths.len() >= limits.paths {
                return Err(too_many_paths("add", limits.paths).into());
            }
            paths.push(text.to_owned());
        } else {
            return Err(
                "Usage: gos v add|status --repository REPO --checkout ID [--path PATH ...]".into(),
            );
        }
    }
    if add && paths.is_empty() {
        return Err("add requires at least one --path PATH".into());
    }
    Ok(Options {
        repository: repository.ok_or("tracking requires --repository REPO")?,
        checkout: checkout.ok_or("tracking requires --checkout ID")?,
        paths,
    })
}
/// Refuse a command that names more paths than one command may name.
/// # Arguments
/// * `command` - The verb, such as `add`.
/// * `ceiling` - The most paths that one command names.
/// # Returns
/// The plain refusal.
pub(crate) fn too_many_paths(command: &str, ceiling: usize) -> String {
    format!("{command} names more than the {ceiling} paths that one command may name")
}

/// Authenticate the locator/native binding, then enroll paths or list tracked status.
/// # Arguments
/// * `args` - Exact command suffix after add/status.
/// * `add` - Whether to enroll requested paths before listing.
/// # Returns
/// Success after complete tracked state and actual scope output completes.
/// # Errors
/// Propagates exact grammar, admission, capture, transaction and output failures.
/// A failed output after add cannot undo its committed enrollment.
/// # Panics
/// Panics if invoked within an already running Tokio runtime.
pub(crate) fn run(args: &[OsString], add: bool) -> Result<(), Error> {
    let options = options(args, add, limits())?;
    let attempt = || {
        let options = options.clone();
        async move {
            let (mut owner, view) =
                crate::vcs_open::admit_existing(options.repository, options.checkout).await?;
            let mut warnings = Vec::new();
            if add {
                let paths: Vec<&str> = options.paths.iter().map(String::as_str).collect();
                owner
                    .add_files(
                        &view.checkout_id,
                        &paths,
                        &view,
                        crate::vcs_open::limits(),
                        limits(),
                    )
                    .await?;
                warnings = crate::vcs_open::large_file_warnings(&view.registered_root, &paths);
            }
            let entries = owner
                .pending_adds(&view.checkout_id, crate::vcs_open::limits(), limits())
                .await?;
            let paths = owner
                .tracked_paths(&view.checkout_id, crate::vcs_open::limits(), limits())
                .await?;
            let scopes = owner
                .path_scopes(&view.checkout_id, crate::vcs_open::limits(), limits())
                .await?;
            Ok::<_, Error>((paths, entries, scopes, warnings))
        }
    };
    let (paths, entries, scopes, warnings) = tokio::runtime::Builder::new_current_thread()
        .build()?
        .block_on(crate::vcs_open::retry_locked(attempt, |error: &Error| {
            crate::vcs_open::is_locked(&**error)
        }))?;
    for warning in warnings {
        eprintln!("{warning}");
    }
    use std::io::Write;
    let mut output = std::io::stdout().lock();
    writeln!(output, "Tracked paths (A: pending addition, R: recorded):")?;
    // Sets keep the status of many files linear in their count.
    let pending: std::collections::BTreeSet<&str> =
        entries.iter().map(|entry| entry.path.as_str()).collect();
    let chosen: std::collections::BTreeMap<&str, crate::storage::PathScope> = scopes
        .iter()
        .map(|entry| (entry.path.as_str(), entry.scope))
        .collect();
    for path in paths {
        let state = if pending.contains(path.as_str()) {
            "A"
        } else {
            "R"
        };
        let scope = match chosen.get(path.as_str()) {
            Some(crate::storage::PathScope::Private) => "private",
            Some(crate::storage::PathScope::Shared) => "shared",
            None => "unselected",
        };
        writeln!(output, "{state}\t{scope}\t{path:?}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Check explicit selectors, repeated path options and argument ceilings.
    use super::*;
    /// Exact bounded grammar rejects unknown flags, duplicate selectors and status paths.
    #[test]
    fn grammar_is_explicit_and_bounded() {
        let args = |values: &[&str]| values.iter().map(OsString::from).collect::<Vec<_>>();
        let options = |args: &[OsString], add| options(args, add, limits());
        let base = ["--repository", "repo", "--checkout", "work"];
        assert!(options(&args(&base), false).is_ok());
        assert!(options(&args(&base), true).is_err());
        for suffix in [
            vec!["--path", "a"],
            vec!["--private", "yes"],
            vec!["--checkout", "other"],
            vec!["--root", "root"],
            vec!["extra"],
            vec!["--repository", "other"],
        ] {
            let mut values = base.to_vec();
            values.extend(suffix);
            assert!(options(&args(&values), false).is_err());
        }
        let valid = args(&[
            "--repository",
            "repo",
            "--checkout",
            "work",
            "--path",
            "a",
            "--path",
            "b",
        ]);
        assert_eq!(options(&valid, true).unwrap().paths, ["a", "b"]);
    }
    /// A command names as many paths as the ceiling, and one more is refused with a
    /// plain message.
    /// # Panics
    /// Panics if the ceiling is refused or one more path is accepted.
    #[test]
    fn path_count_ceiling_is_inclusive_and_plain() {
        let small = TrackingLimits {
            paths: 3,
            ..limits()
        };
        let mut args: Vec<OsString> = ["--repository", "repo", "--checkout", "work"]
            .map(OsString::from)
            .to_vec();
        for index in 0..3 {
            args.extend([OsString::from("--path"), format!("file-{index}").into()]);
        }
        assert_eq!(options(&args, true, small).unwrap().paths.len(), 3);
        args.extend([OsString::from("--path"), OsString::from("one-more")]);
        assert_eq!(
            options(&args, true, small).err().unwrap().to_string(),
            "add names more than the 3 paths that one command may name"
        );
    }
    /// Refuse empty, oversized or NUL-containing IDs and oversized path arguments.
    /// # Panics
    /// Panics if an invalid selector or path is accepted or returns the wrong error.
    #[test]
    fn selector_and_path_ceiling_errors_are_explicit() {
        let args = |id: &str, path: &str| {
            ["--repository", "repo", "--checkout", id, "--path", path].map(OsString::from)
        };
        for id in [String::new(), "x".repeat(1025), "a\0b".to_owned()] {
            assert_eq!(
                options(&args(&id, "file"), true, limits())
                    .err()
                    .unwrap()
                    .to_string(),
                "checkout identity limit or spelling"
            );
        }
        assert_eq!(
            options(&args("work", &"x".repeat(4097)), true, limits())
                .err()
                .unwrap()
                .to_string(),
            "tracking path limit"
        );
    }
}
