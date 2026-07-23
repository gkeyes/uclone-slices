use super::{ContainmentObligation, EmergencyManifestError, validate_new_entry};
use crate::emergency_manifest::{EmergencyManifestV1, PackageContainment};

impl EmergencyManifestV1 {
    /// Rejects boot-root records that claim Runtime ownership without a live proof.
    pub(crate) fn validate_new_boot_root(&self) -> Result<(), EmergencyManifestError> {
        if let Some(entry) = self
            .packages()
            .iter()
            .find(|entry| entry.obligation() == ContainmentObligation::RuntimeOwned)
        {
            return Err(EmergencyManifestError::IllegalTransition {
                package: entry.package().to_string(),
                previous: "new_boot".to_owned(),
                next: entry.obligation().as_str().to_owned(),
            });
        }
        Ok(())
    }

    /// Validates generation one without discarding the previous boot's durable fences.
    pub(crate) fn validate_new_boot_from(
        &self,
        previous: &Self,
    ) -> Result<(), EmergencyManifestError> {
        if self.boot_id() == previous.boot_id() {
            return Err(EmergencyManifestError::StaleBoot {
                expected: "new boot id".to_owned(),
                actual: self.boot_id().as_str().to_owned(),
            });
        }
        if self.generation() != 1 {
            return Err(EmergencyManifestError::StaleGeneration {
                expected: 1,
                actual: self.generation(),
            });
        }
        self.validate()?;
        self.validate_new_boot_root()?;
        for previous_entry in previous.packages() {
            let Some(next) = self.entry_for(previous_entry.package()) else {
                return Err(EmergencyManifestError::IllegalTransition {
                    package: previous_entry.package().to_string(),
                    previous: previous_entry.obligation().as_str().to_owned(),
                    next: "missing".to_owned(),
                });
            };
            validate_new_boot_entry_transition(previous_entry, next)?;
        }
        for next_entry in self.packages() {
            if previous.entry_for(next_entry.package()).is_none() {
                validate_new_entry(next_entry)?;
            }
        }
        Ok(())
    }
}

fn validate_new_boot_entry_transition(
    previous: &PackageContainment,
    next: &PackageContainment,
) -> Result<(), EmergencyManifestError> {
    if next.enrollment_epoch() != previous.enrollment_epoch() {
        return Err(EmergencyManifestError::EnrollmentEpoch {
            package: next.package().to_string(),
            expected: previous.enrollment_epoch(),
            actual: next.enrollment_epoch(),
        });
    }
    if previous
        .obligation()
        .can_transition_new_boot_to(next.obligation())
    {
        Ok(())
    } else {
        Err(EmergencyManifestError::illegal(
            next.package().as_str(),
            previous.obligation(),
            next.obligation(),
        ))
    }
}
