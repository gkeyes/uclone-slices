use std::collections::BTreeMap;

use crate::domain::{GateSnapshot, PackageName};

use super::backend::RecoveryBackend;
use super::coordinator::Reconciler;
use super::error::ReconcileError;
use super::model::{PackageReconcileResult, ReconcileOutcome, ReconcileReason};

impl<B: RecoveryBackend> Reconciler<B> {
    pub(super) fn emergency_gate_discovered(
        &mut self,
        package_names: &[PackageName],
    ) -> Result<BTreeMap<PackageName, GateSnapshot>, ReconcileError> {
        let mut snapshots = BTreeMap::new();
        for package in package_names {
            if let Some(snapshot) =
                self.backend
                    .emergency_gate_if_leased(package)
                    .map_err(|source| ReconcileError::EmergencyGate {
                        package: package.clone(),
                        source,
                    })?
            {
                snapshots.insert(package.clone(), snapshot);
            }
        }
        let ungated: Vec<_> = package_names
            .iter()
            .filter(|package| !snapshots.contains_key(*package))
            .cloned()
            .collect();
        snapshots.extend(self.emergency_gate_all(&ungated)?);
        Ok(snapshots)
    }

    pub(super) fn emergency_gate_all(
        &mut self,
        package_names: &[PackageName],
    ) -> Result<BTreeMap<PackageName, GateSnapshot>, ReconcileError> {
        let mut snapshots = BTreeMap::new();
        let mut first_failure = None;
        for package in package_names {
            match self.backend.emergency_gate(package) {
                Ok(snapshot) => {
                    snapshots.insert(package.clone(), snapshot);
                }
                Err(source) if first_failure.is_none() => {
                    first_failure = Some((package.clone(), source));
                }
                Err(_) => {}
            }
        }
        if let Some((package, source)) = first_failure {
            Err(ReconcileError::EmergencyGate { package, source })
        } else {
            Ok(snapshots)
        }
    }

    pub(super) fn rehold_packages(&mut self) -> Result<(), ReconcileError> {
        for held in &self.held {
            let package = held.managed.package_name();
            let snapshot = self.backend.emergency_gate(package).map_err(|source| {
                ReconcileError::EmergencyGate {
                    package: package.clone(),
                    source,
                }
            })?;
            if held.snapshot.is_some_and(|expected| expected != snapshot) {
                return Err(ReconcileError::GateSnapshotMismatch(package.clone()));
            }
        }
        Ok(())
    }

    pub(super) fn rehold_orphans(&mut self) -> Result<(), ReconcileError> {
        for package in &self.orphaned {
            self.backend.emergency_gate(package).map_err(|source| {
                ReconcileError::EmergencyGate {
                    package: package.clone(),
                    source,
                }
            })?;
        }
        Ok(())
    }
}

pub(super) const fn orphan_result(package: PackageName) -> PackageReconcileResult {
    PackageReconcileResult::new(
        package,
        None,
        ReconcileOutcome::RecoveryRequired(ReconcileReason::EnrollmentMetadata),
    )
}
