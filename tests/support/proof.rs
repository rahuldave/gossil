//! Give the refusal of `gos v verify` through the library, for the tamper tests.
//!
//! Open runs only the checks whose cost does not grow with the history. The full
//! proof of `gos v verify` refuses every other change. Each tamper test calls
//! [`verify_refusal`] and gets the refusal of open or of the full proof, as the CLI
//! prints it, after a check that the files kept every byte.
use gossil::storage::{AdmissionLimits, LocalOwnerRepository};
use std::path::Path;

/// Read the bytes of a repository file and its write-ahead log.
/// # Arguments
/// * `path` - The repository file.
/// # Returns
/// The bytes of both files. An absent log reads as empty.
pub fn file_bytes(path: &Path) -> (Vec<u8>, Vec<u8>) {
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    (
        std::fs::read(path).unwrap(),
        std::fs::read(wal).unwrap_or_default(),
    )
}

/// Open a repository and run the full proof, as `gos v verify` does.
/// # Arguments
/// * `path` - The repository file, with no open owner.
/// * `limits` - The ceilings of open and of the proof.
/// # Returns
/// Whether open admitted the repository, and the text of the refusal.
/// # Panics
/// Panics if open and the full proof both pass, or if either changes a byte.
#[allow(
    dead_code,
    reason = "each test target that includes this module uses only some of its functions"
)]
pub async fn verify_refusal(path: &Path, limits: AdmissionLimits) -> (bool, String) {
    let before = file_bytes(path);
    let refusal = match LocalOwnerRepository::open(path, limits).await {
        Err(error) => (false, error.to_string()),
        Ok(mut owner) => {
            let error = owner.verify(limits).await.err();
            drop(owner);
            match error {
                Some(error) => (true, error.to_string()),
                None => panic!("the full proof admitted the repository"),
            }
        }
    };
    assert!(file_bytes(path) == before, "the check changed a byte");
    refusal
}

/// The exact refusal of the full proof for each tamper case, one `SQL<TAB>message` line each.
const REFUSALS: &str = include_str!("../fixtures/tamper-refusals.tsv");

/// Assert that the full proof refused a tamper case with its recorded message.
/// # Arguments
/// * `sql` - The tamper statement, as `tests/fixtures/tamper-refusals.tsv` spells it.
/// * `message` - The refusal that [`verify_refusal`] gave.
/// # Panics
/// Panics if the table has no line for `sql`, or if the first line of the message differs.
#[allow(
    dead_code,
    reason = "each test target that includes this module uses only some of its functions"
)]
pub fn assert_refusal(sql: &str, message: &str) {
    let expected = REFUSALS
        .lines()
        .find_map(|line| line.split_once('\t').filter(|(case, _)| *case == sql))
        .map(|(_, expected)| expected);
    // An older catalog names the repository path, which changes with each run, and
    // adds lines of advice. The table keeps the first line, with the path as `REPO`.
    let first = message.lines().next().unwrap_or_default();
    let shown = match (
        first.strip_prefix("gossil: repository "),
        first.find(" is at"),
    ) {
        (Some(_), Some(end)) => format!("gossil: repository REPO{}", &first[end..]),
        _ => first.to_owned(),
    };
    assert_eq!(expected, Some(shown.as_str()), "{sql}");
}
