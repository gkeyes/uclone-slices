#![doc = "Bounded Android materializer evidence tests with an injected executor."]
#![allow(
    missing_docs,
    unreachable_pub,
    dead_code,
    unused_imports,
    clippy::redundant_pub_crate,
    clippy::unnecessary_wraps,
    clippy::unused_self,
    reason = "path-included executor fixture exposes only the APIs under test"
)]

use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::time::Duration;

use uclone_slot_runtime as runtime;

mod android {
    pub mod system {
        use super::super::{Duration, OsString, io};

        #[derive(Debug)]
        pub enum ProcessFailure {
            Io(io::Error),
            TimedOut,
            OutputTooLarge { size: usize },
        }

        #[derive(Debug)]
        pub struct ProcessInvocation;

        impl ProcessInvocation {
            pub(crate) fn fixed_with_limits<I>(
                _program: &'static str,
                _args: I,
                _timeout: Duration,
                _output_limit: usize,
            ) -> Self
            where
                I: IntoIterator<Item = OsString>,
            {
                Self
            }
        }

        #[derive(Debug)]
        pub struct ProcessOutput;

        impl ProcessOutput {
            pub const fn exit_code(&self) -> Option<i32> {
                Some(0)
            }

            pub const fn stdout(&self) -> &[u8] {
                &[]
            }
        }

        pub trait ProcessExecutor: core::fmt::Debug {
            fn execute(
                &mut self,
                _invocation: &ProcessInvocation,
            ) -> Result<ProcessOutput, ProcessFailure>;
        }

        #[derive(Debug, Default, Clone, Copy)]
        pub struct StdProcessExecutor;

        impl StdProcessExecutor {
            pub const fn new() -> Self {
                Self
            }
        }

        impl ProcessExecutor for StdProcessExecutor {
            fn execute(
                &mut self,
                _invocation: &ProcessInvocation,
            ) -> Result<ProcessOutput, ProcessFailure> {
                Err(ProcessFailure::Io(io::Error::other(
                    "process execution is not used by evidence fixtures",
                )))
            }
        }
    }
}

mod catalog {
    pub(crate) use crate::runtime::catalog::PathSecurityProof;
}

mod materializer {
    pub(crate) use crate::runtime::materializer::{BackendFailure, DataDomain};
}

mod target {
    pub(crate) use crate::runtime::target::FSPROBE_PATH;
}

#[path = "../src/android/materializer/command.rs"]
mod command;
#[path = "../src/android/materializer/executor.rs"]
mod executor;

mod tree {
    #[derive(Debug)]
    pub(super) struct TreeInspection;

    impl TreeInspection {
        pub(super) const fn uid(&self) -> u32 {
            0
        }

        pub(super) const fn gid(&self) -> u32 {
            0
        }

        pub(super) const fn mode(&self) -> u32 {
            0o700
        }
    }
}

#[path = "../src/android/materializer/evidence.rs"]
mod evidence;

use command::MaterializerCommand;
use executor::{
    MAX_EXECUTOR_OUTPUT, MaterializerExecError, MaterializerExecutor, MaterializerOutput,
};
use materializer::{BackendFailure, DataDomain};

#[derive(Debug, Clone)]
struct FakeExecutor {
    response: Result<MaterializerOutput, MaterializerExecError>,
}

impl FakeExecutor {
    const fn output(stdout: Vec<u8>) -> Self {
        Self {
            response: Ok(MaterializerOutput::new(stdout)),
        }
    }

    const fn error(error: MaterializerExecError) -> Self {
        Self {
            response: Err(error),
        }
    }
}

impl MaterializerExecutor for FakeExecutor {
    fn execute(
        &mut self,
        _command: &MaterializerCommand,
    ) -> Result<MaterializerOutput, MaterializerExecError> {
        self.response.clone()
    }
}

fn code<T>(result: &Result<T, BackendFailure>) -> Option<&str> {
    result.as_ref().err().map(BackendFailure::code)
}

#[test]
fn parsers_normalize_only_allowed_transport_suffixes() {
    assert_eq!(
        evidence::parse_selinux(b"u:object_r:app_data_file:s0\0\r\n"),
        Ok(String::from("u:object_r:app_data_file:s0"))
    );
    assert_eq!(
        evidence::parse_policy(
            b"ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789\r\n"
        ),
        Ok(String::from(
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
        ))
    );
    assert_eq!(
        code(&evidence::parse_selinux(b"bad\ncontext")),
        Some("selinux_context_invalid")
    );
    assert_eq!(
        code(&evidence::parse_policy(b"abcd")),
        Some("fscrypt_policy_invalid")
    );
}

#[test]
fn injected_output_above_the_executor_bound_is_rejected() {
    let mut executor = FakeExecutor::output(vec![b'x'; MAX_EXECUTOR_OUTPUT + 1]);

    let result =
        evidence::read_selinux(&mut executor, Path::new("/host-only/fake"), DataDomain::Ce);

    assert_eq!(code(&result), Some("command_output_too_large"));
}

#[test]
fn fscrypt_helper_unavailable_and_execution_errors_are_distinct() {
    let path = Path::new("/host-only/fake");
    let mut unavailable = FakeExecutor::error(MaterializerExecError::StartFailed);
    assert_eq!(
        code(&evidence::read_fscrypt_policy(
            &mut unavailable,
            path,
            DataDomain::Ce
        )),
        Some("fscrypt_helper_unavailable")
    );

    for failure in [
        MaterializerExecError::Rejected,
        MaterializerExecError::OutputTooLarge,
        MaterializerExecError::ReadFailed,
    ] {
        let mut executor = FakeExecutor::error(failure);
        assert_eq!(
            code(&evidence::read_fscrypt_policy(
                &mut executor,
                path,
                DataDomain::De
            )),
            Some("fscrypt_helper_failed")
        );
    }
}

#[test]
fn fscrypt_invalid_and_mismatched_values_remain_fail_closed() {
    let path = Path::new("/host-only/fake");
    let mut invalid = FakeExecutor::output(b"not-a-policy\n".to_vec());
    assert_eq!(
        code(&evidence::read_fscrypt_policy(
            &mut invalid,
            path,
            DataDomain::Ce
        )),
        Some("fscrypt_policy_invalid")
    );

    let expected = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let observed = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let mut mismatch = FakeExecutor::output(format!("{observed}\n").into_bytes());
    let result = evidence::read_fscrypt_policy(&mut mismatch, path, DataDomain::Ce);
    assert_eq!(result.as_deref(), Ok(observed));
    assert_ne!(result.as_deref(), Ok(expected));
}
