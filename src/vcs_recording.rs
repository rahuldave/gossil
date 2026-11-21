//! Adapt explicit scope selection and private recording of all tracked files for the CLI.
//!
//! ## Recommendations
//! Open the exact checkout, add files and select their scopes explicitly. Next,
//! record with all tracked paths and an explicit actor, comment and date override.
use crate::storage::{
    PathScope, PathScopeSelection, PrivateRecordingOutcome, PrivateRecordingRequest, TrackingLimits,
};
use std::{ffi::OsString, path::PathBuf};
/// Preserve underlying grammar, admission, storage and output failures.
type Error = Box<dyn std::error::Error>;
/// Hold explicit caller selections without repository defaults from the current directory.
struct Options {
    /// Native repository filename.
    repository: PathBuf,
    /// Exact registered checkout identity.
    checkout: String,
    /// Required explicit scope.
    scope: PathScope,
    /// Explicit repeated scope-selection paths.
    paths: Vec<String>,
    /// Explicit actor for recording, absent for scope selection.
    actor: Option<String>,
    /// Explicit comment for recording, absent for scope selection.
    comment: Option<String>,
    /// Required date override for recording, absent for scope selection.
    date: Option<String>,
}
/// Supply bounded full inventory and capture work independent of Gest settings.
/// # Returns
/// Finite pathname, descriptor and cumulative capture ceilings.
fn limits() -> TrackingLimits {
    TrackingLimits {
        paths: 4096,
        path_bytes: 4096,
        components: 128,
        retained_bytes: 67108864,
        io_buffer_bytes: 65536,
    }
}
/// Parse exact explicit command grammar before any repository observation.
/// # Arguments
/// * `args` - Suffix after the scope or commit verb.
/// * `commit` - Whether private all-tracked recording grammar is required.
/// # Returns
/// Bounded explicit selectors and recording metadata.
/// # Errors
/// Refuses duplicate/unknown options, missing values, implicit scope, missing
/// all-tracked consent, shared recording, non-UTF-8 metadata and exhausted bounds.
fn options(args: &[OsString], commit: bool) -> Result<Options, Error> {
    if args.len() > 16 + 2 * limits().paths {
        return Err("recording argument count limit".into());
    }
    let (mut repository, mut checkout, mut scope) = (None, None, None);
    let (mut actor, mut comment, mut date) = (None, None, None);
    let mut all = false;
    let mut paths = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--all-tracked" && commit && !all {
            all = true;
            continue;
        }
        if !matches!(
            arg.to_str(),
            Some(
                "--repository"
                    | "--checkout"
                    | "--scope"
                    | "--path"
                    | "--user"
                    | "--comment"
                    | "--date-override"
            )
        ) {
            return Err("unknown or duplicate recording option".into());
        }
        let value = args.next().ok_or("recording option requires a value")?;
        if arg == "--repository" && repository.is_none() {
            repository = Some(PathBuf::from(value));
            continue;
        }
        let text = value.to_str().ok_or("recording option must be UTF-8")?;
        if text.contains('\0') {
            return Err("recording option contains NUL".into());
        }
        if arg == "--checkout" && checkout.is_none() {
            if text.is_empty() || text.len() > 1024 {
                return Err("checkout identity limit or spelling".into());
            }
            checkout = Some(text.to_owned());
        } else if arg == "--scope" && scope.is_none() {
            scope = Some(match text {
                "private" => PathScope::Private,
                "shared" if !commit => PathScope::Shared,
                _ => return Err("explicit supported scope required".into()),
            });
        } else if arg == "--path" && !commit {
            if text.is_empty() || text.len() > limits().path_bytes || paths.len() >= limits().paths
            {
                return Err("scope path limit".into());
            }
            paths.push(text.to_owned());
        } else if arg == "--user" && commit && actor.is_none() {
            if text.is_empty() || text.len() > 4096 {
                return Err("recording actor limit".into());
            }
            actor = Some(text.to_owned());
        } else if arg == "--comment" && commit && comment.is_none() {
            if text.is_empty() || text.len() > 1048576 {
                return Err("recording comment limit".into());
            }
            comment = Some(text.to_owned());
        } else if arg == "--date-override" && commit && date.is_none() {
            if text.is_empty() || text.len() > 64 {
                return Err("recording date limit".into());
            }
            date = Some(text.to_owned());
        } else {
            return Err("unknown or duplicate recording option".into());
        }
    }
    if commit && (!all || actor.is_none() || comment.is_none() || date.is_none()) {
        return Err(
            "commit requires --all-tracked --user ACTOR --comment TEXT --date-override DATE".into(),
        );
    }
    if !commit && paths.is_empty() {
        return Err("scope requires at least one --path PATH".into());
    }
    Ok(Options {
        repository: repository.ok_or("recording requires --repository REPO")?,
        checkout: checkout.ok_or("recording requires --checkout ID")?,
        scope: scope.ok_or("recording requires --scope shared|private")?,
        paths,
        actor,
        comment,
        date,
    })
}
/// Admit the existing locator binding and execute one explicit scope or commit operation.
/// # Arguments
/// * `args` - Exact suffix following the selected verb.
/// * `commit` - Select private recording rather than explicit scope persistence.
/// # Returns
/// Success after the committed result has been written to standard output.
/// # Errors
/// Propagates grammar, locator, authority, capture, SQL, transaction and output
/// errors. Output failure after COMMIT cannot undo a durable operation.
/// # Panics
/// Panics if called from an already running Tokio runtime.
pub(crate) fn run(args: &[OsString], commit: bool) -> Result<(), Error> {
    let options = options(args, commit)?;
    let message = tokio::runtime::Builder::new_current_thread()
        .build()?
        .block_on(async {
            let (mut owner, view) =
                crate::vcs_open::admit_existing(options.repository, options.checkout).await?;
            if commit {
                let tracked = owner
                    .tracked_paths(&view.checkout_id, crate::vcs_open::limits(), limits())
                    .await?;
                let paths: Vec<&str> = tracked.iter().map(String::as_str).collect();
                let outcome = owner
                    .commit_private_tracked_files(
                        &view.checkout_id,
                        PrivateRecordingRequest {
                            expected: &view,
                            paths: &paths,
                            scope: options.scope,
                            actor: options
                                .actor
                                .as_deref()
                                .ok_or("missing recording actor")?
                                .as_bytes(),
                            comment: options
                                .comment
                                .as_deref()
                                .ok_or("missing recording comment")?
                                .as_bytes(),
                            date: options
                                .date
                                .as_deref()
                                .ok_or("missing recording date")?
                                .as_bytes(),
                        },
                        crate::vcs_open::limits(),
                        limits(),
                    )
                    .await?;
                Ok::<_, Error>(match outcome {
                    PrivateRecordingOutcome::Recorded { native_name, .. } => {
                        format!("Recorded private check-in: {native_name}")
                    }
                    PrivateRecordingOutcome::NoChange { .. } => "No changes".to_owned(),
                })
            } else {
                let selections: Vec<_> = options
                    .paths
                    .into_iter()
                    .map(|path| PathScopeSelection {
                        path,
                        scope: options.scope,
                    })
                    .collect();
                owner
                    .select_path_scopes(
                        &view.checkout_id,
                        &view,
                        &selections,
                        crate::vcs_open::limits(),
                        limits(),
                    )
                    .await?;
                Ok(format!("Selected scopes: {}", selections.len()))
            }
        })?;
    use std::io::Write;
    writeln!(std::io::stdout().lock(), "{message}")?;
    Ok(())
}
#[cfg(test)]
/// Check explicit scope and all-tracked recording command grammar.
mod tests {
    use super::*;

    /// Keep each metadata ceiling inclusive and reject empty, oversized or NUL input.
    #[test]
    /// # Panics
    /// Panics if the parser accepts refused input or rejects the admitted grammar and bounds.
    fn metadata_byte_boundaries_precede_repository_access() {
        let base: Vec<OsString> = [
            "--repository",
            "absent-repository",
            "--checkout",
            "work",
            "--scope",
            "private",
            "--all-tracked",
            "--user",
            "Actor",
            "--comment",
            "Message",
            "--date-override",
            "2026-11-20T00:00:01",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        for (index, ceiling, error) in [
            (3, 1024, "checkout identity limit or spelling"),
            (8, 4096, "recording actor limit"),
            (10, 1048576, "recording comment limit"),
            (12, 64, "recording date limit"),
        ] {
            let mut args = base.clone();
            args[index] = "x".repeat(ceiling).into();
            assert!(options(&args, true).is_ok(), "inclusive ceiling {index}");
            for size in [0, ceiling + 1] {
                args[index] = "x".repeat(size).into();
                assert_eq!(options(&args, true).err().unwrap().to_string(), error);
            }
            args[index] = "prefix\0suffix".into();
            assert_eq!(
                options(&args, true).err().unwrap().to_string(),
                "recording option contains NUL"
            );
        }
    }

    /// Bound raw argument count, per-path bytes and the complete explicit path list.
    #[test]
    /// # Panics
    /// Panics if the parser accepts refused input or rejects the admitted grammar and bounds.
    fn scope_path_and_argument_count_boundaries_are_explicit() {
        let base: Vec<OsString> = [
            "--repository",
            "absent-repository",
            "--checkout",
            "work",
            "--scope",
            "private",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        let too_many = vec![OsString::from("--path"); 8209];
        assert_eq!(
            options(&too_many, false).err().unwrap().to_string(),
            "recording argument count limit"
        );
        let mut args = base.clone();
        args.extend([OsString::from("--path"), "p".repeat(4096).into()]);
        assert!(options(&args, false).is_ok());
        for size in [0, 4097] {
            args[7] = "p".repeat(size).into();
            assert_eq!(
                options(&args, false).err().unwrap().to_string(),
                "scope path limit"
            );
        }
        let mut args = base;
        for index in 0..4096 {
            args.extend([OsString::from("--path"), format!("file-{index}").into()]);
        }
        assert_eq!(options(&args, false).unwrap().paths.len(), 4096);
        args.extend([OsString::from("--path"), OsString::from("one-more")]);
        assert_eq!(
            options(&args, false).err().unwrap().to_string(),
            "scope path limit"
        );
    }

    /// Reject implicit selection, metadata duplication and unsupported commit grammar.
    #[test]
    /// # Panics
    /// Panics if the parser accepts refused input or rejects the admitted grammar and bounds.
    fn grammar_requires_explicit_scope_and_complete_recording_intent() {
        let args = |values: &[&str]| values.iter().map(OsString::from).collect::<Vec<_>>();
        let base = [
            "--repository",
            "repo",
            "--checkout",
            "work",
            "--scope",
            "private",
        ];
        let mut valid = base.to_vec();
        valid.extend([
            "--all-tracked",
            "--user",
            "Actor",
            "--comment",
            "Message",
            "--date-override",
            "2026-11-20T00:00:01",
        ]);
        assert!(options(&args(&valid), true).is_ok());
        let metadata_required =
            "commit requires --all-tracked --user ACTOR --comment TEXT --date-override DATE";
        for (flag, width, diagnostic) in [
            ("--repository", 2, "recording requires --repository REPO"),
            ("--checkout", 2, "recording requires --checkout ID"),
            ("--scope", 2, "recording requires --scope shared|private"),
            ("--all-tracked", 1, metadata_required),
            ("--user", 2, metadata_required),
            ("--comment", 2, metadata_required),
            ("--date-override", 2, metadata_required),
        ] {
            let mut values = valid.clone();
            let start = values.iter().position(|value| *value == flag).unwrap();
            drop(values.drain(start..start + width));
            assert_eq!(
                options(&args(&values), true).err().unwrap().to_string(),
                diagnostic,
                "omitted {flag}"
            );
        }
        let mut dangling = valid.clone();
        dangling.push("--comment");
        assert_eq!(
            options(&args(&dangling), true).err().unwrap().to_string(),
            "recording option requires a value"
        );
        {
            use std::os::unix::ffi::OsStringExt;
            for flag in [
                "--checkout",
                "--scope",
                "--user",
                "--comment",
                "--date-override",
            ] {
                let mut values = args(&valid);
                let index = valid.iter().position(|value| *value == flag).unwrap() + 1;
                values[index] = OsString::from_vec(vec![0xff]);
                assert_eq!(
                    options(&values, true).err().unwrap().to_string(),
                    "recording option must be UTF-8",
                    "invalid UTF-8 in {flag}"
                );
            }
            let mut values = args(&base);
            values.extend([OsString::from("--path"), OsString::from_vec(vec![0xff])]);
            assert_eq!(
                options(&values, false).err().unwrap().to_string(),
                "recording option must be UTF-8"
            );
        }
        assert!(options(&args(&base), true).is_err());
        for suffix in [
            vec!["--all-tracked"],
            vec!["--scope", "private"],
            vec!["--user", "Other"],
            vec!["--path", "file"],
            vec!["--date-override", "other"],
            vec!["--force"],
        ] {
            let mut values = valid.clone();
            values.extend(suffix);
            assert!(options(&args(&values), true).is_err());
        }
        valid[5] = "shared";
        assert!(options(&args(&valid), true).is_err());
        let mut scope = base.to_vec();
        scope.extend(["--path", "a", "--path", "b"]);
        assert!(options(&args(&scope), false).is_ok());
        assert!(options(&args(&base), false).is_err());
    }
}
