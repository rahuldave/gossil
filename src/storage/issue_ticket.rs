//! Store and replay private Gest task and project tickets in a catalog 6 repository.
//!
//! See the [issue ticket guide](https://rahuldave.com/gossil/issue-ticket.html) for
//! the rows, the limits of this build and a library example.
//!
//! A writer call takes the creation artifact of one task or project, as
//! [`crate::task_ticket::task_creation_bytes`] or
//! [`crate::gest_ticket::project_creation_bytes`] emits it. In one exclusive
//! transaction it writes the `rcvfrom`, `blob`, `private`, `tag`, `tagxref`,
//! `ticket`, `ticketchng` and `event` rows, which stock Fossil 2.29 writes when it
//! receives that ticket as a private artifact. It adds one typed row in
//! `g_gest_tasks` or `g_gest_projects`. A reader call takes a ticket ID, derives the
//! ticket state again from its one stored artifact, and checks that every stored
//! row equals that state.
//!
//! The rows follow Fossil `manifest.c:2597-2604` and `2137-2225`, `tkt.c:272-427`
//! and `450-497`, `tag.c` `tag_insert`, and `content.c` `content_put_ex`, at
//! b3a1c8efb2731dffbd5cc38c58ac349746de6da6. Fossil is Copyright D. Richard Hipp
//! and contributors, and the frozen source evidence keeps its BSD two-clause
//! license.
//!
//! This build supports one shape only. Each ticket ID has one creation artifact,
//! the ticket is private, and no text field contains `[`. It writes no selector,
//! tag, link, audit or undo row.
//!
//! ## Recommendations
//! Route both calls through the admission of [`LocalOwnerRepository::open`] once that
//! admission accepts issue rows.
use super::recorded_history::{native_name, parse_recording_budgeted};
use super::{AdmissionLimits, LocalOwnerRepository, ParseWork, PathScope, StorageError};
use crate::artifact::{ArtifactType, ParsedArtifact};
use crate::gest_project_values::{ProjectValueInputs, ProjectValues, project_from_values};
use crate::gest_task_values::{TaskValueInputs, TaskValues, task_from_values};
use crate::native_date::NativeDate;
use crate::storage::{WriteTransaction, content::Budget, metered_query::SqlWork};
use libsql::Value;

/// The `scope_key` of every typed row of this build, which names the owner's combined scope.
const SCOPE: &str = "combined";

/// The refusal of every ticket shape other than one private task or project creation.
const UNSUPPORTED: &str =
    "only one complete Gest task or project creation per ticket ID is supported in this build";

/// Ask for one private task or project ticket. No field grants owner authority.
pub struct PrivateIssueRequest<'a> {
    /// The complete creation artifact, as the task or project emitter writes it.
    pub artifact: &'a [u8],
    /// The requested scope. This build writes [`PathScope::Private`] only.
    pub scope: PathScope,
}

/// Every finite ceiling of one issue ticket call.
#[derive(Clone, Copy)]
pub struct IssueLimits {
    /// Content, decoded-byte and parser ceilings, shared by the whole call.
    pub admission: AdmissionLimits,
    /// SQL result rows that the call may read, preflight rows included.
    pub sql_rows: u64,
    /// SQL result payload bytes that the call may read.
    pub sql_bytes: u64,
}

/// The validated record that one creation artifact carries.
#[derive(Clone, Debug, PartialEq)]
pub enum IssueRecord {
    /// A task, with all eleven model fields.
    Task(TaskValues),
    /// A project, with all four model fields.
    Project(ProjectValues),
}

/// One stored issue ticket, as the writer stored it or the reader derived it again.
#[derive(Clone, Debug, PartialEq)]
pub struct IssueTicket {
    /// The forty lowercase hexadecimal bytes of the K card.
    pub ticket_id: String,
    /// The full SHA3-256 native name of the creation artifact.
    pub native_name: String,
    /// The task or project that the artifact carries.
    pub record: IssueRecord,
}

/// The fixed SQL of one Gest kind. The column lists follow the sorted J cards.
struct KindSql {
    /// One count that is zero only when no row holds the ticket ID, its tag, its name or its Gest ID.
    exists: &'static str,
    /// Insert the `ticket` row from the ticket ID, the date and every J value in card order.
    ticket: &'static str,
    /// Insert the `ticketchng` row from the ticket ID, RID, date, actor and every J value.
    change: &'static str,
    /// Insert the typed row from the values of [`KindSql::typed_fields`].
    typed: &'static str,
    /// Read the typed rows of one Gest ID in every scope, in typed column order.
    typed_read: &'static str,
    /// The J field of each typed column after `scope_key`, in table column order.
    typed_fields: &'static [&'static str],
}

/// The fixed SQL of the task kind.
const TASK: KindSql = KindSql {
    exists: "SELECT (SELECT count(*) FROM ticket WHERE tkt_uuid=?1)+(SELECT count(*) FROM tag WHERE tagname=?2)+(SELECT count(*) FROM blob WHERE uuid=?3)+(SELECT count(*) FROM g_gest_tasks WHERE id=?4)",
    ticket: "INSERT INTO ticket(tkt_uuid,tkt_mtime,tkt_ctime,assigned_to,created_at,description,gest_id,gossil_deleted,gossil_kind,gossil_schema,metadata,priority,project_id,resolved_at,status,title,updated_at) VALUES(?1,?2,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
    change: "INSERT INTO ticketchng(tkt_id,tkt_rid,tkt_mtime,tkt_user,assigned_to,created_at,description,gest_id,gossil_deleted,gossil_kind,gossil_schema,metadata,priority,project_id,resolved_at,status,title,updated_at) SELECT tkt_id,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18 FROM ticket WHERE tkt_uuid=?1",
    typed: "INSERT INTO g_gest_tasks(scope_key,id,project_id,title,priority,status,description,assigned_to,metadata,resolved_at,created_at,updated_at) VALUES('combined',?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
    typed_read: "SELECT scope_key,id,project_id,title,priority,status,description,assigned_to,metadata,resolved_at,created_at,updated_at FROM g_gest_tasks WHERE id=?1",
    typed_fields: &[
        "gest_id",
        "project_id",
        "title",
        "priority",
        "status",
        "description",
        "assigned_to",
        "metadata",
        "resolved_at",
        "created_at",
        "updated_at",
    ],
};

/// The fixed SQL of the project kind.
const PROJECT: KindSql = KindSql {
    exists: "SELECT (SELECT count(*) FROM ticket WHERE tkt_uuid=?1)+(SELECT count(*) FROM tag WHERE tagname=?2)+(SELECT count(*) FROM blob WHERE uuid=?3)+(SELECT count(*) FROM g_gest_projects WHERE id=?4)",
    ticket: "INSERT INTO ticket(tkt_uuid,tkt_mtime,tkt_ctime,archived_at,created_at,gest_id,gossil_deleted,gossil_kind,gossil_schema,updated_at) VALUES(?1,?2,?2,?3,?4,?5,?6,?7,?8,?9)",
    change: "INSERT INTO ticketchng(tkt_id,tkt_rid,tkt_mtime,tkt_user,archived_at,created_at,gest_id,gossil_deleted,gossil_kind,gossil_schema,updated_at) SELECT tkt_id,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11 FROM ticket WHERE tkt_uuid=?1",
    typed: "INSERT INTO g_gest_projects(scope_key,id,created_at,updated_at,archived_at) VALUES('combined',?1,?2,?3,?4)",
    typed_read: "SELECT scope_key,id,created_at,updated_at,archived_at FROM g_gest_projects WHERE id=?1",
    typed_fields: &["gest_id", "created_at", "updated_at", "archived_at"],
};

/// One creation artifact after it was parsed and emitted again with the same bytes.
struct Creation {
    /// The fixed SQL of its kind.
    kind: &'static KindSql,
    /// The K card.
    ticket_id: String,
    /// The decoded U card.
    user: String,
    /// The native Julian day of the D card, as Fossil stores it.
    mtime: Value,
    /// Each J card name and decoded value, in card order.
    fields: Vec<(String, String)>,
    /// The validated record.
    record: IssueRecord,
}
impl Creation {
    /// Return the decoded value of one J card.
    /// # Arguments
    /// * `name` - The J field name.
    /// # Returns
    /// The decoded value, or `None` when the artifact has no such card.
    fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, value)| value.as_str())
    }
    /// Return the values of the typed row, after `scope_key`, in column order.
    /// # Returns
    /// A JSON string as TEXT, a JSON number as INTEGER, a JSON null as NULL, and the
    /// metadata JSON text unchanged.
    fn typed_values(&self) -> Vec<Value> {
        self.kind
            .typed_fields
            .iter()
            .map(|name| {
                let raw = self.field(name).unwrap_or("null");
                match (*name, serde_json::from_str(raw)) {
                    ("metadata", _) => Value::Text(raw.into()),
                    (_, Ok(serde_json::Value::String(text))) => Value::Text(text),
                    (_, Ok(serde_json::Value::Number(number))) => {
                        number.as_i64().map_or(Value::Null, Value::Integer)
                    }
                    _ => Value::Null,
                }
            })
            .collect()
    }
    /// Return the `comment` and `brief` of the native event, as Fossil `manifest_ticket_event` writes them for a new ticket.
    /// # Returns
    /// The comment with the HTML-escaped `title` column, which is empty for a project, and the brief.
    fn event_text(&self) -> (String, String) {
        let (long, short) = (&self.ticket_id[..16], &self.ticket_id[..10]);
        let title = htmlize(self.field("title").unwrap_or(""));
        (
            format!("New ticket [{long}|{short}] <i>{title}</i>."),
            format!("New ticket [{long}|{short}]."),
        )
    }
}

/// Escape text as Fossil `htmlize` does for the `%h` conversion.
/// # Arguments
/// * `text` - Text without NUL.
/// # Returns
/// The text with `<`, `>`, `&`, `"` and `'` replaced by their HTML entities.
fn htmlize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Parse one creation artifact and accept it only when its emitter writes the same bytes again.
/// # Arguments
/// * `tx` - The reserved transaction, which also supplies the native date of the D card.
/// * `bytes` - The complete artifact.
/// * `parse` - The call's parser ledger.
/// * `budget` - The call's content ledger.
/// # Returns
/// The kind, cards and validated record of the creation.
/// # Errors
/// Refuses a malformed artifact, a J value that contains `[`, and every artifact that
/// is not a complete canonical task or project creation.
async fn derive_creation(
    tx: &mut WriteTransaction<'_>,
    bytes: &[u8],
    parse: &mut ParseWork,
    budget: &mut Budget,
) -> Result<Creation, StorageError> {
    let parsed = parse_recording_budgeted(tx, bytes, parse, budget).await?;
    if parsed.artifact_type() != ArtifactType::Ticket {
        return Err(StorageError::state(UNSUPPORTED));
    }
    if parsed
        .ticket_fields()
        .iter()
        .any(|field| field.value().bytes.contains(&b'['))
    {
        return Err(StorageError::state(
            "an issue text field cannot contain '[' in this build, because Markdown backlinks are not ported",
        ));
    }
    let ticket_id = parsed.ticket_id().unwrap_or_default().to_vec();
    let actor = parsed
        .user()
        .map(|user| user.bytes.clone())
        .unwrap_or_default();
    let date = NativeDate::parse(parsed.date_token().unwrap_or_default())
        .map_err(|_| StorageError::state(UNSUPPORTED))?;
    let json = |name: &str| {
        raw_field(&parsed, name)
            .and_then(|v| serde_json::from_slice(v).ok())
            .unwrap_or(serde_json::Value::Null)
    };
    let (kind, record, emitted) = match raw_field(&parsed, "gossil_kind") {
        Some(b"task") => {
            let task = task_from_values(TaskValueInputs {
                gest_id: json("gest_id"),
                project_id: json("project_id"),
                title: json("title"),
                priority: json("priority"),
                status: json("status"),
                description: json("description"),
                assigned_to: json("assigned_to"),
                metadata: json("metadata"),
                resolved_at: json("resolved_at"),
                created_at: json("created_at"),
                updated_at: json("updated_at"),
            })
            .map_err(|_| StorageError::state(UNSUPPORTED))?;
            let emitted = crate::task_ticket::task_creation_bytes(
                &task,
                &ticket_id,
                &actor,
                date,
                bytes.len(),
            );
            (&TASK, IssueRecord::Task(task), emitted)
        }
        Some(b"project") => {
            let project = project_from_values(ProjectValueInputs {
                gest_id: json("gest_id"),
                archived_at: json("archived_at"),
                created_at: json("created_at"),
                updated_at: json("updated_at"),
            })
            .map_err(|_| StorageError::state(UNSUPPORTED))?;
            let emitted = crate::gest_ticket::project_creation_bytes(
                &project,
                &ticket_id,
                &actor,
                date,
                bytes.len(),
            );
            (&PROJECT, IssueRecord::Project(project), emitted)
        }
        _ => return Err(StorageError::state(UNSUPPORTED)),
    };
    let user = String::from_utf8(actor).map_err(|_| StorageError::state(UNSUPPORTED))?;
    if emitted.ok().as_deref() != Some(bytes) || user.contains('\0') {
        return Err(StorageError::state(UNSUPPORTED));
    }
    let fields = parsed
        .ticket_fields()
        .iter()
        .map(|field| {
            (
                String::from_utf8_lossy(field.name()).into_owned(),
                String::from_utf8_lossy(&field.value().bytes).into_owned(),
            )
        })
        .collect();
    Ok(Creation {
        kind,
        ticket_id: String::from_utf8_lossy(&ticket_id).into_owned(),
        user,
        mtime: super::native_stored_date(parsed.date().unwrap_or_default()),
        fields,
        record,
    })
}

/// Find the decoded value of one J card of a parsed artifact.
/// # Arguments
/// * `parsed` - The parsed ticket.
/// * `name` - The J field name.
/// # Returns
/// The decoded bytes, or `None` when the artifact has no such card.
fn raw_field<'a>(parsed: &'a ParsedArtifact, name: &str) -> Option<&'a [u8]> {
    parsed
        .ticket_fields()
        .iter()
        .find(|field| field.name() == name.as_bytes())
        .map(|field| field.value().bytes.as_slice())
}

/// The steps of one write, after each of which a unit test can inject a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IssuePhase {
    /// The local receive record exists.
    Receipt,
    /// The private blob exists and names that receive record.
    Content,
    /// The `tkt-` tag and its `tagxref` row exist.
    Tag,
    /// The `ticket` and `ticketchng` rows exist.
    Ticket,
    /// The native event exists.
    Event,
    /// The typed row exists.
    Typed,
    /// The new admission proof is captured, and only COMMIT is left.
    Admission,
}

/// Mark the end of one write step. A unit test may make the step fail.
/// # Arguments
/// * `phase` - The step that just ended.
/// # Returns
/// Success, unless a unit test selected this step for a failure.
/// # Errors
/// Only a unit test can inject an error, which the transaction then rolls back.
fn issue_boundary(phase: IssuePhase) -> Result<(), StorageError> {
    #[cfg(test)]
    if ISSUE_FAILURE.with(|selected| selected.get() == Some(phase)) {
        return Err(StorageError::state("injected issue ticket failure"));
    }
    let _ = phase;
    Ok(())
}
#[cfg(test)]
thread_local! {
    /// The step after which the current unit test makes the write fail.
    static ISSUE_FAILURE: std::cell::Cell<Option<IssuePhase>> = const { std::cell::Cell::new(None) };
}

impl LocalOwnerRepository {
    /// Store one private task or project creation artifact in one transaction.
    ///
    /// The call writes the native rows of a private Fossil ticket and one typed row,
    /// then commits. [`Self::open`] refuses a repository that holds an issue until
    /// its admission accepts issue rows, so keep this owner to read the ticket back
    /// with [`Self::read_private_issue`].
    /// # Arguments
    /// * `request` - The creation artifact and the requested scope.
    /// * `limits` - Finite content, parser and SQL ceilings for the whole call.
    /// # Returns
    /// The stored ticket ID, native name and record.
    /// # Errors
    /// Refuses a shared scope, stale admission, a repository at another catalog and an
    /// artifact that is not one canonical creation. Also refuses a `[` in a J value, a
    /// ticket ID, tag, artifact or Gest ID that a row already holds, exhausted ceilings
    /// and SQL failures. The refusal writes nothing.
    /// # Examples
    /// ```no_run
    /// use gossil::storage::{IssueLimits, IssueTicket, LocalOwnerRepository, PathScope,
    ///     PrivateIssueRequest, StorageError};
    /// async fn store(owner: &mut LocalOwnerRepository, artifact: &[u8], limits: IssueLimits)
    ///     -> Result<IssueTicket, StorageError> {
    ///     owner.create_private_issue(PrivateIssueRequest { artifact, scope: PathScope::Private },
    ///         limits).await
    /// }
    /// ```
    pub async fn create_private_issue(
        &mut self,
        request: PrivateIssueRequest<'_>,
        limits: IssueLimits,
    ) -> Result<IssueTicket, StorageError> {
        if request.scope != PathScope::Private {
            return Err(StorageError::state(
                "issue tickets are private only in this build",
            ));
        }
        let mut budget = Budget::new(limits.admission.content)?;
        let mut parse = ParseWork::new(limits.admission.parser)?;
        let mut sql = SqlWork {
            rows: limits.sql_rows,
            bytes: limits.sql_bytes,
        };
        let bytes = request.artifact;
        let mut tx = self.store.begin_write().await?;
        let mut admitted = false;
        let result = async {
            tx.check_admission_budgeted(&self.principal, &self.admission, &mut budget).await?;
            admitted = true;
            let creation = derive_creation(&mut tx, bytes, &mut parse, &mut budget).await?;
            let name = native_name(bytes, &mut budget)?;
            let tag = format!("tkt-{}", creation.ticket_id);
            let gest_id = creation.typed_values().remove(0);
            let k = || Value::Text(creation.ticket_id.clone());
            let held = tx.query_bound_work(creation.kind.exists, vec![k(), Value::Text(tag.clone()), Value::Text(name.clone()), gest_id], 1, &mut sql, &mut budget).await?;
            if held != vec![vec![Value::Integer(0)]] {
                return Err(StorageError::state(UNSUPPORTED));
            }
            tx.execute_bound("INSERT INTO rcvfrom(uid,mtime,nonce,ipaddr) VALUES(0,julianday('now'),NULL,NULL)", vec![]).await?;
            issue_boundary(IssuePhase::Receipt)?;
            let rid = tx.put_private_file_content_budget_with_sql(&self.principal, &name, bytes, &mut budget, &mut sql).await?;
            tx.execute_bound("UPDATE blob SET rcvid=(SELECT max(rcvid) FROM rcvfrom) WHERE rid=?1", vec![Value::Integer(rid)]).await?;
            issue_boundary(IssuePhase::Content)?;
            tx.execute_bound("INSERT INTO tag(tagname) VALUES(?1)", vec![Value::Text(tag.clone())]).await?;
            tx.execute_bound("INSERT INTO tagxref(tagid,tagtype,srcid,origid,value,mtime,rid) SELECT tagid,1,?1,?1,NULL,?2,?1 FROM tag WHERE tagname=?3", vec![Value::Integer(rid), creation.mtime.clone(), Value::Text(tag.clone())]).await?;
            issue_boundary(IssuePhase::Tag)?;
            let values = creation.fields.iter().map(|(_, value)| Value::Text(value.clone()));
            tx.execute_bound(creation.kind.ticket, [k(), creation.mtime.clone()].into_iter().chain(values.clone()).collect()).await?;
            tx.execute_bound(creation.kind.change, [k(), Value::Integer(rid), creation.mtime.clone(), Value::Text(creation.user.clone())].into_iter().chain(values).collect()).await?;
            issue_boundary(IssuePhase::Ticket)?;
            let (comment, brief) = creation.event_text();
            tx.execute_bound("REPLACE INTO event(type,tagid,mtime,objid,user,comment,brief) SELECT 't',tagid,?1,?2,?3,?4,?5 FROM tag WHERE tagname=?6", vec![creation.mtime.clone(), Value::Integer(rid), Value::Text(creation.user.clone()), Value::Text(comment), Value::Text(brief), Value::Text(tag)]).await?;
            issue_boundary(IssuePhase::Event)?;
            tx.execute_bound(creation.kind.typed, creation.typed_values()).await?;
            issue_boundary(IssuePhase::Typed)?;
            let proof = tx.capture_admission_budgeted(&mut budget).await?;
            issue_boundary(IssuePhase::Admission)?;
            Ok((IssueTicket { ticket_id: creation.ticket_id, native_name: name, record: creation.record }, proof))
        }
        .await;
        let result = tx.record(result);
        let termination = tx.commit().await;
        match termination.and(result) {
            Ok((ticket, proof)) => {
                self.admission = proof;
                Ok(ticket)
            }
            Err(error) => {
                // A rolled-back insert still counts in the connection's change counter. The
                // admitted state is unchanged after the rollback, so keep the owner usable.
                if admitted && let Some(connection) = &self.store.connection {
                    self.admission.total_changes = connection.total_changes();
                }
                Err(error)
            }
        }
    }

    /// Derive one private issue ticket again from its artifact and check every stored row.
    ///
    /// The ticket ID must have exactly one artifact and no attachment. The call reads
    /// that artifact, parses it as a creation, and compares the `ticket`, `ticketchng`,
    /// `event`, `tagxref`, `private`, `unclustered`, `unsent`, `rcvfrom`, `backlink`
    /// and typed rows with the rows that the writer derives from it. It changes nothing.
    /// # Arguments
    /// * `ticket_id` - The K card of the ticket.
    /// * `limits` - Finite content, parser and SQL ceilings for the whole call.
    /// # Returns
    /// The ticket ID, native name and record that the artifact carries.
    /// # Errors
    /// Refuses an unknown ticket ID, every other ticket shape as unsupported in this
    /// build, a stored row that differs from the artifact, stale admission, exhausted
    /// ceilings and SQL failures.
    /// # Examples
    /// ```no_run
    /// use gossil::storage::{IssueLimits, IssueTicket, LocalOwnerRepository, StorageError};
    /// async fn read(owner: &mut LocalOwnerRepository, k: &str, limits: IssueLimits)
    ///     -> Result<IssueTicket, StorageError> {
    ///     owner.read_private_issue(k, limits).await
    /// }
    /// ```
    pub async fn read_private_issue(
        &mut self,
        ticket_id: &str,
        limits: IssueLimits,
    ) -> Result<IssueTicket, StorageError> {
        let mut budget = Budget::new(limits.admission.content)?;
        let mut parse = ParseWork::new(limits.admission.parser)?;
        let mut sql = SqlWork {
            rows: limits.sql_rows,
            bytes: limits.sql_bytes,
        };
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission_budgeted(&self.principal, &self.admission, &mut budget).await?;
            let k = || Value::Text(ticket_id.into());
            let tag = Value::Text(format!("tkt-{ticket_id}"));
            let shape = tx.query_bound_work("SELECT x.tagid,x.rid,b.uuid,(SELECT count(*) FROM attachment WHERE target=?2) FROM tag t JOIN tagxref x ON x.tagid=t.tagid JOIN blob b ON b.rid=x.rid WHERE t.tagname=?1", vec![tag, k()], 4, &mut sql, &mut budget).await?;
            let (tagid, rid, name) = match shape.as_slice() {
                [] => return Err(StorageError::state("issue ticket not found")),
                [row] if row[3] == Value::Integer(0) => (row[0].clone(), row[1].clone(), row[2].as_text().cloned().unwrap_or_default()),
                _ => return Err(StorageError::state(UNSUPPORTED)),
            };
            let bytes = tx.read_owner_content_budget_with_sql(&self.principal, &name, &mut budget, &mut sql).await?;
            let creation = derive_creation(&mut tx, &bytes, &mut parse, &mut budget).await?;
            let user = Value::Text(creation.user.clone());
            let ticket_columns = column_names(&mut tx, "ticket", &mut sql, &mut budget).await?;
            let ticket = tx.query_bound_work("SELECT * FROM ticket WHERE tkt_uuid=?1", vec![k()], ticket_columns.len(), &mut sql, &mut budget).await?;
            let tkt_id = ticket.first().and_then(|row| row.first()).cloned().unwrap_or(Value::Null);
            let expected_ticket = expected_row(&ticket_columns, &creation, |column| match column {
                "tkt_id" => Some(tkt_id.clone()),
                "tkt_uuid" => Some(k()),
                "tkt_mtime" | "tkt_ctime" => Some(creation.mtime.clone()),
                _ => None,
            });
            let change_columns = column_names(&mut tx, "ticketchng", &mut sql, &mut budget).await?;
            let change = tx.query_bound_work("SELECT * FROM ticketchng WHERE tkt_id=?1 OR tkt_rid=?2", vec![tkt_id.clone(), rid.clone()], change_columns.len(), &mut sql, &mut budget).await?;
            let expected_change = expected_row(&change_columns, &creation, |column| match column {
                "tkt_id" => Some(tkt_id.clone()),
                "tkt_rid" => Some(rid.clone()),
                "tkt_mtime" => Some(creation.mtime.clone()),
                "tkt_user" => Some(user.clone()),
                _ => None,
            });
            let (comment, brief) = creation.event_text();
            let event = tx.query_bound_work("SELECT type,tagid,mtime,objid,user,comment,brief,omtime,uid,bgcolor,euser,ecomment FROM event WHERE objid=?1 OR tagid=?2", vec![rid.clone(), tagid.clone()], 12, &mut sql, &mut budget).await?;
            let expected_event = vec![Value::Text("t".into()), tagid.clone(), creation.mtime.clone(), rid.clone(), user, Value::Text(comment), Value::Text(brief), Value::Null, Value::Null, Value::Null, Value::Null, Value::Null];
            let tags = tx.query_bound_work("SELECT tagid,tagtype,srcid,origid,value,mtime,rid FROM tagxref WHERE rid=?1", vec![rid.clone()], 7, &mut sql, &mut budget).await?;
            let expected_tags = vec![tagid, Value::Integer(1), rid.clone(), rid.clone(), Value::Null, creation.mtime.clone(), rid.clone()];
            let local = tx.query_bound_work("SELECT (SELECT count(*) FROM private WHERE rid=?1),(SELECT count(*) FROM unclustered WHERE rid=?1),(SELECT count(*) FROM unsent WHERE rid=?1),(SELECT count(*) FROM blob b JOIN rcvfrom r ON r.rcvid=b.rcvid WHERE b.rid=?1 AND r.uid=0 AND r.nonce IS NULL AND r.ipaddr IS NULL),(SELECT count(*) FROM backlink WHERE srcid=?1)", vec![rid], 5, &mut sql, &mut budget).await?;
            let expected_local = [1, 0, 0, 1, 0].map(Value::Integer).to_vec();
            let gest_id = creation.typed_values().remove(0);
            let typed = tx.query_bound_work(creation.kind.typed_read, vec![gest_id], creation.kind.typed_fields.len() + 1, &mut sql, &mut budget).await?;
            let expected_typed = [Value::Text(SCOPE.into())].into_iter().chain(creation.typed_values()).collect();
            let checks = [
                (vec![vec![Value::Text(creation.ticket_id.clone())]], vec![vec![k()]]),
                (ticket, vec![expected_ticket]),
                (change, vec![expected_change]),
                (event, vec![expected_event]),
                (tags, vec![expected_tags]),
                (local, vec![expected_local]),
                (typed, vec![expected_typed]),
            ];
            if checks.iter().any(|(stored, expected)| stored != expected) {
                return Err(StorageError::state("stored issue rows differ from the ticket artifact"));
            }
            Ok(IssueTicket { ticket_id: creation.ticket_id, native_name: name, record: creation.record })
        }
        .await;
        let result = tx.record(result);
        tx.rollback().await?;
        result
    }
}

/// Read the column names of one native table, in column order.
/// # Arguments
/// * `tx` - The reserved transaction.
/// * `table` - The fixed name of the table.
/// * `sql` - The call's SQL ledger.
/// * `budget` - The call's content ledger.
/// # Returns
/// Each column name.
/// # Errors
/// Refuses exhausted ceilings and SQL failures.
async fn column_names(
    tx: &mut WriteTransaction<'_>,
    table: &str,
    sql: &mut SqlWork,
    budget: &mut Budget,
) -> Result<Vec<String>, StorageError> {
    let rows = tx
        .query_bound_work(
            "SELECT name FROM pragma_table_info(?1) ORDER BY cid",
            vec![Value::Text(table.into())],
            1,
            sql,
            budget,
        )
        .await?;
    Ok(rows
        .iter()
        .map(|row| row[0].as_text().cloned().unwrap_or_default())
        .collect())
}

/// Build the row that one creation gives a native ticket table.
/// # Arguments
/// * `columns` - The column names, in column order.
/// * `creation` - The derived creation.
/// * `special` - The value of a bookkeeping column, or `None` for a field column.
/// # Returns
/// The special value, else the decoded J value as TEXT, else NULL.
fn expected_row(
    columns: &[String],
    creation: &Creation,
    special: impl Fn(&str) -> Option<Value>,
) -> Vec<Value> {
    columns
        .iter()
        .map(|column| {
            special(column).unwrap_or_else(|| {
                creation
                    .field(column)
                    .map_or(Value::Null, |value| Value::Text(value.into()))
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/support/storage_issue_ticket.rs"]
mod tests;
