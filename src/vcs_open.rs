//! Open or explicitly register native checkouts through the shared locator.
//!
//! ## Recommendations
//! Use explicit existing repository selection; recover with open, and request first registration
//! with `--root ROOT --checkout ID`. Without `--root`, open never allocates a checkout.
use crate::{
    checkout_locator as locator,
    storage::{AdmissionLimits, CheckoutView, Limits, LocalOwnerRepository, TrackingLimits},
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
/// Preserve adapter, storage and filesystem failures through one private result type.
type Error = Box<dyn std::error::Error>;

/// Parsed explicit selections; repository paths retain their native bytes.
struct Options {
    /// Explicit existing database filename with native path bytes preserved.
    repository: PathBuf,
    /// Exact selected UTF-8 checkout registration identity.
    checkout: String,
    /// Explicit registration root, or absence for existing registration discovery.
    root: Option<PathBuf>,
}
/// Parse one unambiguous explicit native open command before observing storage.
/// # Arguments
/// * `args` - Open-command arguments after the executable, group and open subcommand.
/// # Returns
/// Explicit repository and checkout selections, with an optional registration root.
/// # Errors
/// Reject missing, duplicate or unknown options, missing values and non-UTF-8 checkout IDs.
fn options(args: &[OsString]) -> Result<Options, Error> {
    let mut repository = None;
    let mut checkout = None;
    let mut root = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--repository" && repository.is_none() {
            repository = Some(PathBuf::from(
                args.next().ok_or("--repository requires PATH")?,
            ));
        } else if arg == "--checkout" && checkout.is_none() {
            checkout = Some(
                args.next()
                    .ok_or("--checkout requires ID")?
                    .to_str()
                    .ok_or("checkout ID must be UTF-8")?
                    .to_owned(),
            );
        } else if arg == "--root" && root.is_none() {
            root = Some(PathBuf::from(args.next().ok_or("--root requires ROOT")?));
        } else {
            return Err(
                "Usage: gossil vcs open --repository PATH --checkout ID [--root ROOT]".into(),
            );
        }
    }
    Ok(Options {
        repository: repository.ok_or("open requires --repository PATH")?,
        checkout: checkout.ok_or("open requires --checkout ID")?,
        root,
    })
}
/// The largest file, in bytes, that `add` reads and `commit` stores: 2,147,418,109.
///
/// It is the largest file that stock Fossil 2.29 reads back from a repository, 3 bytes
/// under its `MAX_BLOB_SIZE` of 0x7fff0000. `fossil test-integrity` refuses a file of
/// 2,147,418,110 bytes with `out of memory`. It is also the largest decoded content
/// object and the largest artifact of every command.
pub(crate) const FILE_BYTES: u32 = 0x7fff_0000 - 3;
/// The largest stored envelope of one artifact, in bytes: 999,998,976.
///
/// SQLite holds one row of at most 1,000,000,000 bytes, the default `SQLITE_MAX_LENGTH`
/// that Fossil also keeps. The row of a blob holds the envelope, its name, its size
/// and its receipt, so 1,024 bytes are kept for them. A file that does not compress
/// therefore fits up to about 999,700,000 bytes, as in Fossil.
pub(crate) const STORED_BYTES: u64 = 1_000_000_000 - 1024;
/// The size of a file, in bytes, above which `add` and `commit` print a warning:
/// 20,000,000, the default `large-file-size` of Fossil.
///
/// `add` warns for each file that it reads. `commit` warns only for each file whose
/// content differs from the parent check-in.
pub(crate) const LARGE_FILE_BYTES: u64 = 20_000_000;
/// The most cards and list entries of one artifact: half of [`FILE_BYTES`].
///
/// A card takes at least two bytes, so the byte ceiling of an artifact binds first.
pub(crate) const ARTIFACT_CARDS: usize = FILE_BYTES as usize / 2;

/// Set finite limits for validating supported native history, independent of Gest project policy.
///
/// The ceilings bound one artifact and one row. The two totals are
/// [`crate::storage::UNMETERED`], the total ceiling of a proof, so no command has a
/// ceiling on its whole work, as in Fossil.
/// # Returns
/// Finite native admission limits, independent of Gest project policy, with explicit content and parser ceilings.
pub(crate) fn limits() -> AdmissionLimits {
    AdmissionLimits {
        content: Limits {
            max_stored_bytes: STORED_BYTES,
            max_output_bytes: FILE_BYTES,
            max_chain_depth: 128,
            max_total_stored_bytes: crate::storage::UNMETERED,
            max_total_decoded_bytes: crate::storage::UNMETERED,
        },
        parser: crate::artifact::ParseLimits {
            input_bytes: FILE_BYTES as usize,
            cards: ARTIFACT_CARDS,
            owned_bytes: OWNED_RATIO * FILE_BYTES as usize,
            entries: ARTIFACT_CARDS,
        },
    }
}

/// The bytes that the parser owns for each byte of a large manifest, rounded up.
///
/// `parser_owns_less_than_the_ratio` measured 3.3 on a manifest of 20,000 short
/// paths and 2.5 on a cluster of 20,000 cards.
pub(crate) const OWNED_RATIO: usize = 4;

/// Give the one set of tracking ceilings of `add`, `status`, `scope` and `commit`.
/// # Returns
/// As many paths as [`ARTIFACT_CARDS`], paths of at most 4,096 bytes and 128
/// components, a file of at most [`FILE_BYTES`], and a read buffer of 64 KiB. The
/// count of paths lets the ceiling on the manifest bound the tracked set.
pub(crate) fn tracking_limits() -> TrackingLimits {
    TrackingLimits {
        paths: ARTIFACT_CARDS,
        path_bytes: 4096,
        components: 128,
        retained_bytes: u64::from(FILE_BYTES),
        io_buffer_bytes: 65536,
    }
}

/// Give a warning line for each file over [`LARGE_FILE_BYTES`].
///
/// Fossil asks before it commits such a file. Gossil warns on standard error and
/// does not ask.
/// # Arguments
/// * `root` - The registered root of the checkout.
/// * `paths` - The paths, relative to the root, that `add` read or that a commit stored.
/// # Returns
/// One line for each regular file over the size, in the order of `paths`. A path
/// whose status cannot be read gives no line.
pub(crate) fn large_file_warnings(root: &Path, paths: &[&str]) -> Vec<String> {
    paths
        .iter()
        .filter_map(|path| {
            let size = std::fs::symlink_metadata(root.join(path))
                .ok()
                .filter(std::fs::Metadata::is_file)?
                .len();
            (size > LARGE_FILE_BYTES).then(|| {
                format!("warning: the file {path:?} has {size} bytes, more than {LARGE_FILE_BYTES}")
            })
        })
        .collect()
}

/// The text inside the storage refusal of a command that waited for the write lock
/// past its timeout.
pub(crate) const LOCKED: &str = "database is locked";

/// Say whether an error, or any error that caused it, is a lock timeout.
/// # Arguments
/// * `error` - The failure of one attempt.
/// # Returns
/// True when the message of the error or of one of its sources holds [`LOCKED`].
pub(crate) fn is_locked(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut next = Some(error);
    while let Some(error) = next {
        if error.to_string().contains(LOCKED) {
            return true;
        }
        next = error.source();
    }
    false
}

/// Run one attempt of a command, and run it once more after a lock timeout.
///
/// Every command takes the write lock, even a command that only reads. So a command
/// can wait for that lock past its busy timeout and fail with [`LOCKED`]. That failure
/// rolls back its own transaction, and a second attempt opens the repository again.
///
/// `vcs add` adds the files in one transaction and reads the list that it prints in
/// three more transactions. So a failure in one of those three comes after the files
/// are added. The second attempt is still safe, because add skips each path that is
/// already tracked.
///
/// Every command that opens an existing repository runs inside this
/// function, except `vcs open`. A first registration of `vcs open` that waits past the
/// timeout has already made the `.gossil` directory, so a second attempt would fail on
/// the incomplete marker. `vcs init` makes a new file, which no other process holds.
/// # Arguments
/// * `attempt` - Opens the repository and runs the command, each time it is called.
/// * `retryable` - Says whether a failure is a lock timeout.
/// # Returns
/// The result of the first attempt, or of the second one after a lock timeout.
/// # Errors
/// Gives the failure of the first attempt when it is not a lock timeout, and the
/// failure of the second attempt otherwise.
pub(crate) async fn retry_locked<T, E, F, A>(
    mut attempt: A,
    retryable: impl Fn(&E) -> bool,
) -> Result<T, E>
where
    A: FnMut() -> F,
    F: std::future::Future<Output = Result<T, E>>,
{
    match attempt().await {
        Err(failure) if retryable(&failure) => attempt().await,
        result => result,
    }
}
/// Observe the nearest marker; missing locator remains an incomplete marker, never a default.
/// # Arguments
/// * `root` - Canonical requested root or invocation directory from which ancestor discovery starts.
/// # Returns
/// The nearest marker root and its optional locator, or no marker. A missing locator remains an incomplete marker.
/// # Errors
/// Return traversal I/O failures and reject unsafe markers or invalid locators without ancestor fallback.
pub(crate) fn marker(root: &Path) -> Result<Option<(PathBuf, Option<locator::Locator>)>, Error> {
    for ancestor in root.ancestors() {
        let marker = ancestor.join(".gossil");
        match std::fs::symlink_metadata(&marker) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
            Ok(_) => locator::directory(&marker)?,
        }
        let path = marker.join("checkout.json");
        let value = match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
            Ok(_) => Some(locator::read(&path)?),
        };
        return Ok(Some((ancestor.to_owned(), value)));
    }
    Ok(None)
}
/// Native selections observed through the common invocation policy.
struct Selection {
    /// Canonical existing database path.
    repository: PathBuf,
    /// Exact discovered or explicitly requested canonical root.
    selected_root: PathBuf,
    /// Nearest marker and its optional complete assertion.
    found: Option<(PathBuf, Option<locator::Locator>)>,
}
/// Resolve explicit selections through the one invocation/config/marker policy.
/// No registration, locator publication or database allocation occurs here.
/// # Arguments
/// * `options` - Explicit selectors and optional first registration root.
/// # Returns
/// Canonical selections and the nearest observed marker.
/// # Errors
/// Propagates typed config/locator causes and refuses conflicting selections.
fn selection(options: &Options) -> Result<Selection, Error> {
    let registration_requested = options.root.is_some();
    let invocation_cwd = std::fs::canonicalize(std::env::current_dir()?)?;
    let requested_root = match &options.root {
        Some(root) => root.clone(),
        None => invocation_cwd.clone(),
    };
    let root = std::fs::canonicalize(&requested_root)?;
    if !root.is_dir() {
        return Err("checkout root is not a directory".into());
    }
    let config = crate::checkout_config::load(&invocation_cwd)?;
    let found = marker(&root)?;
    if registration_requested
        && found
            .as_ref()
            .is_some_and(|(registered_root, _)| registered_root != &root)
    {
        return Err("explicit checkout root differs from discovered registration root".into());
    }
    let selected_root = found.as_ref().map(|(root, _)| root.clone()).unwrap_or(root);
    // Canonicalize an EXISTING repository only: this command never creates a database.
    let repository = std::fs::canonicalize(&options.repository)?;
    locator::validate_path(&repository)?;
    if let Some(directory) = config.explicit {
        // An explicit config directory is a selection request, even with an explicit CLI path.
        // A configured directory without that database names other storage, so it conflicts too.
        let configured = std::fs::canonicalize(directory.join("gossil.db")).ok();
        if configured.as_ref() != Some(&repository) {
            return Err("configured storage conflicts with explicit repository".into());
        }
    }
    if let Some((_, Some(value))) = &found {
        if value.path()? != repository {
            return Err("explicit repository conflicts with checkout locator".into());
        }
        if options.checkout != value.checkout_id {
            return Err("explicit checkout conflicts with checkout locator".into());
        }
    }
    if registration_requested && found.as_ref().is_some_and(|(_, value)| value.is_none()) {
        return Err("incomplete checkout marker forbids new registration; omit --root for existing registration recovery".into());
    }
    Ok(Selection {
        repository,
        selected_root,
        found,
    })
}

/// Admit the complete locator and exact native binding for tracking, scope and recording commands.
/// Missing or incomplete markers require explicit recovery with `vcs open`.
/// # Arguments
/// * `repository` - Explicit existing repository path.
/// * `checkout` - Explicit exact registered identity.
/// # Returns
/// Admitted owner and lean snapshot after complete locator/native binding checks.
/// # Errors
/// Reject conflicting selections, malformed locators, missing markers and native admission errors.
pub(crate) async fn admit_existing(
    repository: PathBuf,
    checkout: String,
) -> Result<(LocalOwnerRepository, CheckoutView), Error> {
    let options = Options {
        repository,
        checkout,
        root: None,
    };
    let Selection {
        repository,
        selected_root,
        found,
    } = selection(&options)?;
    let Some((_, Some(existing))) = found else {
        return Err(
            "checkout operation requires a complete checkout locator; run vcs open first".into(),
        );
    };
    let mut owner = LocalOwnerRepository::open(&repository, limits()).await?;
    let view = owner
        .checkout_view_for_root(&selected_root, limits())
        .await?;
    admitted_locator(&repository, &view, &options.checkout, Some(&existing))?;
    Ok((owner, view))
}

/// Admit existing storage and exact root before constructing or publishing any lookup assertion.
/// # Arguments
/// * `options` - Explicit repository/checkout selection and optional exact registration root.
/// # Returns
/// An admitted checkout view after required locator publication or recovery; no working files are selected or materialized.
/// # Errors
/// Reject selection/configuration conflicts, invalid roots, native admission failures and locator preparation or publication failures.
/// A committed registration survives publication or later output failure; locator errors retain their phase.
async fn open(options: Options) -> Result<CheckoutView, Error> {
    let registration_requested = options.root.is_some();
    let Selection {
        repository,
        selected_root,
        found,
    } = selection(&options)?;
    let mut owner = LocalOwnerRepository::open(&repository, limits()).await?;
    let marker = selected_root.join(".gossil");
    if registration_requested && found.is_none() {
        let id = &options.checkout;
        let (repository_id, _) = owner.initialization_identity().await?;
        // Serialize, bound, privately prepare and sync complete bytes BEFORE database mutation.
        let value = locator::Locator::new(&repository, repository_id, id.clone())?;
        locator::prepare_directory(&marker)?;
        let prepared = locator::Prepared::new(&marker, &value)?;
        let view = owner
            .register_checkout(id, &selected_root, limits())
            .await?;
        // The successful API return is the checked commit boundary.
        registration_matches(&view, &value, &selected_root)?;
        prepared.publish()?;
        return Ok(view);
    }
    // Missing locator reconstruction uses exactly one authorized existing registration.
    let view = owner
        .checkout_view_for_root(&selected_root, limits())
        .await?;
    let value = admitted_locator(
        &repository,
        &view,
        &options.checkout,
        found.as_ref().and_then(|(_, existing)| existing.as_ref()),
    )?;
    locator::prepare_directory(&marker)?;
    // This path mutates no application state; the registration was already durably committed.
    locator::publish(&marker, &value)?;
    Ok(view)
}
/// Check returned committed facts against the bounded payload prepared before mutation.
/// # Arguments
/// * `view` - Actual view returned after successful native registration commit.
/// * `value` - Bounded locator assertion prepared before the registration write.
/// * `root` - Exact selected canonical registration root.
/// # Returns
/// Success when committed repository, checkout and root facts match the prepared assertion.
/// # Errors
/// Reject a committed view whose repository, checkout or root differs from the prepared assertion.
fn registration_matches(
    view: &CheckoutView,
    value: &locator::Locator,
    root: &Path,
) -> Result<(), Error> {
    if view.repository_id != value.repository_id
        || view.checkout_id != value.checkout_id
        || view.registered_root != root
    {
        return Err(
            "committed; locator unpublished: admitted registration differs from prepared facts"
                .into(),
        );
    }
    Ok(())
}
/// Reconstruct the unchanged lookup assertion only from an admitted native view and exact selections.
/// # Arguments
/// * `repository` - Actual canonical existing repository filename.
/// * `view` - Authenticated existing native checkout view.
/// * `checkout` - Exact checkout identity selected by the caller.
/// * `existing` - Discovered locator to compare, if one exists.
/// # Returns
/// The validated lookup assertion reconstructed from the admitted view and matching explicit selections.
/// # Errors
/// Reject checkout selection or existing-locator conflicts and any unrepresentable lookup payload.
fn admitted_locator(
    repository: &Path,
    view: &CheckoutView,
    checkout: &str,
    existing: Option<&locator::Locator>,
) -> Result<locator::Locator, Error> {
    if checkout != view.checkout_id {
        return Err("explicit checkout differs from exact-root registration".into());
    }
    let value = locator::Locator::new(
        repository,
        view.repository_id.clone(),
        view.checkout_id.clone(),
    )?;
    if let Some(existing) = existing
        && existing != &value
    {
        return Err("checkout locator differs from admitted native binding".into());
    }
    Ok(value)
}
/// Enter the native adapter using the existing plain argument dispatch convention.
/// # Arguments
/// * `args` - Open-command arguments after executable and group dispatch.
/// # Returns
/// Success after native open and locator completion and writing the checkout receipt to stdout.
/// # Errors
/// Return argument, runtime creation, native open or locator errors. A stdout write failure preserves completed publication.
/// # Panics
/// Panics if called from within an already running Tokio runtime.
pub(crate) fn run(args: &[OsString]) -> Result<(), Error> {
    let options = options(args)?;
    let view = tokio::runtime::Builder::new_current_thread()
        .build()?
        .block_on(open(options))?;
    use std::io::Write;
    writeln!(
        std::io::stdout().lock(),
        "Opened checkout {} at {} (repository {}, head {})",
        view.checkout_id,
        view.registered_root.display(),
        view.repository_id,
        view.shared_head
    )?;
    Ok(())
}

/// Compare actual native admission facts without mutating or fabricating a returned view.
#[cfg(test)]
mod tests {
    use super::*;
    /// A file of exactly 20,000,000 bytes gives no warning and one byte more gives one.
    /// A directory, a symbolic link and a missing path give none.
    /// # Panics
    /// Panics if a warning is missing or extra.
    #[test]
    fn a_warning_only_above_twenty_million_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for (name, size) in [
            ("at.bin", LARGE_FILE_BYTES),
            ("over.bin", LARGE_FILE_BYTES + 1),
        ] {
            std::fs::File::create(root.join(name))
                .unwrap()
                .set_len(size)
                .unwrap();
        }
        std::fs::create_dir(root.join("dir")).unwrap();
        std::os::unix::fs::symlink("over.bin", root.join("link")).unwrap();
        let paths = ["at.bin", "over.bin", "dir", "link", "missing"];
        assert_eq!(
            large_file_warnings(root, &paths),
            ["warning: the file \"over.bin\" has 20000001 bytes, more than 20000000"]
        );
    }
    /// The ceilings of the CLI agree: `add` reads the file that `commit` stores, the
    /// artifact ceiling is the file ceiling, and the tracked paths are as many as the
    /// cards of one manifest.
    /// # Panics
    /// Panics if two ceilings disagree.
    #[test]
    fn ceilings_of_add_and_commit_agree() {
        let (admission, tracking) = (limits(), tracking_limits());
        assert_eq!(admission.content.max_output_bytes, 2_147_418_109);
        assert_eq!(admission.content.max_stored_bytes, 999_998_976);
        assert_eq!(
            tracking.retained_bytes,
            u64::from(admission.content.max_output_bytes)
        );
        assert_eq!(
            admission.parser.input_bytes,
            admission.content.max_output_bytes as usize
        );
        assert_eq!(admission.parser.cards, 1_073_709_054);
        assert_eq!(admission.parser.entries, admission.parser.cards);
        assert_eq!(tracking.paths, admission.parser.cards);
        assert_eq!(
            admission.parser.owned_bytes,
            OWNED_RATIO * admission.parser.input_bytes
        );
        assert_eq!(
            (
                admission.content.max_total_stored_bytes,
                admission.content.max_total_decoded_bytes
            ),
            (crate::storage::UNMETERED, crate::storage::UNMETERED)
        );
    }

    /// Run the retry once after a lock timeout, never after another failure, and
    /// never a third time.
    /// # Panics
    /// Panics if the count of attempts or a result differs.
    #[tokio::test]
    async fn a_lock_timeout_is_retried_once() {
        let locked = || -> Error { "SQLite failure: `database is locked`".into() };
        let retryable = |error: &Error| is_locked(&**error);
        for (failures, attempts, passed) in [(0, 1, true), (1, 2, true), (2, 2, false)] {
            let mut count = 0;
            let result: Result<(), Error> = retry_locked(
                || {
                    count += 1;
                    let fail = count <= failures;
                    async move { if fail { Err(locked()) } else { Ok(()) } }
                },
                retryable,
            )
            .await;
            assert_eq!((count, result.is_ok()), (attempts, passed));
        }
        let mut count = 0;
        let result: Result<(), Error> = retry_locked(
            || {
                count += 1;
                async { Err("tracking path limit".into()) }
            },
            retryable,
        )
        .await;
        assert_eq!(
            (count, result.unwrap_err().to_string().as_str()),
            (1, "tracking path limit")
        );
        // A cause that holds the lock text counts too.
        let wrapped = std::io::Error::other(crate::storage::StorageError::from(
            libsql::Error::SqliteFailure(5, LOCKED.into()),
        ));
        assert!(is_locked(&wrapped));
        assert!(!is_locked(&std::io::Error::other("other")));
    }

    /// The parser owns less than [`OWNED_RATIO`] bytes for each byte of a large
    /// manifest of short paths, and of a large cluster, and admits more than the old
    /// ceiling of 16,384 cards.
    /// # Panics
    /// Panics if a parse fails, or if it owns more than the ratio.
    #[tokio::test]
    async fn parser_owns_less_than_the_ratio() {
        struct Dates;
        impl crate::artifact::DateProvider for Dates {
            async fn julianday(
                &self,
                date: Option<&[u8]>,
            ) -> Result<Option<f64>, crate::artifact::DateProviderError> {
                Ok(date.map(|_| 2_461_000.5))
            }
        }
        let sealed = |mut input: Vec<u8>| {
            let checksum = crate::artifact_checksum::md5(&input);
            input.extend_from_slice(b"Z ");
            for byte in checksum {
                input.extend_from_slice(format!("{byte:02x}").as_bytes());
            }
            input.push(b'\n');
            input
        };
        let mut manifest = b"C save\nD 2026-11-20T00:00:00.000\n".to_vec();
        for index in 0..20_000 {
            manifest.extend_from_slice(
                format!("F d{:03}/f{index:05}.txt {index:064x}\n", index / 100).as_bytes(),
            );
        }
        manifest.extend_from_slice(b"U tester\n");
        let mut cluster = Vec::new();
        for index in 0..20_000 {
            cluster.extend_from_slice(format!("M {index:040x}\n").as_bytes());
        }
        // A cluster has no date card, so the provider gives no date for it.
        assert_eq!(
            crate::artifact::DateProvider::julianday(&Dates, None)
                .await
                .unwrap(),
            None
        );
        for (input, cards) in [(sealed(manifest), 20_004), (sealed(cluster), 20_001)] {
            let (_, usage) =
                crate::artifact::parse_artifact_with_usage(&input, limits().parser, &Dates)
                    .await
                    .unwrap();
            assert_eq!(usage.cards, cards);
            assert!(usage.owned_bytes < OWNED_RATIO * input.len());
        }
    }
    /// A real registered checkout remains unchanged while each caller/payload mismatch is rejected.
    #[tokio::test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    async fn admitted_fact_helpers_refuse_identity_root_and_locator_mismatch() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let other = temp.path().join("other");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&other).unwrap();
        let root = root.canonicalize().unwrap();
        let repository = temp.path().join("gossil.db");
        let bounds = limits();
        let mut owner = LocalOwnerRepository::initialize(
            &repository,
            crate::storage::InitializeOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "native-helper-owner",
                date: "2026-11-20 12:34:56",
                user: b"native owner",
                password: "secret",
                sha1: false,
                content_limits: bounds.content,
                parse_limits: bounds.parser,
            },
        )
        .await
        .unwrap();
        let view = owner
            .register_checkout("native", &root, bounds)
            .await
            .unwrap();
        let repository = repository.canonicalize().unwrap();
        let value = admitted_locator(&repository, &view, "native", None).unwrap();
        assert!(registration_matches(&view, &value, &root).is_ok());
        assert_eq!(
            admitted_locator(&repository, &view, "native", Some(&value)).unwrap(),
            value
        );
        let wrong_repository =
            locator::Locator::new(&repository, "f".repeat(40), "native".into()).unwrap();
        let wrong_checkout =
            locator::Locator::new(&repository, view.repository_id.clone(), "another".into())
                .unwrap();
        for candidate in [&wrong_repository, &wrong_checkout] {
            assert!(
                registration_matches(&view, candidate, &root)
                    .unwrap_err()
                    .to_string()
                    .contains("committed; locator unpublished")
            );
            assert!(
                admitted_locator(&repository, &view, "native", Some(candidate))
                    .unwrap_err()
                    .to_string()
                    .contains("differs from admitted native binding")
            );
        }
        assert!(registration_matches(&view, &value, &other).is_err());
        assert!(
            admitted_locator(&repository, &view, "wrong-explicit", None)
                .unwrap_err()
                .to_string()
                .contains("explicit checkout differs")
        );
        assert!(admitted_locator(Path::new("relative"), &view, "native", None).is_err());
        assert_eq!(owner.checkout_view("native", bounds).await.unwrap(), view);
    }
    /// Real traversal errors remain discovery errors rather than absent-marker fallback.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn marker_discovery_does_not_treat_permission_denial_as_absence() {
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: geteuid observes this process without pointer arguments.
        assert_ne!(
            unsafe { libc::geteuid() },
            0,
            "permission fixture requires unprivileged owner"
        );
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o000)).unwrap();
        let denied_marker = marker(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(denied_marker.is_err());
        let directory = root.join(".gossil");
        std::fs::create_dir(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o600)).unwrap();
        let denied_locator = marker(&root);
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(denied_locator.is_err());
        let incomplete = marker(&root).unwrap().unwrap();
        assert_eq!(incomplete.0, root);
        assert!(incomplete.1.is_none());
    }
}
