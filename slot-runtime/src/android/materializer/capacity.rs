use std::path::Path;

use crate::domain::ManagedPackage;
use crate::materializer::{BackendFailure, BaseAnchor, DataDomain};

use super::command::MaterializerCommand;
use super::executor::{MAX_EXECUTOR_OUTPUT, MaterializerExecutor};
use super::policy;

const SHARED_RESERVE_BYTES: u64 = 512 * 1024 * 1024;
const SPLIT_RESERVE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SpaceEvidence {
    device_id: u64,
    available_bytes: u64,
}

pub(super) fn verify<E: MaterializerExecutor>(
    executor: &mut E,
    package: &ManagedPackage,
    base: &BaseAnchor,
) -> Result<(), BackendFailure> {
    let ce = read_space(
        executor,
        &policy::base(package.package_name(), DataDomain::Ce),
        DataDomain::Ce,
    )?;
    let de = read_space(
        executor,
        &policy::base(package.package_name(), DataDomain::De),
        DataDomain::De,
    )?;
    if ce.device_id != base.domain(DataDomain::Ce).device_id()
        || de.device_id != base.domain(DataDomain::De).device_id()
    {
        return Err(BackendFailure::new("capacity_device_mismatch"));
    }
    if ce.device_id == de.device_id {
        let total = base
            .bytes()
            .total()
            .ok_or_else(|| BackendFailure::new("capacity_overflow"))?;
        require(
            ce.available_bytes.min(de.available_bytes),
            total,
            SHARED_RESERVE_BYTES,
        )
    } else {
        require(
            ce.available_bytes,
            base.bytes().domain(DataDomain::Ce),
            SPLIT_RESERVE_BYTES,
        )?;
        require(
            de.available_bytes,
            base.bytes().domain(DataDomain::De),
            SPLIT_RESERVE_BYTES,
        )
    }
}

fn read_space<E: MaterializerExecutor>(
    executor: &mut E,
    path: &Path,
    domain: DataDomain,
) -> Result<SpaceEvidence, BackendFailure> {
    let output = executor
        .execute(&MaterializerCommand::free_space(domain, path))
        .map_err(|_| BackendFailure::new("capacity_probe_failed"))?;
    if output.stdout().len() > MAX_EXECUTOR_OUTPUT {
        return Err(BackendFailure::new("command_output_too_large"));
    }
    parse_space(output.stdout())
}

fn parse_space(bytes: &[u8]) -> Result<SpaceEvidence, BackendFailure> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| BackendFailure::new("capacity_probe_invalid"))?;
    let value = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .ok_or_else(|| BackendFailure::new("capacity_probe_invalid"))?;
    let (device, available) = value
        .split_once(':')
        .ok_or_else(|| BackendFailure::new("capacity_probe_invalid"))?;
    if device.is_empty() || available.is_empty() || value.matches(':').count() != 1 {
        return Err(BackendFailure::new("capacity_probe_invalid"));
    }
    let device_id = device
        .parse::<u64>()
        .map_err(|_| BackendFailure::new("capacity_probe_invalid"))?;
    let available_bytes = available
        .parse::<u64>()
        .map_err(|_| BackendFailure::new("capacity_probe_invalid"))?;
    if device_id == 0 {
        return Err(BackendFailure::new("capacity_probe_invalid"));
    }
    Ok(SpaceEvidence {
        device_id,
        available_bytes,
    })
}

fn require(available: u64, source: u64, reserve: u64) -> Result<(), BackendFailure> {
    let required = source
        .checked_mul(2)
        .and_then(|value| value.checked_add(reserve.max(source / 5)))
        .ok_or_else(|| BackendFailure::new("capacity_overflow"))?;
    if available >= required {
        Ok(())
    } else {
        Err(BackendFailure::new("insufficient_capacity"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_space_evidence() {
        assert_eq!(
            parse_space(b"253:987654321\n"),
            Ok(SpaceEvidence {
                device_id: 253,
                available_bytes: 987_654_321,
            })
        );
        for invalid in [b"253 12\n".as_slice(), b"0:12\n", b"253:-1\n", b"253:12"] {
            assert_eq!(
                parse_space(invalid).map_err(|error| error.code().to_owned()),
                Err("capacity_probe_invalid".to_owned())
            );
        }
    }

    #[test]
    fn requires_two_copies_plus_the_fixed_reserve() {
        assert!(require(SHARED_RESERVE_BYTES + 200, 100, SHARED_RESERVE_BYTES).is_ok());
        assert_eq!(
            require(SHARED_RESERVE_BYTES + 199, 100, SHARED_RESERVE_BYTES)
                .unwrap_err()
                .code(),
            "insufficient_capacity"
        );
    }
}
