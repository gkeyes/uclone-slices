use super::{EmergencyGateLease, EmergencyGatePhase, GateLease, GateLeaseError, StoredGateLease};
use crate::domain::{DataInodes, GateSnapshot, PackageEnabledState, PackageName, UserId};

impl EmergencyGateLease {
    pub(super) fn bytes(&self) -> Vec<u8> {
        encode(&LeaseRecord {
            package: self.package_name(),
            user_id: self.user_id(),
            snapshot: self.snapshot(),
            phase: Some(self.phase()),
            base_inodes: None,
        })
    }
}

impl GateLease {
    pub(super) fn bytes(&self) -> Vec<u8> {
        encode(&LeaseRecord {
            package: self.package_name(),
            user_id: self.user_id(),
            snapshot: self.snapshot(),
            phase: None,
            base_inodes: Some(self.base_inodes()),
        })
    }
}

impl StoredGateLease {
    pub(super) fn parse(content: &str) -> Result<Self, GateLeaseError> {
        let content = content
            .strip_suffix('\n')
            .ok_or(GateLeaseError::InvalidArtifact)?;
        let mut lines = content.lines();
        let package_name = PackageName::parse(field(&mut lines, "package=")?)
            .map_err(|_| GateLeaseError::InvalidArtifact)?;
        let user_id = field(&mut lines, "user_id=")?
            .parse::<u32>()
            .map_err(|_| GateLeaseError::InvalidArtifact)?;
        let user_id = UserId::try_from(user_id).map_err(|_| GateLeaseError::InvalidArtifact)?;
        let enabled_state = parse_enabled_state(field(&mut lines, "enabled_state=")?)?;
        let suspended = field(&mut lines, "suspended=")?
            .parse::<bool>()
            .map_err(|_| GateLeaseError::InvalidArtifact)?;
        let first_anchor = lines.next().ok_or(GateLeaseError::InvalidArtifact)?;
        let (phase, ce_raw) = match first_anchor.strip_prefix("phase=") {
            Some(raw) => (
                Some(parse_phase(raw)?),
                field(&mut lines, "base_ce_inode=")?,
            ),
            None => (
                None,
                first_anchor
                    .strip_prefix("base_ce_inode=")
                    .ok_or(GateLeaseError::InvalidArtifact)?,
            ),
        };
        let ce = parse_inode(ce_raw)?;
        let de = parse_inode(field(&mut lines, "base_de_inode=")?)?;
        if lines.next().is_some() {
            return Err(GateLeaseError::InvalidArtifact);
        }
        let snapshot = GateSnapshot::new(enabled_state, suspended);
        if phase.is_some() && (ce != 0 || de != 0) {
            return Err(GateLeaseError::InvalidArtifact);
        }
        match (ce, de) {
            (0, 0) => Ok(Self::Emergency(EmergencyGateLease {
                package_name,
                user_id,
                snapshot,
                phase: phase.ok_or(GateLeaseError::InvalidArtifact)?,
            })),
            (0, _) | (_, 0) => Err(GateLeaseError::InvalidArtifact),
            (ce, de) => Ok(Self::Enrolled(GateLease {
                package_name,
                user_id,
                snapshot,
                base_inodes: DataInodes::new(ce, de)
                    .map_err(|_| GateLeaseError::InvalidArtifact)?,
            })),
        }
    }
}

struct LeaseRecord<'a> {
    package: &'a PackageName,
    user_id: UserId,
    snapshot: GateSnapshot,
    phase: Option<EmergencyGatePhase>,
    base_inodes: Option<DataInodes>,
}

fn encode(record: &LeaseRecord<'_>) -> Vec<u8> {
    let (ce, de) = record
        .base_inodes
        .map_or((0, 0), |inodes| (inodes.ce().get(), inodes.de().get()));
    let phase_line = record.phase.map_or_else(String::new, |phase| {
        format!("phase={}\n", phase_name(phase))
    });
    format!(
        "package={}\nuser_id={}\nenabled_state={}\nsuspended={}\n{phase_line}base_ce_inode={ce}\nbase_de_inode={de}\n",
        record.package,
        record.user_id.get(),
        enabled_state_name(record.snapshot.enabled_state()),
        record.snapshot.suspended(),
    )
    .into_bytes()
}

fn field<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
    prefix: &str,
) -> Result<&'a str, GateLeaseError> {
    lines
        .next()
        .and_then(|line| line.strip_prefix(prefix))
        .ok_or(GateLeaseError::InvalidArtifact)
}

fn parse_inode(raw: &str) -> Result<u64, GateLeaseError> {
    raw.parse::<u64>()
        .map_err(|_| GateLeaseError::InvalidArtifact)
}

fn parse_enabled_state(raw: &str) -> Result<PackageEnabledState, GateLeaseError> {
    match raw {
        "default" => Ok(PackageEnabledState::Default),
        "enabled" => Ok(PackageEnabledState::Enabled),
        "disabled" => Ok(PackageEnabledState::Disabled),
        "disabled-user" => Ok(PackageEnabledState::DisabledUser),
        "disabled-until-used" => Ok(PackageEnabledState::DisabledUntilUsed),
        _ => Err(GateLeaseError::InvalidArtifact),
    }
}

fn parse_phase(raw: &str) -> Result<EmergencyGatePhase, GateLeaseError> {
    match raw {
        "prepared" => Ok(EmergencyGatePhase::Prepared),
        "held" => Ok(EmergencyGatePhase::Held),
        _ => Err(GateLeaseError::InvalidArtifact),
    }
}

const fn phase_name(phase: EmergencyGatePhase) -> &'static str {
    match phase {
        EmergencyGatePhase::Prepared => "prepared",
        EmergencyGatePhase::Held => "held",
    }
}

const fn enabled_state_name(state: PackageEnabledState) -> &'static str {
    match state {
        PackageEnabledState::Default => "default",
        PackageEnabledState::Enabled => "enabled",
        PackageEnabledState::Disabled => "disabled",
        PackageEnabledState::DisabledUser => "disabled-user",
        PackageEnabledState::DisabledUntilUsed => "disabled-until-used",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "validated codec fixtures")]
mod tests {
    use super::*;

    #[test]
    fn preliminary_lease_has_explicit_prepared_phase_and_two_zero_anchors() {
        let lease = EmergencyGateLease {
            package_name: PackageName::parse("com.uclone.slotprobe").unwrap(),
            user_id: UserId::PRIMARY,
            snapshot: GateSnapshot::new(PackageEnabledState::Enabled, true),
            phase: EmergencyGatePhase::Prepared,
        };

        let bytes = String::from_utf8(lease.bytes()).unwrap();

        assert_eq!(
            bytes,
            "package=com.uclone.slotprobe\nuser_id=0\nenabled_state=enabled\nsuspended=true\nphase=prepared\nbase_ce_inode=0\nbase_de_inode=0\n"
        );
        assert!(matches!(
            StoredGateLease::parse(&bytes).unwrap(),
            StoredGateLease::Emergency(_)
        ));
    }

    #[test]
    fn old_zero_anchor_lease_without_phase_is_rejected_closed() {
        let content = "package=com.uclone.slotprobe\nuser_id=0\nenabled_state=default\nsuspended=false\nbase_ce_inode=0\nbase_de_inode=0\n";

        assert_eq!(
            StoredGateLease::parse(content),
            Err(GateLeaseError::InvalidArtifact)
        );
    }

    #[test]
    fn held_phase_round_trips_without_changing_the_original_snapshot() {
        let lease = EmergencyGateLease {
            package_name: PackageName::parse("com.uclone.slotprobe").unwrap(),
            user_id: UserId::PRIMARY,
            snapshot: GateSnapshot::new(PackageEnabledState::Default, false),
            phase: EmergencyGatePhase::Prepared,
        };

        let held = lease.held();
        let parsed = StoredGateLease::parse(&String::from_utf8(held.bytes()).unwrap()).unwrap();

        assert!(matches!(
            parsed,
            StoredGateLease::Emergency(ref parsed)
                if parsed.phase() == EmergencyGatePhase::Held
                    && parsed.snapshot() == lease.snapshot()
        ));
    }

    #[test]
    fn mixed_zero_and_nonzero_anchors_are_never_a_valid_lease() {
        let content = "package=com.uclone.slotprobe\nuser_id=0\nenabled_state=default\nsuspended=false\nbase_ce_inode=0\nbase_de_inode=42\n";

        assert_eq!(
            StoredGateLease::parse(content),
            Err(GateLeaseError::InvalidArtifact)
        );
    }

    #[test]
    fn enrolled_lease_matches_the_rescue_scripts_nonzero_six_line_contract() {
        let package_name = PackageName::parse("com.uclone.slotprobe").unwrap();
        let lease = GateLease {
            package_name,
            user_id: UserId::PRIMARY,
            snapshot: GateSnapshot::new(PackageEnabledState::Default, false),
            base_inodes: DataInodes::new(101, 202).unwrap(),
        };

        let bytes = String::from_utf8(lease.bytes()).unwrap();

        assert_eq!(
            bytes,
            "package=com.uclone.slotprobe\nuser_id=0\nenabled_state=default\nsuspended=false\nbase_ce_inode=101\nbase_de_inode=202\n"
        );
        let parsed = StoredGateLease::parse(&bytes).unwrap();
        assert!(matches!(parsed, StoredGateLease::Enrolled(_)));
        assert_eq!(
            parsed.base_inodes(),
            Some(DataInodes::new(101, 202).unwrap())
        );
    }
}
