//! Read the ancestry of the checkout, whole or one page at a time.
//!
//! The reader walks the manifests from the current check-in to the end of the page
//! and reads the event row of each check-in that it prints. Dates and hashes come
//! from the manifests, never from display projections or working files. `gos v
//! verify` compares the event rows with the manifests.
//!
//! ## Recommendations
//! Select an explicit existing checkout and supply the finite ceilings of one
//! artifact and one row.
use super::recorded_history::{Reach, RecordedCommit, bounded_rows};
use super::{AdmissionLimits, LocalOwnerRepository, StorageError};
use crate::storage::{WriteTransaction, content::Budget};
use libsql::Value;

/// One authenticated native check-in of the current-first ancestry of a checkout.
///
/// Byte fields preserve the original native date token and decoded user/comment
/// bytes. The supported admitted family requires UTF-8 metadata without NUL.
#[derive(Debug, PartialEq, Eq)]
pub struct RecordedCheckin {
    /// Full verified native content identity, without abbreviation.
    pub hash: String,
    /// Exact native D-card token, without date normalization.
    pub date: Vec<u8>,
    /// Exact decoded original native U-card bytes.
    pub actor: Vec<u8>,
    /// Exact decoded original native C-card bytes.
    pub comment: Vec<u8>,
    /// True for a private ancestor; false only for the immutable shared root.
    pub private: bool,
}

impl LocalOwnerRepository {
    /// Read the complete supported native ancestry of an explicitly selected checkout.
    ///
    /// It returns [`Self::recorded_timeline_page`] with no offset and no limit.
    /// # Arguments
    /// * `checkout` - Existing nonempty NUL-free registration ID, at most 1024 bytes and parser input bytes.
    /// * `limits` - The ceilings of one artifact and one row.
    /// # Returns
    /// Complete authenticated current-first ancestry including the shared root,
    /// only after checked transaction completion; never a truncated or empty history.
    /// # Errors
    /// The refusals of [`Self::recorded_timeline_page`].
    /// # Examples
    /// ```no_run
    /// async fn history(owner: &mut gossil::storage::LocalOwnerRepository, limits: gossil::storage::AdmissionLimits) {
    ///     let rows = owner.recorded_timeline("work", limits).await.unwrap();
    ///     assert!(!rows.last().unwrap().private);
    /// }
    /// ```
    pub async fn recorded_timeline(
        &mut self,
        checkout: &str,
        limits: AdmissionLimits,
    ) -> Result<Vec<RecordedCheckin>, StorageError> {
        self.recorded_timeline_page(checkout, 0, 0, limits).await
    }

    /// Read one page of the native ancestry of an explicitly selected checkout.
    ///
    /// The ancestry starts with the current private check-in, follows its first
    /// parents, and ends with the immutable shared empty root. Before the first
    /// private recording, that root is the only entry. The page skips the first
    /// `offset` entries and then holds at most `limit` entries, or every remaining
    /// entry when `limit` is 0, as the `--offset` and `--limit` options of
    /// `fossil timeline` do.
    ///
    /// One transaction runs the checks of each command, then walks the manifests from
    /// the current check-in to the end of the page, and reads the event row of each
    /// entry of the page. Each manifest has the ceilings of one artifact, and each row
    /// the ceilings of one row. The read has no parser or content ceiling on the whole
    /// chain, so its cost follows `offset` and `limit`, not the length of the history.
    /// The page keeps each manifest that it walks, and the parse of each, up to 1 GiB
    /// in all. The first manifest, the current check-in, does not count toward that
    /// ceiling, so only the older manifests count toward it.
    ///
    /// The read refuses an event whose user or comment is not the U or C card of its
    /// manifest. When the walk reaches the shared root, it also refuses a chain whose
    /// length is not the generation of the checkout. It reads no file content, makes
    /// no durable mutation and publishes no locator. Pending paths and working files
    /// do not change the result.
    /// # Arguments
    /// * `checkout` - Existing nonempty NUL-free registration ID, at most 1024 bytes and parser input bytes.
    /// * `offset` - The count of newest entries to skip.
    /// * `limit` - The most entries to return, or 0 for every remaining entry.
    /// * `limits` - The ceilings of one artifact and one row. The total content
    ///   ceilings are not used.
    /// # Returns
    /// The entries of the page, newest first, only after checked transaction
    /// completion. The page is empty only when `offset` passes the shared root.
    /// # Errors
    /// Refuses missing/foreign checkout, changed authority, unsupported or corrupt
    /// state, invalid limits, an artifact or row over its ceiling, a page whose
    /// manifests and parses after the first would pass 1 GiB,
    /// allocation/provider and checked cleanup errors. Existing operation causes and
    /// separate cleanup failures are preserved.
    /// # Examples
    /// ```no_run
    /// async fn newest(owner: &mut gossil::storage::LocalOwnerRepository, limits: gossil::storage::AdmissionLimits) {
    ///     let rows = owner.recorded_timeline_page("work", 0, 20, limits).await.unwrap();
    ///     assert!(rows.len() <= 20);
    /// }
    /// ```
    pub async fn recorded_timeline_page(
        &mut self,
        checkout: &str,
        offset: usize,
        limit: usize,
        limits: AdmissionLimits,
    ) -> Result<Vec<RecordedCheckin>, StorageError> {
        if checkout.is_empty()
            || checkout.contains('\0')
            || checkout.len() > 1024.min(limits.parser.input_bytes)
        {
            return Err(StorageError::state("checkout identity limit or spelling"));
        }
        let (content, mut work, mut parse) = super::verification::proof_ledgers(limits)?;
        let limits = AdmissionLimits {
            content,
            parser: limits.parser,
        };
        // A limit of 0 has no end, as in Fossil.
        let limit = if limit == 0 { usize::MAX } else { limit };
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission_budgeted(&self.principal, &mut work)
                .await?;
            tx.check_recording_state(&self.principal, limits, &mut work)
                .await?;
            let selections = bounded_rows(
                &mut tx,
                "SELECT checkout_id,shared_head FROM g_checkout",
                vec![],
                1,
                limits.parser.input_bytes,
                &mut work,
            )
            .await?;
            let one = selections
                .first()
                .filter(|_| selections.len() == 1)
                .map(Vec::as_slice);
            let Some([Value::Text(id), Value::Text(shared)]) = one else {
                return Err(StorageError::state("recorded checkout unavailable"));
            };
            if id != checkout {
                return Err(StorageError::state("recorded checkout unavailable"));
            }
            // The page reads the manifests up to its end, and no file.
            let walk = tx
                .walk_recorded_history(
                    &self.principal,
                    shared,
                    limits,
                    &mut work,
                    &mut parse,
                    Reach::Manifests(
                        offset.saturating_add(limit),
                        super::recorded_history::PAGE_RETAINED_BYTES,
                    ),
                )
                .await?;
            let root = walk
                .complete
                .then_some((walk.shared, false, walk.shared_cards));
            let entries = walk.commits.into_iter().rev().map(|(commit, _, artifact)| {
                let cards = (
                    artifact.user().map(|card| card.bytes.clone()),
                    artifact.comment().map(|card| card.bytes.clone()),
                );
                (commit, true, cards)
            });
            let (page, cards): (Vec<_>, Vec<_>) = entries
                .chain(root)
                .skip(offset)
                .take(limit)
                .map(|(commit, private, cards)| ((commit, private), cards))
                .unzip();
            let printed = project_history(&mut tx, page, limits, &mut work).await?;
            // Each printed user and comment must be the U and C card of its manifest.
            if printed.iter().zip(&cards).any(|(row, (user, comment))| {
                user.as_deref() != Some(row.actor.as_slice())
                    || comment.as_deref() != Some(row.comment.as_slice())
            }) {
                return Err(StorageError::state(EVENT_CARDS));
            }
            Ok(printed)
        }
        .await;
        complete_read(tx, result).await
    }
}

/// The refusal of an event row whose user or comment is not that of its manifest.
const EVENT_CARDS: &str = "recorded event metadata differs from its manifest";

/// Charge and reserve the complete public result capacity before retaining rows.
/// # Arguments
/// * `count` - The count of entries of the page.
/// * `work` - Borrowed operation content/ownership ledger.
/// # Returns
/// Empty vector with capacity for every entry of the page.
/// # Errors
/// Refuses multiplication/isize overflow, exhausted retention or allocation failure.
fn reserve_result(count: usize, work: &mut Budget) -> Result<Vec<RecordedCheckin>, StorageError> {
    let bytes = count
        .checked_mul(std::mem::size_of::<RecordedCheckin>())
        .filter(|bytes| *bytes <= isize::MAX as usize)
        .ok_or_else(|| StorageError::state("recorded timeline result capacity overflow"))?;
    work.charge_decoded(bytes)?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(count)
        .map_err(|error| StorageError::caused("recorded timeline result allocation", error))?;
    Ok(result)
}

/// Move metadata and identity into page order without cloning fields.
/// # Arguments
/// * `tx` - Same reserved transaction that completed all admission guards.
/// * `page` - Each authenticated entry of the page, newest first, and whether it is private.
/// * `limits` - Finite bound for each admitted metadata row.
/// * `work` - Same ledger used by authentication and history parsing.
/// # Returns
/// The rows of the page, in its order.
/// # Errors
/// Refuses capacity, metadata shape/cardinality, provider or work failure.
async fn project_history(
    tx: &mut WriteTransaction<'_>,
    page: Vec<(RecordedCommit, bool)>,
    limits: AdmissionLimits,
    work: &mut Budget,
) -> Result<Vec<RecordedCheckin>, StorageError> {
    let mut result = reserve_result(page.len(), work)?;
    for (commit, private) in page {
        result.push(project_commit(tx, commit, private, limits, work).await?);
    }
    Ok(result)
}

/// Read original metadata for exactly one authenticated RID and move its ownership.
/// # Arguments
/// * `tx` - Fully validated same-transaction native owner.
/// * `commit` - Existing authenticated RID/hash/date ownership.
/// * `private` - Visibility established by its position in authenticated history.
/// * `limits` - Caller metadata width ceiling, at least the complete native manifest width.
/// * `work` - Borrowed operation ledger; `bounded_rows` reserves and charges provider result ownership.
/// # Returns
/// One public row with all four variable-size fields moved rather than copied.
/// # Errors
/// Refuses missing/duplicate/non-text metadata, SQL, width or cumulative allocation bounds.
async fn project_commit(
    tx: &mut WriteTransaction<'_>,
    commit: RecordedCommit,
    private: bool,
    limits: AdmissionLimits,
    work: &mut Budget,
) -> Result<RecordedCheckin, StorageError> {
    // Charge the fixed argument vector before its allocation; bounded_rows also
    // retains its existing binding/clone/result charges on this same ledger.
    work.charge_decoded(std::mem::size_of::<Value>())?;
    let rows = bounded_rows(
        tx,
        "SELECT user,comment FROM event WHERE objid=?1",
        vec![Value::Integer(commit.rid)],
        1,
        limits.parser.input_bytes,
        work,
    )
    .await?;
    let (actor, comment) = take_metadata(rows)?;
    Ok(RecordedCheckin {
        hash: commit.hash,
        date: commit.date,
        actor,
        comment,
        private,
    })
}

/// Consume exactly one original event row, preserving owned string allocations.
/// # Arguments
/// * `rows` - Bounded original event metadata for one authenticated RID.
/// # Returns
/// Exact user and comment bytes moved from provider-owned strings.
/// # Errors
/// Refuses missing/duplicate rows and any shape or storage class other than two Text cells.
fn take_metadata(rows: Vec<Vec<Value>>) -> Result<(Vec<u8>, Vec<u8>), StorageError> {
    let mut rows = rows.into_iter();
    let row = rows
        .next()
        .ok_or_else(|| StorageError::state("recorded event metadata unavailable"))?;
    if rows.next().is_some() || row.len() != 2 {
        return Err(StorageError::state("recorded event metadata shape"));
    }
    let mut cells = row.into_iter();
    match (cells.next(), cells.next()) {
        (Some(Value::Text(actor)), Some(Value::Text(comment))) => {
            Ok((actor.into_bytes(), comment.into_bytes()))
        }
        _ => Err(StorageError::state(
            "recorded event metadata must be original text",
        )),
    }
}

/// Withhold every public row until the recorded operation and checked commit succeed.
/// # Arguments
/// * `tx` - Reserved transaction whose existing latch owns first-error precedence.
/// * `result` - Complete projection or original operation failure.
/// # Returns
/// Complete projection after checked commit and idle restoration.
/// # Errors
/// Preserves the first operation cause and separate cleanup error, or commit failure.
async fn complete_read(
    mut tx: WriteTransaction<'_>,
    result: Result<Vec<RecordedCheckin>, StorageError>,
) -> Result<Vec<RecordedCheckin>, StorageError> {
    let result = tx.record(result);
    tx.commit().await?;
    result
}

#[cfg(test)]
/// Exercise ownership, independent accounting and checked transaction completion.
mod tests {
    use super::super::ParseWork;
    use super::*;
    use crate::artifact::{ParseLimits, ParseUsage};
    use crate::storage::{
        InitializeOptions, Limits, LocalStore, PathScope, PathScopeSelection,
        PrivateRecordingOutcome, PrivateRecordingRequest, TrackingLimits,
    };

    /// Supply generous finite native and aggregate parser bounds for independent fixtures.
    /// # Returns
    /// Explicit limits without unbounded fallback.
    fn bounds() -> AdmissionLimits {
        AdmissionLimits {
            content: Limits {
                max_stored_bytes: 1 << 20,
                max_output_bytes: 1 << 20,
                max_chain_depth: 128,
                max_total_stored_bytes: 64 << 20,
                max_total_decoded_bytes: 256 << 20,
            },
            parser: ParseLimits {
                input_bytes: 4 << 20,
                cards: 65536,
                owned_bytes: 16 << 20,
                entries: 8192,
            },
        }
    }

    /// Build accepted S-only or repeated-private history with known non-ASCII metadata.
    /// # Arguments
    /// * `commits` - Zero through two strictly later private recordings.
    /// # Returns
    /// Disposable directory owner and admitted repository retained for the test.
    /// # Panics
    /// Panics on accepted producer or filesystem failure.
    async fn fixture(commits: usize) -> (tempfile::TempDir, LocalOwnerRepository) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let limits = bounds();
        let mut owner = LocalOwnerRepository::initialize(
            temp.path().join("repo"),
            InitializeOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "owner",
                date: "2026-11-20T00:00:00",
                user: "Róot 雪".as_bytes(),
                password: "secret",
                sha1: false,
                content_limits: limits.content,
                parse_limits: limits.parser,
            },
        )
        .await
        .unwrap();
        let mut view = owner
            .register_checkout("work", &root, limits)
            .await
            .unwrap();
        let tracking = TrackingLimits {
            paths: 16,
            path_bytes: 4096,
            components: 16,
            retained_bytes: 1 << 20,
            io_buffer_bytes: 1024,
        };
        if commits > 0 {
            std::fs::write(root.join("a"), b"first").unwrap();
            view = owner
                .add_files("work", &["a"], &view, limits, tracking)
                .await
                .unwrap();
            view = owner
                .select_path_scopes(
                    "work",
                    &view,
                    &[PathScopeSelection {
                        path: "a".into(),
                        scope: PathScope::Private,
                    }],
                    limits,
                    tracking,
                )
                .await
                .unwrap();
        }
        for (index, date) in [
            b"2026-11-20T00:00:01".as_slice(),
            b"2026-11-20T00:00:02".as_slice(),
        ]
        .into_iter()
        .take(commits)
        .enumerate()
        {
            std::fs::write(root.join("a"), format!("recorded {index}")).unwrap();
            view = require_recorded(
                owner
                    .commit_private_tracked_files(
                        "work",
                        PrivateRecordingRequest {
                            expected: &view,
                            paths: &["a"],
                            scope: PathScope::Private,
                            actor: "User 雪".as_bytes(),
                            comment: "quote \" \\ newline\n".as_bytes(),
                            date,
                        },
                        limits,
                        tracking,
                    )
                    .await
                    .unwrap(),
            );
        }
        (temp, owner)
    }

    /// Require the fixture producer to create a real changed recording.
    /// # Arguments
    /// * `outcome` - Actual accepted producer outcome.
    /// # Returns
    /// Refreshed view only for a changed native recording.
    /// # Panics
    /// Panics on NoChange, which cannot establish the expected fixture ancestry.
    fn require_recorded(outcome: PrivateRecordingOutcome) -> crate::storage::CheckoutView {
        match outcome {
            PrivateRecordingOutcome::Recorded { view, .. } => view,
            PrivateRecordingOutcome::NoChange { .. } => panic!("changed recording required"),
        }
    }

    /// Decode one exact nullable native byte-cast result for the accounting oracle.
    /// # Arguments
    /// * `row` - Provider-owned singleton CAST(... AS BLOB) row.
    /// # Returns
    /// Byte length, or zero for SQL NULL.
    /// # Panics
    /// Panics on any unexpected cardinality or non-BLOB/non-NULL storage class.
    fn native_byte_length(row: &[Value]) -> usize {
        match row {
            [Value::Blob(bytes)] => bytes.len(),
            [Value::Null] => 0,
            _ => panic!("native byte cast required"),
        }
    }

    /// Decode the exact native content name and integer RID used by the parser oracle.
    /// # Arguments
    /// * `row` - Native identity query result with two exact storage classes.
    /// # Returns
    /// Borrowed full name and integer RID without coercion.
    /// # Panics
    /// Panics on missing/extra columns or any unexpected storage class.
    fn native_identity(row: &[Value]) -> (&str, i64) {
        match row {
            [Value::Text(hash), Value::Integer(rid)] => (hash, *rid),
            _ => panic!("fixture identity shape"),
        }
    }

    #[tokio::test]
    #[should_panic(expected = "changed recording required")]
    /// Reject a real accepted NoChange producer outcome as a changed-history fixture.
    /// # Panics
    /// The expected panic proves the fixture oracle refuses an unchanged recording.
    async fn fixture_oracle_refuses_real_nochange() {
        let (temp, mut owner) = fixture(1).await;
        let view = owner.checkout_view("work", bounds()).await.unwrap();
        assert_eq!(
            std::fs::read(temp.path().join("root/a")).unwrap(),
            b"recorded 0"
        );
        let outcome = owner
            .commit_private_tracked_files(
                "work",
                PrivateRecordingRequest {
                    expected: &view,
                    paths: &["a"],
                    scope: PathScope::Private,
                    actor: b"Owner",
                    comment: b"unchanged",
                    date: b"2026-11-20T00:00:02",
                },
                bounds(),
                TrackingLimits {
                    paths: 16,
                    path_bytes: 4096,
                    components: 16,
                    retained_bytes: 1 << 20,
                    io_buffer_bytes: 1024,
                },
            )
            .await
            .unwrap();
        require_recorded(outcome);
    }

    #[test]
    /// Refuse malformed byte-cast and identity rows instead of undercounting oracle work.
    /// # Panics
    /// Panics if a wrong storage class/cardinality is accepted or the intended cause changes.
    fn fixture_oracles_refuse_unexpected_storage_classes_and_row_shapes() {
        for row in [
            vec![],
            vec![Value::Integer(3)],
            vec![Value::Text("bytes".into())],
            vec![Value::Null, Value::Null],
        ] {
            let cause = std::panic::catch_unwind(|| native_byte_length(&row)).unwrap_err();
            assert_eq!(
                cause.downcast_ref::<&str>().copied(),
                Some("native byte cast required")
            );
        }
        for row in [
            vec![],
            vec![Value::Text("hash".into())],
            vec![Value::Blob(vec![1]), Value::Integer(1)],
            vec![Value::Text("hash".into()), Value::Text("1".into())],
            vec![Value::Text("hash".into()), Value::Integer(1), Value::Null],
        ] {
            let cause = std::panic::catch_unwind(|| native_identity(&row)).unwrap_err();
            assert_eq!(
                cause.downcast_ref::<&str>().copied(),
                Some("fixture identity shape")
            );
        }
    }

    #[test]
    /// Charge exact result slots cumulatively and reject a one-under or overflowing reserve.
    /// # Panics
    /// Panics if ownership capacity can be reserved without its full prior debit.
    fn result_capacity_exact_one_under_overflow_and_zero() {
        let slot = std::mem::size_of::<RecordedCheckin>();
        for count in [1, 3] {
            let mut limits = bounds().content;
            limits.max_total_decoded_bytes = (17 + count * slot) as u64;
            let mut exact = Budget::new(limits).unwrap();
            exact.charge_decoded(17).unwrap();
            let rows = reserve_result(count, &mut exact).unwrap();
            assert!(rows.capacity() >= count);
            assert!(rows.is_empty());
            assert!(exact.charge_decoded(1).is_err());
            limits.max_total_decoded_bytes -= 1;
            let mut short = Budget::new(limits).unwrap();
            short.charge_decoded(17).unwrap();
            assert!(reserve_result(count, &mut short).is_err());
        }
        let mut work = Budget::new(bounds().content).unwrap();
        assert_eq!(reserve_result(0, &mut work).unwrap().capacity(), 0);
        assert!(reserve_result(usize::MAX, &mut work).is_err());
        assert!(reserve_result(isize::MAX as usize / slot + 1, &mut work).is_err());
    }

    #[test]
    /// Move exact metadata allocations while refusing missing/duplicate/wrong-class cells.
    /// # Panics
    /// Panics on cloning, byte changes or permissive shape/type conversion.
    fn metadata_ownership_and_shape_are_exact() {
        let actor = String::from("User 雪\n");
        let comment = String::from("quote \" \\ \t");
        let actor_pointer = actor.as_ptr();
        let comment_pointer = comment.as_ptr();
        let (actor, comment) =
            take_metadata(vec![vec![Value::Text(actor), Value::Text(comment)]]).unwrap();
        assert_eq!(actor.as_ptr(), actor_pointer);
        assert_eq!(comment.as_ptr(), comment_pointer);
        assert_eq!(actor, "User 雪\n".as_bytes());
        assert_eq!(comment, b"quote \" \\ \t");
        for rows in [
            vec![],
            vec![vec![]],
            vec![vec![Value::Null, Value::Text("c".into())]],
            vec![vec![Value::Text("u".into()), Value::Blob(vec![99])]],
            vec![vec![
                Value::Text("u".into()),
                Value::Text("c".into()),
                Value::Null,
            ]],
            vec![vec![Value::Text("u".into()), Value::Text("c".into())]; 2],
        ] {
            assert!(take_metadata(rows).is_err());
        }
    }

    #[tokio::test]
    /// Independently derive repeated metadata ownership and result capacity exact/one-under bounds.
    /// # Panics
    /// Panics if per-RID queries reset work, undercharge ownership or reject the exact contract cost.
    async fn metadata_and_result_costs_are_cumulative_and_independent() {
        for commits in [0, 2] {
            let (_temp, mut owner) = fixture(commits).await;
            let limits = bounds();
            let mut tx = owner.store.begin_write().await.unwrap();
            // Accepted bounded_rows protocol: numeric bind slots cost 32 twice; sizing
            // SQL +64, +512 per selected row +128, and the retained user and comment
            // bytes. The reader also precharges one Value argument slot and its public
            // result slots. No charge depends on the size of the repository.
            let per_row = std::mem::size_of::<Value>()
                + 2 * 32
                + "SELECT count(*) FROM (SELECT user,comment FROM event WHERE objid=?1)".len()
                + 64
                + 512
                + 128;
            let mut retained = 0;
            for column in ["user", "comment"] {
                for row in tx
                    .query(&format!(
                        "SELECT CAST({column} AS BLOB) FROM event WHERE type='ci'"
                    ))
                    .await
                    .unwrap()
                {
                    retained += native_byte_length(&row);
                }
            }
            let count = commits + 1;
            let exact = count * (per_row + std::mem::size_of::<RecordedCheckin>()) + retained;
            for allowance in [exact, exact - 1] {
                let mut full_work = Budget::new(limits.content).unwrap();
                let mut parse = ParseWork::new(limits.parser).unwrap();
                let history = tx
                    .validate_recording_repository_history(
                        &owner.principal,
                        limits,
                        &mut full_work,
                        &mut parse,
                    )
                    .await
                    .unwrap()
                    .unwrap();
                let hash_pointer = history.shared.hash.as_ptr();
                let date_pointer = history.shared.date.as_ptr();
                let mut content = limits.content;
                content.max_total_decoded_bytes = allowance as u64;
                let mut work = Budget::new(content).unwrap();
                let page = history
                    .commits
                    .into_iter()
                    .rev()
                    .map(|commit| (commit, true))
                    .chain([(history.shared, false)])
                    .collect();
                let result = project_history(&mut tx, page, limits, &mut work).await;
                if allowance == exact {
                    let result = result.unwrap();
                    assert_eq!(result.len(), count);
                    assert_eq!(result.last().unwrap().hash.as_ptr(), hash_pointer);
                    assert_eq!(result.last().unwrap().date.as_ptr(), date_pointer);
                    assert!(work.charge_decoded(1).is_err());
                } else {
                    assert!(result.is_err());
                }
            }
            tx.commit().await.unwrap();
        }
    }

    /// Sum parser-reported native usage independently of the new admission-return helper.
    /// # Arguments
    /// * `tx` - Fixture transaction owning native date validation.
    /// * `principal` - Fixture local principal for independent content reads.
    /// # Returns
    /// Exact expected root-twice plus private-once structural parser usage.
    /// # Panics
    /// Panics on native reads, parser or fixture identity failures.
    async fn expected_parse_usage(
        tx: &mut WriteTransaction<'_>,
        principal: &super::super::Principal,
    ) -> ParseUsage {
        let limits = bounds();
        let mut work = Budget::new(limits.content).unwrap();
        let mut total = ParseUsage {
            input_bytes: 0,
            cards: 0,
            owned_bytes: 0,
            entries: 0,
        };
        let names = tx.query("SELECT uuid,event.objid FROM blob JOIN event ON event.objid=blob.rid WHERE event.type='ci' ORDER BY event.objid").await.unwrap();
        for row in names {
            let (hash, rid) = native_identity(&row);
            let bytes = tx
                .read_recording_content_budget(principal, hash, &mut work)
                .await
                .unwrap();
            let (_, usage) = crate::artifact::parse_artifact_with_usage(
                &bytes,
                limits.parser,
                &super::super::OwnerDates(tx),
            )
            .await
            .unwrap();
            let repeats = if rid == 1 { 2 } else { 1 };
            total.input_bytes += usage.input_bytes * repeats;
            total.cards += usage.cards * repeats;
            total.owned_bytes += usage.owned_bytes * repeats;
            total.entries += usage.entries * repeats;
        }
        total
    }

    #[tokio::test]
    /// Preserve wrapper parser costs and debit root twice/private once on the same ledger.
    /// # Panics
    /// Panics if a helper resets parser work, changes legacy charges or accepts one-under totals.
    async fn complete_admission_parser_totals_and_wrapper_are_unchanged() {
        for commits in [0, 2] {
            let (_temp, mut owner) = fixture(commits).await;
            let mut tx = owner.store.begin_write().await.unwrap();
            let expected = expected_parse_usage(&mut tx, &owner.principal).await;
            let exact = ParseLimits {
                input_bytes: expected.input_bytes,
                cards: expected.cards,
                owned_bytes: expected.owned_bytes,
                entries: expected.entries,
            };
            for history_return in [false, true] {
                let mut work = Budget::new(bounds().content).unwrap();
                let mut parse = ParseWork::new(exact).unwrap();
                if history_return {
                    assert!(
                        tx.validate_recording_repository_history(
                            &owner.principal,
                            bounds(),
                            &mut work,
                            &mut parse
                        )
                        .await
                        .unwrap()
                        .is_some()
                    );
                } else {
                    tx.validate_recording_repository_history(
                        &owner.principal,
                        bounds(),
                        &mut work,
                        &mut parse,
                    )
                    .await
                    .unwrap();
                }
                assert_eq!(parse.remaining.input_bytes, 0);
                assert_eq!(parse.remaining.cards, 0);
                assert_eq!(parse.remaining.owned_bytes, 0);
                assert_eq!(parse.remaining.entries, 0);
            }
            tx.commit().await.unwrap();
            for axis in 0..4 {
                let mut parser = exact;
                match axis {
                    0 => parser.input_bytes -= 1,
                    1 => parser.cards -= 1,
                    2 => parser.owned_bytes -= 1,
                    _ => parser.entries -= 1,
                }
                let mut tx = owner.store.begin_write().await.unwrap();
                let mut work = Budget::new(bounds().content).unwrap();
                let mut parse = ParseWork::new(parser).unwrap();
                let result = tx
                    .validate_recording_repository_history(
                        &owner.principal,
                        bounds(),
                        &mut work,
                        &mut parse,
                    )
                    .await;
                assert!(result.is_err(), "axis {axis}");
                assert!(tx.commit().await.is_err());
            }
        }
    }

    #[tokio::test]
    /// A page walk reads only its count, and the generation bounds every walk.
    ///
    /// A walk refuses as soon as it passes the generation. A walk that reaches the
    /// shared root also refuses a chain shorter than the generation. A page that
    /// stops before the root checks only the manifests that it reads.
    /// # Panics
    /// Panics if a walk reads past its page or admits a chain that is not the generation.
    async fn page_walks_read_their_count_and_the_generation_bounds_each_walk() {
        let (_temp, mut owner) = fixture(2).await;
        let limits = bounds();
        let mut tx = owner.store.begin_write().await.unwrap();
        let rows = tx
            .query("SELECT shared_head,0 FROM g_checkout")
            .await
            .unwrap();
        let shared = native_identity(&rows[0]).0.to_string();
        let mismatch = "recording generation/history mismatch";
        for (generation, reach, expected) in [
            (2, Reach::Manifests(1, usize::MAX), Some((1, false))),
            (2, Reach::Manifests(2, usize::MAX), Some((2, true))),
            (2, Reach::Manifests(usize::MAX, usize::MAX), Some((2, true))),
            (2, Reach::Parent, Some((2, true))),
            (2, Reach::Proof, Some((2, true))),
            (1, Reach::Manifests(1, usize::MAX), Some((1, false))),
            (1, Reach::Manifests(2, usize::MAX), None),
            (1, Reach::Parent, None),
            (1, Reach::Proof, None),
            (3, Reach::Manifests(1, usize::MAX), Some((1, false))),
            (3, Reach::Manifests(2, usize::MAX), None),
            (3, Reach::Manifests(3, usize::MAX), None),
            (3, Reach::Parent, None),
            (3, Reach::Proof, None),
        ] {
            tx.execute_bound(
                "UPDATE g_checkout SET generation=?1,pending_version=max(pending_version,?1)",
                vec![Value::Integer(generation)],
            )
            .await
            .unwrap();
            let mut work = Budget::new(limits.content).unwrap();
            let mut parse = ParseWork::per_artifact(limits.parser).unwrap();
            let walk = tx
                .walk_recorded_history(
                    &owner.principal,
                    &shared,
                    limits,
                    &mut work,
                    &mut parse,
                    reach,
                )
                .await;
            match expected {
                Some((count, complete)) => {
                    let walk = walk.unwrap();
                    assert_eq!(
                        (walk.commits.len(), walk.complete),
                        (count, complete),
                        "{generation} {reach:?}"
                    );
                }
                None => assert_eq!(
                    walk.err().unwrap().operation,
                    mismatch,
                    "{generation} {reach:?}"
                ),
            }
        }
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    /// A page keeps at most its ceiling of manifest bytes and parses, and its first
    /// manifest does not count toward that ceiling. With a ceiling of the exact cost
    /// of the second manifest, a page of two manifests passes. With one byte less, it
    /// refuses with the message `PAGE_TOO_LARGE`.
    /// # Panics
    /// Panics if the boundary or the message differs.
    async fn a_page_keeps_at_most_its_ceiling_of_manifests() {
        let (_temp, mut owner) = fixture(2).await;
        let limits = bounds();
        let mut tx = owner.store.begin_write().await.unwrap();
        let rows = tx
            .query("SELECT shared_head,0 FROM g_checkout")
            .await
            .unwrap();
        let shared = native_identity(&rows[0]).0.to_string();
        let mut walk = async |count: usize, ceiling: usize| {
            let mut work = Budget::new(limits.content).unwrap();
            let mut parse = ParseWork::per_artifact(limits.parser).unwrap();
            tx.walk_recorded_history(
                &owner.principal,
                &shared,
                limits,
                &mut work,
                &mut parse,
                Reach::Manifests(count, ceiling),
            )
            .await
            .map(|walk| {
                walk.commits
                    .iter()
                    .map(|commit| commit.0.hash.clone())
                    .collect::<Vec<_>>()
            })
            .map_err(|error| error.operation)
        };
        let page = super::super::recorded_history::PAGE_TOO_LARGE;
        // The first manifest is kept over any ceiling, the second is not.
        assert_eq!(walk(1, 0).await.unwrap().len(), 1);
        assert_eq!(walk(2, 0).await, Err(page.to_owned()));
        // Find the least ceiling that holds two manifests.
        let (mut refused, mut held) = (0, 1 << 24);
        let both = walk(2, held).await.unwrap();
        assert_eq!(both.len(), 2);
        while held - refused > 1 {
            let middle = (refused + held) / 2;
            match walk(2, middle).await {
                Ok(_) => held = middle,
                Err(error) => {
                    assert_eq!(error, page);
                    refused = middle;
                }
            }
        }
        assert_eq!(walk(2, held).await, Ok(both.clone()));
        assert_eq!(walk(2, held - 1).await, Err(page.to_owned()));
        assert_eq!(walk(2, usize::MAX).await, Ok(both.clone()));
        // The ceiling counts exactly the second manifest, the older one, and its
        // parse, which owns more than its bytes. It does not count the newest one.
        let (_, _, older) = tx
            .recorded_content(
                &owner.principal,
                &both[0],
                &mut Budget::new(limits.content).unwrap(),
            )
            .await
            .unwrap();
        let mut parse = ParseWork::per_artifact(limits.parser).unwrap();
        let mut work = Budget::new(limits.content).unwrap();
        super::super::recorded_history::parse_recording_budgeted(
            &mut tx, &older, &mut parse, &mut work,
        )
        .await
        .unwrap();
        assert_eq!(held, older.len() + parse.owned());
        assert!(held > 2 * older.len(), "{held} {}", older.len());
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    /// Withhold results on commit failure and preserve original causes through cleanup failure.
    /// # Panics
    /// Panics if output escapes failed completion or cleanup replaces the first failure.
    async fn checked_completion_and_failure_latch_preserve_first_cause() {
        use std::error::Error;
        let temp = tempfile::tempdir().unwrap();
        let mut store = LocalStore::open(temp.path().join("db")).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.fail_control = Some("COMMIT");
        assert!(complete_read(tx, Ok(Vec::new())).await.is_err());
        assert!(!store.is_poisoned());
        let mut tx = store.begin_write().await.unwrap();
        tx.fail_control = Some("ROLLBACK");
        let error = StorageError::caused(
            "metadata provider failed",
            std::io::Error::other("original fixture cause"),
        );
        let error = complete_read(tx, Err(error)).await.unwrap_err();
        assert_eq!(error.operation, "metadata provider failed");
        assert_eq!(
            error.source().unwrap().to_string(),
            "original fixture cause"
        );
        assert!(error.cleanup.is_some());
        assert!(store.is_poisoned());
    }

    #[tokio::test]
    /// Keep unregistered None valid for legacy admission while the public reader refuses it.
    /// # Panics
    /// Panics if the helper invents history or a missing checkout becomes an empty result.
    async fn unregistered_repository_is_valid_but_has_no_selected_timeline() {
        let temp = tempfile::tempdir().unwrap();
        let limits = bounds();
        let mut owner = LocalOwnerRepository::initialize(
            temp.path().join("repo"),
            InitializeOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "owner",
                date: "2026-11-20T00:00:00",
                user: b"Owner",
                password: "secret",
                sha1: false,
                content_limits: limits.content,
                parse_limits: limits.parser,
            },
        )
        .await
        .unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        let mut work = Budget::new(limits.content).unwrap();
        let mut parse = ParseWork::new(limits.parser).unwrap();
        assert!(
            tx.validate_recording_repository_history(
                &owner.principal,
                limits,
                &mut work,
                &mut parse
            )
            .await
            .unwrap()
            .is_none()
        );
        tx.commit().await.unwrap();
        assert!(owner.recorded_timeline("work", limits).await.is_err());
        assert!(!owner.store.is_poisoned());
    }

    #[tokio::test]
    /// Preserve a metadata SQL cause through a later projection failure and checked rollback.
    /// # Panics
    /// Panics if provider preparation errors lose their source or partial output escapes.
    async fn metadata_provider_failure_latches_before_projection_completion() {
        use std::error::Error;
        let temp = tempfile::tempdir().unwrap();
        let mut store = LocalStore::open(temp.path().join("db")).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let mut work = Budget::new(bounds().content).unwrap();
        let commit = RecordedCommit {
            rid: 1,
            hash: "0".repeat(64),
            date: b"2026-11-20T00:00:00".to_vec(),
            julian: 0.0,
            receipt: 1,
            files: vec![],
        };
        let first = project_commit(&mut tx, commit, false, bounds(), &mut work)
            .await
            .unwrap_err();
        assert!(first.source().is_some());
        let later = StorageError::state("later projection failure");
        let error = complete_read(tx, Err(later)).await.unwrap_err();
        assert_eq!(error.operation, first.operation);
        assert_eq!(
            error.source().unwrap().to_string(),
            first.source().unwrap().to_string()
        );
        assert!(!store.is_poisoned());
    }
}
