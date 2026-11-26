//! Read tasks and projects from their typed rows, without a replay.
//!
//! `task list` and the lookup of `task show` read only the rows that they print or
//! match. Their cost grows with the number of tasks that they read, and not with
//! the rest of the repository. `gos v verify` compares the typed rows with the
//! artifacts.
//!
//! ## Recommendations
//! Prove one task with [`LocalOwnerRepository::verify_issue`] before printing its
//! artifact.
use super::recorded_history::{bounded_rows, scalar_i64};
use super::{
    AdmissionLimits, IssueLimits, IssueMatch, IssueRecord, IssueTicket, LocalOwnerRepository,
    StorageError,
};
use crate::gest_project_values::{ProjectValueInputs, project_from_values};
use crate::gest_task_values::{TaskValueInputs, task_from_values};
use crate::storage::{WriteTransaction, content::Budget};
use libsql::Value;

/// The refusal of a typed row that does not hold a valid task or project.
const TYPED: &str = "a stored typed row does not hold a valid task or project";

/// Read each `combined` task and project row with its ticket ID and artifact name.
///
/// The rows come in receipt order. A row joins its ticket by the Gest ID, the `tkt-`
/// tag of that ticket, and the blob that the tag names. A project row puts its three
/// dates in the columns of the project ID, the title and the priority.
const TYPED_ROWS: &str = "SELECT b.rcvid,b.rid,k.tkt_uuid,b.uuid,'task',t.id,t.project_id,t.title,t.priority,t.status,t.description,t.assigned_to,t.metadata,t.resolved_at,t.created_at,t.updated_at FROM g_gest_tasks t JOIN ticket k ON k.gest_id=json_quote(t.id) JOIN tag g ON g.tagname='tkt-'||k.tkt_uuid JOIN tagxref x ON x.tagid=g.tagid JOIN blob b ON b.rid=x.rid WHERE t.scope_key='combined' UNION ALL SELECT b.rcvid,b.rid,k.tkt_uuid,b.uuid,'project',p.id,p.created_at,p.updated_at,p.archived_at,NULL,NULL,NULL,NULL,NULL,NULL,NULL FROM g_gest_projects p JOIN ticket k ON k.gest_id=json_quote(p.id) JOIN tag g ON g.tagname='tkt-'||k.tkt_uuid JOIN tagxref x ON x.tagid=g.tagid JOIN blob b ON b.rid=x.rid WHERE p.scope_key='combined' ORDER BY 1,2";

/// Count the `combined` tasks whose ID starts with a pattern. With `?2` set to 1, only
/// the tasks in a status that is not terminal are counted.
const TASKS_MATCHING: &str = "SELECT count(*) FROM g_gest_tasks WHERE scope_key='combined' AND id GLOB ?1 AND (?2=0 OR status NOT IN ('done','cancelled'))";

/// Read the ID of each task that [`TASKS_MATCHING`] counts.
const TASK_MATCHING: &str = "SELECT id FROM g_gest_tasks WHERE scope_key='combined' AND id GLOB ?1 AND (?2=0 OR status NOT IN ('done','cancelled'))";

/// Turn one typed value into the JSON value of its J card.
/// # Arguments
/// * `value` - The stored value.
/// * `raw` - True for `metadata`, whose text is the JSON of the card itself.
/// # Returns
/// The JSON value.
/// # Errors
/// Refuses a value of another type and `metadata` text that is not JSON.
fn json(value: &Value, raw: bool) -> Result<serde_json::Value, StorageError> {
    Ok(match value {
        Value::Null => serde_json::Value::Null,
        Value::Integer(number) => (*number).into(),
        Value::Text(text) if raw => {
            serde_json::from_str(text).map_err(|_| StorageError::state(TYPED))?
        }
        Value::Text(text) => text.clone().into(),
        _ => return Err(StorageError::state(TYPED)),
    })
}

/// Turn one row of [`TYPED_ROWS`] into a ticket.
/// # Arguments
/// * `row` - The sixteen values of the row.
/// # Returns
/// The ticket ID, the artifact name and the task or project.
/// # Errors
/// Refuses a row of another shape and values that do not make a valid task or project.
fn typed_issue(row: &[Value]) -> Result<IssueTicket, StorageError> {
    let [
        _,
        _,
        Value::Text(ticket_id),
        Value::Text(native_name),
        Value::Text(kind),
        fields @ ..,
    ] = row
    else {
        return Err(StorageError::state(TYPED));
    };
    let field = |index: usize| json(&fields[index], index == 7);
    let record = if kind == "task" {
        IssueRecord::Task(
            task_from_values(TaskValueInputs {
                gest_id: field(0)?,
                project_id: field(1)?,
                title: field(2)?,
                priority: field(3)?,
                status: field(4)?,
                description: field(5)?,
                assigned_to: field(6)?,
                metadata: field(7)?,
                resolved_at: field(8)?,
                created_at: field(9)?,
                updated_at: field(10)?,
            })
            .map_err(|_| StorageError::state(TYPED))?,
        )
    } else {
        IssueRecord::Project(
            project_from_values(ProjectValueInputs {
                gest_id: field(0)?,
                created_at: field(1)?,
                updated_at: field(2)?,
                archived_at: field(3)?,
            })
            .map_err(|_| StorageError::state(TYPED))?,
        )
    };
    Ok(IssueTicket {
        ticket_id: ticket_id.clone(),
        native_name: native_name.clone(),
        record,
    })
}

impl WriteTransaction<'_> {
    /// Read every task and project from its typed row, in receipt order.
    ///
    /// The read replays no artifact. It reads each `combined` typed row, and the
    /// ticket, tag and blob rows that give its ticket ID and artifact name.
    /// # Arguments
    /// * `limits` - The width of one row.
    /// * `work` - The ledger of the operation.
    /// # Returns
    /// The tasks and projects.
    /// # Errors
    /// Refuses a row wider than the ceiling, a row that does not hold a valid task or
    /// project, exhausted work and SQL failures.
    pub(super) async fn read_typed_issues(
        &mut self,
        limits: IssueLimits,
        work: &mut Budget,
    ) -> Result<Vec<IssueTicket>, StorageError> {
        let rows = bounded_rows(
            self,
            TYPED_ROWS,
            vec![],
            // The list has no ceiling on its count of issues.
            usize::MAX,
            limits.admission.parser.input_bytes,
            work,
        )
        .await?;
        rows.iter().map(|row| typed_issue(row)).collect()
    }
}

impl LocalOwnerRepository {
    /// Find the one task that an ID or a prefix names, as Gest `resolve_id` does.
    ///
    /// The tasks in a status that is not terminal are searched first. When none of
    /// them matches, every task is searched. Projects are never matched. The search
    /// reads only the `combined` task rows whose ID starts with the prefix, by their
    /// primary key. It changes nothing.
    /// # Arguments
    /// * `prefix` - A Gest ID prefix, from 1 to 32 letters from `k` to `z`.
    /// * `limits` - The ceilings of the operation.
    /// # Returns
    /// No match, the one full Gest ID, or the count of matches in the first set that
    /// has a match.
    /// # Errors
    /// Refuses an invalid prefix, the refusals of the checks of each command, exhausted
    /// work and SQL failures.
    /// # Examples
    /// ```no_run
    /// use gossil::storage::{AdmissionLimits, IssueMatch, LocalOwnerRepository, StorageError};
    /// async fn find(owner: &mut LocalOwnerRepository, limits: AdmissionLimits)
    ///     -> Result<IssueMatch, StorageError> {
    ///     owner.find_task("kk", limits).await
    /// }
    /// ```
    pub async fn find_task(
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
        let mut budget = Budget::new(limits.content)?;
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission_budgeted(&self.principal, &mut budget)
                .await?;
            tx.check_recording_state(&self.principal, limits, &mut budget)
                .await?;
            let pattern = Value::Text(format!("{prefix}*"));
            for active in [1, 0] {
                let args = vec![pattern.clone(), Value::Integer(active)];
                let count = scalar_i64(&tx.query_bound(TASKS_MATCHING, args.clone()).await?)?;
                match count {
                    0 => {}
                    1 => {
                        let rows =
                            bounded_rows(&mut tx, TASK_MATCHING, args, 1, 128, &mut budget).await?;
                        let id = match rows.first().map(Vec::as_slice) {
                            Some([Value::Text(id)]) => id.clone(),
                            _ => return Err(StorageError::state(TYPED)),
                        };
                        return Ok(IssueMatch::One(id));
                    }
                    many => return Ok(IssueMatch::Many(u64::try_from(many).unwrap_or(u64::MAX))),
                }
            }
            Ok(IssueMatch::None)
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
    /// `json` keeps each stored type, parses only `metadata`, and refuses other values.
    fn each_typed_value_becomes_its_card() {
        assert_eq!(json(&Value::Null, false).unwrap(), serde_json::Value::Null);
        assert_eq!(
            json(&Value::Integer(7), false).unwrap(),
            serde_json::json!(7)
        );
        assert_eq!(
            json(&Value::Text("a".into()), false).unwrap(),
            serde_json::json!("a")
        );
        assert_eq!(
            json(&Value::Text("{\"a\":1}".into()), true).unwrap(),
            serde_json::json!({"a": 1})
        );
        for (value, raw) in [(Value::Text("{".into()), true), (Value::Real(1.0), false)] {
            assert_eq!(json(&value, raw).unwrap_err().operation, TYPED);
        }
    }

    #[test]
    /// `typed_issue` refuses a row of another shape and values of no valid task or project.
    fn a_bad_typed_row_is_refused() {
        assert_eq!(typed_issue(&[]).unwrap_err().operation, TYPED);
        let row = |kind: &str| {
            let mut row = vec![
                Value::Integer(1),
                Value::Integer(1),
                Value::Text("k".into()),
                Value::Text("n".into()),
                Value::Text(kind.into()),
            ];
            row.extend(std::iter::repeat_n(Value::Null, 11));
            row
        };
        for kind in ["task", "project"] {
            assert_eq!(typed_issue(&row(kind)).unwrap_err().operation, TYPED);
        }
        let mut bad = row("task");
        bad[12] = Value::Real(1.0);
        assert_eq!(typed_issue(&bad).unwrap_err().operation, TYPED);
    }
}
