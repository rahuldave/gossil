//! Adapt native add and tracked status to bounded explicit commands.
//!
//! ## Recommendations
//! Run `vcs open` to establish or recover the locator first. Add explicit regular
//! files, choose scopes explicitly through the library, and inspect recorded versus pending paths.
use crate::storage::TrackingLimits;
use std::{ffi::OsString, path::PathBuf};
/// Preserve storage, locator and I/O causes through the command boundary.
type Error = Box<dyn std::error::Error>;
/// Explicit selections and bounded UTF-8 relative paths.
struct Options {
    /// Explicit existing database path.
    repository: PathBuf,
    /// Explicit exact registered identity.
    checkout: String,
    /// Exact caller paths, retained only within finite ceilings.
    paths: Vec<String>,
}
/// Finite complete-inventory and descriptor-capture policy for this CLI slice.
/// # Returns
/// Fixed finite bounds independent of Gest project policy.
fn limits() -> TrackingLimits {
    TrackingLimits {
        // A check-in manifest of 1 MiB holds fewer files than this.
        paths: 16384,
        path_bytes: 4096,
        components: 128,
        retained_bytes: 67108864,
        io_buffer_bytes: 65536,
    }
}
/// Parse exact long options; reject unsupported status paths and ambiguous selections.
/// # Arguments
/// * `args` - Command suffix after the selected add/status verb.
/// * `add` - Whether explicit repeated path options are required and permitted.
/// # Returns
/// Explicit selectors with bounded owned path strings.
/// # Errors
/// Refuses unknown/duplicate selectors, missing values, non-UTF-8 identities/paths and bounds.
fn options(args: &[OsString], add: bool) -> Result<Options, Error> {
    if args.len() > 4 + 2 * limits().paths {
        return Err("tracking argument count limit".into());
    }
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
            if text.len() > limits().path_bytes || paths.len() >= limits().paths {
                return Err("tracking path limit".into());
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
    let options = options(args, add)?;
    let (paths, entries, scopes) = tokio::runtime::Builder::new_current_thread()
        .build()?
        .block_on(async {
            let (mut owner, view) =
                crate::vcs_open::admit_existing(options.repository, options.checkout).await?;
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
            Ok::<_, Error>((paths, entries, scopes))
        })?;
    use std::io::Write;
    let mut output = std::io::stdout().lock();
    writeln!(output, "Tracked paths (A: pending addition, R: recorded):")?;
    for path in paths {
        let state = if entries.iter().any(|entry| entry.path == path) {
            "A"
        } else {
            "R"
        };
        let scope = match scopes
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| entry.scope)
        {
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
        let mut excessive = valid;
        let mut most = args(&["--repository", "repo", "--checkout", "work"]);
        for index in 0..limits().paths {
            most.extend([OsString::from("--path"), format!("file-{index}").into()]);
        }
        assert_eq!(options(&most, true).unwrap().paths.len(), 16384);
        excessive.extend((0..2 * limits().paths).map(|_| OsString::from("x")));
        assert_eq!(
            options(&excessive, true).err().unwrap().to_string(),
            "tracking argument count limit"
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
                options(&args(&id, "file"), true).err().unwrap().to_string(),
                "checkout identity limit or spelling"
            );
        }
        assert_eq!(
            options(&args("work", &"x".repeat(4097)), true)
                .err()
                .unwrap()
                .to_string(),
            "tracking path limit"
        );
    }
}
