use crate::android::system::{
    ProcessExecutor, ProcessFailure, ProcessInvocation, StdProcessExecutor,
};

use super::command::MaterializerCommand;

pub(super) const MAX_EXECUTOR_OUTPUT: usize = 4_096;

#[doc = "Bounded stdout returned by one fixed materializer command."]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MaterializerOutput {
    stdout: Vec<u8>,
}

impl MaterializerOutput {
    #[doc = "Constructs output for an injected executor; the adapter enforces its bound."]
    pub const fn new(stdout: Vec<u8>) -> Self {
        Self { stdout }
    }

    #[doc = "Constructs an empty successful command output."]
    pub const fn empty() -> Self {
        Self { stdout: Vec::new() }
    }

    #[doc = "Returns the captured stdout bytes."]
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }
}

#[doc = "Stable failure from one fixed process execution."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MaterializerExecError {
    #[doc = "The fixed executable could not be started."]
    #[error("fixed materializer executable could not be started")]
    StartFailed,
    #[doc = "The fixed command returned a non-success status."]
    #[error("fixed materializer command was rejected")]
    Rejected,
    #[doc = "The fixed command stdout exceeded its hard bound."]
    #[error("fixed materializer command output exceeded its bound")]
    OutputTooLarge,
    #[doc = "The fixed command stdout could not be read."]
    #[error("fixed materializer command output could not be read")]
    ReadFailed,
    #[doc = "The fixed command exceeded its hard deadline."]
    #[error("fixed materializer command exceeded its deadline")]
    TimedOut,
}

#[doc = "Injected no-shell boundary for fixed, typed materializer commands."]
pub trait MaterializerExecutor: core::fmt::Debug {
    #[doc = "Executes one adapter-built command and returns bounded stdout."]
    fn execute(
        &mut self,
        command: &MaterializerCommand,
    ) -> Result<MaterializerOutput, MaterializerExecError>;
}

#[doc = "Production executor for fixed absolute Android materializer commands."]
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemMaterializerExecutor;

impl MaterializerExecutor for SystemMaterializerExecutor {
    fn execute(
        &mut self,
        command: &MaterializerCommand,
    ) -> Result<MaterializerOutput, MaterializerExecError> {
        let mut process_executor = StdProcessExecutor::new();
        Self::execute_with(command, &mut process_executor)
    }
}

impl SystemMaterializerExecutor {
    #[doc = "Executes through an injected process boundary using the command deadline."]
    pub(crate) fn execute_with<E: ProcessExecutor>(
        command: &MaterializerCommand,
        process_executor: &mut E,
    ) -> Result<MaterializerOutput, MaterializerExecError> {
        let invocation = ProcessInvocation::fixed_with_limits(
            command.program(),
            command.arguments().iter().cloned(),
            command.execution_timeout(),
            MAX_EXECUTOR_OUTPUT,
        );
        let output = process_executor
            .execute(&invocation)
            .map_err(map_process_failure)?;
        if output.exit_code() == Some(0) {
            Ok(MaterializerOutput::new(output.stdout().to_vec()))
        } else {
            Err(MaterializerExecError::Rejected)
        }
    }
}

fn map_process_failure(error: ProcessFailure) -> MaterializerExecError {
    match error {
        ProcessFailure::Io(source) if source.kind() == std::io::ErrorKind::NotFound => {
            MaterializerExecError::StartFailed
        }
        ProcessFailure::Io(_) => MaterializerExecError::ReadFailed,
        ProcessFailure::TimedOut => MaterializerExecError::TimedOut,
        ProcessFailure::OutputTooLarge { .. } => MaterializerExecError::OutputTooLarge,
    }
}
