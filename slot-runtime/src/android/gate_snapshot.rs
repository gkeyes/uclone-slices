use std::thread;
use std::time::Duration;

use crate::domain::{GateSnapshot, PackageName, UserId};

use super::{PackageProbe, ProbeError};

const MAX_GATE_PROBE_ATTEMPTS: usize = 41;
const GATE_PROBE_INTERVAL: Duration = Duration::from_millis(25);

pub(super) fn retry<P: PackageProbe>(
    probe: &mut P,
    package: &PackageName,
    user_id: UserId,
) -> Result<GateSnapshot, ProbeError> {
    let mut last = ProbeError::Unavailable;
    for attempt in 0..MAX_GATE_PROBE_ATTEMPTS {
        match probe.gate_snapshot(package, user_id) {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) => last = error,
        }
        if attempt + 1 < MAX_GATE_PROBE_ATTEMPTS {
            thread::sleep(GATE_PROBE_INTERVAL);
        }
    }
    Err(last)
}
