use crate::reconcile::{NativeBaseRecoveryBackend, RecoveryBackend};
use std::io::Write as _;

use super::{
    RescueError, RescueEvent, RescueExecution, RescueId, RescueJournalStore, RescuePhase,
    RescueSpec,
};

mod phases;

#[doc = "Deterministic crash boundary used only by host rescue tests."]
pub trait RescueFaultInjector: core::fmt::Debug {
    #[doc = "Returns an injected failure immediately after a durable phase."]
    fn after_phase(&mut self, phase: RescuePhase) -> Result<(), RescueError>;

    #[doc = "Interrupts after exact gate restoration but before its event publication."]
    fn after_gate_restore(&mut self) -> Result<(), RescueError> {
        Ok(())
    }
}

pub(super) trait RescueDiagnosticSink: core::fmt::Debug {
    fn record_failure(&mut self, rescue_id: &RescueId, phase: RescuePhase, code: &'static str);
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct StderrRescueDiagnostics;

impl RescueDiagnosticSink for StderrRescueDiagnostics {
    fn record_failure(&mut self, rescue_id: &RescueId, phase: RescuePhase, code: &'static str) {
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(
            stderr,
            "ucloned rescue diagnostic: id={} phase={} code={code}",
            rescue_id,
            phase.name(),
        );
    }
}

#[doc = "Production rescue fault injector that never interrupts execution."]
#[derive(Debug, Default, Clone, Copy)]
pub struct NoRescueFault;

impl RescueFaultInjector for NoRescueFault {
    fn after_phase(&mut self, _phase: RescuePhase) -> Result<(), RescueError> {
        Ok(())
    }
}

pub(super) struct RescueCoordinator<'a, B, F, D> {
    backend: &'a mut B,
    journal: &'a RescueJournalStore,
    faults: &'a mut F,
    diagnostics: &'a mut D,
}

impl<'a, B, F, D> RescueCoordinator<'a, B, F, D>
where
    B: RecoveryBackend + NativeBaseRecoveryBackend,
    F: RescueFaultInjector,
    D: RescueDiagnosticSink,
{
    pub(super) const fn new(
        backend: &'a mut B,
        journal: &'a RescueJournalStore,
        faults: &'a mut F,
        diagnostics: &'a mut D,
    ) -> Self {
        Self {
            backend,
            journal,
            faults,
            diagnostics,
        }
    }

    pub(super) fn resume(
        &mut self,
        base: &crate::domain::ManagedPackage,
        spec: &RescueSpec,
    ) -> RescueExecution {
        match self.resume_inner(base, spec) {
            Ok(()) => RescueExecution::CompletedBase,
            Err(RescueError::InjectedCrash(_)) => RescueExecution::RecoveryRequired,
            Err(_) => {
                self.record_failure(spec);
                if self.contain(base) {
                    RescueExecution::RecoveryRequired
                } else {
                    RescueExecution::ContainmentFailed
                }
            }
        }
    }

    fn resume_inner(
        &mut self,
        base: &crate::domain::ManagedPackage,
        spec: &RescueSpec,
    ) -> Result<(), RescueError> {
        loop {
            let transaction = self
                .journal
                .load()?
                .ok_or_else(|| RescueError::Corrupt("prepared rescue missing".to_owned()))?;
            match transaction.phase() {
                RescuePhase::Prepared => self.gate(base, spec)?,
                RescuePhase::GateHeld => self.quiesce(base, spec)?,
                RescuePhase::ProcessesQuiesced => self.begin_base(spec)?,
                RescuePhase::BaseApplying => self.apply_base(base, spec)?,
                RescuePhase::BaseVerified => self.commit_base(base, spec)?,
                RescuePhase::BaseCommitted => self.release_gate(base, spec)?,
                RescuePhase::GateReleased => self.complete(base, spec)?,
                RescuePhase::Completed => {
                    self.prove_completed(base, spec)?;
                    self.backend
                        .retire_gate_lease(base)
                        .map_err(platform_error)?;
                    return Ok(());
                }
            }
        }
    }

    pub(super) fn advance(
        &mut self,
        spec: &RescueSpec,
        event: RescueEvent,
        phase: RescuePhase,
    ) -> Result<(), RescueError> {
        self.journal.append(spec.rescue_id(), event)?;
        self.faults.after_phase(phase)
    }

    fn record_failure(&mut self, spec: &RescueSpec) {
        let phase = self
            .journal
            .load()
            .ok()
            .flatten()
            .map_or(RescuePhase::Prepared, |transaction| transaction.phase());
        self.diagnostics
            .record_failure(spec.rescue_id(), phase, "recovery_required");
    }

    fn contain(&mut self, base: &crate::domain::ManagedPackage) -> bool {
        self.backend.emergency_gate(base.package_name()).is_ok()
    }
}

pub(super) fn platform_error(error: impl Into<crate::runtime::PlatformError>) -> RescueError {
    let error = error.into();
    RescueError::Corrupt(error.to_string())
}
