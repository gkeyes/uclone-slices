use super::super::{ContainmentObligation, EmergencyManifestError, PackageContainment};
use super::EmergencyManifestV1;

impl EmergencyManifestV1 {
    /// Validates a same-boot generation and every package obligation transition.
    pub fn validate_transition_from(&self, previous: &Self) -> Result<(), EmergencyManifestError> {
        if self.boot_id() != previous.boot_id() {
            return Err(EmergencyManifestError::StaleBoot {
                expected: previous.boot_id().as_str().to_owned(),
                actual: self.boot_id().as_str().to_owned(),
            });
        }
        let expected_generation = previous
            .generation()
            .checked_add(1)
            .ok_or(EmergencyManifestError::BoundExceeded("generation"))?;
        if self.generation() != expected_generation {
            return Err(EmergencyManifestError::StaleGeneration {
                expected: expected_generation,
                actual: self.generation(),
            });
        }
        self.validate()?;
        for previous_entry in previous.packages() {
            match self.entry_for(previous_entry.package()) {
                Some(next) => validate_entry_transition(previous_entry, next)?,
                None => {
                    return Err(EmergencyManifestError::IllegalTransition {
                        package: previous_entry.package().to_string(),
                        previous: previous_entry.obligation().as_str().to_owned(),
                        next: "missing".to_owned(),
                    });
                }
            }
        }
        for next_entry in self.packages() {
            if previous.entry_for(next_entry.package()).is_none() {
                validate_new_entry(next_entry)?;
            }
        }
        Ok(())
    }

    fn entry_for(&self, package: &crate::domain::PackageName) -> Option<&PackageContainment> {
        self.packages()
            .binary_search_by(|entry| entry.package().cmp(package))
            .ok()
            .and_then(|index| self.packages().get(index))
    }
}

fn validate_new_entry(entry: &PackageContainment) -> Result<(), EmergencyManifestError> {
    if entry.enrollment_epoch() != 1 {
        return Err(EmergencyManifestError::EnrollmentEpoch {
            package: entry.package().to_string(),
            expected: 1,
            actual: entry.enrollment_epoch(),
        });
    }
    if matches!(
        entry.obligation(),
        ContainmentObligation::Held
            | ContainmentObligation::HeldRecovery
            | ContainmentObligation::NotManaged
    ) {
        Ok(())
    } else {
        Err(EmergencyManifestError::IllegalTransition {
            package: entry.package().to_string(),
            previous: "missing".to_owned(),
            next: entry.obligation().as_str().to_owned(),
        })
    }
}

fn validate_entry_transition(
    previous: &PackageContainment,
    next: &PackageContainment,
) -> Result<(), EmergencyManifestError> {
    let next_epoch = match (previous.obligation(), next.obligation()) {
        (
            ContainmentObligation::BaseRetired | ContainmentObligation::NotManaged,
            ContainmentObligation::Held | ContainmentObligation::HeldRecovery,
        ) => previous
            .enrollment_epoch()
            .checked_add(1)
            .ok_or(EmergencyManifestError::BoundExceeded("enrollment epoch"))?,
        _ => previous.enrollment_epoch(),
    };
    if next.enrollment_epoch() != next_epoch {
        return Err(EmergencyManifestError::EnrollmentEpoch {
            package: next.package().to_string(),
            expected: next_epoch,
            actual: next.enrollment_epoch(),
        });
    }
    if previous.obligation() == ContainmentObligation::BaseRetired
        && matches!(
            next.obligation(),
            ContainmentObligation::Held | ContainmentObligation::HeldRecovery
        )
    {
        return Ok(());
    }
    if previous.obligation().can_transition_to(next.obligation()) {
        Ok(())
    } else {
        Err(EmergencyManifestError::illegal(
            next.package().as_str(),
            previous.obligation(),
            next.obligation(),
        ))
    }
}
