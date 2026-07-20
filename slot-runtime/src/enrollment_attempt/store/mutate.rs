use crate::domain::{PackageKey, UserId};

use super::super::super::model::{EnrollmentAttempt, EnrollmentAttemptPhase};
use super::super::super::storage;
use super::super::super::{CommitProof, CommittedAnchors, EnrollmentAttemptError, RetirementProof};
use super::super::EnrollmentAttemptStore;

impl EnrollmentAttemptStore {
    #[doc = "Appends a recovery-required generation without overwriting history."]
    pub fn mark_recovery_required(
        &self,
        package: &PackageKey,
    ) -> Result<EnrollmentAttempt, EnrollmentAttemptError> {
        let current = self
            .load(package)?
            .ok_or_else(|| EnrollmentAttemptError::NotFound(package.clone()))?;
        if current.is_authoritative() && current.phase() == EnrollmentAttemptPhase::Committed {
            return Err(EnrollmentAttemptError::Invalid(
                "committed attempt cannot be downgraded".to_owned(),
            ));
        }
        if current.phase() == EnrollmentAttemptPhase::RecoveryRequired {
            return Ok(current);
        }
        self.append(
            package,
            &current,
            EnrollmentAttemptPhase::RecoveryRequired,
            current.committed_anchors(),
        )
    }

    #[doc = "Publishes a commit generation, then a separate durable commit marker."]
    pub fn commit<P: Into<CommitProof>>(
        &self,
        package: &PackageKey,
        proof: P,
    ) -> Result<EnrollmentAttempt, EnrollmentAttemptError> {
        let proof = proof.into();
        let current = self
            .load(package)?
            .ok_or_else(|| EnrollmentAttemptError::NotFound(package.clone()))?;
        if current.is_authoritative() && current.phase() == EnrollmentAttemptPhase::Committed {
            return Err(EnrollmentAttemptError::Invalid(
                "enrollment attempt is already committed".to_owned(),
            ));
        }
        if proof.managed().package_name() != package.package_name()
            || proof.managed().user_id() != UserId::PRIMARY
        {
            return Err(EnrollmentAttemptError::IncompleteCommitProof(
                "managed package key differs from attempt".to_owned(),
            ));
        }
        let anchors = CommittedAnchors::from_proof(&proof)?;
        let committed = self.append(
            package,
            &current,
            EnrollmentAttemptPhase::Committed,
            Some(anchors.clone()),
        )?;
        let marker = storage::CommitMarker::new(package, &committed, &anchors)?;
        storage::write_marker(&self.package_path(package), &marker)?;
        Ok(committed.with_authority(true))
    }

    #[doc = "Retires an authoritative committed attempt only after exact proof."]
    pub fn retire<P: Into<RetirementProof>>(
        &self,
        package: &PackageKey,
        proof: P,
    ) -> Result<(), EnrollmentAttemptError> {
        let proof = proof.into();
        let current = self
            .load(package)?
            .ok_or_else(|| EnrollmentAttemptError::NotFound(package.clone()))?;
        if current.phase() != EnrollmentAttemptPhase::Committed || !current.is_authoritative() {
            return Err(EnrollmentAttemptError::Invalid(
                "only an authoritative commit may retire".to_owned(),
            ));
        }
        Self::require_retirement_proof(&current, proof)?;
        super::cleanup::remove_attempt(self, package)
    }

    #[doc = "Retires a still-pending prepublication attempt after exact cleanup proof."]
    pub fn abort_pending<P: Into<RetirementProof>>(
        &self,
        package: &PackageKey,
        proof: P,
    ) -> Result<(), EnrollmentAttemptError> {
        let proof = proof.into();
        let current = self
            .load(package)?
            .ok_or_else(|| EnrollmentAttemptError::NotFound(package.clone()))?;
        if current.phase() != EnrollmentAttemptPhase::Pending {
            return Err(EnrollmentAttemptError::Invalid(
                "only a still-pending attempt may abort".to_owned(),
            ));
        }
        Self::require_retirement_proof(&current, proof)?;
        super::cleanup::remove_attempt(self, package)
    }

    #[doc = "Convenience alias for callers with an explicit publication proof."]
    pub fn commit_published(
        &self,
        package: &PackageKey,
        proof: &CommitProof,
    ) -> Result<EnrollmentAttempt, EnrollmentAttemptError> {
        self.commit(package, proof.clone())
    }

    fn require_retirement_proof(
        current: &EnrollmentAttempt,
        proof: RetirementProof,
    ) -> Result<(), EnrollmentAttemptError> {
        if current.gate_snapshot() != proof.gate_snapshot() {
            return Err(EnrollmentAttemptError::IncompleteRetirementProof(
                "gate snapshot differs from the attempt".to_owned(),
            ));
        }
        if !proof.gate_restored() || !proof.lease_retired() {
            return Err(EnrollmentAttemptError::IncompleteRetirementProof(
                "exact gate restore and lease retirement were not proved".to_owned(),
            ));
        }
        Ok(())
    }

    fn append(
        &self,
        package: &PackageKey,
        current: &EnrollmentAttempt,
        phase: EnrollmentAttemptPhase,
        committed: Option<CommittedAnchors>,
    ) -> Result<EnrollmentAttempt, EnrollmentAttemptError> {
        let generation = current
            .generation()
            .checked_add(1)
            .ok_or_else(|| EnrollmentAttemptError::Corrupt("generation overflow".to_owned()))?;
        let next = EnrollmentAttempt::new(
            package.clone(),
            current.gate_snapshot(),
            phase,
            committed,
            generation,
            Some(current.sha256().to_owned()),
        )?;
        storage::write_generation(&self.package_path(package), &next)?;
        Ok(next.with_authority(false))
    }
}
