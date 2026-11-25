//! Read complete supported checkout ancestry after full catalog6 admission.
//!
//! Original event metadata is admitted against canonical native U/C cards before
//! its owned strings are moved into the result. Dates and hashes come directly
//! from authenticated history, never display projections or working files.
//!
//! ## Recommendations
//! Select an explicit existing checkout and supply finite whole-operation limits.
use super::recorded_history::{RecordedCommit, RecordedHistory, bounded_rows};
use super::{AdmissionLimits, LocalOwnerRepository, ParseWork, StorageError};
use crate::storage::{WriteTransaction, content::Budget};
use libsql::Value;

/// One authenticated native check-in in a complete current-first checkout ancestry.
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
    /// The current private check-in comes first and the immutable shared empty
    /// root comes last. Before the first private recording, that root is the sole
    /// result. Pending paths, dirty/deleted working files and filesystem contents
    /// do not affect recorded history. This performs no durable mutation and
    /// publishes no locator. A single content and parser ledger covers freshness,
    /// full admission, metadata ownership and result retention in one transaction.
    /// # Arguments
    /// * `checkout` - Existing nonempty NUL-free registration ID, at most 1024 bytes and parser input bytes.
    /// * `limits` - Explicit finite cumulative content, metadata and parser ceilings.
    /// # Returns
    /// Complete authenticated current-first ancestry including the shared root,
    /// only after checked transaction completion; never a truncated or empty history.
    /// # Errors
    /// Refuses missing/foreign checkout, stale authority, unsupported or corrupt
    /// state, invalid/exhausted limits, allocation/provider and checked cleanup errors.
    /// Existing operation causes and separate cleanup failures are preserved.
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
        if checkout.is_empty()
            || checkout.contains('\0')
            || checkout.len() > 1024.min(limits.parser.input_bytes)
        {
            return Err(StorageError::state("checkout identity limit or spelling"));
        }
        let mut work = Budget::new(limits.content)?;
        let mut parse = ParseWork::new(limits.parser)?;
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission_budgeted(&self.principal, &self.admission, &mut work).await?;
            let history = tx.validate_recording_repository_history(&self.principal, limits, &mut work, &mut parse).await?
                .ok_or_else(|| StorageError::state("recorded checkout unavailable"))?;
            let selections = bounded_rows(&mut tx, "SELECT checkout_id FROM g_checkout", vec![], 1, limits.parser.input_bytes, &mut work).await?;
            if !matches!(selections.as_slice(), [row] if matches!(row.as_slice(), [Value::Text(id)] if id == checkout)) {
                return Err(StorageError::state("recorded checkout unavailable"));
            }
            project_history(&mut tx, history, limits, &mut work).await
        }.await;
        complete_read(tx, result).await
    }
}

/// Charge and reserve the complete public result capacity before retaining rows.
/// # Arguments
/// * `count` - Complete supported ancestry length, including the shared root.
/// * `work` - Borrowed whole-operation content/ownership ledger.
/// # Returns
/// Empty vector with sufficient capacity for the full result.
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

/// Move metadata and identity into complete ancestry order without cloning fields.
/// # Arguments
/// * `tx` - Same reserved transaction that completed all admission guards.
/// * `history` - Authenticated complete history, oldest private ancestor first.
/// * `limits` - Finite bound for each admitted metadata row.
/// * `work` - Same cumulative ledger used by authentication and history parsing.
/// # Returns
/// Complete current-first result with S exactly once at the end.
/// # Errors
/// Refuses capacity, metadata shape/cardinality, provider or cumulative work failure.
async fn project_history(
    tx: &mut WriteTransaction<'_>,
    history: RecordedHistory,
    limits: AdmissionLimits,
    work: &mut Budget,
) -> Result<Vec<RecordedCheckin>, StorageError> {
    let count = history
        .commits
        .len()
        .checked_add(1)
        .ok_or_else(|| StorageError::state("recorded timeline length overflow"))?;
    let mut result = reserve_result(count, work)?;
    for commit in history.commits.into_iter().rev() {
        result.push(project_commit(tx, commit, true, limits, work).await?);
    }
    result.push(project_commit(tx, history.shared, false, limits, work).await?);
    Ok(result)
}

/// Read original metadata for exactly one authenticated RID and move its ownership.
/// # Arguments
/// * `tx` - Fully validated same-transaction native owner.
/// * `commit` - Existing authenticated RID/hash/date ownership.
/// * `private` - Visibility established by its position in authenticated history.
/// * `limits` - Caller metadata width ceiling, at least the complete native manifest width.
/// * `work` - Borrowed operation ledger; bounded_rows precharges provider result ownership.
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

    /// Independently inventory provider byte lengths for every variable field in its text allowance.
    /// # Arguments
    /// * `tx` - Fixture transaction before the metadata projection budget starts.
    /// # Returns
    /// Exact whole-family UTF-8/native byte count, including SQL numeric-to-byte casts.
    /// # Panics
    /// Panics on provider failure or a non-BLOB/non-NULL cast result.
    async fn text_inventory(tx: &mut WriteTransaction<'_>) -> usize {
        let surfaces: &[(&str, &[&str])] = &[
            ("blob", &["uuid"]),
            (
                "event",
                &[
                    "type", "user", "comment", "euser", "ecomment", "bgcolor", "brief",
                ],
            ),
            ("filename", &["name"]),
            ("tag", &["tagname"]),
            ("tagxref", &["value"]),
            ("g_vfile", &["checkout_id", "pathname"]),
            ("g_path_scope", &["checkout_id", "path", "scope"]),
            ("rcvfrom", &["uid", "mtime", "nonce", "ipaddr"]),
            ("g_vvar", &["checkout_id", "name", "value"]),
            ("config", &["name", "value"]),
            (
                "g_checkout",
                &[
                    "checkout_id",
                    "root",
                    "shared_head",
                    "combined_head",
                    "availability",
                ],
            ),
            (
                "g_checkout_owner",
                &["checkout_id", "owner_id", "root_device", "root_inode"],
            ),
        ];
        let mut total = 0;
        for (table, columns) in surfaces {
            for column in *columns {
                for row in tx
                    .query(&format!("SELECT CAST({column} AS BLOB) FROM {table}"))
                    .await
                    .unwrap()
                {
                    total += native_byte_length(&row);
                }
            }
        }
        total
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
            let text = text_inventory(&mut tx).await;
            // Accepted bounded_rows protocol: numeric bind slots cost32 twice;
            // sizing SQL +64, literal full-family SQL1771+64, provider ownership
            // 2*all-native-text +512 per selected row +128. The new reader also
            // precharges one Value argument slot and its public result slots.
            let per_row = std::mem::size_of::<Value>()
                + 2 * 32
                + "SELECT count(*) FROM (SELECT user,comment FROM event WHERE objid=?1)".len()
                + 64
                + 1771
                + 64
                + 2 * text
                + 512
                + 128;
            let count = commits + 1;
            let exact = count * (per_row + std::mem::size_of::<RecordedCheckin>());
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
                let result = project_history(&mut tx, history, limits, &mut work).await;
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
                    tx.validate_recording_repository(
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
