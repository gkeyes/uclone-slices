#![allow(
    dead_code,
    clippy::redundant_pub_crate,
    clippy::unwrap_used,
    reason = "shared service fixtures are validated constants used across private test modules"
)]

use std::cell::RefCell;

use uclone_slot_runtime::domain::{ManagedPackage, PackageKey, SlotId, SlotView};
use uclone_slot_runtime::reconcile::ReconcileOutcome;
use uclone_slot_runtime::service::{
    PackageSnapshot, PackageState, RescueExecution, ServiceError, SwitchExecution,
};

mod fixtures;
mod platform;

#[allow(unused_imports)]
pub(crate) use fixtures::{allowed, managed_base, preview_view, ready_base, request};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FailurePoint {
    Probe,
    State,
    CaptureGate,
    BeginEnrollment,
    AbortEnrollment,
    HoldGate,
    Quiesce,
    Enroll,
    EnrollAmbiguous,
    ProveBase,
    RestoreGate,
    RetireGate,
    MarkRecovery,
    MarkEnrollment,
    Contain,
    Materialize,
    Switch,
    Reconcile,
    Rescue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Call {
    Probe,
    State,
    CaptureGate,
    BeginEnrollment,
    AbortEnrollment,
    HoldGate,
    Quiesce,
    Enroll,
    ProveBase,
    RestoreGate,
    RetireGate,
    MarkRecovery,
    MarkEnrollment(ServiceError),
    Contain(ServiceError),
    Materialize,
    Switch { slot: SlotId, prepared: bool },
    Reconcile,
    Rescue,
}

#[derive(Debug)]
pub(crate) struct FakePlatform {
    calls: RefCell<Vec<Call>>,
    state: RefCell<PackageState>,
    enrollment_anchor: RefCell<bool>,
    failures: Vec<(FailurePoint, ServiceError)>,
    switch_execution: Option<SwitchExecution>,
    rescue_execution: Option<RescueExecution>,
    reconcile_outcome: ReconcileOutcome,
}

impl Default for FakePlatform {
    fn default() -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            state: RefCell::new(PackageState::Absent),
            enrollment_anchor: RefCell::new(false),
            failures: Vec::new(),
            switch_execution: None,
            rescue_execution: None,
            reconcile_outcome: ReconcileOutcome::RestoredBase,
        }
    }
}

impl FakePlatform {
    pub(crate) fn with_state(state: PackageState) -> Self {
        Self {
            state: RefCell::new(state),
            ..Self::default()
        }
    }

    pub(crate) fn fail_at(mut self, point: FailurePoint, error: ServiceError) -> Self {
        self.failures.push((point, error));
        self
    }

    pub(crate) fn with_switch_execution(mut self, execution: SwitchExecution) -> Self {
        self.switch_execution = Some(execution);
        self
    }

    pub(crate) const fn with_rescue_execution(mut self, execution: RescueExecution) -> Self {
        self.rescue_execution = Some(execution);
        self
    }

    pub(crate) fn with_reconcile_outcome(mut self, outcome: ReconcileOutcome) -> Self {
        self.reconcile_outcome = outcome;
        self
    }

    pub(crate) fn calls(&self) -> Vec<Call> {
        self.calls.borrow().clone()
    }

    pub(crate) fn clear_calls(&self) {
        self.calls.borrow_mut().clear();
    }

    pub(crate) fn enrollment_anchor(&self) -> bool {
        *self.enrollment_anchor.borrow()
    }

    fn record(&self, call: Call) {
        self.calls.borrow_mut().push(call);
    }

    fn fail(&self, point: FailurePoint) -> Result<(), ServiceError> {
        self.failures
            .iter()
            .find_map(|(selected, error)| (*selected == point).then_some(*error))
            .map_or(Ok(()), Err)
    }

    fn set_active(&self, view: &SlotView) {
        let PackageState::Ready(snapshot) = self.state.borrow().clone() else {
            return;
        };
        let managed = snapshot.managed();
        let active = ManagedPackage::new(
            PackageKey::new(managed.package_name().clone(), managed.user_id()),
            managed.identity().clone(),
            managed.base_inodes(),
            view.clone(),
            managed.lifecycle_state(),
        )
        .unwrap();
        self.state
            .replace(PackageState::Ready(Box::new(PackageSnapshot::new(
                active,
                snapshot.preview().cloned(),
                snapshot.gate(),
            ))));
    }
}
