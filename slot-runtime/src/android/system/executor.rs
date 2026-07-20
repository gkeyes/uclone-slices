#![allow(
    unreachable_pub,
    dead_code,
    unused_imports,
    reason = "path-included integration fixtures select executor APIs independently"
)]

use std::ffi::OsString;
use std::fmt::Debug;
use std::io;
use std::process::{Command, Stdio};
use std::time::Duration;

use super::{MAX_PROCESS_OUTPUT_BYTES, PROCESS_TIMEOUT_SECONDS};

#[path = "executor/runtime.rs"]
mod runtime;
use runtime::{execute_child, stop_and_reap};
/// One fixed executable invocation with bounded runtime and output.
#[derive(Debug, Clone)]
pub struct ProcessInvocation {
    program: &'static str,
    args: Vec<OsString>,
    timeout: Duration,
    output_limit: usize,
}

impl ProcessInvocation {
    /// Returns the fixed executable path.
    pub const fn program(&self) -> &'static str {
        self.program
    }
    /// Returns the exact argument vector passed to the executable.
    pub const fn args(&self) -> &[OsString] {
        self.args.as_slice()
    }
    /// Returns the maximum permitted process runtime.
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }
    /// Returns the maximum permitted stdout size in bytes.
    pub const fn output_limit(&self) -> usize {
        self.output_limit
    }
    pub(crate) fn fixed(program: &'static str, args: impl IntoIterator<Item = OsString>) -> Self {
        Self::fixed_with_limits(
            program,
            args,
            Duration::from_secs(PROCESS_TIMEOUT_SECONDS),
            MAX_PROCESS_OUTPUT_BYTES,
        )
    }

    pub(crate) fn fixed_with_limits(
        program: &'static str,
        args: impl IntoIterator<Item = OsString>,
        timeout: Duration,
        output_limit: usize,
    ) -> Self {
        Self {
            program,
            args: args.into_iter().collect(),
            timeout,
            output_limit,
        }
    }
    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "path-included integration executor fixtures use this constructor"
    )]
    pub(crate) const fn for_test(
        program: &'static str,
        args: Vec<OsString>,
        timeout: Duration,
        output_limit: usize,
    ) -> Self {
        Self {
            program,
            args,
            timeout,
            output_limit,
        }
    }
}
/// Captured result of one fixed executable invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    exit_code: Option<i32>,
    stdout: Vec<u8>,
}

impl ProcessOutput {
    /// Constructs output for production results or injected fake executors.
    pub const fn new(exit_code: Option<i32>, stdout: Vec<u8>) -> Self {
        Self { exit_code, stdout }
    }
    /// Returns the process exit code, or `None` when terminated by a signal.
    pub const fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }
    /// Returns the captured bounded stdout bytes.
    pub const fn stdout(&self) -> &[u8] {
        self.stdout.as_slice()
    }
}
/// Failure to start or safely complete a fixed executable invocation.
#[derive(Debug, thiserror::Error)]
pub enum ProcessFailure {
    /// An operating-system process or pipe operation failed.
    #[error("process operation failed: {0}")]
    Io(#[source] io::Error),
    /// The child exceeded its fixed deadline and was terminated.
    #[error("process exceeded its deadline")]
    TimedOut,
    /// Captured stdout exceeded the configured byte limit.
    #[error("process output exceeded its limit after {size} bytes")]
    OutputTooLarge {
        /// Number of bytes observed, capped at one byte above the limit.
        size: usize,
    },
}
/// Execution boundary for fixed, shell-free process invocations.
pub trait ProcessExecutor: Debug {
    /// Executes one invocation and captures bounded stdout.
    ///
    /// # Errors
    /// Returns a typed failure when process I/O fails, the deadline expires, or stdout is too
    /// large.
    fn execute(&mut self, invocation: &ProcessInvocation) -> Result<ProcessOutput, ProcessFailure>;
}
/// Standard shell-free executor backed by [`Command`].
#[derive(Debug, Default, Clone, Copy)]
pub struct StdProcessExecutor;

impl StdProcessExecutor {
    /// Constructs the stateless production executor.
    pub const fn new() -> Self {
        Self
    }
}
impl ProcessExecutor for StdProcessExecutor {
    fn execute(&mut self, invocation: &ProcessInvocation) -> Result<ProcessOutput, ProcessFailure> {
        let mut child = Command::new(invocation.program())
            .args(invocation.args())
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(ProcessFailure::Io)?;
        let Some(stdout) = child.stdout.take() else {
            stop_and_reap(&mut child).map_err(ProcessFailure::Io)?;
            return Err(ProcessFailure::Io(io::Error::other(
                "child stdout pipe missing",
            )));
        };
        execute_child(&mut child, stdout, invocation)
    }
}
