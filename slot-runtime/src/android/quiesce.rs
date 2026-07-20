use std::thread;
use std::time::Duration;

use crate::domain::{PackageName, UserId};
use crate::runtime::PlatformError;

use super::backend::AndroidBackend;
use super::command::CommandRunner;
use super::lease::GateLeaseStore;
use super::policy::{Stage, failure};
use super::probe::{PackageProbe, ProbeError};

const PROCESS_EXIT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const PROCESS_EXIT_MAX_CHECKS: usize = 401;

impl<R: CommandRunner, P: PackageProbe, L: GateLeaseStore> AndroidBackend<R, P, L> {
    pub(super) fn wait_for_processes_to_exit(
        &mut self,
        package: &PackageName,
        user_id: UserId,
    ) -> Result<(), PlatformError> {
        for check in 0..PROCESS_EXIT_MAX_CHECKS {
            let detail = match self.probe.running_process_count(package, user_id) {
                Ok(0) => return Ok(()),
                Ok(_) => "processes_still_running",
                Err(ProbeError::InvalidResponse) => "probe_invalid_response",
                Err(ProbeError::Unavailable) => "probe_unavailable",
            };
            if check + 1 == PROCESS_EXIT_MAX_CHECKS {
                return Err(failure(Stage::Quiesce, detail));
            }
            thread::sleep(PROCESS_EXIT_POLL_INTERVAL);
        }
        Err(failure(Stage::Quiesce, "process_exit_wait_exhausted"))
    }
}
