use std::thread;
use std::time::Duration;

use crate::domain::{PackageName, PackageObservation, UserId};

use super::{PackageProbe, ProbeError};

const MAX_ATTEMPTS: usize = 401;
const INTERVAL: Duration = Duration::from_millis(25);

pub(super) fn retry<P: PackageProbe>(
    probe: &mut P,
    package: &PackageName,
    user_id: UserId,
) -> Result<PackageObservation, ProbeError> {
    let mut last = ProbeError::Unavailable;
    for attempt in 0..MAX_ATTEMPTS {
        match probe.observe_package(package, user_id) {
            Ok(observation) => return Ok(observation),
            Err(error) => last = error,
        }
        if attempt + 1 < MAX_ATTEMPTS {
            thread::sleep(INTERVAL);
        }
    }
    Err(last)
}
