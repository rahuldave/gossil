//! Supply inherited-test tools, bounded byte primitives, Fossil glob matching, lexical
//! filename simplification, check-in graph paths, local storage and decoded task values.
pub mod diff;
mod invocation;
pub use invocation::{InvocationId, InvocationPlan, PlanPin, PlanRow, Position};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::{fs::PermissionsExt, process::CommandExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Private syscall seam for deterministic failure and interruption tests.
struct ProcessOps<'a> {
    /// Observe a child without reaping it.
    observe: &'a dyn Fn(u32) -> std::io::Result<bool>,
    /// Query the current group of the still-unreaped child.
    group: &'a dyn Fn(i32) -> i32,
    /// Signal a verified positive group.
    signal: &'a dyn Fn(i32) -> std::io::Result<()>,
}
/// Observe only our child without releasing its PID.
/// # Arguments
/// * `pid` - Identifier of the direct child.
/// # Returns
/// Whether exit was observed, or the original wait error.
/// # Errors
/// Returns the OS error when `waitid` cannot observe the child.
fn observe(pid: u32) -> std::io::Result<bool> {
    // SAFETY: siginfo_t accepts zero initialization and waitid writes before inspection.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    // SAFETY: P_PID targets the specified direct child; WNOWAIT prevents reaping.
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            pid,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    // SAFETY: successful waitid initializes the si_pid field.
    (result == 0)
        .then(|| unsafe { info.si_pid() } != 0)
        .ok_or_else(std::io::Error::last_os_error)
}
/// Read the process group without changing it.
/// # Arguments
/// * `pid` - Identifier of the child to inspect.
/// # Returns
/// Process group identifier, or minus one when the OS cannot find it.
fn group(pid: i32) -> i32 {
    // SAFETY: getpgid accepts a process identifier and has no pointer arguments.
    unsafe { libc::getpgid(pid) }
}
/// Signal only an already verified positive process group.
/// # Arguments
/// * `pgid` - Verified owned process group, distinct from the caller group.
/// # Returns
/// Success or the original signal error.
/// # Errors
/// Returns the OS error when the group cannot receive SIGKILL.
fn signal(pgid: i32) -> std::io::Result<()> {
    // SAFETY: callers verify positive, owned, distinct process groups before this boundary.
    let result = unsafe { libc::kill(-pgid, libc::SIGKILL) };
    (result == 0)
        .then_some(())
        .ok_or_else(std::io::Error::last_os_error)
}

/// Retain the unreaped child until all owned same-group processes are stopped.
struct OwnedChild {
    /// Direct child; never observed with a reaping poll.
    child: std::process::Child,
    /// Positive process group established and verified immediately after spawn.
    group: i32,
    /// Whether group signaling was verified safe for this invocation.
    verified: bool,
    /// Whether normal completion already reaped the direct child.
    reaped: bool,
}
impl Drop for OwnedChild {
    /// Terminate the owned child group and reap the direct child on early return.
    fn drop(&mut self) {
        if !self.reaped {
            if self.verified {
                // SAFETY: this verified group belongs to the still-unreaped direct child.
                unsafe {
                    libc::kill(-self.group, libc::SIGKILL);
                }
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// A preserved setup, configuration or I/O failure, distinct from failed assertions.
#[derive(Debug)]
pub enum RunnerError {
    /// Underlying filesystem or process error.
    Io(std::io::Error),
    /// Invalid JSON configuration or report serialization.
    Json(serde_json::Error),
    /// Invalid configuration or incomplete assertion report.
    Invalid(&'static str),
    /// Explicit input is not an executable regular file.
    NotExecutable(PathBuf),
}
impl From<std::io::Error> for RunnerError {
    /// Preserve an underlying I/O error.
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<serde_json::Error> for RunnerError {
    /// Preserve a JSON decoding or serialization error.
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
impl std::fmt::Display for RunnerError {
    /// Format the underlying cause or runner diagnostic.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Json(error) => write!(f, "{error}"),
            Self::Invalid(message) => write!(f, "{message}"),
            Self::NotExecutable(path) => write!(f, "not executable: {}", path.display()),
        }
    }
}
impl std::error::Error for RunnerError {
    /// Expose an underlying I/O or JSON cause when present.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Invalid(_) | Self::NotExecutable(_) => None,
        }
    }
}

/// One unchanged upstream assertion.
#[derive(Debug, Serialize, Deserialize)]
pub struct Assertion {
    /// Original assertion name, including spaces.
    pub name: String,
    /// The status `pass` or `fail`. Only `pass` contributes to passing totals.
    pub state: String,
    /// Upstream `knownBug` annotation, retained independently from acceptance status.
    pub known_bug: bool,
    /// Synthetic framework assertion, separate from real suite assertions.
    pub framework: bool,
    /// Expected source-derived identity assigned only after exact plan matching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_invocation_id: Option<InvocationId>,
}
/// Individual results and independently reported upstream totals.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Report {
    /// Assertions in execution order.
    pub assertions: Vec<Assertion>,
    /// Upstream error count; absent for truncated or disabled drivers.
    pub errors: Option<usize>,
    /// Upstream assertion count, including framework checks.
    pub total: Option<usize>,
    /// Every observed final-summary occurrence, including malformed numeric fields.
    pub summary_count: usize,
    /// Malformed or duplicate records invalidate the run.
    pub malformed: bool,
    /// Unmodified upstream skipped-test summaries, never counted as passing.
    pub skipped: Vec<String>,
}
/// Parse exact Fossil driver output.
///
/// Every final-summary occurrence counts, even if its numeric fields are invalid.
/// Nonnumeric or overflowing counts and duplicate summaries permanently mark
/// the report malformed. Call [`validate_report`] before accepting it.
/// # Arguments
/// * `text` - Captured process output.
/// # Returns
/// Individual results and totals, requiring separate validation.
/// # Examples
/// ```
/// let report = gossil::parse_report("test example OK\n***** Final results: 0 errors out of 1 tests\n");
/// assert_eq!(report.assertions[0].name, "example");
/// assert!(gossil::validate_report(&report, Some(0)).is_ok());
/// ```
pub fn parse_report(text: &str) -> Report {
    parse_internal(text, true)
}
/// Shared lexical parser; plan mode changes only duplicate-name handling.
/// # Arguments
/// * `text` - Captured stdout without any repairs.
/// * `reject_duplicates` - Preserve strict default identity rejection.
/// # Returns
/// Every retained assertion and independent malformed-syntax state.
fn parse_internal(text: &str, reject_duplicates: bool) -> Report {
    let mut report = Report::default();
    for line in text.lines() {
        if let Some(record) = line.strip_prefix("test ") {
            let split = record
                .rfind(" OK")
                .into_iter()
                .chain(record.rfind(" FAILED"))
                .max();
            if let Some(index) = split {
                let name = &record[..index];
                let outcome = &record[index + 1..];
                report.malformed |= name.is_empty()
                    || (reject_duplicates && report.assertions.iter().any(|a| a.name == name));
                let state = if outcome == "OK" || outcome == "OK (knownBug)?" {
                    "pass"
                } else if outcome == "FAILED"
                    || outcome == "FAILED!"
                    || outcome == "FAILED (knownBug)!"
                {
                    "fail"
                } else {
                    report.malformed = true;
                    "fail"
                };
                report.assertions.push(Assertion {
                    name: name.into(),
                    state: state.into(),
                    known_bug: outcome.contains("knownBug"),
                    framework: name.starts_with("test-framework-"),
                    expected_invocation_id: None,
                });
            } else {
                report.malformed = true;
            }
        }
        if let Some(skipped) = line.strip_prefix("***** Skipped tests: ") {
            report.skipped.push(skipped.into());
        }
        if let Some(summary) = line.strip_prefix("***** Final results: ") {
            report.summary_count += 1;
            report.malformed |= report.summary_count > 1;
            let words: Vec<_> = summary.split_whitespace().collect();
            if words.len() == 6 && words[1..4] == ["errors", "out", "of"] && words[5] == "tests" {
                report.errors = words[0].parse().ok();
                report.total = words[4].parse().ok();
                report.malformed |= report.errors.is_none() || report.total.is_none();
            } else {
                report.malformed = true;
            }
        }
    }
    report
}
/// Match every retained original name against the complete pinned sequence.
/// # Arguments
/// * `text` - Captured stdout without repairs.
/// * `plan` - Verified complete source-derived expected sequence.
/// # Returns
/// Retained assertions with expected IDs only after exact sequence matching.
fn parse_planned(text: &str, plan: &InvocationPlan) -> Report {
    let mut report = parse_internal(text, false);
    if report.assertions.len() != plan.rows.len()
        || report
            .assertions
            .iter()
            .zip(&plan.rows)
            .any(|(actual, expected)| actual.name != expected.name)
    {
        report.malformed = true;
    } else {
        for (actual, expected) in report.assertions.iter_mut().zip(&plan.rows) {
            actual.expected_invocation_id = Some(expected.id.clone());
        }
    }
    report
}
/// Validate execution, real assertions and exact count reconciliation.
/// # Arguments
/// * `report` - Parsed assertions and totals.
/// * `exit_code` - Actual process exit status, absent for a signal.
/// # Returns
/// Success only when real assertions pass and all totals reconcile.
/// # Errors
/// Returns [`RunnerError::Invalid`] for a nonzero or absent exit code, malformed
/// output, missing or inconsistent counts, skips, failed assertions or no real assertion.
/// # Examples
/// ```
/// let report = gossil::parse_report("Ok\n");
/// assert!(gossil::validate_report(&report, Some(0)).is_err());
/// ```
pub fn validate_report(report: &Report, exit_code: Option<i32>) -> Result<(), RunnerError> {
    if exit_code != Some(0)
        || report.malformed
        || report.errors != Some(0)
        || !report.skipped.is_empty()
        || report.total != Some(report.assertions.len())
        || !report.assertions.iter().any(|a| !a.framework)
        || report.assertions.iter().any(|a| a.state != "pass")
    {
        return Err(RunnerError::Invalid(
            "failed, incomplete, ignored, or unexecuted suite",
        ));
    }
    Ok(())
}
/// Explicit pinned inputs for one reference or candidate run, with no fallback.
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    /// reference or candidate; never an overall compatibility claim.
    pub role: String,
    /// Pinned upstream source revision.
    pub revision: String,
    /// Explicit Tcl executable.
    pub tcl: PathBuf,
    /// Separately patched `tester.tcl`.
    pub driver: PathBuf,
    /// Explicit program under test.
    pub executable: PathBuf,
    /// One suite basename.
    pub suite: String,
    /// New owned directory; existing directories are refused.
    pub run_dir: PathBuf,
    /// Driver deadline in milliseconds.
    pub timeout_ms: u64,
    /// Deterministic Tcl random seed.
    pub seed: u32,
    /// Pristine source, fixture and patch inputs recorded by SHA-256.
    pub inputs: Vec<PathBuf>,
    /// Explicit build capabilities.
    pub capabilities: BTreeMap<String, String>,
    /// Optional expected-hash pin for a complete ordered invocation plan.
    #[serde(default)]
    pub invocation_plan: Option<PlanPin>,
}
/// Durable run evidence with exact command and isolated environment additions.
#[derive(Debug, Serialize, Deserialize)]
pub struct Run {
    /// Complete invocation configuration.
    pub config: Config,
    /// Exact configured source, fixture and patch hashes.
    pub hashes: BTreeMap<String, String>,
    /// Argument vector without shell interpretation.
    pub command: Vec<String>,
    /// Environment additions overriding personal state.
    pub environment: BTreeMap<String, String>,
    /// Driver exit status; absent after a signal.
    pub exit_code: Option<i32>,
    /// Whether the owned process group exceeded its deadline.
    pub timed_out: bool,
    /// Decoded stdout view; exact bytes remain in stdout.log.
    pub stdout: String,
    /// Decoded stderr view; exact bytes remain in stderr.log.
    pub stderr: String,
    /// Whether both output streams are valid UTF-8; invalid bytes reject acceptance.
    pub output_utf8: bool,
    /// Hash binding the retained stdout.log bytes.
    pub stdout_sha256: String,
    /// Hash binding the retained stderr.log bytes.
    pub stderr_sha256: String,
    /// Individual assertion results.
    pub report: Report,
    /// Complete verified plan retained with every original row and identity.
    #[serde(default)]
    pub invocation_plan: Option<InvocationPlan>,
    /// Strict acceptance of this suite alone.
    pub accepted: bool,
    /// pass, fail, blocked, or not-run for this invocation.
    pub status: String,
    /// Whether cleanup signaled the owned group or found no signalable members.
    pub cleanup: String,
}
/// Hash exact file bytes.
/// # Arguments
/// * `path` - Input file whose bytes are retained by hash.
/// # Returns
/// SHA-256 text or the original read error.
/// # Errors
/// Returns [`RunnerError::Io`] when the file cannot be read.
fn hash(path: &Path) -> Result<String, RunnerError> {
    fs::read(path)
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
        .map_err(RunnerError::Io)
}
/// Require a regular executable before any suite runs.
/// # Arguments
/// * `path` - Explicit program to validate.
/// # Returns
/// Success only for an executable regular file without setuid or setgid bits.
/// # Errors
/// Returns [`RunnerError::Io`] for unavailable metadata or
/// [`RunnerError::NotExecutable`] for an unsuitable file or permission mode.
fn executable(path: &Path) -> Result<(), RunnerError> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o111 == 0
        || metadata.permissions().mode() & 0o6000 != 0
    {
        return Err(RunnerError::NotExecutable(path.to_path_buf()));
    }
    Ok(())
}
/// Run one inherited suite in fresh scratch storage and persist `report.json`.
/// # Arguments
/// * `config` - Explicit inputs and owned output directory.
/// # Returns
/// Recorded results, including failed suites; setup failures return an error.
/// # Errors
/// Returns [`RunnerError::Invalid`] for invalid configuration or unsafe process ownership,
/// [`RunnerError::NotExecutable`] for an unsuitable executable, [`RunnerError::Io`]
/// for filesystem or process failures, or [`RunnerError::Json`] for serialization failures.
/// A rejected suite normally returns a [`Run`] with `accepted` set to false.
/// # Examples
/// ```no_run
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config: gossil::Config = serde_json::from_slice(&std::fs::read("CONFIG.json")?)?;
/// let run = gossil::execute(config)?;
/// println!("accepted: {}", run.accepted);
/// # Ok(())
/// # }
/// ```
pub fn execute(config: Config) -> Result<Run, RunnerError> {
    execute_with_ops(
        config,
        &ProcessOps {
            observe: &observe,
            group: &group,
            signal: &signal,
        },
    )
}
/// Run with private syscall boundaries so failure cleanup can be tested deterministically.
/// # Arguments
/// * `config` - Explicit pinned inputs and fresh output directory.
/// * `ops` - Per-invocation process operations, without mutable global state.
/// # Returns
/// Complete run evidence or a preserved setup or process error.
/// # Errors
/// Returns the configuration, executable, I/O or serialization errors described by [`execute`].
fn execute_with_ops(config: Config, ops: &ProcessOps<'_>) -> Result<Run, RunnerError> {
    if !["reference", "candidate"].contains(&config.role.as_str())
        || config.timeout_ms == 0
        || config.suite.is_empty()
        || config.suite.contains(['/', '\\'])
        || config.revision.is_empty()
        || ![
            &config.tcl,
            &config.driver,
            &config.executable,
            &config.run_dir,
        ]
        .into_iter()
        .chain(config.inputs.iter())
        .all(|p| p.is_absolute())
    {
        return Err(RunnerError::Invalid(
            "invalid role, timeout or suite basename",
        ));
    }
    executable(&config.tcl)?;
    executable(&config.executable)?;
    let suite = config
        .driver
        .with_file_name(format!("{}.test", config.suite));
    let mut hashes = BTreeMap::new();
    for path in
        config
            .inputs
            .iter()
            .chain([&config.driver, &suite, &config.tcl, &config.executable])
    {
        hashes.insert(path.display().to_string(), hash(path)?);
    }
    let verified_plan = invocation::verify(&config, &suite, &mut hashes)?;
    fs::create_dir(&config.run_dir)?;
    let root = fs::canonicalize(&config.run_dir)?;
    if let Some(verified) = &verified_plan {
        fs::write(root.join("invocation-plan.json"), &verified.bytes)?;
        fs::write(root.join("corpus-manifest.json"), &verified.corpus)?;
    }
    let environment: BTreeMap<_, _> = [
        "HOME",
        "FOSSIL_HOME",
        "FOSSIL_TEST_TEMP",
        "TEMP",
        "TMP",
        "TMPDIR",
    ]
    .into_iter()
    .map(|key| (key.into(), root.display().to_string()))
    .chain([
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("LC_ALL".into(), "C".into()),
        ("TZ".into(), "UTC".into()),
        ("USER".into(), "gossil-conformance".into()),
        ("LOGNAME".into(), "gossil-conformance".into()),
    ])
    .collect();
    let launcher = root.join("launch.tcl");
    let launcher_source = format!(
        "set argv0 [lindex $argv 0]\nset argv [lrange $argv 1 end]\nexpr {{srand({})}}\nsource $argv0\n",
        config.seed
    );
    fs::write(&launcher, launcher_source)?;
    let command = vec![
        config.tcl.display().to_string(),
        launcher.display().to_string(),
        config.driver.display().to_string(),
        config.executable.display().to_string(),
        config.suite.clone(),
        "-keep".into(),
    ];
    let stdout_file = root.join("stdout.log");
    let stderr_file = root.join("stderr.log");
    let child = Command::new(&command[0])
        .args(&command[1..])
        .current_dir(&root)
        .env_clear()
        .envs(&environment)
        .stdout(Stdio::from(fs::File::create(&stdout_file)?))
        .stderr(Stdio::from(fs::File::create(&stderr_file)?))
        .process_group(0)
        .spawn()?;
    let process_group = child.id() as i32;
    // SAFETY: query the freshly spawned child before observing/reaping its exit.
    let initial_group = (ops.group)(process_group);
    // SAFETY: getpgrp reports only this process's group.
    let caller_group = unsafe { libc::getpgrp() };
    let verified =
        process_group > 1 && initial_group == process_group && initial_group != caller_group;
    let mut owned = OwnedChild {
        child,
        group: process_group,
        verified,
        reaped: false,
    };
    if !verified {
        return Err(RunnerError::Invalid("refusing an unverified process group"));
    }

    let start = Instant::now();
    let mut timed_out = false;
    let leader_exited = loop {
        let exited = match (ops.observe)(owned.child.id()) {
            Ok(exited) => exited,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        };
        timed_out |= start.elapsed() >= Duration::from_millis(config.timeout_ms);
        if exited || timed_out {
            break exited;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    // SAFETY: the unreaped direct child still reserves its PID and process-group identity.
    let owned_group = (ops.group)(process_group);
    // SAFETY: getpgrp has no arguments and reports our own process group.
    let caller_group = unsafe { libc::getpgrp() };
    if (owned_group != -1 && owned_group != process_group) || process_group == caller_group {
        owned.verified = false;
        return Err(RunnerError::Invalid(
            "refusing to signal an unverified process group",
        ));
    }
    // SAFETY: this positive, distinct group belongs to our unreaped child. No PID0/reused-PID signaling.
    let mut cleanup = "signaled-owned-group";
    if let Err(error) = (ops.signal)(process_group) {
        // XNU excludes zombies and returns EPERM when no signalable member remains.
        // This inference is bounded to trusted same-UID children without credential/group escape.
        // https://raw.githubusercontent.com/apple-oss-distributions/xnu/main/bsd/kern/kern_sig.c
        if error.raw_os_error() == Some(libc::ESRCH)
            || (cfg!(target_os = "macos")
                && leader_exited
                && error.raw_os_error() == Some(libc::EPERM))
        {
            cleanup = "no-signalable-members";
        } else {
            return Err(error.into());
        }
    }
    let process_status = owned.child.wait()?;
    owned.reaped = true;
    let stdout_raw = fs::read(stdout_file)?;
    let stderr_raw = fs::read(stderr_file)?;
    let output_utf8 =
        std::str::from_utf8(&stdout_raw).is_ok() && std::str::from_utf8(&stderr_raw).is_ok();
    let stdout_sha256 = format!("{:x}", Sha256::digest(&stdout_raw));
    let stderr_sha256 = format!("{:x}", Sha256::digest(&stderr_raw));
    let stdout = String::from_utf8_lossy(&stdout_raw).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_raw).into_owned();
    let report = match &verified_plan {
        Some(verified) => parse_planned(&stdout, &verified.plan),
        None => parse_report(&stdout),
    };
    let accepted =
        output_utf8 && !timed_out && validate_report(&report, process_status.code()).is_ok();
    let status = if accepted {
        "pass"
    } else if !output_utf8 || timed_out || report.total.is_none() || report.malformed {
        "blocked"
    } else if !report.skipped.is_empty() {
        "not-run"
    } else {
        "fail"
    }
    .into();
    let run = Run {
        config,
        hashes,
        command,
        environment,
        exit_code: process_status.code(),
        timed_out,
        stdout,
        stderr,
        output_utf8,
        stdout_sha256,
        stderr_sha256,
        report,
        invocation_plan: verified_plan.map(|verified| verified.plan),
        accepted,
        status,
        cleanup: cleanup.into(),
    };
    fs::write(root.join("report.json"), serde_json::to_vec_pretty(&run)?)?;
    Ok(run)
}
/// Run the CLI with one JSON configuration path.
/// # Arguments
/// * `args` - Arguments after the executable name.
/// # Returns
/// Zero for accepted suite, one for failed suite, two for invalid setup.
/// # Examples
/// ```
/// assert_eq!(gossil::cli(&[]), 2);
/// ```
pub fn cli(args: &[String]) -> i32 {
    let result = (|| {
        if args.len() != 1 {
            return Err(RunnerError::Invalid(
                "usage: gossil-conformance CONFIG.json",
            ));
        }
        let bytes = fs::read(&args[0])?;
        let config = serde_json::from_slice(&bytes)?;
        execute(config)
    })();
    match result {
        Ok(run) => {
            println!("{}", run.config.run_dir.join("report.json").display());
            i32::from(!run.accepted)
        }
        Err(error) => {
            eprintln!("{error}");
            2
        }
    }
}

#[cfg(test)]
/// Verify runner diagnostics and owned-child cleanup boundaries.
/// Unit controls cover deterministic error paths and process ownership.
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::error::Error;

    /// Build an explicit positive-control configuration in fresh storage.
    fn config(root: &Path, name: &str) -> Config {
        let project = Path::new(env!("CARGO_MANIFEST_DIR"));
        Config {
            role: "reference".into(),
            revision: "b3a1c8efb2731dffbd5cc38c58ac349746de6da6".into(),
            tcl: project.join(".jlog/bin/tclsh"),
            driver: project.join("tests/fixtures/controls/tester.tcl"),
            executable: project.join(".jlog/bin/fossil"),
            suite: "pass".into(),
            run_dir: root.join(name),
            timeout_ms: 3000,
            seed: 20261117,
            inputs: vec![],
            capabilities: BTreeMap::new(),
            invocation_plan: None,
        }
    }

    #[test]
    /// Check diagnostic formatting and preserved error causes.
    fn preserves_error_causes_and_diagnostics() {
        let io = RunnerError::from(std::io::Error::from_raw_os_error(libc::EACCES));
        assert!(io.source().is_some());
        assert!(!io.to_string().is_empty());
        let json = RunnerError::from(serde_json::from_slice::<Config>(b"invalid").unwrap_err());
        assert!(json.source().is_some());
        assert!(json.to_string().contains("expected"));
        for error in [
            RunnerError::Invalid("bad config"),
            RunnerError::NotExecutable(PathBuf::from("inert")),
        ] {
            assert!(error.source().is_none());
            assert!(!error.to_string().is_empty());
        }
        assert!(observe(u32::MAX).is_err());
    }

    #[test]
    /// Check that interrupted child observation retries with local state.
    fn interrupted_observation_retries_without_global_state() {
        let temp = tempfile::tempdir().unwrap();
        let first = Cell::new(true);
        let observation = |pid| {
            if first.replace(false) {
                Err(std::io::ErrorKind::Interrupted.into())
            } else {
                observe(pid)
            }
        };
        let ops = ProcessOps {
            observe: &observation,
            group: &group,
            signal: &signal,
        };
        assert!(
            execute_with_ops(config(temp.path(), "retry"), &ops)
                .unwrap()
                .accepted
        );
    }

    #[test]
    /// Check that each injected process failure reaps the owned child.
    fn every_post_spawn_failure_reaps_owned_child() {
        let temp = tempfile::tempdir().unwrap();
        for failure in ["observation", "initial-group", "changed-group", "signal"] {
            let pid = Cell::new(0_u32);
            let calls = Cell::new(0);
            let observation = |value| {
                pid.set(value);
                if failure == "observation" {
                    Err(std::io::ErrorKind::Other.into())
                } else {
                    observe(value)
                }
            };
            let grouping = |value| {
                pid.set(value as u32);
                let call = calls.replace(calls.get() + 1);
                if failure == "initial-group" || (failure == "changed-group" && call > 0) {
                    0
                } else {
                    group(value)
                }
            };
            let signaling = |_value| Err(std::io::ErrorKind::Other.into());
            let ops = ProcessOps {
                observe: &observation,
                group: &grouping,
                signal: &signaling,
            };
            assert!(execute_with_ops(config(temp.path(), failure), &ops).is_err());
            assert!(pid.get() > 1);
            assert_eq!(
                observe(pid.get()).unwrap_err().raw_os_error(),
                Some(libc::ECHILD)
            );
        }
    }
}

/// Pack and unpack Fossil blobs with bounded zlib operations.
pub mod compression;
/// Compute Fossil byte digests independently of repository hash policy.
pub mod content_hash;
pub mod delta;

/// Own local database connections and exclusive write boundaries.
pub mod storage;
/// Encode and decode Fossil token bytes.
pub mod token_codec;

/// Parse structural artifacts with explicit resource limits.
pub mod artifact;
/// Compute legacy Fossil artifact checksums over bytes.
pub mod artifact_checksum;

/// Convert independently admitted Gest task fields into validated owned values.
pub mod gest_task_values;

/// Convert complete admitted canonical author fields into owned values.
pub mod gest_author_values;
/// Convert complete admitted canonical project fields into owned values.
pub mod gest_project_values;
/// Share decoded-value validation without exposing a second public authority.
mod gest_value_validation;

/// Load inherited configuration before native checkout selection.
mod checkout_config;
/// Prepare and publish bounded owner-local checkout lookup assertions.
mod checkout_locator;
/// Shared implementation for both product executable names.
pub mod product;
/// Adapt authenticated current checkout information for product executables.
mod vcs_info;
/// Adapt checkout-free native initialization for the product executables.
mod vcs_init;
/// Compose native registration and locator recovery for the product CLI.
mod vcs_open;
/// Adapt explicit scope selection and private all-tracked recording for product executables.
mod vcs_recording;
/// Print complete recorded checkout ancestry as bounded native-metadata NDJSON.
mod vcs_timeline;
/// Adapt explicit tracking enrollment and complete tracked status for product executables.
mod vcs_tracking;

/// Represent validated native dates as integer nanoseconds.
pub mod native_date;
/// Emit bounded task creation ticket bytes.
pub mod task_ticket;

/// Bounded Fossil glob parsing, matching and SQL expression construction.
pub mod glob;

/// Simplify filename bytes by the bounded lexical rules of Fossil.
pub mod file_path;

/// Find shortest paths and common ancestors in an explicit check-in graph, as Fossil does.
pub mod history;
