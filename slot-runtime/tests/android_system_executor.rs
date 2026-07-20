#![doc = "Bounded Android executor integration tests."]

use std::ffi::OsString;
use std::time::Duration;

const MAX_PROCESS_OUTPUT_BYTES: usize = 16 * 1024;
const PROCESS_TIMEOUT_SECONDS: u64 = 5;

/// Production executor source compiled directly to exercise crate-private limits.
#[path = "../src/android/system/executor.rs"]
pub mod executor;

use executor::{
    ProcessExecutor, ProcessFailure, ProcessInvocation, ProcessOutput, StdProcessExecutor,
};

#[test]
fn fixed_invocation_exposes_production_limits_and_arguments() {
    // Given: typed arguments for one fixed executable.
    let arguments = [OsString::from("first"), OsString::from("second")];

    // When: the production invocation is constructed.
    let invocation = ProcessInvocation::fixed("/usr/bin/printf", arguments);

    // Then: every immutable invocation property is observable.
    assert_eq!(invocation.program(), "/usr/bin/printf");
    assert_eq!(
        invocation.args(),
        [OsString::from("first"), OsString::from("second")].as_slice()
    );
    assert_eq!(invocation.timeout(), Duration::from_secs(5));
    assert_eq!(invocation.output_limit(), 16 * 1024);
}

#[test]
fn process_output_constructor_exposes_fake_executor_values() {
    // Given: values returned by a fake executor.
    let stdout = b"captured".to_vec();

    // When: a process output is constructed.
    let output = ProcessOutput::new(Some(7), stdout);

    // Then: both public values are available without exposing fields.
    assert_eq!(output.exit_code(), Some(7));
    assert_eq!(output.stdout(), b"captured");
}

#[test]
fn executor_preserves_argv_without_shell_interpretation() -> Result<(), ProcessFailure> {
    // Given: arguments containing whitespace and shell syntax.
    let invocation = ProcessInvocation::for_test(
        "/usr/bin/printf",
        vec![
            OsString::from("<%s><%s>"),
            OsString::from("alpha beta"),
            OsString::from("$(false)"),
        ],
        Duration::from_secs(1),
        1024,
    );
    let mut executor = StdProcessExecutor::new();

    // When: the host executable is launched directly.
    let output = executor.execute(&invocation)?;

    // Then: argument boundaries and metacharacters remain literal.
    assert_eq!(output.exit_code(), Some(0));
    assert_eq!(output.stdout(), b"<alpha beta><$(false)>");
    Ok(())
}

#[test]
fn executor_reports_host_exit_code() -> Result<(), ProcessFailure> {
    // Given: a host executable with a stable failure status.
    let invocation =
        ProcessInvocation::for_test("/usr/bin/false", Vec::new(), Duration::from_secs(1), 1024);
    let mut executor = StdProcessExecutor::new();

    // When: the executable exits normally.
    let output = executor.execute(&invocation)?;

    // Then: its exact exit code is retained.
    assert_eq!(output.exit_code(), Some(1));
    assert!(output.stdout().is_empty());
    Ok(())
}

#[test]
fn executor_rejects_output_above_the_limit() {
    // Given: a five-byte host command and a four-byte limit.
    let invocation = ProcessInvocation::for_test(
        "/usr/bin/printf",
        vec![OsString::from("12345")],
        Duration::from_secs(1),
        4,
    );
    let mut executor = StdProcessExecutor::new();

    // When: the bounded executor captures stdout.
    let result = executor.execute(&invocation);

    // Then: only the first byte beyond the limit is observed.
    assert!(matches!(
        result,
        Err(ProcessFailure::OutputTooLarge { size: 5 })
    ));
}

#[test]
fn executor_terminates_a_process_at_the_deadline() {
    // Given: a process that runs beyond a short custom deadline.
    let invocation = ProcessInvocation::for_test(
        "/bin/sleep",
        vec![OsString::from("1")],
        Duration::from_millis(30),
        1024,
    );
    let mut executor = StdProcessExecutor::new();

    // When: the executor waits for the deadline.
    let result = executor.execute(&invocation);

    // Then: the process is killed and reported as timed out.
    assert!(matches!(result, Err(ProcessFailure::TimedOut)));
}
