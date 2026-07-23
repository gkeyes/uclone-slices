use super::{EmergencyManifestV1, MAX_PACKAGES, SCHEMA_VERSION, derive_overall};
use crate::emergency_manifest::{
    ContainmentObligation, DiscoveryIntegrity, EmergencyManifestError, RuntimeOwnerRole,
};

impl EmergencyManifestV1 {
    pub(crate) fn validate(&self) -> Result<(), EmergencyManifestError> {
        if self.schema_version != SCHEMA_VERSION || self.generation == 0 {
            return Err(EmergencyManifestError::InvalidManifest(
                "unsupported schema or zero generation".to_owned(),
            ));
        }
        if self.packages.len() > MAX_PACKAGES {
            return Err(EmergencyManifestError::BoundExceeded("package entries"));
        }
        for pair in self.packages.windows(2) {
            let [previous, next] = pair else {
                return Err(EmergencyManifestError::Corrupt(
                    "invalid package ordering window".to_owned(),
                ));
            };
            if previous.package() >= next.package() {
                return Err(EmergencyManifestError::InvalidManifest(
                    "packages must be unique and sorted".to_owned(),
                ));
            }
        }
        if self
            .packages
            .iter()
            .any(|entry| entry.enrollment_epoch() == 0)
        {
            return Err(EmergencyManifestError::InvalidManifest(
                "enrollment epoch must be non-zero".to_owned(),
            ));
        }
        self.validate_runtime_owner()?;
        if derive_overall(self.discovery_integrity, &self.packages) != self.overall_disposition {
            return Err(EmergencyManifestError::InvalidManifest(
                "overall disposition does not match package obligations".to_owned(),
            ));
        }
        Ok(())
    }

    fn validate_runtime_owner(&self) -> Result<(), EmergencyManifestError> {
        let requires_owner = self
            .packages
            .iter()
            .any(|entry| entry.obligation() == ContainmentObligation::RuntimeOwned);
        let has_active_obligation = self.packages.iter().any(|entry| {
            matches!(
                entry.obligation(),
                ContainmentObligation::Held
                    | ContainmentObligation::HeldRecovery
                    | ContainmentObligation::RuntimeOwned
            )
        });
        if self.discovery_integrity == DiscoveryIntegrity::Untrusted
            && (requires_owner || self.runtime_owner_proof.is_some())
        {
            return Err(EmergencyManifestError::InvalidManifest(
                "untrusted discovery forbids Runtime ownership".to_owned(),
            ));
        }
        let Some(proof) = self.runtime_owner_proof.as_ref() else {
            return if requires_owner {
                Err(EmergencyManifestError::InvalidManifest(
                    "RuntimeOwned requires an exact owner proof".to_owned(),
                ))
            } else {
                Ok(())
            };
        };
        if !has_active_obligation {
            return Err(EmergencyManifestError::InvalidManifest(
                "owner proof is forbidden without an active containment obligation".to_owned(),
            ));
        }
        if proof.boot_id() != &self.boot_id {
            return Err(EmergencyManifestError::InvalidManifest(
                "Runtime owner proof belongs to another boot".to_owned(),
            ));
        }
        if proof.pid() == 0
            || proof.start_ticks() == 0
            || proof.role() != RuntimeOwnerRole::Ucloned
            || proof.lock_device() == 0
            || proof.lock_inode() == 0
        {
            return Err(EmergencyManifestError::InvalidManifest(
                "Runtime owner proof has an invalid process or lock identity".to_owned(),
            ));
        }
        Ok(())
    }
}
