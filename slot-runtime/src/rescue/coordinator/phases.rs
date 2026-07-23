use crate::reconcile::{NativeBaseRecoveryBackend, RecoveryBackend};

use super::{RescueCoordinator, RescueDiagnosticSink, platform_error};
use crate::rescue::{RescueError, RescueEvent, RescueFaultInjector, RescuePhase, RescueSpec};

impl<B, F, D> RescueCoordinator<'_, B, F, D>
where
    B: RecoveryBackend + NativeBaseRecoveryBackend,
    F: RescueFaultInjector,
    D: RescueDiagnosticSink,
{
    pub(super) fn gate(
        &mut self,
        base: &crate::domain::ManagedPackage,
        spec: &RescueSpec,
    ) -> Result<(), RescueError> {
        let snapshot = self
            .backend
            .emergency_gate_if_leased(base.package_name())
            .map_err(platform_error)?
            .ok_or_else(|| RescueError::Corrupt("rescue gate lease missing".to_owned()))?;
        if snapshot != spec.gate_snapshot() {
            return Err(RescueError::Corrupt(
                "rescue gate snapshot mismatch".to_owned(),
            ));
        }
        self.backend
            .confirm_emergency_gate(base)
            .map_err(platform_error)?;
        self.backend
            .verify_gate_held(base)
            .map_err(platform_error)?;
        self.advance(spec, RescueEvent::GateHeld, RescuePhase::GateHeld)
    }

    pub(super) fn quiesce(
        &mut self,
        base: &crate::domain::ManagedPackage,
        spec: &RescueSpec,
    ) -> Result<(), RescueError> {
        self.backend
            .verify_gate_held(base)
            .map_err(platform_error)?;
        self.backend
            .quiesce_processes(base)
            .map_err(platform_error)?;
        self.advance(
            spec,
            RescueEvent::ProcessesQuiesced,
            RescuePhase::ProcessesQuiesced,
        )
    }

    pub(super) fn begin_base(&mut self, spec: &RescueSpec) -> Result<(), RescueError> {
        self.advance(spec, RescueEvent::BaseApplying, RescuePhase::BaseApplying)
    }

    pub(super) fn apply_base(
        &mut self,
        base: &crate::domain::ManagedPackage,
        spec: &RescueSpec,
    ) -> Result<(), RescueError> {
        self.backend
            .verify_gate_held(base)
            .map_err(platform_error)?;
        self.backend
            .quiesce_processes(base)
            .map_err(platform_error)?;
        self.backend
            .restore_native_base_unconditionally(base)
            .map_err(platform_error)?;
        self.backend
            .verify_native_base(base)
            .map_err(platform_error)?;
        self.advance(spec, RescueEvent::BaseVerified, RescuePhase::BaseVerified)
    }

    pub(super) fn commit_base(
        &mut self,
        base: &crate::domain::ManagedPackage,
        spec: &RescueSpec,
    ) -> Result<(), RescueError> {
        self.backend
            .verify_gate_held(base)
            .map_err(platform_error)?;
        self.backend
            .verify_native_base(base)
            .map_err(platform_error)?;
        self.advance(
            spec,
            RescueEvent::BaseCommitted {
                nonce: spec.commit_nonce().clone(),
            },
            RescuePhase::BaseCommitted,
        )
    }

    pub(super) fn release_gate(
        &mut self,
        base: &crate::domain::ManagedPackage,
        spec: &RescueSpec,
    ) -> Result<(), RescueError> {
        self.backend
            .verify_native_base(base)
            .map_err(platform_error)?;
        if !self.exact_gate(base, spec)? {
            self.backend
                .verify_gate_held(base)
                .map_err(platform_error)?;
            self.restore_exact_gate(base, spec, "exact rescue gate was not restored")?;
            self.faults.after_gate_restore()?;
        }
        self.advance(spec, RescueEvent::GateReleased, RescuePhase::GateReleased)
    }

    pub(super) fn complete(
        &mut self,
        base: &crate::domain::ManagedPackage,
        spec: &RescueSpec,
    ) -> Result<(), RescueError> {
        self.backend
            .verify_native_base(base)
            .map_err(platform_error)?;
        if !self.exact_gate(base, spec)? {
            self.backend
                .verify_gate_held(base)
                .map_err(platform_error)?;
            self.restore_exact_gate(
                base,
                spec,
                "exact rescue gate proof failed before completion",
            )?;
        }
        self.advance(spec, RescueEvent::Completed, RescuePhase::Completed)
    }

    pub(super) fn prove_completed(
        &mut self,
        base: &crate::domain::ManagedPackage,
        spec: &RescueSpec,
    ) -> Result<(), RescueError> {
        self.backend
            .verify_native_base(base)
            .map_err(platform_error)?;
        if self.exact_gate(base, spec)? {
            return Ok(());
        }
        self.backend
            .verify_gate_held(base)
            .map_err(platform_error)?;
        self.restore_exact_gate(
            base,
            spec,
            "exact rescue gate proof failed after completion",
        )
    }

    fn exact_gate(
        &mut self,
        base: &crate::domain::ManagedPackage,
        spec: &RescueSpec,
    ) -> Result<bool, RescueError> {
        self.backend
            .verify_exact_gate_state(base, spec.gate_snapshot())
            .map_err(platform_error)
    }

    fn restore_exact_gate(
        &mut self,
        base: &crate::domain::ManagedPackage,
        spec: &RescueSpec,
        failure: &'static str,
    ) -> Result<(), RescueError> {
        self.backend
            .restore_gate(base, spec.gate_snapshot())
            .map_err(platform_error)?;
        if self.exact_gate(base, spec)? {
            Ok(())
        } else {
            Err(RescueError::Corrupt(failure.to_owned()))
        }
    }
}
