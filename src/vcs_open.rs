//! Open or explicitly register native checkouts through the shared locator.
//!
//! ## Recommendations
//! Use explicit existing repository selection; recover with open, and request first registration
//! with `--root ROOT --checkout ID`. Without `--root`, open never allocates a checkout.
use crate::{
    checkout_locator as locator,
    storage::{AdmissionLimits, CheckoutView, Limits, LocalOwnerRepository},
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
/// Set finite limits for validating supported native history, independent of Gest project policy.
/// # Returns
/// Finite native admission limits, independent of Gest project policy, with explicit content and parser ceilings.
pub(crate) fn limits() -> AdmissionLimits {
    AdmissionLimits {
        content: Limits {
            max_stored_bytes: 1048576,
            max_output_bytes: 1048576,
            max_chain_depth: 128,
            max_total_stored_bytes: 67108864,
            max_total_decoded_bytes: 67108864,
        },
        parser: crate::artifact::ParseLimits {
            input_bytes: 1048576,
            cards: 4096,
            owned_bytes: 8388608,
            entries: 4096,
        },
    }
}
/// Observe the nearest marker; missing locator remains an incomplete marker, never a default.
/// # Arguments
/// * `root` - Canonical requested root or invocation directory from which ancestor discovery starts.
/// # Returns
/// The nearest marker root and its optional locator, or no marker. A missing locator remains an incomplete marker.
/// # Errors
/// Return traversal I/O failures and reject unsafe markers or invalid locators without ancestor fallback.
fn marker(root: &Path) -> Result<Option<(PathBuf, Option<locator::Locator>)>, Error> {
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
        let configured = std::fs::canonicalize(directory.join("gossil.db"))?;
        if configured != repository {
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
