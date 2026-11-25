//! Record the complete tracked working tree atomically with explicit private scope.
//!
//! ## Recommendations
//! Keep scope explicit and qualify this source with the accepted tracking base.
use super::path_scope::PathScope;
use super::recorded_history::{
    ProjectionInput, Reach, RecordedFile, bounded_rows, bounded_scalar, check_recorded_commit,
    native_name, parse_recording_budgeted, scalar_i64,
};
use super::recording_manifest::{
    ManifestFile, ManifestParent, PrivateManifestSpec, PrivateParents, build_private_manifest,
};
use super::tracking::{TrackingLimits, capture};
use super::{AdmissionLimits, CheckoutView, LocalOwnerRepository, ParseWork, StorageError};
use crate::storage::{WriteTransaction, content::Budget};
use libsql::Value;
use std::{collections::BTreeSet, path::Path};

/// Request an explicit recording of the complete tree. No field grants owner authority.
pub struct PrivateRecordingRequest<'a> {
    /// All seven expected checkout facts from the admitted owner.
    pub expected: &'a CheckoutView,
    /// Nonempty, duplicate-free exact set of every tracked path, including new additions.
    pub paths: &'a [&'a str],
    /// Explicit intent; [`PathScope::Shared`] is unsupported even when every payload is private.
    pub scope: PathScope,
    /// Nonempty decoded UTF-8 native actor without NUL.
    pub actor: &'a [u8],
    /// Decoded UTF-8 comment, excluding NUL and every literal left bracket.
    pub comment: &'a [u8],
    /// Native date token strictly later than every manifest parent.
    pub date: &'a [u8],
}

/// Report the durable recording result. `NoChange` creates no receipt or native mutation.
#[derive(Debug)]
pub enum PrivateRecordingOutcome {
    /// Checked unchanged bytes and modes, with the exact admitted view retained.
    NoChange {
        /// Exact admitted view, with unchanged heads and mutation counters.
        view: CheckoutView,
    },
    /// A newly committed full manifest and its refreshed owner view.
    Recorded {
        /// Refreshed admitted view after the changed transaction commits.
        view: CheckoutView,
        /// Full native identity of the newly committed private manifest.
        native_name: String,
    },
}

impl LocalOwnerRepository {
    /// Record all explicitly private tracked files in one checked transaction.
    /// # Arguments
    /// * `checkout` - Exact owned registration identity.
    /// * `request` - Explicit complete set, metadata, scope and all seven expectations.
    /// * `limits` - Whole-operation native content and parser ceilings.
    /// * `tracking` - Complete path, descriptor and retained working-byte ceilings.
    /// # Returns
    /// A durable recorded result, or [`PrivateRecordingOutcome::NoChange`] with unchanged
    /// logical repository state, heads and counters.
    /// # Errors
    /// Rejects a changed owner or checkout, unsupported scopes or partial sets, malformed metadata,
    /// corrupt history, descriptor drift, exhausted bounds and SQL failures atomically.
    /// # Examples
    /// ```no_run
    /// use gossil::storage::{AdmissionLimits, CheckoutView, LocalOwnerRepository,
    ///     PathScope, PrivateRecordingOutcome, PrivateRecordingRequest, StorageError, TrackingLimits};
    /// async fn record(owner: &mut LocalOwnerRepository, view: &CheckoutView,
    ///     admission: AdmissionLimits, tracking: TrackingLimits)
    ///     -> Result<PrivateRecordingOutcome, StorageError> {
    ///     owner.commit_private_tracked_files(&view.checkout_id,
    ///         PrivateRecordingRequest { expected: view, paths: &["notes.txt"],
    ///             scope: PathScope::Private, actor: b"Owner", comment: b"save notes",
    ///             date: b"2026-11-20T00:00:01" }, admission, tracking).await
    /// }
    /// ```
    pub async fn commit_private_tracked_files(
        &mut self,
        checkout: &str,
        request: PrivateRecordingRequest<'_>,
        limits: AdmissionLimits,
        tracking: TrackingLimits,
    ) -> Result<PrivateRecordingOutcome, StorageError> {
        let mut work = Budget::new(limits.content)?;
        let mut parse = ParseWork::new(limits.parser)?;
        #[cfg(test)]
        {
            // The debit enters the real ledgers before path checks, authority,
            // capture, parsing or any transaction. No counter is assigned/reset.
            let [stored, decoded, parser_input] = RECORDING_INITIAL_DEBITS.with(|slot| slot.get());
            work.charge_stored(stored)?;
            work.charge_decoded(decoded)?;
            parse.charge(crate::artifact::ParseUsage {
                input_bytes: parser_input,
                cards: 0,
                owned_bytes: 0,
                entries: 0,
            })?;
            RECORDING_INITIAL_DEBIT_SEEN
                .with(|slot| slot.set(Some([stored, decoded, parser_input])));
        }
        if request.scope != PathScope::Private
            || request.actor.is_empty()
            || request.actor.contains(&0)
            || request.comment.contains(&0)
            || request.comment.contains(&b'[')
            || std::str::from_utf8(request.actor).is_err()
            || std::str::from_utf8(request.comment).is_err()
            || request.paths.is_empty()
            || request.paths.len() > tracking.paths
        {
            return Err(StorageError::state("unsupported private recording request"));
        }
        let mut unique = BTreeSet::new();
        for path in request.paths {
            capture::checked_path(path, tracking)?;
            work.charge_decoded(path.len().saturating_add(64))?;
            if !unique.insert(*path) {
                return Err(StorageError::state("duplicate recording path"));
            }
            work.charge_decoded(path.len())?;
        }
        let mut tx = self.store.begin_write().await?;
        let result = async {
            #[cfg(test)]
            RECORDING_AUTH_SAW_DEBIT.with(|slot|slot.set(RECORDING_INITIAL_DEBIT_SEEN.with(|seen|seen.get().is_some())));
            tx.check_admission_budgeted(&self.principal, &mut work).await?;
            tx.require_native_checkout().await?;
            tx.check_recording_state(&self.principal, limits, &mut work).await?;
            let before = tx.read_checkout_view_budgeted(&self.principal, checkout, &mut work).await?;
            if &before != request.expected { return Err(StorageError::state("recording checkout changed")); }
            // Commit reads only the parent check-in and its own parent, so its cost does
            // not grow with the history. It checks every fact of the parent that it
            // builds on, so it never stores a check-in that the full proof refuses.
            tx.require_recording_empty_tables().await?;
            let walk = tx.walk_recorded_history(&self.principal, &before.shared_head, limits, &mut work, &mut parse, Reach::Parent).await?;
            if let Some((parent, bytes, artifact)) = walk.commits.last() {
                let previous = if walk.commits.len() == 2 { &walk.commits[0].0 } else { &walk.shared };
                check_recorded_commit(walk.commits.len() == 1, (parent, bytes, artifact), previous, &walk.shared, &walk.branch, &mut work, limits.parser.input_bytes)?;
            }
            let selected = walk.commits.last().map_or(&walk.shared, |(commit, _, _)| commit);
            tx.validate_recorded_tracking(checkout, selected, walk.generation, walk.pending, limits, &mut work).await?;
            let history = walk.history();
            let scopes = tx.read_path_scopes_budgeted(checkout, tracking, &mut work).await?;
            let tracked = bounded_rows(&mut tx,
                "SELECT CAST(pathname AS BLOB),isexe FROM g_vfile WHERE checkout_id=?1 ORDER BY pathname",
                vec![Value::Text(checkout.into())], tracking.paths, tracking.path_bytes.saturating_add(64), &mut work).await?;
            let mut paths = Vec::new();
            for row in tracked {
                let [Value::Blob(bytes), Value::Integer(_)] = row.as_slice() else { return Err(StorageError::state("recording tracked shape")); };
                let path = String::from_utf8(bytes.clone()).map_err(|e|StorageError::caused("recording path UTF-8",e))?;
                capture::checked_path(&path, tracking)?;
                paths.push(path);
            }
            if paths.len() != unique.len() || paths.iter().any(|path|!unique.contains(path.as_str())) {
                return Err(StorageError::state("recording requires exact all-tracked set"));
            }
            if scopes.len() != paths.len() || paths.iter().any(|path|!scopes.iter().any(|s|s.path == *path && s.scope == PathScope::Private)) {
                return Err(StorageError::state("recording requires explicit private scope for every tracked path"));
            }
            let bindings = bounded_rows(&mut tx,"SELECT root_device,root_inode FROM g_checkout_owner WHERE checkout_id=?1",vec![Value::Text(checkout.into())],1,128,&mut work).await?;
            let [row] = bindings.as_slice() else { return Err(StorageError::state("recording root binding unavailable")); };
            let [Value::Text(device),Value::Text(inode)] = row.as_slice() else { return Err(StorageError::state("recording root binding invalid")); };
            let number = |s:&str|s.parse::<u64>().map_err(|e|StorageError::caused("recording root identity",e));
            let root = capture::open_root_budget(&before.registered_root,number(device)?,number(inode)?,&mut work)?;
            let protected = tx.tracking_protected_paths()?;
            let protected:Vec<&Path> = protected.iter().map(|p|p.as_path()).collect();
            work.charge_decoded(paths.len().checked_mul(tracking.path_bytes.saturating_add(512)).ok_or_else(||StorageError::state("recording capture capacity overflow"))?)?;
            let mut captures = Vec::new();
            let mut files = Vec::new();
            let mut remaining = tracking.retained_bytes;
            for path in &paths {
                let captured = capture::capture_budget(&root,path,&protected,tracking,remaining,&mut work)?;
                remaining = remaining.checked_sub(captured.bytes.len() as u64).ok_or_else(||StorageError::state("recording retained bytes"))?;
                let hash = native_name(&captured.bytes,&mut work)?;
                files.push(RecordedFile { path:path.clone(), hash, rid:0, mode:captured.mode });
                captures.push(captured);
            }
            // Build and parse even a no-op so malformed comments/actors/dates never
            // bypass validation. The caller's operation ledgers remain borrowed.
            let references:Vec<_> = files.iter().map(|f|ManifestFile{path:&f.path,hash:&f.hash,mode:f.mode}).collect();
            let shared = ManifestParent{hash:&history.shared.hash,date:&history.shared.date};
            let parents = match history.commits.last() {
                None => PrivateParents::First{shared},
                Some(previous) => PrivateParents::Later{previous:ManifestParent{hash:&previous.hash,date:&previous.date},shared},
            };
            let bytes = build_private_manifest(&PrivateManifestSpec{parents,files:&references,actor:request.actor,comment:request.comment,date:request.date,branch:&history.branch},&mut work,limits.parser.input_bytes)?;
            work.charge_decoded(bytes.len())?;
            let artifact=parse_recording_budgeted(&mut tx,&bytes,&mut parse,&mut work).await?;
            let julian=artifact.date().ok_or_else(||StorageError::state("recording native date unavailable"))?;
            if !julian.is_finite() || julian<=history.shared.julian || history.commits.last().is_some_and(|parent|julian<=parent.julian) {return Err(StorageError::state("recording SQL date must follow every parent"));}
            let changed = files.len()!=history.current_files().len() || files.iter().zip(history.current_files()).any(|(a,b)|a.path!=b.path || a.hash!=b.hash || a.mode!=b.mode);
            if !changed {
                for ((path,captured),_) in paths.iter().zip(&captures).zip(&files) {
                    capture::recheck_budget(&root,path,captured,&protected,tracking,&mut work)?;
                }
                tx.tracking_protected_paths()?;
                tx.check_recording_state(&self.principal, limits, &mut work).await?;
                return Ok(PrivateRecordingOutcome::NoChange{view:before});
            }
            let generation = before.generation.checked_add(1).and_then(|n|i64::try_from(n).ok()).ok_or_else(||StorageError::state("recording generation exhausted"))?;
            let pending = before.pending_version.checked_add(1).and_then(|n|i64::try_from(n).ok()).ok_or_else(||StorageError::state("recording pending exhausted"))?;
            // One local-origin receipt; actor bytes do not establish a native uid.
            tx.recording_execute("INSERT INTO rcvfrom(uid,mtime,nonce,ipaddr) VALUES(0,julianday('now'),NULL,NULL)",vec![],&mut work).await?;
            recording_boundary(RecordingPhase::Receipt)?;
            let receipt = bounded_scalar(&mut tx,"SELECT max(rcvid) FROM rcvfrom",vec![],
                &mut work,
            ).await?;
            for (file,captured) in files.iter_mut().zip(&captures) {
                file.rid=tx.recording_put(&self.principal,&file.hash,&captured.bytes,receipt,limits,&mut work).await?;
            }
            recording_boundary(RecordingPhase::Payloads)?;
            let name=native_name(&bytes,&mut work)?;
            let rid=tx.recording_put(&self.principal,&name,&bytes,receipt,limits,&mut work).await?;
            let inserted=bounded_scalar(&mut tx,"SELECT rcvid FROM blob WHERE rid=?1",vec![Value::Integer(rid)],&mut work).await?;
            if inserted != receipt { return Err(StorageError::state("recording manifest must own operation receipt")); }
            recording_boundary(RecordingPhase::Manifest)?;
            tx.project_recording(&history,ProjectionInput{rid,bytes:&bytes,files:&files},limits,&mut work,&mut parse).await?;
            recording_boundary(RecordingPhase::Projections)?;
            for (file,captured) in files.iter().zip(&captures) {
                let count=tx.recording_execute("UPDATE g_vfile SET vid=?1,rid=?2,mrid=?2,isexe=?3,mtime=?4 WHERE checkout_id=?5 AND pathname=?6",vec![Value::Integer(rid),Value::Integer(file.rid),Value::Integer(file.mode),Value::Integer(captured.mtime),Value::Text(checkout.into()),Value::Text(file.path.clone())],&mut work).await?;
                if count!=1 { return Err(StorageError::state("recording tracked row changed")); }
            }
            recording_boundary(RecordingPhase::Inventory)?;
            let count=tx.recording_execute("UPDATE g_checkout SET combined_head=?1,generation=?2,pending_version=?3 WHERE checkout_id=?4 AND generation=?5 AND pending_version=?6 AND availability='ready'",vec![Value::Text(name.clone()),Value::Integer(generation),Value::Integer(pending),Value::Text(checkout.into()),Value::Integer(i64::try_from(before.generation).map_err(|_|StorageError::state("recording generation"))?),Value::Integer(i64::try_from(before.pending_version).map_err(|_|StorageError::state("recording pending"))?)],&mut work).await?;
            if count!=1 { return Err(StorageError::state("recording checkout changed")); }
            recording_boundary(RecordingPhase::Heads)?;
            let fingerprint=tx.recording_fingerprint(receipt,limits.parser.input_bytes,&mut work).await?;
            for (key,value) in [("checkout",Value::Integer(rid)),("checkout-hash",Value::Text(name.clone())),("fingerprint",Value::Text(fingerprint))] {
                if tx.recording_execute("UPDATE g_vvar SET value=?1 WHERE checkout_id=?2 AND name=?3",vec![value,Value::Text(checkout.into()),Value::Text(key.into())],&mut work).await?!=1 { return Err(StorageError::state("recording native vvar missing")); }
            }
            recording_boundary(RecordingPhase::Fingerprint)?;
            for (path,captured) in paths.iter().zip(&captures) { capture::recheck_budget(&root,path,captured,&protected,tracking,&mut work)?; }
            tx.tracking_protected_paths()?;
            tx.check_recording_state(&self.principal, limits, &mut work).await?;
            recording_boundary(RecordingPhase::Poststate)?;
            let view=tx.read_checkout_view_budgeted(&self.principal,checkout,&mut work).await?;
            Ok(PrivateRecordingOutcome::Recorded{view,native_name:name})
        }.await;
        let result = tx.record(result);
        tx.commit().await?;
        result
    }
}

impl WriteTransaction<'_> {
    /// Insert verified full content or reuse an unchanged admitted identity.
    /// # Arguments
    /// * `principal` - Privately authenticated owner.
    /// * `name` - Verified full SHA3 native identity.
    /// * `bytes` - Captured full payload or canonical manifest.
    /// * `receipt` - This changed operation's sole local-origin receive record.
    /// * `limits` - Finite native envelope ceilings.
    /// * `work` - Borrowed whole-operation ledger.
    /// # Returns
    /// Existing unchanged RID, or newly private full-content RID.
    /// # Errors
    /// Rejects shun, unavailable existing content, conflicts, bounds or SQL errors.
    async fn recording_put(
        &mut self,
        principal: &crate::storage::policy::Principal,
        name: &str,
        bytes: &[u8],
        receipt: i64,
        limits: AdmissionLimits,
        work: &mut Budget,
    ) -> Result<i64, StorageError> {
        if bytes.len() as u64 > u64::from(limits.content.max_output_bytes) {
            return Err(StorageError::state("recording content output limit"));
        }
        work.charge_decoded(bytes.len())?;
        self.verify_content_name(name, bytes)?;
        let existing = bounded_rows(
            self,
            "SELECT rid FROM blob WHERE uuid=?1",
            vec![Value::Text(name.into())],
            1,
            64,
            work,
        )
        .await?;
        if !existing.is_empty() {
            let rid = scalar_i64(&existing)?;
            let old = self
                .read_recording_content_budget(principal, name, work)
                .await?;
            require_same_content(&old, bytes)?;
            return Ok(rid);
        }
        if bounded_scalar(
            self,
            "SELECT count(*) FROM shun WHERE uuid=?1",
            vec![Value::Text(name.into())],
            work,
        )
        .await?
            != 0
        {
            return Err(StorageError::state("recording content shunned"));
        }
        work.charge_decoded(
            bytes
                .len()
                .checked_add(8192)
                .ok_or_else(|| StorageError::state("recording compression work overflow"))?,
        )?;
        let ceiling = work.packed_output_ceiling();
        let packed = crate::compression::pack_bounded(bytes, -1, ceiling)
            .map_err(|e| StorageError::caused("recording compression", e))?;
        work.charge_stored(packed.len())?;
        self.recording_execute(
            "INSERT INTO blob(rcvid,size,uuid,content) VALUES(?1,?2,?3,?4)",
            vec![
                Value::Integer(receipt),
                Value::Integer(bytes.len() as i64),
                Value::Text(name.into()),
                Value::Blob(packed),
            ],
            work,
        )
        .await?;
        let rid = bounded_scalar(
            self,
            "SELECT rid FROM blob WHERE uuid=?1",
            vec![Value::Text(name.into())],
            work,
        )
        .await?;
        self.recording_execute(
            "INSERT INTO private(rid) VALUES(?1)",
            vec![Value::Integer(rid)],
            work,
        )
        .await?;
        self.recording_execute(
            "INSERT INTO unsent(rid) VALUES(?1)",
            vec![Value::Integer(rid)],
            work,
        )
        .await?;
        Ok(rid)
    }
}

/// Preserve deduplication's byte-equality defense after both identities verify.
/// # Arguments
/// * `existing` - Complete verified existing content.
/// * `captured` - Complete verified candidate content.
/// # Returns
/// Success only for byte-for-byte equality, including empty and binary content.
/// # Errors
/// Rejects different lengths or bytes without changing identity or provenance.
fn require_same_content(existing: &[u8], captured: &[u8]) -> Result<(), StorageError> {
    if existing != captured {
        return Err(StorageError::state("conflicting recording content"));
    }
    Ok(())
}

/// Enumerate the private mutation checkpoints used by rollback instrumentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
enum RecordingPhase {
    /// Local receive record exists.
    Receipt = 1,
    /// Full payloads are available.
    Payloads = 2,
    /// Canonical manifest is stored.
    Manifest = 4,
    /// Native links and event projections exist.
    Projections = 8,
    /// Tracked inventory references the new tree.
    Inventory = 16,
    /// Checkout heads and counters are updated.
    Heads = 32,
    /// Exact current receipt checksum is stored.
    Fingerprint = 64,
    /// The checks of each command passed again on the new state.
    Poststate = 128,
}

/// Keep deterministic late-write failure injection private to source controls.
/// # Arguments
/// * `phase` - Exact private checkpoint after a whole native mutation phase.
/// # Returns
/// Success in production and when this phase is not selected by a unit control.
/// # Errors
/// Unit controls may inject an error that the enclosing transaction must roll back.
fn recording_boundary(phase: RecordingPhase) -> Result<(), StorageError> {
    #[cfg(test)]
    RECORDING_PHASES.with(|visited| visited.set(visited.get() | phase as u16));
    #[cfg(test)]
    if RECORDING_FAILURE.with(|selected| selected.get() == Some(phase)) {
        return Err(StorageError::state("injected recording failure"));
    }
    let _ = phase;
    Ok(())
}
#[cfg(test)]
thread_local! {
    /// A synchronous thread-local selector; no environment or public authority.
    static RECORDING_FAILURE:std::cell::Cell<Option<RecordingPhase>>=const{std::cell::Cell::new(None)};
    /// Phases actually reached by a source unit control; no public observation API.
    static RECORDING_PHASES:std::cell::Cell<u16>=const{std::cell::Cell::new(0)};
    /// Initial real-ledger debit in stored bytes, decoded bytes and parser input bytes.
    static RECORDING_INITIAL_DEBITS:std::cell::Cell<[usize;3]>=const{std::cell::Cell::new([0;3])};
    /// Exact debit witnessed before authority or any later phase; None means not reached.
    static RECORDING_INITIAL_DEBIT_SEEN:std::cell::Cell<Option<[usize;3]>>=const{std::cell::Cell::new(None)};
    /// Whether the authority entry point observed the already applied initial debit.
    static RECORDING_AUTH_SAW_DEBIT:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};
}

#[cfg(test)]
/// Check private recording with fixtures and failure injection.
mod rollback_controls {
    use super::*;
    use crate::storage::{InitializeOptions, Limits};

    /// Supply finite ceilings for full history and schema snapshot work.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Generous finite bounds.
    /// # Errors
    /// Cannot fail.
    fn bounds() -> AdmissionLimits {
        AdmissionLimits {
            content: Limits {
                max_stored_bytes: 1 << 20,
                max_output_bytes: 1 << 20,
                max_chain_depth: 128,
                max_total_stored_bytes: 64 << 20,
                max_total_decoded_bytes: 256 << 20,
            },
            parser: crate::artifact::ParseLimits {
                input_bytes: 4 << 20,
                cards: 65536,
                owned_bytes: 16 << 20,
                entries: 8192,
            },
        }
    }
    /// Supply complete finite descriptor bounds.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Finite tracking ceilings.
    /// # Errors
    /// Cannot fail.
    fn tracking() -> TrackingLimits {
        TrackingLimits {
            paths: 64,
            path_bytes: 4096,
            components: 32,
            retained_bytes: 1 << 20,
            io_buffer_bytes: 1024,
        }
    }
    /// Read every native and local table through the same retained connection.
    /// # Arguments
    /// * `owner` - Fixture owner with no outstanding transaction.
    /// # Returns
    /// Deterministically sorted complete table values.
    /// # Panics
    /// Panics if fixture provider queries fail.
    async fn snapshot(owner: &mut LocalOwnerRepository) -> Vec<(String, Vec<Vec<Value>>)> {
        let mut tx = owner.store.begin_write().await.unwrap();
        let mut names = tx
            .connection()
            .query(
                "SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name",
                (),
            )
            .await
            .unwrap();
        let mut tables = Vec::new();
        while let Some(row) = names.next().await.unwrap() {
            assert_eq!(row.column_count(), 1);
            tables.push(row.get::<String>(0).unwrap());
        }
        drop(names);
        let mut all = Vec::new();
        for name in tables {
            assert!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
            let mut values = tx.query(&format!("SELECT * FROM {name}")).await.unwrap();
            values.sort_by_key(|row| format!("{row:?}"));
            all.push((name, values));
        }
        tx.commit().await.unwrap();
        all
    }
    /// Read all fixture tables without ending the reserved transaction.
    /// # Arguments
    /// * `tx` - Reserved native transaction containing any intentionally staged inputs.
    /// # Returns
    /// Complete sorted native and local table values.
    /// # Panics
    /// Panics on invalid fixture identifiers or provider failure.
    async fn transaction_snapshot(tx: &mut WriteTransaction<'_>) -> Vec<(String, Vec<Vec<Value>>)> {
        let names = tx
            .query("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")
            .await
            .unwrap();
        let mut all = Vec::new();
        for row in names {
            assert_eq!(row.len(), 1);
            let name = row[0].as_text().expect("native table name");
            assert!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
            let mut values = tx.query(&format!("SELECT * FROM {name}")).await.unwrap();
            values.sort_by_key(|row| format!("{row:?}"));
            all.push((name.clone(), values));
        }
        all
    }

    #[tokio::test]
    /// Real staged content exercises projection copy bounds, branch identity and rollback.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after exact entry and copy refusals, a branch tag that takes the next free
    /// ID after tag 13, and a successful neighboring projection.
    /// # Panics
    /// Panics on an unexpected refusal, early write on budget failure or incomplete rollback.
    async fn staged_projection_bounds_and_tag_identity_roll_back_every_table() {
        for case in ["entry-limit", "copy-budget", "tag-identity", "positive"] {
            let temp = tempfile::tempdir().unwrap();
            let repo = temp.path().join("repo");
            let root = temp.path().join("root");
            std::fs::create_dir(&root).unwrap();
            let root = std::fs::canonicalize(root).unwrap();
            let b = bounds();
            let mut owner = LocalOwnerRepository::initialize(
                &repo,
                InitializeOptions {
                    repository_id: "0123456789abcdef0123456789abcdef01234567",
                    owner_id: "owner",
                    date: "2026-11-20T00:00:00",
                    user: b"Owner",
                    password: "secret",
                    sha1: false,
                    content_limits: b.content,
                    parse_limits: b.parser,
                },
            )
            .await
            .unwrap();
            let view = owner.register_checkout("work", &root, b).await.unwrap();
            let original = snapshot(&mut owner).await;
            // Authenticate fixture history in a completed read-only operation.
            let mut tx = owner.store.begin_write().await.unwrap();
            let mut fixture_work = Budget::new(b.content).unwrap();
            let mut fixture_parse = ParseWork::new(b.parser).unwrap();
            let history = tx
                .validate_recorded_history(
                    &owner.principal,
                    &view.shared_head,
                    b,
                    &mut fixture_work,
                    &mut fixture_parse,
                )
                .await
                .unwrap();
            assert!(history.commits.is_empty());
            tx.rollback().await.unwrap();
            let projection_limits = if case == "entry-limit" {
                AdmissionLimits {
                    parser: crate::artifact::ParseLimits {
                        entries: 1,
                        ..b.parser
                    },
                    ..b
                }
            } else {
                b
            };
            // The private projection trial starts here. These matching ledgers are
            // borrowed through real receipt/content staging and projection without reset.
            let mut tx = owner.store.begin_write().await.unwrap();
            let mut work = Budget::new(projection_limits.content).unwrap();
            let mut parse = ParseWork::new(projection_limits.parser).unwrap();
            tx.recording_execute(
                "INSERT INTO rcvfrom(uid,mtime,nonce,ipaddr) VALUES(0,julianday('now'),NULL,NULL)",
                vec![],
                &mut work,
            )
            .await
            .unwrap();
            let receipt =
                bounded_scalar(&mut tx, "SELECT max(rcvid) FROM rcvfrom", vec![], &mut work)
                    .await
                    .unwrap();
            let mut files = Vec::new();
            for (path, payload) in [("a", b"alpha".as_slice()), ("b", b"beta".as_slice())] {
                let hash = native_name(payload, &mut work).unwrap();
                let rid = tx
                    .recording_put(
                        &owner.principal,
                        &hash,
                        payload,
                        receipt,
                        projection_limits,
                        &mut work,
                    )
                    .await
                    .unwrap();
                files.push(RecordedFile {
                    path: path.into(),
                    hash,
                    rid,
                    mode: 0,
                });
            }
            let references: Vec<_> = files
                .iter()
                .map(|file| ManifestFile {
                    path: &file.path,
                    hash: &file.hash,
                    mode: file.mode,
                })
                .collect();
            let bytes = build_private_manifest(
                &PrivateManifestSpec {
                    parents: PrivateParents::First {
                        shared: ManifestParent {
                            hash: &history.shared.hash,
                            date: &history.shared.date,
                        },
                    },
                    files: &references,
                    actor: b"Owner",
                    comment: b"projection",
                    date: b"2026-11-20T00:00:01",
                    branch: &history.branch,
                },
                &mut work,
                b.parser.input_bytes,
            )
            .unwrap();
            let hash = native_name(&bytes, &mut work).unwrap();
            let rid = tx
                .recording_put(
                    &owner.principal,
                    &hash,
                    &bytes,
                    receipt,
                    projection_limits,
                    &mut work,
                )
                .await
                .unwrap();
            assert_eq!(
                tx.query_bound(
                    "SELECT rcvid FROM blob WHERE rid=?1",
                    vec![Value::Integer(rid)]
                )
                .await
                .unwrap(),
                vec![vec![Value::Integer(receipt)]]
            );
            if case == "tag-identity" {
                // Deliberately unsupported private-boundary state, never public producer output.
                tx.execute(
                    "INSERT INTO tag(tagid,tagname) VALUES(13,'unrelated-private-boundary')",
                )
                .await
                .unwrap();
            }
            let staged = transaction_snapshot(&mut tx).await;
            if case == "copy-budget" {
                // Consume the actual borrowed remaining allowance; keep the one ledger used above.
                // Per-envelope and stored ceilings stay much larger than one byte in this fixture.
                while work.packed_output_ceiling() > 1 {
                    work.charge_decoded(work.packed_output_ceiling() - 1)
                        .unwrap();
                }
                assert_eq!(work.packed_output_ceiling(), 1);
            }
            assert_eq!(files.len(), 2);
            assert_eq!(parse.remaining.entries, projection_limits.parser.entries);
            let parser_before = [
                parse.remaining.input_bytes,
                parse.remaining.cards,
                parse.remaining.owned_bytes,
                parse.remaining.entries,
            ];
            let result = tx
                .project_recording(
                    &history,
                    ProjectionInput {
                        rid,
                        bytes: &bytes,
                        files: &files,
                    },
                    projection_limits,
                    &mut work,
                    &mut parse,
                )
                .await;
            if case == "entry-limit" {
                assert_eq!(
                    result.unwrap_err().operation,
                    "recording projection entry limit"
                );
                assert_eq!(
                    transaction_snapshot(&mut tx).await,
                    staged,
                    "projection wrote before entry preflight"
                );
                assert_eq!(
                    [
                        parse.remaining.input_bytes,
                        parse.remaining.cards,
                        parse.remaining.owned_bytes,
                        parse.remaining.entries
                    ],
                    parser_before
                );
            } else if case == "copy-budget" {
                assert_eq!(result.unwrap_err().operation, "total decoded content limit");
                assert_eq!(
                    transaction_snapshot(&mut tx).await,
                    staged,
                    "projection wrote before copy debit"
                );
            } else if case == "tag-identity" {
                // An issue may hold tag 13, so the branch tag takes the next free ID.
                // The post-state admission of the real call checks every tag row.
                result.unwrap();
                assert_eq!(
                    tx.query_bound(
                        "SELECT tagid FROM tagxref WHERE srcid=?1 AND tagtype=2 AND value IS NULL",
                        vec![Value::Integer(rid)]
                    )
                    .await
                    .unwrap(),
                    vec![vec![Value::Integer(14)]]
                );
                assert_ne!(transaction_snapshot(&mut tx).await, staged);
            } else {
                result.unwrap();
                assert_eq!(
                    tx.query_bound(
                        "SELECT pid,isprim FROM plink WHERE cid=?1",
                        vec![Value::Integer(rid)]
                    )
                    .await
                    .unwrap(),
                    vec![vec![Value::Integer(history.shared.rid), Value::Integer(1)]]
                );
                assert_eq!(
                    tx.query("SELECT name FROM filename ORDER BY name")
                        .await
                        .unwrap(),
                    vec![vec![Value::Text("a".into())], vec![Value::Text("b".into())]]
                );
                assert_eq!(
                    tx.query_bound(
                        "SELECT fid FROM mlink WHERE mid=?1 ORDER BY fnid",
                        vec![Value::Integer(rid)]
                    )
                    .await
                    .unwrap(),
                    files
                        .iter()
                        .map(|file| vec![Value::Integer(file.rid)])
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    tx.query_bound(
                        "SELECT tagid FROM tag WHERE tagname=?1",
                        vec![Value::Text(format!("sym-{}", history.branch))]
                    )
                    .await
                    .unwrap(),
                    vec![vec![Value::Integer(13)]]
                );
                assert_eq!(
                    tx.query_bound(
                        "SELECT count(*) FROM tagxref WHERE srcid=?1",
                        vec![Value::Integer(rid)]
                    )
                    .await
                    .unwrap(),
                    vec![vec![Value::Integer(3)]]
                );
                assert_eq!(
                    tx.query_bound(
                        "SELECT rid FROM leaf WHERE rid=?1",
                        vec![Value::Integer(rid)]
                    )
                    .await
                    .unwrap(),
                    vec![vec![Value::Integer(rid)]]
                );
            }
            tx.rollback().await.unwrap();
            assert_eq!(snapshot(&mut owner).await, original, "rollback for {case}");
            drop(owner);
            let mut reopened = LocalOwnerRepository::open(&repo, b).await.unwrap();
            assert_eq!(reopened.checkout_view("work", b).await.unwrap(), view);
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        }
    }

    /// Reset injection even when an assertion unwinds.
    struct Reset;
    impl Drop for Reset {
        /// Clear the thread-local injection selector.
        /// # Arguments
        /// * `self` - Active fixture guard.
        /// # Returns
        /// Unit after clearing injection.
        /// # Errors
        /// Cannot fail.
        fn drop(&mut self) {
            RECORDING_FAILURE.with(|slot| slot.set(None));
            RECORDING_INITIAL_DEBITS.with(|slot| slot.set([0; 3]));
            RECORDING_INITIAL_DEBIT_SEEN.with(|slot| slot.set(None));
        }
    }
    #[tokio::test]
    /// Inject after each mutation phase and require complete rollback plus reopen.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after every late-write checkpoint preserves complete prior state.
    /// # Panics
    /// Panics on any atomicity, counter or owner-proof regression.
    async fn every_recording_write_phase_rolls_back_every_table() {
        for phase in [
            RecordingPhase::Receipt,
            RecordingPhase::Payloads,
            RecordingPhase::Manifest,
            RecordingPhase::Projections,
            RecordingPhase::Inventory,
            RecordingPhase::Heads,
            RecordingPhase::Fingerprint,
            RecordingPhase::Poststate,
        ] {
            let temp = tempfile::tempdir().unwrap();
            let repo = temp.path().join("repo");
            let root = temp.path().join("root");
            std::fs::create_dir(&root).unwrap();
            let root = std::fs::canonicalize(root).unwrap();
            std::fs::write(root.join("a"), b"payload").unwrap();
            let b = bounds();
            let mut owner = LocalOwnerRepository::initialize(
                &repo,
                InitializeOptions {
                    repository_id: "0123456789abcdef0123456789abcdef01234567",
                    owner_id: "owner",
                    date: "2026-11-20T00:00:00",
                    user: b"Owner",
                    password: "secret",
                    sha1: false,
                    content_limits: b.content,
                    parse_limits: b.parser,
                },
            )
            .await
            .unwrap();
            let view = owner.register_checkout("work", &root, b).await.unwrap();
            let view = owner
                .add_files("work", &["a"], &view, b, tracking())
                .await
                .unwrap();
            let view = owner
                .select_path_scopes(
                    "work",
                    &view,
                    &[super::super::path_scope::PathScopeSelection {
                        path: "a".into(),
                        scope: PathScope::Private,
                    }],
                    b,
                    tracking(),
                )
                .await
                .unwrap();
            let before = snapshot(&mut owner).await;
            let _reset = Reset;
            RECORDING_FAILURE.with(|slot| slot.set(Some(phase)));
            let error = owner
                .commit_private_tracked_files(
                    "work",
                    PrivateRecordingRequest {
                        expected: &view,
                        paths: &["a"],
                        scope: PathScope::Private,
                        actor: b"Owner",
                        comment: b"rollback",
                        date: b"2026-11-20T00:00:01",
                    },
                    b,
                    tracking(),
                )
                .await
                .unwrap_err();
            RECORDING_FAILURE.with(|slot| slot.set(None));
            assert_eq!(
                error.operation, "injected recording failure",
                "phase {phase:?}"
            );
            assert_eq!(snapshot(&mut owner).await, before, "phase {phase:?}");
            drop(owner);
            let mut reopened = LocalOwnerRepository::open(&repo, b).await.unwrap();
            assert_eq!(reopened.checkout_view("work", b).await.unwrap(), view);
            assert_eq!(std::fs::read(root.join("a")).unwrap(), b"payload");
        }
    }
    #[tokio::test]
    /// Compression respects already consumed cumulative stored allowance before allocation.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after the borrowed one-byte packed capacity refuses a new envelope.
    /// # Panics
    /// Panics on fresh per-blob budgets, wrong failure phase or native mutation.
    async fn compression_uses_remaining_cumulative_capacity_before_pack() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let b = bounds();
        let mut owner = LocalOwnerRepository::initialize(
            &repo,
            InitializeOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "owner",
                date: "2026-11-20T00:00:00",
                user: b"Owner",
                password: "secret",
                sha1: false,
                content_limits: b.content,
                parse_limits: b.parser,
            },
        )
        .await
        .unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        tx.execute(
            "INSERT INTO rcvfrom(uid,mtime,nonce,ipaddr) VALUES(0,julianday('now'),NULL,NULL)",
        )
        .await
        .unwrap();
        let before = tx
            .query("SELECT rid,rcvid,size,uuid,content FROM blob ORDER BY rid")
            .await
            .unwrap();
        let mut limits = b;
        limits.content.max_total_stored_bytes = 16;
        let mut work = Budget::new(limits.content).unwrap();
        work.charge_stored(15).unwrap();
        assert_eq!(work.packed_output_ceiling(), 1);
        let bytes = b"a newly captured opaque payload";
        let name = native_name(bytes, &mut work).unwrap();
        let error = tx
            .recording_put(&owner.principal, &name, bytes, 2, limits, &mut work)
            .await
            .unwrap_err();
        assert_eq!(error.operation, "recording compression");
        assert_eq!(
            tx.query("SELECT rid,rcvid,size,uuid,content FROM blob ORDER BY rid")
                .await
                .unwrap(),
            before
        );
        assert_eq!(
            tx.query("SELECT count(*) FROM private UNION ALL SELECT count(*) FROM unsent")
                .await
                .unwrap(),
            vec![vec![Value::Integer(0)], vec![Value::Integer(0)]]
        );
        tx.rollback().await.unwrap();
    }

    #[test]
    /// The post-hash deduplication policy still requires exact complete byte equality.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after empty, binary, prefix and differing-byte controls.
    /// # Panics
    /// Panics if different content could inherit an existing identity or receipt.
    fn deduplication_requires_complete_byte_equality() {
        for bytes in [b"".as_slice(), b"a\0\xff"] {
            require_same_content(bytes, bytes).unwrap();
        }
        for (old, new) in [
            (b"a".as_slice(), b"ab".as_slice()),
            (b"ab".as_slice(), b"a".as_slice()),
            (b"a\0".as_slice(), b"a\xff".as_slice()),
        ] {
            assert_eq!(
                require_same_content(old, new).unwrap_err().operation,
                "conflicting recording content"
            );
        }
    }

    #[tokio::test]
    /// Real insertion bounds and shun policy refuse before creating any native rows.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after exact output limit, shun and remaining compression-work controls.
    /// # Panics
    /// Panics on wrong refusal, replacement budgets or failed all-table rollback.
    async fn content_insertion_bounds_and_shun_are_atomic() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let b = bounds();
        let mut owner = LocalOwnerRepository::initialize(
            &repo,
            InitializeOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "owner",
                date: "2026-11-20T00:00:00",
                user: b"Owner",
                password: "secret",
                sha1: false,
                content_limits: b.content,
                parse_limits: b.parser,
            },
        )
        .await
        .unwrap();
        let before = snapshot(&mut owner).await;
        let mut tx = owner.store.begin_write().await.unwrap();
        tx.execute(
            "INSERT INTO rcvfrom(uid,mtime,nonce,ipaddr) VALUES(0,julianday('now'),NULL,NULL)",
        )
        .await
        .unwrap();
        let bytes = b"bounded opaque content";
        let name = native_name(bytes, &mut Budget::new(b.content).unwrap()).unwrap();
        let low = AdmissionLimits {
            content: Limits {
                max_output_bytes: bytes.len() as u32 - 1,
                ..b.content
            },
            ..b
        };
        assert_eq!(
            tx.recording_put(
                &owner.principal,
                &name,
                bytes,
                2,
                low,
                &mut Budget::new(low.content).unwrap()
            )
            .await
            .unwrap_err()
            .operation,
            "recording content output limit"
        );
        tx.execute_bound(
            "INSERT INTO shun(uuid) VALUES(?1)",
            vec![Value::Text(name.clone())],
        )
        .await
        .unwrap();
        assert_eq!(
            tx.recording_put(
                &owner.principal,
                &name,
                bytes,
                2,
                b,
                &mut Budget::new(b.content).unwrap()
            )
            .await
            .unwrap_err()
            .operation,
            "recording content shunned"
        );
        tx.execute_bound(
            "DELETE FROM shun WHERE uuid=?1",
            vec![Value::Text(name.clone())],
        )
        .await
        .unwrap();
        // Measure only the two actual fixed read helpers on a separate trial ledger.
        // A predebit then gives the real insertion exactly that lookup allowance,
        // proving failure at compression's distinct bytes+8192 charge.
        // With equal stored and decoded ceilings and no stored debits, the
        // packed ceiling exposes precisely the remaining decoded allowance.
        let measured = Limits {
            max_total_decoded_bytes: 1 << 20,
            ..b.content
        };
        let mut lookup = Budget::new(measured).unwrap();
        lookup.charge_decoded(bytes.len()).unwrap();
        bounded_rows(
            &mut tx,
            "SELECT rid FROM blob WHERE uuid=?1",
            vec![Value::Text(name.clone())],
            1,
            64,
            &mut lookup,
        )
        .await
        .unwrap();
        bounded_rows(
            &mut tx,
            "SELECT count(*) FROM shun WHERE uuid=?1",
            vec![Value::Text(name.clone())],
            1,
            64,
            &mut lookup,
        )
        .await
        .unwrap();
        let remaining = lookup.packed_output_ceiling();
        let mut short = Budget::new(measured).unwrap();
        short.charge_decoded(remaining).unwrap();
        assert_eq!(
            tx.recording_put(&owner.principal, &name, bytes, 2, b, &mut short)
                .await
                .unwrap_err()
                .operation,
            "total decoded content limit"
        );
        assert_eq!(
            tx.query("SELECT count(*) FROM blob WHERE rid<>1")
                .await
                .unwrap(),
            vec![vec![Value::Integer(0)]]
        );
        let rid = tx
            .recording_put(
                &owner.principal,
                &name,
                bytes,
                2,
                b,
                &mut Budget::new(b.content).unwrap(),
            )
            .await
            .unwrap();
        assert!(rid > 1);
        assert_eq!(
            tx.recording_put(
                &owner.principal,
                &name,
                bytes,
                2,
                b,
                &mut Budget::new(b.content).unwrap()
            )
            .await
            .unwrap(),
            rid
        );
        tx.rollback().await.unwrap();
        assert_eq!(snapshot(&mut owner).await, before);
        drop(owner);
        drop(LocalOwnerRepository::open(&repo, b).await.unwrap());
    }

    /// Attempt a changed commit from an exact closed fixture image.
    /// # Arguments
    /// * `repo` - Disposable fixture database whose closed bytes may be restored.
    /// * `image` - Exact tracked/private baseline database bytes.
    /// * `view` - Baseline all-seven expected view.
    /// * `limits` - One distinct candidate cumulative ceiling on the commit call.
    /// # Returns
    /// Success flag, actual reached phase mask and every resulting table value.
    /// # Panics
    /// Panics on setup/provider failure, unexpected NoChange or reopened baseline drift.
    async fn budget_trial(
        repo: &std::path::Path,
        image: &[u8],
        view: &CheckoutView,
        limits: AdmissionLimits,
    ) -> (bool, u16, Vec<(String, Vec<Vec<Value>>)>) {
        std::fs::write(repo, image).unwrap();
        let mut owner = LocalOwnerRepository::open(repo, bounds()).await.unwrap();
        assert_eq!(owner.checkout_view("work", bounds()).await.unwrap(), *view);
        RECORDING_PHASES.with(|phases| phases.set(0));
        let result = owner
            .commit_private_tracked_files(
                "work",
                PrivateRecordingRequest {
                    expected: view,
                    paths: &["a", "b"],
                    scope: PathScope::Private,
                    actor: b"Owner",
                    comment: b"bounded",
                    date: b"2026-11-20T00:00:02",
                },
                limits,
                tracking(),
            )
            .await;
        if let Ok(outcome) = &result {
            assert!(matches!(outcome, PrivateRecordingOutcome::Recorded { .. }));
        }
        let phases = RECORDING_PHASES.with(|visited| visited.get());
        let values = snapshot(&mut owner).await;
        (result.is_ok(), phases, values)
    }

    #[tokio::test]
    /// Exhaust each cumulative compartment near its observed changed-commit boundary.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit for successful finite baselines and late stored/decoded/parser rollback.
    /// # Panics
    /// Panics if a compartment is reset, a refusal at the boundary comes before the first
    /// native write, or a failed transaction changes any baseline table/receipt/scope/head.
    async fn cumulative_stored_decoded_and_parser_limits_fail_late_and_atomically() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        std::fs::write(root.join("a"), b"payload").unwrap();
        std::fs::write(root.join("b"), b"\0\xff first").unwrap();
        let b = bounds();
        let mut owner = LocalOwnerRepository::initialize(
            &repo,
            InitializeOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "owner",
                date: "2026-11-20T00:00:00",
                user: b"Owner",
                password: "secret",
                sha1: false,
                content_limits: b.content,
                parse_limits: b.parser,
            },
        )
        .await
        .unwrap();
        let view = owner.register_checkout("work", &root, b).await.unwrap();
        let view = owner
            .add_files("work", &["a", "b"], &view, b, tracking())
            .await
            .unwrap();
        let view = owner
            .select_path_scopes(
                "work",
                &view,
                &[
                    super::super::path_scope::PathScopeSelection {
                        path: "a".into(),
                        scope: PathScope::Private,
                    },
                    super::super::path_scope::PathScopeSelection {
                        path: "b".into(),
                        scope: PathScope::Private,
                    },
                ],
                b,
                tracking(),
            )
            .await
            .unwrap();
        let outcome = owner
            .commit_private_tracked_files(
                "work",
                PrivateRecordingRequest {
                    expected: &view,
                    paths: &["a", "b"],
                    scope: PathScope::Private,
                    actor: b"Owner",
                    comment: b"history prefix",
                    date: b"2026-11-20T00:00:01",
                },
                b,
                tracking(),
            )
            .await
            .unwrap();
        let view = owner.checkout_view("work", b).await.unwrap();
        assert!(
            matches!(outcome, PrivateRecordingOutcome::Recorded { view: actual, .. } if actual == view)
        );
        let before = snapshot(&mut owner).await;
        drop(owner);
        let image = std::fs::read(&repo).unwrap();
        std::fs::write(root.join("a"), b"changed payload").unwrap();
        std::fs::write(root.join("b"), b"\0\xfe second").unwrap();
        for compartment in 0..3 {
            let mut low = 1u64;
            let mut high = match compartment {
                0 => b.content.max_total_stored_bytes,
                1 => b.content.max_total_decoded_bytes,
                _ => b.parser.input_bytes as u64,
            };
            let set = |amount: u64| {
                let mut candidate = b;
                match compartment {
                    0 => candidate.content.max_total_stored_bytes = amount,
                    1 => candidate.content.max_total_decoded_bytes = amount,
                    _ => candidate.parser.input_bytes = amount as usize,
                };
                candidate
            };
            let (success, phases, _) = budget_trial(&repo, &image, &view, set(high)).await;
            assert!(success);
            assert_eq!(phases, 255);
            while low < high {
                let middle = low + (high - low) / 2;
                let (success, _, values) = budget_trial(&repo, &image, &view, set(middle)).await;
                if success {
                    high = middle;
                } else {
                    assert_eq!(values, before);
                    low = middle + 1;
                }
            }
            // SQL now's decimal receipt-time text can vary by a few bytes between
            // trials. Keep decoded endpoints 4096 bytes apart from the observed
            // transition; stored/parser endpoints retain their exact one-byte step.
            let margin = if compartment == 1 { 4096 } else { 0 };
            let (success, phases, _) = budget_trial(&repo, &image, &view, set(low + margin)).await;
            assert!(success);
            assert_eq!(phases, 255);
            assert!(low > margin + 1);
            let (success, phases, values) =
                budget_trial(&repo, &image, &view, set(low - margin - 1)).await;
            assert!(!success);
            // The last charge of each compartment comes after the first native write,
            // so the refusal tests the rollback of written rows.
            assert!(
                phases & 1 != 0,
                "compartment {compartment} failed before the first native write: {phases}"
            );
            assert_eq!(values, before);
            std::fs::write(&repo, &image).unwrap();
            let mut reopened = LocalOwnerRepository::open(&repo, b).await.unwrap();
            assert_eq!(reopened.checkout_view("work", b).await.unwrap(), view);
            assert_eq!(snapshot(&mut reopened).await, before);
            drop(reopened);
        }
    }

    /// Run a deterministic checked no-op with an initial debit on the genuine ledgers.
    /// # Arguments
    /// * `repo` - Disposable database filename.
    /// * `image` - Exact closed two-file recorded-history baseline image.
    /// * `view` - Baseline all-seven expectation.
    /// * `before` - Complete expected table state for successful and refused no-ops.
    /// * `limits` - Candidate finite operation limits.
    /// * `debit` - Initial stored/decoded/parser-input debit before every phase.
    /// # Returns
    /// Success and whether the authority entry observed the already applied exact debit.
    /// # Panics
    /// Panics on fixture failure, changed outcome, ignored expectation or native mutation.
    async fn noop_debit_trial(
        repo: &std::path::Path,
        image: &[u8],
        view: &CheckoutView,
        before: &[(String, Vec<Vec<Value>>)],
        limits: AdmissionLimits,
        debit: [usize; 3],
    ) -> (bool, bool) {
        std::fs::write(repo, image).unwrap();
        let mut owner = LocalOwnerRepository::open(repo, bounds()).await.unwrap();
        let _reset = Reset;
        RECORDING_INITIAL_DEBITS.with(|slot| slot.set(debit));
        RECORDING_INITIAL_DEBIT_SEEN.with(|slot| slot.set(None));
        RECORDING_AUTH_SAW_DEBIT.with(|slot| slot.set(false));
        RECORDING_PHASES.with(|slot| slot.set(0));
        let result = owner
            .commit_private_tracked_files(
                "work",
                PrivateRecordingRequest {
                    expected: view,
                    paths: &["a", "b"],
                    scope: PathScope::Private,
                    actor: b"Owner",
                    comment: b"deterministic checked no-op",
                    date: b"2026-11-20T00:00:02",
                },
                limits,
                tracking(),
            )
            .await;
        if let Ok(outcome) = &result {
            assert!(
                matches!(outcome, PrivateRecordingOutcome::NoChange { view: actual } if actual == view),
                "no-op produced history or changed its view"
            );
        }
        let witness = RECORDING_INITIAL_DEBIT_SEEN.with(|slot| slot.get() == Some(debit))
            && RECORDING_AUTH_SAW_DEBIT.with(|slot| slot.get());
        assert_eq!(RECORDING_PHASES.with(|slot| slot.get()), 0);
        assert_eq!(snapshot(&mut owner).await.as_slice(), before);
        (result.is_ok(), witness)
    }

    /// Find the successful finite threshold independently for one debit scenario.
    /// # Arguments
    /// * `repo` - Disposable closed image restoration target.
    /// * `image` - Exact two-file existing-history database bytes.
    /// * `view` - Baseline expected view.
    /// * `before` - Complete immutable baseline table state.
    /// * `compartment` - Zero stored, one decoded, two cumulative parser input.
    /// * `debit` - Initial real-ledger precharge; no remaining counter is reset.
    /// # Returns
    /// Minimum successful total ceiling, independently searched for this scenario.
    /// # Panics
    /// Panics if finite positive control fails, initial debit is not witnessed before
    /// authority, the boundary is not one byte, or any operation changes baseline data.
    async fn noop_debit_threshold(
        repo: &std::path::Path,
        image: &[u8],
        view: &CheckoutView,
        before: &[(String, Vec<Vec<Value>>)],
        compartment: usize,
        debit: [usize; 3],
    ) -> u64 {
        let b = bounds();
        let mut low = 1u64;
        let mut high = match compartment {
            0 => b.content.max_total_stored_bytes,
            1 => b.content.max_total_decoded_bytes,
            _ => b.parser.input_bytes as u64,
        };
        let set = |amount: u64| {
            let mut candidate = b;
            match compartment {
                0 => candidate.content.max_total_stored_bytes = amount,
                1 => candidate.content.max_total_decoded_bytes = amount,
                _ => candidate.parser.input_bytes = amount as usize,
            };
            candidate
        };
        let (positive, witness) =
            noop_debit_trial(repo, image, view, before, set(high), debit).await;
        assert!(positive && witness);
        while low < high {
            let middle = low + (high - low) / 2;
            let (success, _) =
                noop_debit_trial(repo, image, view, before, set(middle), debit).await;
            if success {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        let (positive, witness) =
            noop_debit_trial(repo, image, view, before, set(low), debit).await;
        assert!(positive && witness);
        assert!(low > 1);
        let (success, witness) =
            noop_debit_trial(repo, image, view, before, set(low - 1), debit).await;
        assert!(!success && witness);
        low
    }

    #[tokio::test]
    /// A known debit must shift every independently measured total threshold exactly.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after two-file existing-history operations prove no stored/decoded/parser
    /// ledger reset or lost initial debit across full pre/post admission and capture.
    /// # Panics
    /// Panics if any precharge disappears, threshold shifts incorrectly, a positive
    /// control fails, the debit is not present before authority, or data changes.
    /// This deterministic NoChange control complements the separate changed-record
    /// late-failure rollback control; it creates no test-only receipt clock policy.
    async fn initial_borrowed_debits_shift_total_thresholds_exactly() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        std::fs::write(root.join("a"), b"first retained payload").unwrap();
        std::fs::write(root.join("b"), b"\0\xff second retained payload").unwrap();
        let b = bounds();
        let mut owner = LocalOwnerRepository::initialize(
            &repo,
            InitializeOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "owner",
                date: "2026-11-20T00:00:00",
                user: b"Owner",
                password: "secret",
                sha1: false,
                content_limits: b.content,
                parse_limits: b.parser,
            },
        )
        .await
        .unwrap();
        let view = owner.register_checkout("work", &root, b).await.unwrap();
        let view = owner
            .add_files("work", &["a", "b"], &view, b, tracking())
            .await
            .unwrap();
        let view = owner
            .select_path_scopes(
                "work",
                &view,
                &[
                    super::super::path_scope::PathScopeSelection {
                        path: "a".into(),
                        scope: PathScope::Private,
                    },
                    super::super::path_scope::PathScopeSelection {
                        path: "b".into(),
                        scope: PathScope::Private,
                    },
                ],
                b,
                tracking(),
            )
            .await
            .unwrap();
        let outcome = owner
            .commit_private_tracked_files(
                "work",
                PrivateRecordingRequest {
                    expected: &view,
                    paths: &["a", "b"],
                    scope: PathScope::Private,
                    actor: b"Owner",
                    comment: b"recorded history prefix",
                    date: b"2026-11-20T00:00:01",
                },
                b,
                tracking(),
            )
            .await
            .unwrap();
        let view = owner.checkout_view("work", b).await.unwrap();
        assert!(
            matches!(outcome, PrivateRecordingOutcome::Recorded { view: actual, .. } if actual == view)
        );
        let before = snapshot(&mut owner).await;
        drop(owner);
        let image = std::fs::read(&repo).unwrap();
        /// Known initial work charged independently to each cumulative compartment.
        const DEBIT: usize = 64;
        for compartment in 0..3 {
            let baseline =
                noop_debit_threshold(&repo, &image, &view, &before, compartment, [0; 3]).await;
            let mut debit = [0; 3];
            debit[compartment] = DEBIT;
            let precharged =
                noop_debit_threshold(&repo, &image, &view, &before, compartment, debit).await;
            assert_eq!(
                precharged,
                baseline + DEBIT as u64,
                "lost/reset cumulative compartment {compartment}"
            );
        }
        std::fs::write(&repo, &image).unwrap();
        let mut reopened = LocalOwnerRepository::open(&repo, b).await.unwrap();
        assert_eq!(reopened.checkout_view("work", b).await.unwrap(), view);
        assert_eq!(snapshot(&mut reopened).await, before);
    }
}
