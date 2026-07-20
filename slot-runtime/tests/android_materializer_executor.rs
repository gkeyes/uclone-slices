#![doc = "Materializer deadline selection and process-boundary injection tests."]
#![allow(
    dead_code,
    missing_docs,
    unreachable_pub,
    clippy::redundant_pub_crate,
    reason = "path-included production modules expose only the APIs under test"
)]

use std::path::Path;
use std::time::Duration;

const MAX_PROCESS_OUTPUT_BYTES: usize = 16 * 1024;
const PROCESS_TIMEOUT_SECONDS: u64 = 5;

#[path = "../src/android/system/executor.rs"]
mod process_executor;

mod android {
    pub mod system {
        pub use crate::process_executor::{
            ProcessExecutor, ProcessFailure, ProcessInvocation, ProcessOutput, StdProcessExecutor,
        };
    }
}

mod materializer {
    pub(crate) use uclone_slot_runtime::materializer::DataDomain;
}

mod target {
    pub(crate) use uclone_slot_runtime::target::FSPROBE_PATH;
}

#[path = "../src/android/materializer/command.rs"]
mod command;
#[path = "../src/android/materializer/executor.rs"]
mod materializer_executor;

use android::system::{ProcessExecutor, ProcessFailure, ProcessInvocation, ProcessOutput};
use command::MaterializerCommand;
use materializer::DataDomain;
use materializer_executor::{MaterializerExecError, SystemMaterializerExecutor};

#[derive(Debug)]
struct DeadlineAwareExecutor {
    work: Duration,
    observed_timeout: Option<Duration>,
}

impl ProcessExecutor for DeadlineAwareExecutor {
    fn execute(&mut self, invocation: &ProcessInvocation) -> Result<ProcessOutput, ProcessFailure> {
        self.observed_timeout = Some(invocation.timeout());
        if self.work > invocation.timeout() {
            return Err(ProcessFailure::TimedOut);
        }
        Ok(ProcessOutput::new(Some(0), b"ok".to_vec()))
    }
}

#[test]
fn slow_recursive_work_within_budget_is_forwarded_and_succeeds() {
    let command = MaterializerCommand::copy(
        DataDomain::Ce,
        Path::new("/data/source"),
        Path::new("/data/target"),
    );
    let mut process = DeadlineAwareExecutor {
        work: Duration::from_secs(20),
        observed_timeout: None,
    };

    let result = SystemMaterializerExecutor::execute_with(&command, &mut process);

    assert!(matches!(result, Ok(output) if output.stdout() == b"ok"));
    assert_eq!(process.observed_timeout, Some(Duration::from_mins(30)));
}

#[test]
fn work_over_a_fast_metadata_budget_remains_a_typed_timeout() {
    let command = MaterializerCommand::get_selinux(DataDomain::De, Path::new("/data/target"));
    let mut process = DeadlineAwareExecutor {
        work: Duration::from_secs(6),
        observed_timeout: None,
    };

    let result = SystemMaterializerExecutor::execute_with(&command, &mut process);

    assert_eq!(result, Err(MaterializerExecError::TimedOut));
    assert_eq!(process.observed_timeout, Some(Duration::from_secs(5)));
}

#[test]
fn command_timeout_is_always_finite() {
    for command in [
        MaterializerCommand::copy(
            DataDomain::Ce,
            Path::new("/data/source"),
            Path::new("/data/target"),
        ),
        MaterializerCommand::chcon(
            DataDomain::De,
            "u:object_r:app_data_file:s0",
            Path::new("/data/target"),
        ),
        MaterializerCommand::get_selinux(DataDomain::Ce, Path::new("/data/target")),
    ] {
        assert!(!command.execution_timeout().is_zero());
        assert!(command.execution_timeout() <= Duration::from_mins(30));
    }
}
