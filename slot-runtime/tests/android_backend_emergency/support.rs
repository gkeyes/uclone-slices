#![allow(clippy::unwrap_used, reason = "validated test fixtures")]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use uclone_slot_runtime::android::{
    AndroidBackend, AndroidCommand, CommandError, CommandKind, CommandRunner, EmergencyGateLease,
    EmergencyGatePhase, GateLease, GateLeaseError, GateLeaseStore, MountNamespaceProof,
    PackageProbe, ProbeError, StoredGateLease, ViewProof,
};
use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, GateSnapshot, ManagedPackage, PackageKey, PackageName,
    PackageObservation, SlotId, SlotView, UserId,
};
use uclone_slot_runtime::lifecycle::LifecycleState;

pub(super) type Events = Rc<RefCell<Vec<&'static str>>>;

#[derive(Debug)]
pub(super) struct EventRunner {
    pub(super) commands: Vec<AndroidCommand>,
    pub(super) events: Events,
}

impl CommandRunner for EventRunner {
    fn run(&mut self, command: &AndroidCommand) -> Result<(), CommandError> {
        self.events.borrow_mut().push(match command.kind() {
            CommandKind::DisableUser => "disable",
            CommandKind::ForceStop => "force-stop",
            _ => "unexpected-command",
        });
        self.commands.push(command.clone());
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct MemoryLeaseStore {
    pub(super) stored: Option<StoredGateLease>,
    pub(super) events: Events,
}

impl GateLeaseStore for MemoryLeaseStore {
    fn artifact_exists(&mut self, _package: &PackageName) -> Result<bool, GateLeaseError> {
        Ok(self.stored.is_some())
    }

    fn load(&mut self, _package: &PackageName) -> Result<Option<StoredGateLease>, GateLeaseError> {
        Ok(self.stored.clone())
    }

    fn persist_emergency(&mut self, lease: &EmergencyGateLease) -> Result<(), GateLeaseError> {
        self.events.borrow_mut().push("persist-emergency");
        self.stored = Some(StoredGateLease::Emergency(lease.clone()));
        Ok(())
    }

    fn mark_emergency_held(&mut self, lease: &EmergencyGateLease) -> Result<(), GateLeaseError> {
        if lease.phase() != EmergencyGatePhase::Prepared
            || self.stored != Some(StoredGateLease::Emergency(lease.clone()))
        {
            return Err(GateLeaseError::InvalidArtifact);
        }
        self.stored = Some(StoredGateLease::Emergency(lease.held()));
        Ok(())
    }

    fn persist_enrolled(&mut self, lease: &GateLease) -> Result<(), GateLeaseError> {
        self.stored = Some(StoredGateLease::Enrolled(lease.clone()));
        Ok(())
    }

    fn confirm_enrollment(&mut self, lease: &GateLease) -> Result<(), GateLeaseError> {
        self.events.borrow_mut().push("confirm-enrollment");
        if !matches!(
            self.stored.as_ref(),
            Some(StoredGateLease::Emergency(existing))
                if existing.phase() == EmergencyGatePhase::Held
                    && existing.snapshot() == lease.snapshot()
        ) {
            return Err(GateLeaseError::InvalidArtifact);
        }
        self.stored = Some(StoredGateLease::Enrolled(lease.clone()));
        Ok(())
    }

    fn retire(&mut self, expected: &GateLease) -> Result<(), GateLeaseError> {
        if self.stored != Some(StoredGateLease::Enrolled(expected.clone())) {
            return Err(GateLeaseError::InvalidArtifact);
        }
        self.stored = None;
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct EventProbe {
    pub(super) gates: VecDeque<GateSnapshot>,
    pub(super) events: Events,
    pub(super) gate_calls: usize,
}

impl PackageProbe for EventProbe {
    fn mount_namespace_proof(&mut self) -> Result<MountNamespaceProof, ProbeError> {
        Err(ProbeError::Unavailable)
    }

    fn observe_package(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<PackageObservation, ProbeError> {
        Err(ProbeError::Unavailable)
    }

    fn gate_snapshot(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<GateSnapshot, ProbeError> {
        self.events.borrow_mut().push(if self.gate_calls == 0 {
            "capture"
        } else {
            "verify"
        });
        self.gate_calls += 1;
        self.gates.pop_front().ok_or(ProbeError::InvalidResponse)
    }

    fn running_process_count(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<u32, ProbeError> {
        self.events.borrow_mut().push("process-proof");
        Ok(0)
    }

    fn view_proof(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<ViewProof, ProbeError> {
        Err(ProbeError::Unavailable)
    }

    fn slot_inodes(
        &mut self,
        _package: &PackageName,
        _slot_id: &SlotId,
    ) -> Result<Option<DataInodes>, ProbeError> {
        Err(ProbeError::Unavailable)
    }

    fn user0_unlocked(&mut self) -> Result<bool, ProbeError> {
        Err(ProbeError::Unavailable)
    }
}

pub(super) fn backend(
    gates: VecDeque<GateSnapshot>,
) -> AndroidBackend<EventRunner, EventProbe, MemoryLeaseStore> {
    let events = Events::default();
    AndroidBackend::with_gate_lease_store(
        EventRunner {
            commands: Vec::new(),
            events: Rc::clone(&events),
        },
        EventProbe {
            gates,
            events: Rc::clone(&events),
            gate_calls: 0,
        },
        MemoryLeaseStore {
            stored: None,
            events,
        },
    )
}

pub(super) fn managed() -> ManagedPackage {
    let base = DataInodes::new(101, 202).unwrap();
    ManagedPackage::new(
        PackageKey::new(
            PackageName::parse("com.uclone.slotprobe").unwrap(),
            UserId::PRIMARY,
        ),
        AppIdentity::new(
            10_321,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            7,
            "/data/app/slotprobe/base.apk",
        )
        .unwrap(),
        base,
        SlotView::new(SlotId::base(), base),
        LifecycleState::Normal,
    )
    .unwrap()
}
