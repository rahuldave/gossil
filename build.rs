//! Export build date and revision metadata for later product use.
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Export the build date and revision, tracking Git inputs across cached builds.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    track_git_inputs();
    let date = chrono_free_date();
    println!("cargo:rustc-env=BUILD_DATE={date}");
    let sha = git_short_sha().unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=GIT_SHA={sha}");
}

/// Track the worktree HEAD and the actual storage for its attached branch.
fn track_git_inputs() {
    let Some(head) = git_path("HEAD") else {
        return;
    };
    watch_path(&head);
    let Some(reference) =
        command_text("git", &["symbolic-ref", "-q", "HEAD"]).and_then(|name| git_path(&name))
    else {
        return;
    };
    if reference.is_file() {
        watch_path(&reference);
        return;
    }
    // A packed or unborn ref can become a new loose file on the next commit.
    // Watch an existing parent rather than a missing file that reruns forever.
    if let Some(parent) = reference.ancestors().skip(1).find(|path| path.is_dir()) {
        watch_path(parent);
    }
    if let Some(packed) = git_path("packed-refs").filter(|path| path.is_file()) {
        watch_path(&packed);
    }
}

/// Ask Git for a worktree-aware metadata path.
///
/// # Arguments
/// * `name` - Git metadata path, such as HEAD or a full branch reference.
/// # Returns
/// Git's resolved path, or None outside Git or when Git is unavailable.
fn git_path(name: &str) -> Option<PathBuf> {
    command_text("git", &["rev-parse", "--git-path", name]).map(PathBuf::from)
}

/// Register a real Git input with Cargo's build-script change detector.
///
/// # Arguments
/// * `path` - Existing file or directory controlling the current revision.
fn watch_path(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
}

/// Read successful, nonempty command output without introducing a build dependency.
///
/// # Arguments
/// * `program` - Installed executable to invoke.
/// * `args` - Exact command arguments.
/// # Returns
/// Trimmed stdout, or None for missing commands, failed status or invalid/empty output.
fn command_text(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Read the local build date without an additional build dependency.
///
/// # Returns
/// The date reported by the installed date command, or unknown when it fails.
fn chrono_free_date() -> String {
    command_text("date", &["+%Y-%m-%d"]).unwrap_or_else(|| "unknown".to_string())
}

/// Read the checkout revision, returning `None` when Git has no revision.
///
/// # Returns
/// Git's short current commit identity, or None when no revision is available.
fn git_short_sha() -> Option<String> {
    command_text("git", &["rev-parse", "--short", "HEAD"])
}

/// Direct shipping-source controls complement the cached-Cargo/Git Python fixtures.
#[cfg(test)]
#[path = "tests/build_metadata/controls.rs"]
mod controls;
