use crate::domain::{GateSnapshot, ManagedPackage};
use crate::integrity::digest_json;

use super::super::EnrollmentAttemptError;
use super::anchors::CommittedAnchors;

#[doc = "Caller evidence that all three enrollment stores are published."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitProof {
    pub(super) managed: ManagedPackage,
    pub(super) managed_sha256: String,
    pub(super) enrollment_sha256: String,
    pub(super) base_catalog_sha256: String,
    pub(super) package_state_sha256: String,
}

impl CommitProof {
    #[doc = "Builds proof from all independently published store digests."]
    pub fn new(
        managed: ManagedPackage,
        enrollment_sha256: &str,
        base_catalog_sha256: &str,
        package_state_sha256: &str,
    ) -> Result<Self, EnrollmentAttemptError> {
        let managed_sha256 = digest_json(&managed)?;
        let proof = Self {
            managed,
            managed_sha256,
            enrollment_sha256: enrollment_sha256.to_ascii_lowercase(),
            base_catalog_sha256: base_catalog_sha256.to_ascii_lowercase(),
            package_state_sha256: package_state_sha256.to_ascii_lowercase(),
        };
        CommittedAnchors::from_proof(&proof)?;
        Ok(proof)
    }

    #[doc = "Returns the package contract proved by this evidence."]
    pub const fn managed(&self) -> &ManagedPackage {
        &self.managed
    }

    #[doc = "Returns the hash of the proved package contract."]
    pub fn managed_sha256(&self) -> &str {
        &self.managed_sha256
    }

    #[doc = "Builds intentionally incomplete proof for staged callers."]
    pub fn from_managed(managed: ManagedPackage) -> Result<Self, EnrollmentAttemptError> {
        let managed_sha256 = digest_json(&managed)?;
        Ok(Self {
            managed,
            managed_sha256,
            enrollment_sha256: String::new(),
            base_catalog_sha256: String::new(),
            package_state_sha256: String::new(),
        })
    }
}

impl From<ManagedPackage> for CommitProof {
    fn from(value: ManagedPackage) -> Self {
        let managed_sha256 = digest_json(&value).unwrap_or_default();
        Self {
            managed: value,
            managed_sha256,
            enrollment_sha256: String::new(),
            base_catalog_sha256: String::new(),
            package_state_sha256: String::new(),
        }
    }
}

impl From<&ManagedPackage> for CommitProof {
    fn from(value: &ManagedPackage) -> Self {
        value.clone().into()
    }
}

#[doc = "Caller evidence that exact gate restoration and lease retirement completed."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetirementProof {
    gate_snapshot: GateSnapshot,
    gate_restored: bool,
    lease_retired: bool,
}

impl RetirementProof {
    #[doc = "Builds proof after exact restoration and lease retirement completed."]
    pub const fn new(gate_snapshot: GateSnapshot) -> Self {
        Self {
            gate_snapshot,
            gate_restored: true,
            lease_retired: true,
        }
    }

    #[doc = "Builds proof with explicit side-effect results."]
    pub const fn verified(
        gate_snapshot: GateSnapshot,
        gate_restored: bool,
        lease_retired: bool,
    ) -> Self {
        Self {
            gate_snapshot,
            gate_restored,
            lease_retired,
        }
    }

    #[doc = "Returns the exact gate snapshot proved restored."]
    pub const fn gate_snapshot(self) -> GateSnapshot {
        self.gate_snapshot
    }

    #[doc = "Returns whether exact gate restoration was proved."]
    pub const fn gate_restored(self) -> bool {
        self.gate_restored
    }

    #[doc = "Returns whether durable lease retirement was proved."]
    pub const fn lease_retired(self) -> bool {
        self.lease_retired
    }
}

impl From<GateSnapshot> for RetirementProof {
    fn from(value: GateSnapshot) -> Self {
        Self::new(value)
    }
}

impl CommittedAnchors {
    pub(crate) fn from_proof(proof: &CommitProof) -> Result<Self, EnrollmentAttemptError> {
        use crate::domain::UserId;
        use crate::lifecycle::LifecycleState;
        let managed = &proof.managed;
        if managed.user_id() != UserId::PRIMARY
            || managed.package_name().as_str() != crate::protocol::ALLOWED_PACKAGE
            || !managed.active_slot().is_base()
            || managed.active_inodes() != managed.base_inodes()
            || managed.lifecycle_state() != LifecycleState::Normal
        {
            return Err(EnrollmentAttemptError::IncompleteCommitProof(
                "managed package is not a normal user-zero base enrollment".to_owned(),
            ));
        }
        for (label, value) in [
            ("managed", proof.managed_sha256.as_str()),
            ("enrollment", proof.enrollment_sha256.as_str()),
            ("base catalog", proof.base_catalog_sha256.as_str()),
            ("package state", proof.package_state_sha256.as_str()),
        ] {
            if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(EnrollmentAttemptError::IncompleteCommitProof(format!(
                    "{label} digest is not a SHA-256 value"
                )));
            }
        }
        if digest_json(managed)? != proof.managed_sha256 {
            return Err(EnrollmentAttemptError::IncompleteCommitProof(
                "managed package digest does not match the supplied package".to_owned(),
            ));
        }
        Ok(Self::new(
            proof.managed_sha256.clone(),
            managed.identity().clone(),
            managed.base_inodes(),
            proof.enrollment_sha256.clone(),
            proof.base_catalog_sha256.clone(),
            proof.package_state_sha256.clone(),
        ))
    }
}
