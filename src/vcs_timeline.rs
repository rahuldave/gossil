//! Print complete authenticated checkout ancestry as bounded compact NDJSON.
//!
//! Opening admission is a separate operation. The history API completes its own
//! transaction before this adapter preflights, reserves and serializes output.
//!
//! ## Recommendations
//! Open the checkout first, then select its repository and identity explicitly.
use crate::storage::RecordedCheckin;
use serde::Serialize;
use std::{ffi::OsString, io::Write, path::PathBuf};

/// Preserve original grammar, UTF-8, storage, allocation, serialization and I/O causes.
type Error = Box<dyn std::error::Error>;
/// Exact supported grammar, shared by standalone help and option failures.
const USAGE: &str = "Usage: gossil vcs timeline|log --repository REPO --checkout ID";

/// Parsed storage-free help or complete explicit checkout selections.
enum Request {
    /// Standalone help requires no storage observation.
    Help,
    /// Complete explicit selectors for existing locator admission.
    Read {
        /// Supplied existing repository filename.
        repository: PathBuf,
        /// Exact nonempty bounded UTF-8 registration identity.
        checkout: String,
    },
}

/// Parse unique exact selectors before any storage observation.
/// # Arguments
/// * `args` - Suffix after either timeline or log.
/// # Returns
/// Standalone help or complete bounded explicit selections in either order.
/// # Errors
/// Refuses missing/duplicate/unknown options, positional/filter/equals forms,
/// non-UTF-8/NUL/empty selectors and excessive argument or selector lengths.
fn request(args: &[OsString]) -> Result<Request, Error> {
    if matches!(args, [arg] if arg == "--help" || arg == "-h") {
        return Ok(Request::Help);
    }
    if args.len() > 4 {
        return Err(USAGE.into());
    }
    let (mut repository, mut checkout) = (None, None);
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg != "--repository" && arg != "--checkout" {
            return Err(USAGE.into());
        }
        let value = args.next().ok_or("timeline option requires a value")?;
        let text = value.to_str().ok_or("timeline selector must be UTF-8")?;
        let maximum = if arg == "--repository" { 4096 } else { 1024 };
        if text.is_empty() || text.contains('\0') || text.len() > maximum {
            return Err("timeline selector limit or spelling".into());
        }
        if arg == "--repository" && repository.is_none() {
            repository = Some(PathBuf::from(text));
        } else if arg == "--checkout" && checkout.is_none() {
            checkout = Some(text.to_owned());
        } else {
            return Err(USAGE.into());
        }
    }
    Ok(Request::Read {
        repository: repository.ok_or("timeline requires --repository REPO")?,
        checkout: checkout.ok_or("timeline requires --checkout ID")?,
    })
}

/// Borrow a row in the exact public output key order without cloning strings.
#[derive(Serialize)]
struct JsonRow<'a> {
    /// Full verified native hash, counted once in the source byte bound.
    hash: &'a str,
    /// Exact native date token represented as a JSON string.
    date: &'a str,
    /// Exact decoded native actor represented as a JSON string.
    actor: &'a str,
    /// Exact decoded native comment represented as a JSON string.
    comment: &'a str,
    /// Native private visibility; false only for the final shared root.
    private: bool,
}
impl<'a> JsonRow<'a> {
    /// Borrow exact native bytes after strict UTF-8 conversion of every byte field.
    /// # Arguments
    /// * `row` - Completed authenticated API result; no storage is observed here.
    /// # Returns
    /// Borrowed JSON string fields with no loss, normalization or date reformatting.
    /// # Errors
    /// Preserves the original UTF-8 failure for date, actor or comment.
    fn borrow(row: &'a RecordedCheckin) -> Result<Self, Error> {
        Ok(Self {
            hash: &row.hash,
            date: std::str::from_utf8(&row.date)?,
            actor: std::str::from_utf8(&row.actor)?,
            comment: std::str::from_utf8(&row.comment)?,
            private: row.private,
        })
    }
}

/// Accumulate one checked worst-case row bound including exact fixed JSON syntax.
/// # Arguments
/// * `previous` - Complete bound already required by preceding rows.
/// * `source` - Summed hash/date/actor/comment UTF-8 source bytes, each counted once.
/// * `private` - Selects true (four bytes) or false (five bytes).
/// # Returns
/// Updated bound using six output bytes per source byte, exact syntax and one LF.
/// # Errors
/// Refuses multiplication/addition overflow or a capacity exceeding `isize::MAX`.
fn row_bound(previous: usize, source: usize, private: bool) -> Result<usize, Error> {
    let fixed = if private { 61 } else { 62 };
    previous
        .checked_add(source.checked_mul(6).ok_or("timeline JSON size overflow")?)
        .and_then(|value| value.checked_add(fixed))
        .filter(|value| *value <= isize::MAX as usize)
        .ok_or_else(|| "timeline JSON size overflow".into())
}

/// Preflight every field and the complete cumulative serialization capacity.
/// # Arguments
/// * `rows` - Complete checked API result in native ancestry order.
/// * `limit` - Separate finite output capacity, derived from the existing CLI limits.
/// # Returns
/// Complete worst-case capacity before any output buffer allocation.
/// # Errors
/// Refuses invalid UTF-8, arithmetic/platform overflow or exhausted output capacity.
fn output_bound(rows: &[RecordedCheckin], limit: usize) -> Result<usize, Error> {
    let mut total = 0;
    for row in rows {
        let row = JsonRow::borrow(row)?;
        let source = [
            row.hash.len(),
            row.date.len(),
            row.actor.len(),
            row.comment.len(),
        ]
        .into_iter()
        .try_fold(0_usize, usize::checked_add)
        .ok_or("timeline JSON source size overflow")?;
        total = row_bound(total, source, row.private)?;
        if total > limit {
            return Err("timeline JSON output limit".into());
        }
    }
    Ok(total)
}

/// Serialize complete ordered NDJSON only after one full output reservation.
/// # Arguments
/// * `rows` - Complete checked API result, borrowed without cloning metadata or strings.
/// * `limit` - Finite separate serialization capacity.
/// # Returns
/// Compact key-ordered NDJSON with exactly one LF per row and no extra output.
/// # Errors
/// Preserves UTF-8, bound, allocation or serializer failure before stdout is touched.
fn serialize(rows: &[RecordedCheckin], limit: usize) -> Result<Vec<u8>, Error> {
    let capacity = output_bound(rows, limit)?;
    let mut output = Vec::new();
    output.try_reserve_exact(capacity)?;
    for row in rows {
        serde_json::to_writer(&mut output, &JsonRow::borrow(row)?)?;
        output.push(b'\n');
    }
    Ok(output)
}

/// Write the already complete bounded response, retaining the original stream cause.
/// # Arguments
/// * `writer` - Selected stdout stream or a test-owned failing writer.
/// * `output` - Complete response after admission, checked completion and serialization.
/// # Returns
/// Success after the entire response was written.
/// # Errors
/// Preserves original I/O failures; an I/O failure may leave partial stdout.
fn write_output(mut writer: impl Write, output: &[u8]) -> Result<(), Error> {
    writer.write_all(output)?;
    Ok(())
}

/// Admit an existing locator, complete the history API and then write bounded NDJSON.
/// # Arguments
/// * `args` - Command suffix; standalone help requires no repository or checkout.
/// # Returns
/// Success after complete help or history output; both `timeline` and `log` use this adapter.
/// # Errors
/// Preserves selector, admission, checked API, capacity, UTF-8, serializer and stdout causes.
/// Opening admission and the API each own separate operation ledgers; serialization
/// owns a separate finite capacity derived from the unchanged CLI content limit.
/// # Panics
/// Panics if called from an already running Tokio runtime.
pub(crate) fn run(args: &[OsString]) -> Result<(), Error> {
    let Request::Read {
        repository,
        checkout,
    } = request(args)?
    else {
        return write_output(std::io::stdout().lock(), format!("{USAGE}\n").as_bytes());
    };
    let rows = tokio::runtime::Builder::new_current_thread()
        .build()?
        .block_on(async {
            let (mut owner, view) = crate::vcs_open::admit_existing(repository, checkout).await?;
            owner
                .recorded_timeline(&view.checkout_id, crate::vcs_open::limits())
                .await
                .map_err(Error::from)
        })?;
    let limit = usize::try_from(crate::vcs_open::limits().content.max_total_decoded_bytes)?;
    let output = serialize(&rows, limit)?;
    write_output(std::io::stdout().lock(), &output)
}

#[cfg(test)]
/// Check bounded wire encoding independently of admitted storage fixtures.
mod tests {
    use super::*;

    /// Build a small wire-only row whose four source fields each have known byte length.
    /// # Arguments
    /// * `private` - Exact boolean spelling selected for the row.
    /// # Returns
    /// Owned fixture metadata independent of the formatter and API implementation.
    fn row(private: bool) -> RecordedCheckin {
        RecordedCheckin {
            hash: "h".into(),
            date: b"D".to_vec(),
            actor: b"U".to_vec(),
            comment: b"C".to_vec(),
            private,
        }
    }

    /// Convert literal option spellings into operating-system arguments.
    /// # Arguments
    /// * `values` - Exact test-owned tokens.
    /// # Returns
    /// Complete owned argument vector.
    fn arguments(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    /// Keep standalone help storage-free and require both unique exact bounded selectors.
    /// # Panics
    /// Panics if grammar becomes ambiguous, implicit or permissive about invalid text.
    fn exact_options_and_standalone_help() {
        for flag in ["-h", "--help"] {
            assert!(matches!(
                request(&arguments(&[flag])).unwrap(),
                Request::Help
            ));
        }
        for values in [
            vec!["--repository", "../repo", "--checkout", "work"],
            vec!["--checkout", "work", "--repository", "../repo"],
        ] {
            assert!(
                matches!(request(&arguments(&values)).unwrap(), Request::Read { repository, checkout } if repository == std::path::Path::new("../repo") && checkout == "work")
            );
        }
        for (values, expected) in [
            (vec![], "timeline requires --repository REPO"),
            (vec!["--repository"], "timeline option requires a value"),
            (vec!["--checkout"], "timeline option requires a value"),
            (
                vec!["--repository", "repo"],
                "timeline requires --checkout ID",
            ),
            (
                vec!["--checkout", "work"],
                "timeline requires --repository REPO",
            ),
        ] {
            assert_eq!(
                request(&arguments(&values)).err().unwrap().to_string(),
                expected
            );
        }
        for values in [
            vec!["--repository", "repo", "--repository", "other"],
            vec!["--checkout", "work", "--checkout", "other"],
            vec![
                "--repository",
                "repo",
                "--checkout",
                "work",
                "--repository",
                "other",
            ],
            vec![
                "--checkout",
                "work",
                "--repository",
                "repo",
                "--checkout",
                "other",
            ],
            vec!["--help", "--repository", "repo"],
            vec!["--repository=repo", "--checkout", "work"],
            vec!["--repository", "repo", "--checkout=work"],
            vec!["current"],
            vec!["--limit", "1"],
            vec!["--repository", "repo", "--checkout", "work", "extra"],
        ] {
            assert_eq!(
                request(&arguments(&values)).err().unwrap().to_string(),
                USAGE,
                "{values:?}"
            );
        }
        use std::os::unix::ffi::OsStringExt;
        for (flag, counterpart, valid, maximum) in [
            ("--repository", "--checkout", "work", 4096),
            ("--checkout", "--repository", "repo", 1024),
        ] {
            let boundary = "x".repeat(maximum);
            for selected_first in [true, false] {
                let complete = |value: OsString| {
                    if selected_first {
                        vec![flag.into(), value, counterpart.into(), valid.into()]
                    } else {
                        vec![counterpart.into(), valid.into(), flag.into(), value]
                    }
                };
                let (expected_repo, expected_checkout) = if flag == "--repository" {
                    (boundary.as_str(), valid)
                } else {
                    (valid, boundary.as_str())
                };
                assert!(matches!(
                    request(&complete(boundary.clone().into())).unwrap(),
                    Request::Read { repository, checkout }
                        if repository == std::path::Path::new(expected_repo)
                            && checkout == expected_checkout
                ));
                for invalid in [
                    String::new(),
                    "nul\0value".to_owned(),
                    "x".repeat(maximum + 1),
                ] {
                    assert_eq!(
                        request(&complete(invalid.into()))
                            .err()
                            .unwrap()
                            .to_string(),
                        "timeline selector limit or spelling"
                    );
                }
                assert_eq!(
                    request(&complete(OsString::from_vec(vec![0xff])))
                        .err()
                        .unwrap()
                        .to_string(),
                    "timeline selector must be UTF-8"
                );
            }
        }
    }

    #[test]
    /// Count each source field once and require exact cumulative worst-case output capacity.
    /// # Panics
    /// Panics on a wrong fixed overhead, double-counted hash, reset ledger or permissive cap.
    fn independent_fixed_overhead_exact_one_under_cumulative_and_zero() {
        let true_skeleton =
            b"{\"hash\":\"\",\"date\":\"\",\"actor\":\"\",\"comment\":\"\",\"private\":true}\n";
        let false_skeleton =
            b"{\"hash\":\"\",\"date\":\"\",\"actor\":\"\",\"comment\":\"\",\"private\":false}\n";
        assert_eq!(true_skeleton.len(), 61);
        assert_eq!(false_skeleton.len(), 62);
        let rows = [row(true), row(false)];
        // Four one-byte source values per row: 6*(1+1+1+1) + literal syntax.
        let expected = 2 * 6 * 4 + true_skeleton.len() + false_skeleton.len();
        assert_eq!(output_bound(&rows, expected).unwrap(), 171);
        assert!(output_bound(&rows, expected - 1).is_err());
        assert!(
            output_bound(&rows, 86).is_err(),
            "complete rows may not reset the cap"
        );
        let output = serialize(&rows, expected).unwrap();
        assert_eq!(output, b"{\"hash\":\"h\",\"date\":\"D\",\"actor\":\"U\",\"comment\":\"C\",\"private\":true}\n{\"hash\":\"h\",\"date\":\"D\",\"actor\":\"U\",\"comment\":\"C\",\"private\":false}\n");
        assert_eq!(
            output.capacity(),
            expected,
            "serialization must fit its single reservation"
        );
        assert!(serialize(&rows, expected - 1).is_err());
        assert_eq!(output_bound(&[], 0).unwrap(), 0);
        assert!(serialize(&[], 0).unwrap().is_empty());
        assert!(row_bound(0, usize::MAX, false).is_err());
        assert!(row_bound(usize::MAX, 0, true).is_err());
        assert!(row_bound(0, isize::MAX as usize / 6, false).is_err());
    }

    #[test]
    /// Escape every supported control form and preserve multibyte Unicode without normalization.
    /// # Panics
    /// Panics on wrong standard JSON escapes, source-byte arithmetic or ownership changes.
    fn literal_control_escapes_unicode_quotes_and_backslash() {
        let mut row = row(true);
        row.actor = vec![0, 1, 8, 9, 10, 11, 12, 13, 31];
        row.comment = "é雪\"\\".as_bytes().to_vec();
        let actor_pointer = row.actor.as_ptr();
        let comment_pointer = row.comment.as_ptr();
        // h1 + D1 + nine control bytes + é2/雪3/quote1/backslash1 =18.
        let expected_bound = 6 * 18 + 61;
        assert_eq!(
            output_bound(std::slice::from_ref(&row), expected_bound).unwrap(),
            169
        );
        let output = serialize(std::slice::from_ref(&row), expected_bound).unwrap();
        assert_eq!(output, "{\"hash\":\"h\",\"date\":\"D\",\"actor\":\"\\u0000\\u0001\\b\\t\\n\\u000b\\f\\r\\u001f\",\"comment\":\"é雪\\\"\\\\\",\"private\":true}\n".as_bytes());
        assert_eq!(row.actor.as_ptr(), actor_pointer);
        assert_eq!(row.comment.as_ptr(), comment_pointer);
        assert_eq!(output.capacity(), expected_bound);
    }

    #[test]
    /// Refuse invalid UTF-8 in each native byte field before any output is produced.
    /// # Panics
    /// Panics if encoding becomes lossy, normalizes bytes or hides the original UTF-8 cause.
    fn every_byte_field_requires_strict_utf8() {
        for field in 0..3 {
            let mut row = row(false);
            match field {
                0 => row.date = vec![0xff],
                1 => row.actor = vec![0xff],
                _ => row.comment = vec![0xff],
            }
            let error = serialize(&[row], 4096).unwrap_err();
            assert!(error.downcast_ref::<std::str::Utf8Error>().is_some());
        }
    }

    #[test]
    /// Preserve original writer errors and write exactly the already-complete response.
    /// # Panics
    /// Panics if output changes, failures are swallowed or the original I/O kind is lost.
    fn output_io_cause_and_complete_bytes() {
        let mut output = Vec::new();
        write_output(&mut output, b"complete\n").unwrap();
        assert_eq!(output, b"complete\n");
        let mut destination = [];
        let error = write_output(destination.as_mut_slice(), b"complete\n").unwrap_err();
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::WriteZero
        );
    }
}
