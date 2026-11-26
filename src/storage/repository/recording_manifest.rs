//! Construct full manifests for the bounded private recorder without accessing storage.
//!
//! Parent and branch fields are borrowed facts authenticated by the private
//! coordinator. This module cannot authenticate them, select scopes, classify
//! content, inspect paths on disk, or mutate native state. Tracking admission
//! remains responsible for the complete canonical path namespace.
//!
//! Comments deliberately exclude every literal `[` in decoded bytes. This narrow
//! grammar keeps native wiki TOKEN_LINK/backlink extraction empty; it is not a
//! claim of full Fossil comment support or an approximate hexadecimal-link filter.
//!
//! ## Recommendations
//! Wire this private child only through authenticated recording admission, and
//! qualify its source controls with the complete shared operation budget.

use crate::storage::{StorageError, content::Budget};

/// Borrow the native name and date of an already authenticated parent.
#[derive(Clone, Copy)]
pub(super) struct ManifestParent<'a> {
    /// Full canonical native name; syntax checks do not establish authority.
    pub(super) hash: &'a str,
    /// Exact date token obtained from the authenticated parent artifact.
    pub(super) date: &'a [u8],
}

/// Select the first or later history shape; S always occupies the final parent slot.
#[derive(Clone, Copy)]
pub(super) enum PrivateParents<'a> {
    /// First L uses exactly P S.
    First {
        /// Authenticated immutable public root, the sole first-recording parent.
        shared: ManifestParent<'a>,
    },
    /// Later L uses exactly P Lprev S, with distinct native identities.
    Later {
        /// Authenticated preceding private manifest, placed first.
        previous: ManifestParent<'a>,
        /// Authenticated immutable public root, placed last.
        shared: ManifestParent<'a>,
    },
}

/// Describe one file in the complete tree; its opaque payload may be binary or empty.
#[derive(Clone, Copy)]
pub(super) struct ManifestFile<'a> {
    /// Canonical namespace already validated by shared tracking admission.
    pub(super) path: &'a str,
    /// Actual native content name authenticated against the captured bytes.
    pub(super) hash: &'a str,
    /// Native mode: zero regular, one executable; all others are unsupported.
    pub(super) mode: i64,
}

/// Collect inputs for the private coordinator; constructing this value grants no authority.
#[derive(Clone, Copy)]
pub(super) struct PrivateManifestSpec<'a> {
    /// Explicit authenticated first/later parent facts.
    pub(super) parents: PrivateParents<'a>,
    /// Complete bytewise strictly ordered F tree; never a selected-path subset.
    pub(super) files: &'a [ManifestFile<'a>],
    /// Decoded bounded UTF-8 actor, nonempty and NUL-free.
    pub(super) actor: &'a [u8],
    /// Decoded bounded UTF-8 comment, NUL-free and left-bracket-free.
    pub(super) comment: &'a [u8],
    /// Supported native date token, preserved exactly and later than both parents.
    pub(super) date: &'a [u8],
    /// Authenticated private branch derived from persisted native server-code.
    pub(super) branch: &'a str,
}

/// Check canonical native output names without authenticating their content.
/// The parser's hash helper is private; this mirrors its 40/64 syntax narrowed
/// to the lowercase spelling emitted by existing native content-name helpers.
/// # Arguments
/// * `name` - Candidate native content name or branch suffix.
/// # Returns
/// Whether the name has 40 or 64 lowercase hexadecimal digits.
fn canonical_name(name: &str) -> bool {
    matches!(name.len(), 40 | 64)
        && name
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Accumulate logical byte counts with overflow checks before allocating output or tokens.
/// # Arguments
/// * `total` - Running byte count to update after the addition succeeds.
/// * `count` - Additional bytes to include.
/// # Returns
/// Unit after updating the total.
/// # Errors
/// Refuses arithmetic overflow without replacing the prior total.
fn add(total: &mut usize, count: usize) -> Result<(), StorageError> {
    *total = total
        .checked_add(count)
        .ok_or_else(|| StorageError::state("private manifest size overflow"))?;
    Ok(())
}

/// Validate and measure decoded token bytes without allocating an encoded token.
/// # Arguments
/// * `bytes` - Decoded UTF-8 token bytes to validate and measure.
/// * `max` - Maximum decoded and encoded token size in bytes.
/// # Returns
/// The encoded token size, without allocating its bytes.
/// # Errors
/// Refuses oversized, NUL-containing or invalid UTF-8 input and token-size overflow.
fn token_size(bytes: &[u8], max: usize) -> Result<usize, StorageError> {
    if bytes.len() > max || bytes.contains(&0) || std::str::from_utf8(bytes).is_err() {
        return Err(StorageError::state("private manifest UTF-8 token limit"));
    }
    crate::token_codec::fossilized_size(bytes, max)
        .map_err(|e| StorageError::caused("private manifest token size", e))
}

/// Emit one measured token; its temporary allocation was charged in preflight.
/// # Arguments
/// * `out` - Manifest buffer to receive the encoded token.
/// * `bytes` - Decoded token bytes whose encoding size was already checked.
/// # Returns
/// Unit after appending the encoded token to the buffer.
/// # Errors
/// Preserves token-encoding failures, including allocation and size failures.
fn token(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), StorageError> {
    let encoded = crate::token_codec::fossilize(bytes)
        .map_err(|e| StorageError::caused("private manifest token encoding", e))?;
    out.extend_from_slice(&encoded);
    Ok(())
}

/// Encode a complete native private manifest with exact parent ordering and Z.
///
/// # Arguments
/// * `spec` - Borrowed complete tree and privately authenticated parent/branch facts.
/// * `work` - The same caller-owned budget used for capture, content and later work.
/// * `max_manifest_bytes` - Finite output ceiling, checked before allocating.
/// # Returns
/// Canonical full manifest bytes, retaining the exact accepted date/comment bytes.
/// # Errors
/// Rejects malformed tokens, names, order, duplicates, modes, dates and capacity.
/// Invalid input does not charge work; valid input charges all planned output,
/// temporary token retention and checksum work before allocation. Later allocation
/// failure may consume budget; callers must abandon the failed whole operation.
/// No parent or branch authority is manufactured by successful syntax validation.
pub(super) fn build_private_manifest(
    spec: &PrivateManifestSpec<'_>,
    work: &mut Budget,
    max_manifest_bytes: usize,
) -> Result<Vec<u8>, StorageError> {
    let max = max_manifest_bytes.min(isize::MAX as usize);
    // Explicit grammar validation precedes all output allocation and budget mutation.
    if spec.comment.len() > max
        || spec.comment.contains(&b'[')
        || spec.actor.is_empty()
        || spec.files.is_empty()
        || spec.date.len() > 30
    {
        return Err(StorageError::state("unsupported private manifest grammar"));
    }
    let branch_suffix = spec
        .branch
        .strip_prefix("gossil-private/")
        .ok_or_else(|| StorageError::state("unsupported private branch"))?;
    if branch_suffix.len() != 40 || !canonical_name(branch_suffix) {
        return Err(StorageError::state("unsupported private branch"));
    }
    let date = crate::native_date::NativeDate::parse(spec.date)
        .map_err(|e| StorageError::caused("private manifest native date", e))?;
    let (primary, secondary) = match spec.parents {
        PrivateParents::First { shared } => (shared, None),
        PrivateParents::Later { previous, shared } => {
            if previous.hash == shared.hash {
                return Err(StorageError::state("duplicate private manifest parents"));
            }
            (previous, Some(shared))
        }
    };
    for parent in std::iter::once(primary).chain(secondary) {
        if !canonical_name(parent.hash) || parent.date.len() > 30 {
            return Err(StorageError::state("invalid private manifest parent"));
        }
        let parent_date = crate::native_date::NativeDate::parse(parent.date)
            .map_err(|e| StorageError::caused("private parent native date", e))?;
        if date <= parent_date {
            return Err(StorageError::state(
                "private manifest date must follow all parents",
            ));
        }
    }
    // Fixed C/D/P/T/U/Z card delimiters plus checksum digits are exact, not slack.
    let mut total = b"C \nD \nP \nT *branch * \nT *sym- *\nT -sym-trunk *\nU \nZ ".len();
    add(&mut total, 33)?; // 32 checksum digits and final newline.
    add(&mut total, spec.date.len())?;
    add(&mut total, primary.hash.len())?;
    if let Some(parent) = secondary {
        add(&mut total, 1)?;
        add(&mut total, parent.hash.len())?;
    }
    let mut temporary = 0;
    for bytes in [
        spec.comment,
        spec.actor,
        spec.branch.as_bytes(),
        spec.branch.as_bytes(),
    ] {
        let n = token_size(bytes, max)?;
        add(&mut total, n)?;
        add(&mut temporary, n)?;
    }
    let mut prior = None;
    for file in spec.files {
        if file.path.is_empty()
            || prior.is_some_and(|p: &str| p >= file.path)
            || !canonical_name(file.hash)
            || !matches!(file.mode, 0 | 1)
        {
            return Err(StorageError::state(
                "invalid private manifest file order/name/mode",
            ));
        }
        prior = Some(file.path);
        let n = token_size(file.path.as_bytes(), max)?;
        add(&mut total, n)?;
        add(&mut temporary, n)?;
        add(&mut total, b"F  \n".len())?;
        add(&mut total, file.hash.len())?;
        if file.mode == 1 {
            add(&mut total, 2)?;
        }
    }
    if total > max {
        return Err(StorageError::state(format!(
            "the check-in manifest of {} files needs {total} bytes, more than the {max} bytes of one manifest",
            spec.files.len()
        )));
    }
    // Account output, every temporary encoded token, checksum scan and its hex
    // allocation together before any allocation. Input retention belongs to capture.
    let mut charge = total;
    add(&mut charge, temporary)?;
    add(&mut charge, total - 35)?; // Z card is absent from its own MD5 input.
    add(&mut charge, 32)?;
    work.charge_decoded(charge)?;
    let mut out = Vec::new();
    out.try_reserve_exact(total)
        .map_err(|e| StorageError::caused("private manifest allocation", e))?;
    out.extend_from_slice(b"C ");
    token(&mut out, spec.comment)?;
    out.extend_from_slice(b"\nD ");
    out.extend_from_slice(spec.date);
    out.push(b'\n');
    for file in spec.files {
        out.extend_from_slice(b"F ");
        token(&mut out, file.path.as_bytes())?;
        out.push(b' ');
        out.extend_from_slice(file.hash.as_bytes());
        if file.mode == 1 {
            out.extend_from_slice(b" x");
        }
        out.push(b'\n');
    }
    out.extend_from_slice(b"P ");
    out.extend_from_slice(primary.hash.as_bytes());
    if let Some(parent) = secondary {
        out.push(b' ');
        out.extend_from_slice(parent.hash.as_bytes());
    }
    out.extend_from_slice(b"\nT *branch * ");
    token(&mut out, spec.branch.as_bytes())?;
    out.extend_from_slice(b"\nT *sym-");
    token(&mut out, spec.branch.as_bytes())?;
    out.extend_from_slice(b" *\nT -sym-trunk *\nU ");
    token(&mut out, spec.actor)?;
    out.push(b'\n');
    let checksum = crate::content_hash::hex(&crate::artifact_checksum::md5(&out));
    out.extend_from_slice(b"Z ");
    out.extend_from_slice(checksum.as_bytes());
    out.push(b'\n');
    require_planned_size(out.len(), total)?;
    Ok(out)
}

/// Verify that canonical emission used exactly its preflight allocation plan.
/// # Arguments
/// * `actual` - Complete emitted manifest length, including Z and final newline.
/// * `planned` - Exact length computed before allocation and charged to the ledger.
/// # Returns
/// Success only when the encoder and its allocation plan agree.
/// # Errors
/// Rejects both shorter and longer output as an encoder consistency failure.
fn require_planned_size(actual: usize, planned: usize) -> Result<(), StorageError> {
    if actual != planned {
        return Err(StorageError::state("private manifest preflight mismatch"));
    }
    Ok(())
}

#[cfg(test)]
/// Check exact private manifest bytes, validation and cumulative work bounds.
mod tests {
    use super::*;

    /// Supply the fixed shared-root identity for manifest controls.
    const SHARED: &str = "0000000000000000000000000000000000000000";
    /// Supply the fixed preceding private identity for later-parent controls.
    const PREVIOUS: &str = "2222222222222222222222222222222222222222";
    /// Supply a fixed content identity without needing a stored payload.
    const CONTENT: &str = "1111111111111111111111111111111111111111";
    /// Supply the owner-private branch token used by exact byte controls.
    const BRANCH: &str = "gossil-private/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    /// Supply the shared-root parent date for strict chronology controls.
    const OLD: &[u8] = b"2026-11-20T00:00:00";
    /// Supply a date strictly after the fixture parent.
    const NEW: &[u8] = b"2026-11-20T00:00:01";

    /// Construct finite accounting only in tests; production always borrows it.
    fn budget(total: u64) -> Budget {
        Budget::new(crate::storage::content::Limits {
            max_stored_bytes: 4096,
            max_output_bytes: 4096,
            max_chain_depth: 8,
            max_total_stored_bytes: 4096,
            max_total_decoded_bytes: total,
        })
        .unwrap()
    }

    /// Borrow a first-manifest fixture without manufacturing production authority.
    fn spec<'a>(files: &'a [ManifestFile<'a>]) -> PrivateManifestSpec<'a> {
        PrivateManifestSpec {
            parents: PrivateParents::First {
                shared: ManifestParent {
                    hash: SHARED,
                    date: OLD,
                },
            },
            files,
            actor: b"alice",
            comment: b"hello world",
            date: NEW,
            branch: BRANCH,
        }
    }

    /// Supply a complete tree with one regular file for the independent byte fixture.
    fn files() -> [ManifestFile<'static>; 1] {
        [ManifestFile {
            path: "a.txt",
            hash: CONTENT,
            mode: 0,
        }]
    }

    #[test]
    /// Encoder consistency rejects either direction of divergence from the charged plan.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after empty/exact boundaries and short/long mismatch controls.
    /// # Panics
    /// Panics if an encoder drift could escape its unchanged final guard.
    fn emitted_size_must_equal_the_charged_plan() {
        for size in [0, 1, usize::MAX] {
            require_planned_size(size, size).unwrap();
        }
        for (actual, planned) in [
            (0, 1),
            (1, 0),
            (usize::MAX - 1, usize::MAX),
            (usize::MAX, usize::MAX - 1),
        ] {
            assert_eq!(
                require_planned_size(actual, planned).unwrap_err().operation,
                "private manifest preflight mismatch"
            );
        }
    }

    #[test]
    /// Check the first private manifest spelling, checksum and exact byte ceiling.
    /// # Panics
    /// Panics if fixture setup fails or the asserted manifest contract differs.
    fn first_manifest_has_exact_native_bytes_checksum_and_exact_cap() {
        let files = files();
        let spec = spec(&files);
        let expected = concat!(
            "C hello\\sworld\nD 2026-11-20T00:00:01\n",
            "F a.txt 1111111111111111111111111111111111111111\n",
            "P 0000000000000000000000000000000000000000\n",
            "T *branch * gossil-private/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
            "T *sym-gossil-private/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa *\n",
            "T -sym-trunk *\nU alice\nZ 597e5d650f5c7713ab44236d0b9db2ce\n"
        )
        .as_bytes();
        // Fixed checksum independently calculated with Python hashlib, not this encoder.
        assert_eq!(
            build_private_manifest(&spec, &mut budget(10000), expected.len()).unwrap(),
            expected
        );
        assert!(build_private_manifest(&spec, &mut budget(10000), expected.len() - 1).is_err());
    }

    #[test]
    /// Check later parent order and the exact private branch tag set.
    /// # Panics
    /// Panics if fixture setup fails or the asserted manifest contract differs.
    fn later_manifest_has_exact_previous_then_shared_parents_and_three_tags() {
        let files = files();
        let mut spec = spec(&files);
        spec.parents = PrivateParents::Later {
            previous: ManifestParent {
                hash: PREVIOUS,
                date: OLD,
            },
            shared: ManifestParent {
                hash: SHARED,
                date: OLD,
            },
        };
        let bytes = build_private_manifest(&spec, &mut budget(10000), 4096).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert_eq!(
            text.lines()
                .filter(|s| s.starts_with("P "))
                .collect::<Vec<_>>(),
            vec![
                "P 2222222222222222222222222222222222222222 0000000000000000000000000000000000000000"
            ]
        );
        assert_eq!(text.lines().filter(|s| s.starts_with("T ")).count(), 3);
        let z = bytes.len() - 35;
        assert_eq!(
            &bytes[z + 2..bytes.len() - 1],
            crate::content_hash::hex(&crate::artifact_checksum::md5(&bytes[..z])).as_bytes()
        );
    }

    #[test]
    /// Check escaped metadata and both supported executable-mode spellings.
    /// # Panics
    /// Panics if fixture setup fails or the asserted manifest contract differs.
    fn quoted_tokens_regular_executable_and_binary_content_references() {
        // Content hashing accepts opaque binary/empty payloads; the encoder only
        // receives authenticated names, never treats file payloads as text.
        let binary = crate::content_hash::hex(&crate::content_hash::sha1(b"\0\xff\n"));
        let empty = crate::content_hash::hex(&crate::content_hash::sha3(b"", 256).unwrap());
        let files = [
            ManifestFile {
                path: "a space.txt",
                hash: &binary,
                mode: 0,
            },
            ManifestFile {
                path: "z.txt",
                hash: &empty,
                mode: 1,
            },
        ];
        let mut spec = spec(&files);
        spec.actor = b"a b\\c";
        spec.comment = "caf\u{e9}\tline\nslash\\".as_bytes();
        let bytes = build_private_manifest(&spec, &mut budget(10000), 4096).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.starts_with("C caf\u{e9}\\tline\\nslash\\\\\n"));
        assert!(text.contains(&format!("F a\\sspace.txt {binary}\n")));
        assert!(text.contains(&format!("F z.txt {empty} x\n")));
        assert!(text.contains("\nU a\\sb\\\\c\n"));
    }

    #[test]
    /// Reject malformed metadata and parents before reserving output work.
    /// # Panics
    /// Panics if fixture setup fails or the asserted manifest contract differs.
    fn rejects_invalid_inputs_before_charging_output_budget() {
        let files = files();
        let base = spec(&files);
        let bad_comments: &[&[u8]] = &[
            b"[abcd]",
            b"[/info/abcd]",
            b"[ABCD|label]",
            b"harmless [ text",
            b"\0",
            b"\xff",
        ];
        for comment in bad_comments {
            let mut changed = base;
            changed.comment = comment;
            let mut work = budget(10000);
            assert!(build_private_manifest(&changed, &mut work, 4096).is_err());
            work.charge_decoded(10000).unwrap();
        }
        let bad_actors: &[&[u8]] = &[b"", b"\0", b"\xff"];
        for actor in bad_actors {
            let mut changed = base;
            changed.actor = actor;
            assert!(build_private_manifest(&changed, &mut budget(10000), 4096).is_err());
        }
        for date in [
            OLD,
            b"2026-11-19T00:00:00",
            b"bad date",
            b"2026-02-30T00:00:00",
        ] {
            let mut changed = base;
            changed.date = date;
            assert!(build_private_manifest(&changed, &mut budget(10000), 4096).is_err());
        }
        for branch in [
            "trunk",
            "gossil-private/no",
            "gossil-private/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ] {
            let mut changed = base;
            changed.branch = branch;
            assert!(build_private_manifest(&changed, &mut budget(10000), 4096).is_err());
        }
        let mut changed = base;
        changed.parents = PrivateParents::Later {
            previous: ManifestParent {
                hash: PREVIOUS,
                date: OLD,
            },
            shared: ManifestParent {
                hash: SHARED,
                date: NEW,
            },
        };
        assert!(build_private_manifest(&changed, &mut budget(10000), 4096).is_err());
        changed.parents = PrivateParents::Later {
            previous: ManifestParent {
                hash: SHARED,
                date: OLD,
            },
            shared: ManifestParent {
                hash: SHARED,
                date: OLD,
            },
        };
        assert!(build_private_manifest(&changed, &mut budget(10000), 4096).is_err());
        changed.parents = PrivateParents::First {
            shared: ManifestParent {
                hash: "bad",
                date: OLD,
            },
        };
        assert!(build_private_manifest(&changed, &mut budget(10000), 4096).is_err());
    }

    #[test]
    /// Reject unsupported full-tree path, mode and content identity forms.
    /// # Panics
    /// Panics if fixture setup fails or the asserted manifest contract differs.
    fn rejects_empty_duplicate_unordered_bad_mode_and_hash_trees() {
        let good = files()[0];
        let duplicate = [good, good];
        let unordered = [ManifestFile { path: "z", ..good }, good];
        let bad_mode = [ManifestFile { mode: 2, ..good }];
        let negative_mode = [ManifestFile { mode: -1, ..good }];
        let bad_hash = [ManifestFile {
            hash: "111111111111111111111111111111111111111g",
            ..good
        }];
        let short_hash = [ManifestFile {
            hash: "1234",
            ..good
        }];
        let empty_path = [ManifestFile { path: "", ..good }];
        let nul_path = [ManifestFile {
            path: "a\0b",
            ..good
        }];
        for files in [
            &[][..],
            &duplicate,
            &unordered,
            &bad_mode,
            &negative_mode,
            &bad_hash,
            &short_hash,
            &empty_path,
            &nul_path,
        ] {
            let mut work = budget(10000);
            assert!(build_private_manifest(&spec(files), &mut work, 4096).is_err());
            work.charge_decoded(10000).unwrap();
        }
    }

    #[test]
    /// Check that repeated encodes share the caller cumulative work ledger.
    /// # Panics
    /// Panics if fixture setup fails or the asserted manifest contract differs.
    fn two_encodes_share_cumulative_work_and_charge_prior_capture() {
        let files = files();
        let spec = spec(&files);
        // Exact 320 output + 132 token temporaries + 285 checksum scan + 32 hex.
        /// Set the exact retained work charged by this manifest fixture.
        const CHARGE: u64 = 769;
        let mut exact = budget(CHARGE * 2);
        build_private_manifest(&spec, &mut exact, 320).unwrap();
        build_private_manifest(&spec, &mut exact, 320).unwrap();
        assert!(exact.charge_decoded(1).is_err());
        let mut short = budget(CHARGE * 2 - 1);
        build_private_manifest(&spec, &mut short, 320).unwrap();
        assert!(build_private_manifest(&spec, &mut short, 320).is_err());
        let mut capture = budget(CHARGE);
        capture.charge_decoded(1).unwrap();
        assert!(build_private_manifest(&spec, &mut capture, 320).is_err());
    }
}
