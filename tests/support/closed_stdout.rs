//! Configure a child process to write stdout into a pipe with no read peer.
//!
//! The pipe is created after fork so concurrently spawned children cannot inherit
//! its read end and accidentally keep stdout writable.
//!
//! ## Recommendations
//! Use `configure` for CLI broken-pipe fixtures, then assert both the output error
//! and any durable effects that must survive it.
use std::{
    os::unix::process::CommandExt,
    process::{Command, Stdio},
};

/// Arrange for stdout writes to fail with EPIPE when the command starts.
///
/// # Arguments
/// * `command` - Child command whose stdout should have no read peer.
///
/// # Returns
/// The configured command. Descriptor setup errors are returned by `spawn`,
/// rather than appearing as the child's expected nonzero exit status.
pub fn configure(mut command: Command) -> Command {
    command.stdout(Stdio::null());
    // SAFETY: after fork the callback uses only async-signal-safe descriptor
    // operations and allocation-free OS error construction. It neither allocates
    // nor acquires locks, formats output or panics.
    unsafe {
        command.pre_exec(|| {
            let mut descriptors = [0; 2];
            if libc::pipe(descriptors.as_mut_ptr()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::close(descriptors[0]) != 0 {
                let error = std::io::Error::last_os_error();
                libc::close(descriptors[1]);
                return Err(error);
            }
            if libc::dup2(descriptors[1], libc::STDOUT_FILENO) == -1 {
                let error = std::io::Error::last_os_error();
                libc::close(descriptors[1]);
                return Err(error);
            }
            if descriptors[1] != libc::STDOUT_FILENO && libc::close(descriptors[1]) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
}
