//! Run the full proof of a catalog6 repository, or the proof of one issue, on demand.
//!
//! [`LocalOwnerRepository::verify`] replays each check-in and each issue artifact,
//! and compares each native and typed table with that replay.
//! [`LocalOwnerRepository::verify_issue`] proves one issue from its own rows. Both
//! read in one transaction that they roll back, so they change nothing.
//!
//! ## Recommendations
//! Keep the ceilings of the proof per artifact and per row. The whole repository has
//! no fixed ceiling, so a proof runs on a repository of any size.
use super::recorded_history::{bounded_rows, scalar_i64};
use super::{AdmissionLimits, IssueTicket, LocalOwnerRepository, ParseWork, StorageError};
use crate::storage::content::{Budget, Limits};
use libsql::Value;

/// The total content ceiling of a proof, which no real repository reaches.
const UNMETERED: u64 = 1 << 60;

/// Count the rows of the tables that the proof reads whole.
///
/// The proof uses the sum as its ceiling on the rows of one query, in place of the
/// fixed entry ceiling of a command.
const ROWS: &str = "SELECT (SELECT count(*) FROM blob)+(SELECT count(*) FROM event)+(SELECT count(*) FROM tag)+(SELECT count(*) FROM tagxref)+(SELECT count(*) FROM rcvfrom)+(SELECT count(*) FROM ticket)+(SELECT count(*) FROM ticketchng)+(SELECT count(*) FROM filename)+(SELECT count(*) FROM plink)+(SELECT count(*) FROM mlink)+(SELECT count(*) FROM g_vfile)+(SELECT count(*) FROM g_path_scope)+(SELECT count(*) FROM g_gest_tasks)+(SELECT count(*) FROM g_gest_projects)";

/// Read one text value of a row.
/// # Arguments
/// * `value` - The value, if the row has one.
/// # Returns
/// The text.
/// # Errors
/// Refuses a missing value or one that is not text, as a changed issue row.
fn text(value: Option<&Value>) -> Result<String, StorageError> {
    match value {
        Some(Value::Text(text)) => Ok(text.clone()),
        _ => Err(StorageError::state(
            "stored issue rows differ from the ticket artifact",
        )),
    }
}

/// The counts that a successful proof of the whole repository gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifySummary {
    /// The private check-ins that the proof replayed.
    pub checkins: usize,
    /// The task and project creations that the proof replayed.
    pub issues: usize,
    /// The table comparisons with replay that passed.
    pub tables: usize,
}

/// The issues whose Gest ID starts with a prefix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IssueMatch {
    /// No task or project ID starts with the prefix.
    None,
    /// Exactly one ID starts with the prefix. It holds the full Gest ID.
    One(String),
    /// More than one ID starts with the prefix. It holds the count.
    Many(u64),
}

/// Give the ceilings of a proof: the caller's ceilings of one artifact, and no total.
///
/// The timeline uses the same ledgers, so that a page has no ceiling on the whole
/// chain either.
/// # Arguments
/// * `limits` - The caller's ceilings.
/// # Returns
/// The content ledger and the parser ledger of the proof.
/// # Errors
/// Refuses invalid ceilings.
pub(super) fn proof_ledgers(
    limits: AdmissionLimits,
) -> Result<(Limits, Budget, ParseWork), StorageError> {
    let content = Limits {
        max_total_stored_bytes: UNMETERED,
        max_total_decoded_bytes: UNMETERED,
        ..limits.content
    };
    Ok((
        content,
        Budget::new(content)?,
        ParseWork::per_artifact(limits.parser)?,
    ))
}

impl LocalOwnerRepository {
    /// Run the full proof of the repository and count what it checked.
    ///
    /// The proof replays every check-in and every issue artifact, and compares every
    /// native and typed table with that replay. `limits` bound each artifact and each
    /// row. The proof has no ceiling on the whole repository. It changes nothing.
    /// # Arguments
    /// * `limits` - The ceilings of one artifact and one row. The total content
    ///   ceilings are not used.
    /// # Returns
    /// The counts of check-ins, issues and table comparisons.
    /// # Errors
    /// Gives the fixed refusal of the first check that fails, the refusals of an
    /// artifact or row over its ceiling, and SQL failures.
    /// # Examples
    /// ```no_run
    /// use gossil::storage::{AdmissionLimits, LocalOwnerRepository, StorageError, VerifySummary};
    /// async fn check(owner: &mut LocalOwnerRepository, limits: AdmissionLimits)
    ///     -> Result<VerifySummary, StorageError> {
    ///     owner.verify(limits).await
    /// }
    /// ```
    pub async fn verify(&mut self, limits: AdmissionLimits) -> Result<VerifySummary, StorageError> {
        let (content, mut budget, mut parse) = proof_ledgers(limits)?;
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission_budgeted(&self.principal, &mut budget)
                .await?;
            let rows = scalar_i64(&tx.query_bound(ROWS, vec![]).await?)?;
            let rows =
                usize::try_from(rows).map_err(|_| StorageError::state("verify row count"))?;
            let mut proof = AdmissionLimits {
                content,
                parser: limits.parser,
            };
            proof.parser.entries = proof.parser.entries.max(rows);
            let history = tx
                .validate_recording_repository_history(
                    &self.principal,
                    proof,
                    &mut budget,
                    &mut parse,
                )
                .await?;
            let tags = "SELECT count(*) FROM tag WHERE tagname GLOB 'tkt-*'";
            let issues = scalar_i64(&tx.query_bound(tags, vec![]).await?)?;
            Ok(VerifySummary {
                checkins: history.map_or(0, |history| history.commits.len()),
                issues: usize::try_from(issues)
                    .map_err(|_| StorageError::state("verify issue count"))?,
                tables: budget.compared(),
            })
        }
        .await;
        let result = tx.record(result);
        tx.rollback().await?;
        result
    }

    /// Find the tasks and projects whose Gest ID starts with a prefix.
    ///
    /// It first checks the identity, the schema and the policy. The search then reads
    /// only the rows of the `combined` scope of the typed tables, by their primary key.
    /// It changes nothing.
    /// # Arguments
    /// * `prefix` - A valid Gest ID prefix, from 1 to 32 letters from `k` to `z`.
    /// * `limits` - The ceilings of one artifact and one row.
    /// # Returns
    /// No match, the one full Gest ID, or the count of matches.
    /// # Errors
    /// Refuses an invalid prefix, a failed check and SQL failures.
    /// # Examples
    /// ```no_run
    /// use gossil::storage::{AdmissionLimits, IssueMatch, LocalOwnerRepository, StorageError};
    /// async fn find(owner: &mut LocalOwnerRepository, limits: AdmissionLimits)
    ///     -> Result<IssueMatch, StorageError> {
    ///     owner.find_issue("kk", limits).await
    /// }
    /// ```
    pub async fn find_issue(
        &mut self,
        prefix: &str,
        limits: AdmissionLimits,
    ) -> Result<IssueMatch, StorageError> {
        if prefix.is_empty()
            || prefix.len() > 32
            || !prefix.bytes().all(|b| (b'k'..=b'z').contains(&b))
        {
            return Err(StorageError::state("invalid issue ID prefix"));
        }
        let (_, mut budget, _) = proof_ledgers(limits)?;
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission_budgeted(&self.principal, &mut budget)
                .await?;
            let pattern = Value::Text(format!("{prefix}*"));
            let count = scalar_i64(&tx.query_bound("SELECT (SELECT count(*) FROM g_gest_tasks WHERE scope_key='combined' AND id GLOB ?1)+(SELECT count(*) FROM g_gest_projects WHERE scope_key='combined' AND id GLOB ?1)", vec![pattern.clone()]).await?)?;
            Ok(match count {
                0 => IssueMatch::None,
                1 => {
                    let rows = bounded_rows(&mut tx, "SELECT id FROM g_gest_tasks WHERE scope_key='combined' AND id GLOB ?1 UNION ALL SELECT id FROM g_gest_projects WHERE scope_key='combined' AND id GLOB ?1", vec![pattern], 1, 128, &mut budget).await?;
                    IssueMatch::One(text(rows.first().and_then(|row| row.first()))?)
                }
                many => IssueMatch::Many(u64::try_from(many).unwrap_or(u64::MAX)),
            })
        }
        .await;
        let result = tx.record(result);
        tx.rollback().await?;
        result
    }

    /// Prove one task or project from its own rows.
    ///
    /// The ticket whose `gest_id` names the ID must be the only one. The proof then
    /// replays its one artifact and compares its `event`, `tagxref`, `ticket`,
    /// `ticketchng` and `combined` typed rows with that replay. It refuses a row of
    /// the `unsent`, `unclustered`, `leaf`, `phantom`, `orphan`, `backlink`,
    /// `attachment`, `mlink` or `plink` table that names the artifact or the ticket.
    ///
    /// It replays no other artifact. For a task it checks only that a `combined`
    /// project row with its project ID exists. When none does, it refuses with `a
    /// stored task names a project that the repository does not hold`, although the
    /// full proof admits such a task. It changes nothing.
    ///
    /// The one lookup of the ticket by its Gest ID scans the `ticket` table. The other
    /// reads use an index or a primary key. The proof checks that each table that the
    /// full proof keeps empty in every repository has no row, with one fixed query per
    /// table.
    ///
    /// The proof does not see a row that no index of the issue reaches, or a typed row
    /// in another scope.
    /// # Arguments
    /// * `id` - The full Gest ID of the task or project.
    /// * `limits` - The ceilings of one artifact and one row.
    /// # Returns
    /// The ticket ID, native name and record of the issue.
    /// # Errors
    /// Refuses an ID that no ticket or more than one ticket holds, and a row in a table
    /// that the full proof keeps empty in every repository. Also refuses every
    /// difference from the replay, the refusals of the checks of each command,
    /// exhausted ceilings and SQL failures.
    /// # Examples
    /// ```no_run
    /// use gossil::storage::{AdmissionLimits, IssueTicket, LocalOwnerRepository, StorageError};
    /// async fn prove(owner: &mut LocalOwnerRepository, id: &str, limits: AdmissionLimits)
    ///     -> Result<IssueTicket, StorageError> {
    ///     owner.verify_issue(id, limits).await
    /// }
    /// ```
    pub async fn verify_issue(
        &mut self,
        id: &str,
        limits: AdmissionLimits,
    ) -> Result<IssueTicket, StorageError> {
        let (_, mut budget, mut parse) = proof_ledgers(limits)?;
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission_budgeted(&self.principal, &mut budget)
                .await?;
            let json = serde_json::to_string(id)
                .map_err(|error| StorageError::caused("issue ID", error))?;
            let rows = bounded_rows(
                &mut tx,
                "SELECT tkt_uuid FROM ticket WHERE gest_id=?1",
                vec![Value::Text(json)],
                2,
                128,
                &mut budget,
            )
            .await?;
            let [row] = rows.as_slice() else {
                return Err(StorageError::state(
                    "stored issue rows differ from the ticket artifact",
                ));
            };
            let ticket_id = text(row.first())?;
            tx.prove_issue(&self.principal, &ticket_id, limits, &mut budget, &mut parse)
                .await
        }
        .await;
        let result = tx.record(result);
        tx.rollback().await?;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// `text` reads a text value, and refuses a missing value or a value of another type.
    fn only_a_text_value_is_read() {
        assert_eq!(text(Some(&Value::Text("k".into()))).unwrap(), "k");
        let differs = "stored issue rows differ from the ticket artifact";
        assert_eq!(text(None).unwrap_err().to_string(), differs);
        assert_eq!(
            text(Some(&Value::Integer(1))).unwrap_err().to_string(),
            differs
        );
    }
}
