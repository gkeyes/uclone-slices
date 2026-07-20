use crate::journal::{JournalError, JournalEvent, JournalStore, TransactionSpec};
use crate::lifecycle::{GuardDecision, PackageLifecycleGuard};
use crate::registry::RegistryStore;

use super::{
    FaultInjector, FaultPoint, RecoveryCause, RuntimeBackend, RuntimeError, RuntimeStores,
    SwitchOutcome, SwitchRequest,
};

mod recovery;
mod transaction;

pub(super) const REASON_JOURNAL_FAILURE: &str = "journal_failure";
pub(super) const REASON_PLATFORM_FAILURE: &str = "platform_failure";

#[doc = "Fail-closed coordinator for one package's paired CE and DE view transaction."]
#[derive(Debug)]
pub struct SwitchCoordinator<B> {
    backend: B,
    stores: RuntimeStores,
    faults: FaultInjector,
}

impl<B: RuntimeBackend> SwitchCoordinator<B> {
    #[doc = "Creates a coordinator without fault injection."]
    pub const fn new(backend: B, journal: JournalStore, registry: RegistryStore) -> Self {
        Self {
            backend,
            stores: RuntimeStores::new(journal, registry),
            faults: FaultInjector::disabled(),
        }
    }

    #[doc = "Creates a coordinator with deterministic fault injection."]
    pub const fn with_fault_injector(
        backend: B,
        stores: RuntimeStores,
        faults: FaultInjector,
    ) -> Self {
        Self {
            backend,
            stores,
            faults,
        }
    }

    #[doc = "Returns the platform backend for inspection or later reuse."]
    pub const fn backend(&self) -> &B {
        &self.backend
    }

    #[doc = "Returns the real durable stores owned by this coordinator."]
    pub const fn stores(&self) -> &RuntimeStores {
        &self.stores
    }

    #[doc = "Runs the guarded switch transaction through commit, rollback, or fail-closed recovery."]
    pub fn switch(&mut self, request: &SwitchRequest) -> Result<SwitchOutcome, RuntimeError> {
        let managed = request.managed_package();
        let observation = self.backend.observe_package(managed)?;
        match PackageLifecycleGuard::assess(managed, &observation) {
            GuardDecision::AllowBase | GuardDecision::AllowSlot => {}
            rejected => return Err(RuntimeError::GuardRejected(rejected)),
        }

        let gate_snapshot = self.backend.capture_gate_snapshot(managed)?;
        let spec = TransactionSpec::new(
            request.metadata().transaction_id().clone(),
            managed.clone(),
            request.target_view().clone(),
            gate_snapshot,
            request.metadata().boot_id().as_str(),
        )?;
        self.stores.journal().create(&spec)?;
        self.faults.check(FaultPoint::Prepared)?;

        if let Err(error) = self.backend.acquire_gate(managed) {
            return self.require_recovery(
                &spec,
                RecoveryCause::Platform(error),
                REASON_PLATFORM_FAILURE,
            );
        }
        if let Err(error) = self.backend.verify_gate_held(managed) {
            return self.rollback_platform(&spec, error);
        }
        if let Err(error) = self.append(&spec, JournalEvent::GateHeld) {
            return self.require_recovery(
                &spec,
                RecoveryCause::Journal(error),
                REASON_JOURNAL_FAILURE,
            );
        }
        self.faults.check(FaultPoint::GateHeld)?;
        self.execute_gated(request, &spec)
    }

    pub(super) fn append(
        &self,
        spec: &TransactionSpec,
        event: JournalEvent,
    ) -> Result<(), JournalError> {
        self.stores
            .journal()
            .append(spec.transaction_id(), event)
            .map(|_| ())
    }
}
