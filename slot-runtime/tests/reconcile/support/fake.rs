#![allow(
    clippy::struct_excessive_bools,
    reason = "independent fault toggles model distinct recovery boundaries"
)]

use uclone_slot_runtime::domain::{
    GateSnapshot, ManagedPackage, PackageEnabledState, PackageName, PackageObservation, SlotId,
    SlotView,
};
use uclone_slot_runtime::reconcile::RecoveryBackend;
use uclone_slot_runtime::runtime::{PlatformError, RuntimeBackend};

#[derive(Debug)]
#[allow(missing_docs)]
pub struct FakeBackend {
    #[doc = "Package observation returned before the release proof."]
    pub observation: PackageObservation,
    #[doc = "Currently applied paired view."]
    pub current: SlotView,
    #[doc = "Whether the fake execution gate is held."]
    pub gate_held: bool,
    #[doc = "Whether user0 is reported unlocked."]
    pub unlocked: bool,
    #[doc = "Injects emergency gate acquisition failure."]
    pub fail_emergency_gate: bool,
    pub orphan_lease: bool,
    pub corrupt_orphan_lease: bool,
    pub fail_containment: bool,
    pub fail_gate_restore: bool,
    pub fail_retire_gate_lease: bool,
    #[doc = "Optional package observation returned by later release proofs."]
    pub late_observation: Option<PackageObservation>,
    #[doc = "Number of package observation calls."]
    pub observe_calls: usize,
    #[doc = "Number of paired view applications."]
    pub apply_calls: usize,
    #[doc = "Number of native base proofs."]
    pub native_base_proofs: usize,
    #[doc = "Number of emergency gate calls."]
    pub emergency_gate_calls: usize,
    pub quiesce_calls: usize,
    pub retire_gate_lease_calls: usize,
    pub lease_present: bool,
    pub strict_current_proof: bool,
    snapshot: GateSnapshot,
}

impl FakeBackend {
    #[doc = "Creates the post-reboot native-base platform state."]
    pub fn rebooted(managed: &ManagedPackage) -> Self {
        let base = managed.base_inodes();
        Self {
            observation: PackageObservation::new(
                managed.identity().clone(),
                base,
                base,
                base,
                false,
            ),
            current: SlotView::new(SlotId::base(), base),
            gate_held: false,
            unlocked: true,
            fail_emergency_gate: false,
            orphan_lease: false,
            corrupt_orphan_lease: false,
            fail_containment: false,
            fail_gate_restore: false,
            fail_retire_gate_lease: false,
            late_observation: None,
            observe_calls: 0,
            apply_calls: 0,
            native_base_proofs: 0,
            emergency_gate_calls: 0,
            quiesce_calls: 0,
            retire_gate_lease_calls: 0,
            lease_present: false,
            strict_current_proof: false,
            snapshot: GateSnapshot::new(PackageEnabledState::Enabled, true),
        }
    }
}

impl RuntimeBackend for FakeBackend {
    fn observe_package(
        &mut self,
        _package: &ManagedPackage,
    ) -> Result<PackageObservation, PlatformError> {
        self.observe_calls += 1;
        if self.observe_calls > 1 {
            Ok(self
                .late_observation
                .as_ref()
                .unwrap_or(&self.observation)
                .clone())
        } else {
            Ok(self.observation.clone())
        }
    }

    fn capture_gate_snapshot(
        &mut self,
        _package: &ManagedPackage,
    ) -> Result<GateSnapshot, PlatformError> {
        Ok(self.snapshot)
    }

    fn acquire_gate(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        if self.fail_containment {
            return Err(PlatformError::AcquireGate {
                detail: "fake_containment_failure".to_owned(),
            });
        }
        self.gate_held = true;
        Ok(())
    }

    fn verify_gate_held(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        if self.gate_held {
            Ok(())
        } else {
            Err(PlatformError::VerifyGateHeld {
                detail: "fake_gate_open".to_owned(),
            })
        }
    }

    fn quiesce_processes(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        self.quiesce_calls += 1;
        Ok(())
    }

    fn apply_slot_view(
        &mut self,
        package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), PlatformError> {
        if self.strict_current_proof
            && self.current != SlotView::new(package.active_slot().clone(), package.active_inodes())
        {
            return Err(PlatformError::view_verification(
                "fake_current_view_mismatch",
            ));
        }
        self.apply_calls += 1;
        self.current = view.clone();
        Ok(())
    }

    fn verify_slot_view(
        &mut self,
        _package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), PlatformError> {
        if &self.current == view {
            Ok(())
        } else {
            Err(PlatformError::view_verification("fake_view_mismatch"))
        }
    }

    fn restore_gate(
        &mut self,
        _package: &ManagedPackage,
        snapshot: GateSnapshot,
    ) -> Result<(), PlatformError> {
        if self.fail_gate_restore {
            return Err(PlatformError::RestoreGate {
                detail: "fake_restore_failure".to_owned(),
            });
        }
        assert_eq!(snapshot, self.snapshot);
        assert!(self.lease_present, "gate restoration requires a lease");
        self.gate_held = false;
        Ok(())
    }

    fn retire_gate_lease(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        self.retire_gate_lease_calls += 1;
        if self.fail_retire_gate_lease {
            return Err(PlatformError::RetireGateLease {
                detail: "fake_retire_failure".to_owned(),
            });
        }
        if !self.lease_present {
            return Err(PlatformError::RetireGateLease {
                detail: "fake_missing_lease".to_owned(),
            });
        }
        self.lease_present = false;
        Ok(())
    }
}

impl RecoveryBackend for FakeBackend {
    fn leased_packages(&mut self) -> Result<Vec<PackageName>, PlatformError> {
        if self.orphan_lease || self.corrupt_orphan_lease {
            Ok(vec![PackageName::parse("com.uclone.slotprobe").map_err(
                |_| PlatformError::CaptureGateSnapshot {
                    detail: "invalid_fake_package".to_owned(),
                },
            )?])
        } else {
            Ok(Vec::new())
        }
    }

    fn emergency_gate_if_leased(
        &mut self,
        package: &PackageName,
    ) -> Result<Option<GateSnapshot>, PlatformError> {
        if !self.orphan_lease && !self.corrupt_orphan_lease {
            return Ok(None);
        }
        let snapshot = self.emergency_gate(package)?;
        if self.corrupt_orphan_lease {
            Err(PlatformError::CaptureGateSnapshot {
                detail: "fake_corrupt_orphan_lease".to_owned(),
            })
        } else {
            Ok(Some(snapshot))
        }
    }

    fn emergency_gate(&mut self, _package: &PackageName) -> Result<GateSnapshot, PlatformError> {
        self.emergency_gate_calls += 1;
        if self.fail_emergency_gate || (self.fail_containment && self.emergency_gate_calls > 2) {
            return Err(PlatformError::AcquireGate {
                detail: "fake_emergency_gate_failure".to_owned(),
            });
        }
        self.lease_present = true;
        self.gate_held = true;
        self.quiesce_calls += 1;
        Ok(self.snapshot)
    }

    fn confirm_emergency_gate(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        if self.gate_held {
            Ok(())
        } else {
            Err(PlatformError::VerifyGateHeld {
                detail: "fake_emergency_gate_open".to_owned(),
            })
        }
    }

    fn user0_unlocked(&mut self) -> Result<bool, PlatformError> {
        Ok(self.unlocked)
    }

    fn verify_native_base(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        self.native_base_proofs += 1;
        let expected = SlotView::new(SlotId::base(), package.base_inodes());
        if self.current == expected {
            Ok(())
        } else {
            Err(PlatformError::view_verification(
                "fake_native_base_mismatch",
            ))
        }
    }
}
