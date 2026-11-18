//! Optional pinned, source-derived invocation identities; upstream names remain unchanged.
use crate::{Config, RunnerError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

/// Expected hash and physical bindings for one optional complete plan.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanPin {
    /// Absolute path of the plan JSON.
    pub path: PathBuf,
    /// Expected SHA-256 of those exact bytes.
    pub sha256: String,
    /// Expected full row count; a setup check, not proof of source correspondence.
    #[serde(default)]
    pub expected_invocations: usize,
    /// Absolute path of the original corpus manifest JSON.
    pub corpus_manifest: PathBuf,
    /// Expected SHA-256 of the manifest bytes.
    pub corpus_manifest_sha256: String,
    /// Physical root used to resolve portable corpus paths.
    pub corpus_root: PathBuf,
}
/// Portable expected source correspondence, not an observed trace identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationId {
    /// Original suite basename.
    pub suite: String,
    /// Structured identity within that suite.
    pub position: Position,
}
/// Source-derived identity parameters within the versioned plan namespace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Position {
    /// One corpus mutation invocation.
    Corpus {
        /// Upstream-relative source path with forward slashes.
        source: String,
        /// Zero-based source-loop iteration.
        iteration: u32,
        /// One-based mutation slot.
        slot: u32,
    },
    /// One explicit empty-input control.
    Empty {
        /// One-based empty control index.
        index: u32,
    },
    /// The final upstream framework assertion.
    Framework,
}
/// One expected original assertion name and portable source correspondence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRow {
    /// Original upstream assertion name.
    pub name: String,
    /// Expected source-derived invocation identity.
    pub id: InvocationId,
}
/// Complete ordered plan bound to upstream source, suite and original corpus bytes.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationPlan {
    /// Supported identity schema namespace.
    pub schema: String,
    /// Exact configured upstream suite.
    pub suite: String,
    /// Exact configured upstream source revision.
    pub revision: String,
    /// Expected original suite bytes.
    pub suite_sha256: String,
    /// Expected original corpus manifest bytes.
    pub corpus_manifest_sha256: String,
    /// Every invocation, including unique names and framework assertions, in order.
    pub rows: Vec<PlanRow>,
}
/// Original corpus inventory bound by the plan and configuration.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Corpus {
    /// Exact upstream suite name.
    suite: String,
    /// Exact upstream source revision.
    revision: String,
    /// Every original non-directory input and expected SHA-256.
    files: BTreeMap<String, String>,
}
/// Keep verified bytes for durable evidence; parsing never rereads mutable files.
pub(crate) struct Verified {
    /// Parsed plan from exactly the pinned bytes.
    pub plan: InvocationPlan,
    /// Original verified plan JSON.
    pub bytes: Vec<u8>,
    /// Original verified corpus manifest JSON.
    pub corpus: Vec<u8>,
}
/// Reject ambiguous or nonportable source paths.
/// # Arguments
/// * `path` - Upstream-relative source path.
/// # Returns
/// Whether the path uses unambiguous nonempty forward-slash components.
fn portable(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':'])
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}
/// Verify an expected digest against the same bytes returned for parsing.
/// # Arguments
/// * `path` - Physical input path.
/// * `expected` - Required lowercase SHA-256.
/// # Returns
/// Exactly the verified bytes.
/// # Errors
/// Returns an I/O failure or expected-hash mismatch.
fn pinned(path: &Path, expected: &str) -> Result<Vec<u8>, RunnerError> {
    let bytes = fs::read(path)?;
    if expected.len() != 64 || format!("{:x}", Sha256::digest(&bytes)) != expected {
        return Err(RunnerError::Invalid("invocation plan input hash mismatch"));
    }
    Ok(bytes)
}
/// Verify a complete plan and all pinned bindings before any child executes.
/// # Arguments
/// * `config` - Explicit suite, revision and optional expected plan pins.
/// * `suite` - Physical suite source path.
/// * `hashes` - Evidence map extended with verified corpus and plan hashes.
/// # Returns
/// Verified plan and original JSON bytes, or none in strict default mode.
/// # Errors
/// Returns malformed plan, binding, path, hash or I/O failures before execution.
pub(crate) fn verify(
    config: &Config,
    suite: &Path,
    hashes: &mut BTreeMap<String, String>,
) -> Result<Option<Verified>, RunnerError> {
    let Some(pin) = &config.invocation_plan else {
        return Ok(None);
    };
    if ![&pin.path, &pin.corpus_manifest, &pin.corpus_root]
        .iter()
        .all(|p| p.is_absolute())
    {
        return Err(RunnerError::Invalid("plan paths must be absolute"));
    }
    let bytes = pinned(&pin.path, &pin.sha256)?;
    let plan: InvocationPlan = serde_json::from_slice(&bytes)?;
    let corpus = pinned(&pin.corpus_manifest, &pin.corpus_manifest_sha256)?;
    let manifest: Corpus = serde_json::from_slice(&corpus)?;
    if plan.schema != "gossil.invocation-plan.v1"
        || plan.suite != config.suite
        || plan.revision != config.revision
        || plan.corpus_manifest_sha256 != pin.corpus_manifest_sha256
        || manifest.suite != config.suite
        || manifest.revision != config.revision
        || manifest.files.is_empty()
        || plan.rows.is_empty()
        || plan.rows.len() != pin.expected_invocations
    {
        return Err(RunnerError::Invalid(
            "invocation plan source binding mismatch",
        ));
    }
    pinned(suite, &plan.suite_sha256)?;
    if manifest.files.keys().any(|path| !portable(path)) {
        return Err(RunnerError::Invalid("invalid corpus path"));
    }
    let mut ids = BTreeSet::new();
    let mut covered = BTreeSet::new();
    for row in &plan.rows {
        if row.name.trim().is_empty() || row.id.suite != plan.suite || !ids.insert(&row.id) {
            return Err(RunnerError::Invalid(
                "invalid or duplicate invocation identity",
            ));
        }
        match &row.id.position {
            Position::Corpus { source, slot, .. }
                if portable(source) && *slot > 0 && manifest.files.contains_key(source) =>
            {
                covered.insert(source.clone());
            }
            Position::Empty { index } if *index > 0 => {}
            Position::Framework => {}
            _ => return Err(RunnerError::Invalid("invalid invocation parameters")),
        }
    }
    if covered != manifest.files.keys().cloned().collect() {
        return Err(RunnerError::Invalid(
            "incomplete invocation corpus coverage",
        ));
    }
    for (path, expected) in manifest.files {
        let absolute = pin.corpus_root.join(path);
        pinned(&absolute, &expected)?;
        hashes.insert(absolute.display().to_string(), expected);
    }
    hashes.insert(pin.path.display().to_string(), pin.sha256.clone());
    hashes.insert(
        pin.corpus_manifest.display().to_string(),
        pin.corpus_manifest_sha256.clone(),
    );
    Ok(Some(Verified {
        plan,
        bytes,
        corpus,
    }))
}
