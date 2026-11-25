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
//! The admission of [`LocalOwnerRepository::open`] accepts these rows, and it derives
//! every creation again. The writer runs that full admission before and after its
//! write, and the reader runs it before its read.
//!
//! [`LocalOwnerRepository::init_private_project`], which `gos i init` calls, also
//! binds the new project to the registered checkout, with one `g_project_binding`
//! row and one `g_checkout_project` row in the same transaction.
//! [`LocalOwnerRepository::bound_project`] reads that binding. Admission accepts
//! both tables empty, or with exactly that one binding to an admitted project
//! that is not archived.
//!
//! ## Recommendations
//! Keep the ticket shapes that this module writes and the rows that admission expects
//! in this one module, so the writer and the validator cannot drift apart.
use super::recorded_history::{bounded_rows, native_name, parse_recording_budgeted};
use super::{AdmissionLimits, LocalOwnerRepository, ParseWork, PathScope, Principal, StorageError};
use crate::artifact::{ArtifactType, ParsedArtifact};
use crate::gest_project_values::{ProjectValueInputs, ProjectValues, project_from_values};
use crate::gest_task_values::{TaskValueInputs, TaskValues, task_from_values};
use crate::native_date::NativeDate;
use crate::storage::{WriteTransaction, content::Budget, metered_query::SqlWork};
use libsql::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The `scope_key` of every typed row of this build, which names the owner's combined scope.
const SCOPE: &str = "combined";

/// The refusal message for stored issue rows that differ from the replay of the artifacts.
const DIFFERS: &str = "stored issue rows differ from the ticket artifact";

/// Count the rows that hold the ticket ID, its tag, its artifact or its Gest ID.
///
/// The count is zero only when no row holds any of them. A Gest ID names one task or
/// one project, never both.
const HELD: &str = "SELECT (SELECT count(*) FROM ticket WHERE tkt_uuid=?1)+(SELECT count(*) FROM tag WHERE tagname=?2)+(SELECT count(*) FROM blob WHERE uuid=?3)+(SELECT count(*) FROM g_gest_tasks WHERE id=?4)+(SELECT count(*) FROM g_gest_projects WHERE id=?4)";

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

/// Ask for the project of `issue init`, which is one private project creation bound to a checkout.
///
/// No field grants owner authority. The checkout must be the registered checkout of
/// the repository, with no project bound to it yet.
pub struct ProjectInitRequest<'a> {
    /// The complete project creation artifact, as the project emitter writes it.
    pub artifact: &'a [u8],
    /// The ID of the registered checkout that the project is bound to.
    pub checkout_id: &'a str,
}

/// Check the binding of a project to a checkout. The result is 1 when the two tables
/// are empty. It is also 1 when each table holds one row, and both rows name the
/// registered checkout and the Gest ID of one admitted project that is not archived.
/// The `g_project_binding` row must then hold the exact root of the checkout.
/// Every other state gives 0. The checkout IDs need no type check here. The
/// foreign key check of admission runs first and refuses an ID that is not the
/// text ID of the registered checkout.
const BINDING: &str = "SELECT CASE (SELECT count(*) FROM g_project_binding)+(SELECT count(*) FROM g_checkout_project) WHEN 0 THEN 1 WHEN 2 THEN (SELECT count(*) FROM g_project_binding b JOIN g_checkout_project p ON CAST(p.checkout_id AS BLOB)=CAST(b.checkout_id AS BLOB) AND CAST(p.project_id AS BLOB)=CAST(b.project_id AS BLOB) JOIN g_checkout c ON CAST(c.checkout_id AS BLOB)=CAST(b.checkout_id AS BLOB) AND CAST(c.root AS BLOB)=CAST(b.root AS BLOB) JOIN g_gest_projects g ON g.scope_key='combined' AND CAST(g.id AS BLOB)=CAST(b.project_id AS BLOB) AND g.archived_at IS NULL WHERE typeof(b.project_id)='text' AND typeof(b.root)='text' AND typeof(p.project_id)='text') ELSE 0 END";

/// The refusal of an `issue init` write whose project, checkout or repository does not fit.
///
/// Gest binds the one live project of a repository when one exists. This build
/// creates a project only in a repository that holds none, and does not bind an
/// existing project.
const INIT_REFUSED: &str = "issue init needs one project creation, a registered checkout with no bound project, and a repository with no project; binding an existing project is not available in this build";

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
    /// Insert the `ticket` row from the ticket ID, the date and every J value in card order.
    ticket: &'static str,
    /// Insert the `ticketchng` row from the ticket ID, RID, date, actor and every J value.
    change: &'static str,
    /// Insert the typed row from the values of [`KindSql::typed_fields`].
    typed: &'static str,
    /// Read every typed row of the kind, in typed column order and key order.
    typed_all: &'static str,
    /// The J field of each typed column after `scope_key`, in table column order.
    typed_fields: &'static [&'static str],
}

/// The fixed SQL of the task kind.
const TASK: KindSql = KindSql {
    ticket: "INSERT INTO ticket(tkt_uuid,tkt_mtime,tkt_ctime,assigned_to,created_at,description,gest_id,gossil_deleted,gossil_kind,gossil_schema,metadata,priority,project_id,resolved_at,status,title,updated_at) VALUES(?1,?2,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
    change: "INSERT INTO ticketchng(tkt_id,tkt_rid,tkt_mtime,tkt_user,assigned_to,created_at,description,gest_id,gossil_deleted,gossil_kind,gossil_schema,metadata,priority,project_id,resolved_at,status,title,updated_at) SELECT tkt_id,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18 FROM ticket WHERE tkt_uuid=?1",
    typed: "INSERT INTO g_gest_tasks(scope_key,id,project_id,title,priority,status,description,assigned_to,metadata,resolved_at,created_at,updated_at) VALUES('combined',?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
    typed_all: "SELECT scope_key,id,project_id,title,priority,status,description,assigned_to,metadata,resolved_at,created_at,updated_at FROM g_gest_tasks ORDER BY scope_key,id",
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
    ticket: "INSERT INTO ticket(tkt_uuid,tkt_mtime,tkt_ctime,archived_at,created_at,gest_id,gossil_deleted,gossil_kind,gossil_schema,updated_at) VALUES(?1,?2,?2,?3,?4,?5,?6,?7,?8,?9)",
    change: "INSERT INTO ticketchng(tkt_id,tkt_rid,tkt_mtime,tkt_user,archived_at,created_at,gest_id,gossil_deleted,gossil_kind,gossil_schema,updated_at) SELECT tkt_id,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11 FROM ticket WHERE tkt_uuid=?1",
    typed: "INSERT INTO g_gest_projects(scope_key,id,created_at,updated_at,archived_at) VALUES('combined',?1,?2,?3,?4)",
    typed_all: "SELECT scope_key,id,created_at,updated_at,archived_at FROM g_gest_projects ORDER BY scope_key,id",
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
pub(crate) enum IssuePhase {
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
    /// The project binding and the checkout project rows exist.
    Binding,
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
/// Make the issue writes of this thread fail after one step, or clear that choice.
///
/// The unit tests of the `issue` commands use it for GEST-T08.
/// # Arguments
/// * `phase` - The step after which each write of this thread fails, or `None`.
#[cfg(test)]
pub(crate) fn inject_issue_failure(phase: Option<IssuePhase>) {
    ISSUE_FAILURE.with(|selected| selected.set(phase));
}
#[cfg(test)]
thread_local! {
    /// The step after which the current unit test makes the write fail.
    static ISSUE_FAILURE: std::cell::Cell<Option<IssuePhase>> = const { std::cell::Cell::new(None) };
}

/// One admitted issue creation, with the native rows that its artifact gives.
///
/// The recording validators merge these rows into their complete table comparisons.
pub(super) struct AdmittedIssue {
    /// The RID of the creation artifact.
    pub(super) rid: i64,
    /// The receive record that the artifact owns.
    pub(super) receipt: i64,
    /// The full native name of the artifact.
    pub(super) hash: String,
    /// The ID of the `tkt-K` tag.
    pub(super) tagid: i64,
    /// The name of the `tkt-K` tag.
    pub(super) tag: String,
    /// The `event` row, in the column order type, mtime, objid, tagid, uid, bgcolor,
    /// euser, user, ecomment, comment, brief and omtime.
    pub(super) event: Vec<Value>,
    /// The `tagxref` row, in the column order tagid, tagtype, srcid, origid, value,
    /// mtime and rid.
    pub(super) tagxref: Vec<Value>,
}

/// Give the ID of each tag above the 12 built-in tags, in the order of the receipts that created them.
///
/// The first check-in creates the private branch tag, and each issue creates its
/// `tkt-K` tag. Each takes the next free ID, so the IDs follow the receipt order.
/// # Arguments
/// * `first_commit` - The receipt of the first private check-in, if any.
/// * `branch` - The name of the private branch tag.
/// * `issues` - Every admitted issue.
/// # Returns
/// The ID of the branch tag, or 0 without a check-in, and each expected
/// `(tagid, tagname)` row in ID order.
pub(super) fn assign_tags(
    first_commit: Option<i64>,
    branch: &str,
    issues: &[AdmittedIssue],
) -> (i64, Vec<Vec<Value>>) {
    let mut created: Vec<(i64, &str)> = issues
        .iter()
        .map(|issue| (issue.receipt, issue.tag.as_str()))
        .chain(first_commit.map(|receipt| (receipt, branch)))
        .collect();
    created.sort_unstable();
    let mut branch_id = 0;
    let rows = created
        .into_iter()
        .zip(13..)
        .map(|((_, name), id)| {
            if name == branch {
                branch_id = id;
            }
            vec![Value::Integer(id), Value::Text(name.into())]
        })
        .collect();
    (branch_id, rows)
}

impl WriteTransaction<'_> {
    /// Admit every private issue creation that the repository holds, with each row it gives.
    ///
    /// Each `tkt-` tag names one artifact, which must be one canonical task or project
    /// creation. The `ticket`, `ticketchng`, `g_gest_tasks` and `g_gest_projects` tables
    /// must equal what replay of those artifacts gives, row for row. No ticket ID or
    /// Gest ID may appear twice, across both kinds. The caller compares the other rows
    /// of each issue.
    /// # Arguments
    /// * `principal` - The authenticated owner, who may read private content.
    /// * `limits` - The admission ceilings. The parser entry ceiling bounds the issues.
    /// * `work` - The operation's content ledger.
    /// * `parse` - The operation's parser ledger.
    /// # Returns
    /// Each admitted issue, in receipt order.
    /// # Errors
    /// Refuses an artifact that is not a canonical creation, a row that differs from
    /// replay, a repeated ticket or Gest ID, exhausted ceilings and SQL failures.
    pub(super) async fn admit_issues(
        &mut self,
        principal: &Principal,
        limits: AdmissionLimits,
        work: &mut Budget,
        parse: &mut ParseWork,
    ) -> Result<Vec<AdmittedIssue>, StorageError> {
        let (n, width) = (limits.parser.entries, limits.parser.input_bytes);
        let found = bounded_rows(self, "SELECT x.rid,b.uuid,b.rcvid,t.tagid,t.tagname FROM tag t JOIN tagxref x ON x.tagid=t.tagid JOIN blob b ON b.rid=x.rid WHERE t.tagname GLOB 'tkt-*' ORDER BY b.rcvid,x.rid", vec![], n, width, work).await?;
        let mut issues = Vec::new();
        let mut creations = Vec::new();
        // The unique tag names and ticket IDs keep each K once, and the stored ticket
        // rows must match. The typed tables cannot show a Gest ID that two artifacts share.
        let mut gest_ids = BTreeSet::new();
        for row in &found {
            let [
                Value::Integer(rid),
                Value::Text(hash),
                Value::Integer(receipt),
                Value::Integer(tagid),
                Value::Text(tag),
            ] = row.as_slice()
            else {
                return Err(StorageError::state(DIFFERS));
            };
            let (_, _, bytes) = self.recorded_content(principal, hash, work).await?;
            let creation = derive_creation(self, &bytes, parse, work).await?;
            let gest_id = creation.field("gest_id").unwrap_or_default();
            if *tag != format!("tkt-{}", creation.ticket_id) || !gest_ids.insert(gest_id.to_owned())
            {
                return Err(StorageError::state(DIFFERS));
            }
            let (comment, brief) = creation.event_text();
            let (rid_value, mtime, null) =
                (Value::Integer(*rid), creation.mtime.clone(), Value::Null);
            issues.push(AdmittedIssue {
                rid: *rid,
                receipt: *receipt,
                hash: hash.clone(),
                tagid: *tagid,
                tag: tag.clone(),
                event: vec![
                    Value::Text("t".into()),
                    mtime.clone(),
                    rid_value.clone(),
                    Value::Integer(*tagid),
                    null.clone(),
                    null.clone(),
                    null.clone(),
                    Value::Text(creation.user.clone()),
                    null.clone(),
                    Value::Text(comment),
                    Value::Text(brief),
                    null.clone(),
                ],
                tagxref: vec![
                    Value::Integer(*tagid),
                    Value::Integer(1),
                    rid_value.clone(),
                    rid_value.clone(),
                    null,
                    mtime,
                    rid_value,
                ],
            });
            creations.push(creation);
        }
        let ticket_columns = column_names(self, "ticket", work).await?;
        let change_columns = column_names(self, "ticketchng", work).await?;
        let (mut tickets, mut changes) = (Vec::new(), Vec::new());
        let (mut tasks, mut projects) = (BTreeMap::new(), BTreeMap::new());
        for ((issue, creation), tkt_id) in issues.iter().zip(&creations).zip(1..) {
            let tkt_id = Value::Integer(tkt_id);
            tickets.push(expected_row(
                &ticket_columns,
                creation,
                |column| match column {
                    "tkt_id" => Some(tkt_id.clone()),
                    "tkt_uuid" => Some(Value::Text(creation.ticket_id.clone())),
                    "tkt_mtime" | "tkt_ctime" => Some(creation.mtime.clone()),
                    _ => None,
                },
            ));
            changes.push(expected_row(
                &change_columns,
                creation,
                |column| match column {
                    "tkt_id" => Some(tkt_id.clone()),
                    "tkt_rid" => Some(Value::Integer(issue.rid)),
                    "tkt_mtime" => Some(creation.mtime.clone()),
                    "tkt_user" => Some(Value::Text(creation.user.clone())),
                    _ => None,
                },
            ));
            let typed: Vec<_> = [Value::Text(SCOPE.into())]
                .into_iter()
                .chain(creation.typed_values())
                .collect();
            let key = creation.field("gest_id").unwrap_or_default().to_owned();
            match &creation.record {
                IssueRecord::Task(_) => tasks.insert(key, typed),
                IssueRecord::Project(_) => projects.insert(key, typed),
            };
        }
        let row_bytes = width.saturating_mul(2).saturating_add(512);
        for (sql, expected) in [
            ("SELECT * FROM ticket ORDER BY tkt_id", tickets),
            ("SELECT * FROM ticketchng ORDER BY tkt_id,tkt_rid", changes),
            (TASK.typed_all, tasks.into_values().collect()),
            (PROJECT.typed_all, projects.into_values().collect()),
        ] {
            if bounded_rows(self, sql, vec![], n, row_bytes, work).await? != expected {
                return Err(StorageError::state(DIFFERS));
            }
        }
        Ok(issues)
    }

    /// Admit the binding of a project to the registered checkout, as `issue init` writes it.
    ///
    /// The `g_project_binding` and `g_checkout_project` tables are both empty, or each
    /// holds one row. Both rows then name the registered checkout and the Gest ID of an
    /// admitted project that is not archived, and the `g_project_binding` row holds
    /// the exact root of the checkout. The caller admits the issues first.
    /// # Arguments
    /// * `work` - The operation's content ledger.
    /// # Returns
    /// Success for an empty or exact binding.
    /// # Errors
    /// Refuses every other row in the two tables, exhausted ceilings and SQL failures.
    pub(super) async fn admit_project_binding(
        &mut self,
        work: &mut Budget,
    ) -> Result<(), StorageError> {
        if bounded_rows(self, BINDING, vec![], 1, 64, work).await? != vec![vec![Value::Integer(1)]]
        {
            return Err(StorageError::state("unsupported project binding state"));
        }
        Ok(())
    }
}

impl LocalOwnerRepository {
    /// Store one private task or project creation artifact in one transaction.
    ///
    /// The call runs the full admission of [`Self::open`], writes the native rows of a
    /// private Fossil ticket and one typed row, runs the full admission again, and
    /// then commits. A later [`Self::open`] admits the stored ticket.
    /// # Arguments
    /// * `request` - The creation artifact and the requested scope.
    /// * `limits` - Finite content, parser and SQL ceilings for the whole call.
    /// # Returns
    /// The stored ticket ID, native name and record.
    /// # Errors
    /// Refuses a shared scope, stale or failed admission and an artifact that is not one
    /// canonical creation. Also refuses a `[` in a J value, a ticket ID, tag, artifact
    /// or Gest ID that a row already holds, exhausted ceilings and SQL failures. The
    /// refusal writes nothing.
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
        self.write_private_issue(request.artifact, None, limits)
            .await
    }

    /// Store one private project creation and bind it to the registered checkout, in one transaction.
    ///
    /// `issue init` makes this write. The call writes the rows of
    /// [`Self::create_private_issue`], one `g_project_binding` row with the root of the
    /// checkout, and one `g_checkout_project` row. It runs the full admission of
    /// [`Self::open`] before and after the write, and then commits.
    /// # Arguments
    /// * `request` - The project creation artifact and the checkout ID.
    /// * `limits` - Finite content, parser and SQL ceilings for the whole call.
    /// # Returns
    /// The stored ticket ID, native name and project record.
    /// # Errors
    /// Refuses an artifact that is not one canonical project creation, a checkout that
    /// is not registered or already has a bound project, a repository that already
    /// holds a project, and each refusal of
    /// [`Self::create_private_issue`]. The refusal writes nothing.
    /// # Examples
    /// ```no_run
    /// use gossil::storage::{IssueLimits, IssueTicket, LocalOwnerRepository,
    ///     ProjectInitRequest, StorageError};
    /// async fn init(owner: &mut LocalOwnerRepository, artifact: &[u8], limits: IssueLimits)
    ///     -> Result<IssueTicket, StorageError> {
    ///     owner.init_private_project(ProjectInitRequest { artifact, checkout_id: "work" },
    ///         limits).await
    /// }
    /// ```
    pub async fn init_private_project(
        &mut self,
        request: ProjectInitRequest<'_>,
        limits: IssueLimits,
    ) -> Result<IssueTicket, StorageError> {
        self.write_private_issue(request.artifact, Some(request.checkout_id), limits)
            .await
    }

    /// Return the Gest ID of the project bound to a checkout, after the full admission of [`Self::open`].
    ///
    /// It changes nothing.
    /// # Arguments
    /// * `checkout_id` - The ID of the checkout.
    /// * `limits` - Finite content and parser ceilings for the whole call. The SQL
    ///   ceilings are not used.
    /// # Returns
    /// The Gest ID of the bound project, or `None` when the checkout has no binding.
    /// # Errors
    /// Refuses stale or failed admission, exhausted ceilings and SQL failures.
    /// # Examples
    /// ```no_run
    /// use gossil::storage::{IssueLimits, LocalOwnerRepository, StorageError};
    /// async fn project(owner: &mut LocalOwnerRepository, limits: IssueLimits)
    ///     -> Result<Option<String>, StorageError> {
    ///     owner.bound_project("work", limits).await
    /// }
    /// ```
    pub async fn bound_project(
        &mut self,
        checkout_id: &str,
        limits: IssueLimits,
    ) -> Result<Option<String>, StorageError> {
        let mut budget = Budget::new(limits.admission.content)?;
        let mut parse = ParseWork::new(limits.admission.parser)?;
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission_budgeted(&self.principal, &self.admission, &mut budget)
                .await?;
            tx.validate_recording_repository(
                &self.principal,
                limits.admission,
                &mut budget,
                &mut parse,
            )
            .await?;
            let rows = bounded_rows(
                &mut tx,
                "SELECT project_id FROM g_checkout_project WHERE checkout_id=?1",
                vec![Value::Text(checkout_id.into())],
                1,
                128,
                &mut budget,
            )
            .await?;
            Ok(rows.first().and_then(|row| row[0].as_text().cloned()))
        }
        .await;
        let result = tx.record(result);
        tx.rollback().await?;
        result
    }

    /// Store one creation artifact, and bind a project to a checkout when one is named.
    /// # Arguments
    /// * `bytes` - The complete creation artifact.
    /// * `checkout` - The checkout to bind the project to, for `issue init` only.
    /// * `limits` - Finite content, parser and SQL ceilings for the whole call.
    /// # Returns
    /// The stored ticket ID, native name and record.
    /// # Errors
    /// Each refusal of [`Self::create_private_issue`] and [`Self::init_private_project`].
    async fn write_private_issue(
        &mut self,
        bytes: &[u8],
        checkout: Option<&str>,
        limits: IssueLimits,
    ) -> Result<IssueTicket, StorageError> {
        let mut budget = Budget::new(limits.admission.content)?;
        let mut parse = ParseWork::new(limits.admission.parser)?;
        let mut sql = SqlWork {
            rows: limits.sql_rows,
            bytes: limits.sql_bytes,
        };
        let mut tx = self.store.begin_write().await?;
        let mut admitted = false;
        let result = async {
            tx.check_admission_budgeted(&self.principal, &self.admission, &mut budget).await?;
            admitted = true;
            tx.validate_recording_repository(&self.principal, limits.admission, &mut budget, &mut parse).await?;
            let creation = derive_creation(&mut tx, bytes, &mut parse, &mut budget).await?;
            if let Some(checkout) = checkout {
                let free = tx.query_bound_work("SELECT (SELECT count(*) FROM g_checkout WHERE checkout_id=?1),(SELECT count(*) FROM g_checkout_project),(SELECT count(*) FROM g_gest_projects)", vec![Value::Text(checkout.into())], 3, &mut sql, &mut budget).await?;
                if !matches!(creation.record, IssueRecord::Project(_)) || free != vec![vec![Value::Integer(1), Value::Integer(0), Value::Integer(0)]] {
                    return Err(StorageError::state(INIT_REFUSED));
                }
            }
            let name = native_name(bytes, &mut budget)?;
            let tag = format!("tkt-{}", creation.ticket_id);
            let gest_id = creation.typed_values().remove(0);
            let k = || Value::Text(creation.ticket_id.clone());
            let held = tx.query_bound_work(HELD, vec![k(), Value::Text(tag.clone()), Value::Text(name.clone()), gest_id], 1, &mut sql, &mut budget).await?;
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
            if let Some(checkout) = checkout {
                let project = vec![Value::Text(checkout.into()), creation.typed_values().remove(0)];
                tx.execute_bound("INSERT INTO g_project_binding(checkout_id,project_id,root) SELECT checkout_id,?2,root FROM g_checkout WHERE checkout_id=?1", project.clone()).await?;
                tx.execute_bound("INSERT INTO g_checkout_project(checkout_id,project_id) VALUES(?1,?2)", project).await?;
                issue_boundary(IssuePhase::Binding)?;
            }
            tx.validate_recording_repository(&self.principal, limits.admission, &mut budget, &mut parse).await?;
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

    /// Read one private issue ticket after the full admission of [`Self::open`].
    ///
    /// Admission derives every stored ticket again from its artifact and checks every
    /// row. The call then returns the ticket that the ticket ID names. It changes nothing.
    /// # Arguments
    /// * `ticket_id` - The K card of the ticket.
    /// * `limits` - Finite content and parser ceilings for the whole call. The SQL
    ///   ceilings are not used.
    /// # Returns
    /// The ticket ID, native name and record that the artifact carries.
    /// # Errors
    /// Refuses an unknown ticket ID, stale or failed admission, exhausted ceilings and
    /// SQL failures.
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
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission_budgeted(&self.principal, &self.admission, &mut budget).await?;
            tx.validate_recording_repository(&self.principal, limits.admission, &mut budget, &mut parse).await?;
            let found = bounded_rows(&mut tx, "SELECT b.uuid FROM tag t JOIN tagxref x ON x.tagid=t.tagid JOIN blob b ON b.rid=x.rid WHERE t.tagname=?1", vec![Value::Text(format!("tkt-{ticket_id}"))], 1, 128, &mut budget).await?;
            let Some(Value::Text(name)) = found.first().and_then(|row| row.first()) else {
                return Err(StorageError::state("issue ticket not found"));
            };
            let (_, _, bytes) = tx.recorded_content(&self.principal, name, &mut budget).await?;
            let creation = derive_creation(&mut tx, &bytes, &mut parse, &mut budget).await?;
            Ok(IssueTicket { ticket_id: creation.ticket_id, native_name: name.clone(), record: creation.record })
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
/// * `work` - The operation's content ledger.
/// # Returns
/// Each column name.
/// # Errors
/// Refuses exhausted ceilings and SQL failures.
async fn column_names(
    tx: &mut WriteTransaction<'_>,
    table: &str,
    work: &mut Budget,
) -> Result<Vec<String>, StorageError> {
    let rows = bounded_rows(
        tx,
        "SELECT name FROM pragma_table_info(?1) ORDER BY cid",
        vec![Value::Text(table.into())],
        128,
        128,
        work,
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
