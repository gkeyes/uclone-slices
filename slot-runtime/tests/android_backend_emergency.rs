#![doc = "Pre-enrollment fail-closed Android emergency gate coverage."]
#![allow(
    clippy::redundant_pub_crate,
    clippy::unwrap_used,
    reason = "private emergency-gate fixture shares one validated package constructor"
)]
use std::collections::VecDeque;

use uclone_slot_runtime::android::{EmergencyGatePhase, StoredGateLease};
use uclone_slot_runtime::domain::{GateSnapshot, PackageEnabledState};
use uclone_slot_runtime::reconcile::RecoveryBackend;
use uclone_slot_runtime::runtime::{PlatformError, RuntimeBackend};
#[path = "android_backend_emergency/support.rs"]
mod emergency_support;
use emergency_support::{backend, managed};

#[test]
fn emergency_gate_persists_exact_snapshot_before_disable_and_proves_quiescence() {
    let package = managed();
    let original = GateSnapshot::new(PackageEnabledState::Enabled, true);
    let held = GateSnapshot::new(PackageEnabledState::DisabledUser, true);
    let mut backend = backend(VecDeque::from([original, original, held]));

    let captured = backend.emergency_gate(package.package_name()).unwrap();

    assert_eq!(captured, original);
    assert_eq!(
        backend.gate_lease_store().events.borrow().as_slice(),
        [
            "capture",
            "persist-emergency",
            "verify",
            "disable",
            "verify",
            "force-stop",
            "process-proof",
        ]
    );
    let stored = backend.gate_lease_store().stored.as_ref().unwrap();
    assert!(matches!(stored, StoredGateLease::Emergency(_)));
    assert!(matches!(
        stored,
        StoredGateLease::Emergency(lease) if lease.phase() == EmergencyGatePhase::Held
    ));
    assert_eq!(stored.snapshot(), original);
    assert_eq!(stored.base_inodes(), None);
}

#[test]
fn emergency_gate_rejects_snapshot_drift_before_disable() {
    let package = managed();
    let original = GateSnapshot::new(PackageEnabledState::Enabled, false);
    let drifted = GateSnapshot::new(PackageEnabledState::DisabledUser, true);
    let mut backend = backend(VecDeque::from([original, drifted]));

    let error = backend.emergency_gate(package.package_name()).unwrap_err();

    assert_eq!(
        error,
        PlatformError::AcquireGate {
            detail: "gate_state_changed_before_emergency_acquire".to_owned(),
        }
    );
    assert_eq!(
        backend.gate_lease_store().events.borrow().as_slice(),
        ["capture", "persist-emergency", "verify"]
    );
    assert!(backend.runner().commands.is_empty());
    assert_eq!(
        backend
            .gate_lease_store()
            .stored
            .as_ref()
            .unwrap()
            .snapshot(),
        original
    );
    assert!(matches!(
        backend.gate_lease_store().stored.as_ref(),
        Some(StoredGateLease::Emergency(lease))
            if lease.phase() == EmergencyGatePhase::Prepared
    ));
}

#[test]
fn emergency_gate_probe_failure_after_persistence_never_disables() {
    let package = managed();
    let original = GateSnapshot::new(PackageEnabledState::Default, false);
    let mut backend = backend(VecDeque::from([original]));

    let error = backend.emergency_gate(package.package_name()).unwrap_err();

    assert!(matches!(error, PlatformError::AcquireGate { .. }));
    assert!(backend.runner().commands.is_empty());
    assert!(matches!(
        backend.gate_lease_store().stored.as_ref(),
        Some(StoredGateLease::Emergency(_))
    ));
}

#[test]
fn prepared_lease_with_disabled_live_state_is_recontained_but_not_committed() {
    let package = managed();
    let original = GateSnapshot::new(PackageEnabledState::Enabled, false);
    let drifted = GateSnapshot::new(PackageEnabledState::DisabledUser, false);
    let mut backend = backend(VecDeque::from([
        original, drifted, drifted, drifted, drifted,
    ]));

    let _ = backend.emergency_gate(package.package_name());
    let error = backend
        .emergency_gate_if_leased(package.package_name())
        .unwrap_err();

    assert!(matches!(error, PlatformError::AcquireGate { .. }));
    let kinds: Vec<_> = backend
        .runner()
        .commands
        .iter()
        .map(uclone_slot_runtime::android::AndroidCommand::kind)
        .collect();
    assert_eq!(
        kinds,
        [
            uclone_slot_runtime::android::CommandKind::DisableUser,
            uclone_slot_runtime::android::CommandKind::ForceStop,
            uclone_slot_runtime::android::CommandKind::DisableUser,
            uclone_slot_runtime::android::CommandKind::ForceStop,
        ]
    );
    assert!(matches!(
        backend.gate_lease_store().stored.as_ref(),
        Some(StoredGateLease::Emergency(lease))
            if lease.phase() == EmergencyGatePhase::Prepared
    ));
}

#[test]
fn held_lease_rejects_external_suspension_drift_before_idempotent_gate() {
    let package = managed();
    let original = GateSnapshot::new(PackageEnabledState::Enabled, false);
    let held = GateSnapshot::new(PackageEnabledState::DisabledUser, false);
    let drifted = GateSnapshot::new(PackageEnabledState::DisabledUser, true);
    let mut backend = backend(VecDeque::from([original, original, held, drifted]));

    backend.emergency_gate(package.package_name()).unwrap();
    let error = backend.emergency_gate(package.package_name()).unwrap_err();

    assert!(matches!(error, PlatformError::AcquireGate { .. }));
    assert_eq!(backend.runner().commands.len(), 2);
}

#[test]
fn confirmation_upgrades_zero_anchor_state_and_rechecks_the_gate() {
    let package = managed();
    let original = GateSnapshot::new(PackageEnabledState::Default, false);
    let held = GateSnapshot::new(PackageEnabledState::DisabledUser, false);
    let mut backend = backend(VecDeque::from([original, original, held, held, held]));

    backend.emergency_gate(package.package_name()).unwrap();
    let error = backend.capture_gate_snapshot(&package).unwrap_err();
    assert_eq!(
        error,
        PlatformError::CaptureGateSnapshot {
            detail: "emergency_gate_confirmation_required".to_owned(),
        }
    );
    assert!(matches!(
        backend.gate_lease_store().stored.as_ref(),
        Some(StoredGateLease::Emergency(_))
    ));
    backend.confirm_emergency_gate(&package).unwrap();

    let stored = backend.gate_lease_store().stored.as_ref().unwrap();
    assert!(matches!(stored, StoredGateLease::Enrolled(_)));
    assert_eq!(stored.snapshot(), original);
    assert_eq!(stored.base_inodes(), Some(package.base_inodes()));
    assert_eq!(
        backend
            .gate_lease_store()
            .events
            .borrow()
            .iter()
            .filter(|event| **event == "confirm-enrollment")
            .count(),
        1
    );
}

#[test]
fn repeated_emergency_gate_reuses_the_original_lease_snapshot() {
    let package = managed();
    let original = GateSnapshot::new(PackageEnabledState::Enabled, false);
    let held = GateSnapshot::new(PackageEnabledState::DisabledUser, false);
    let mut backend = backend(VecDeque::from([original, original, held, held, held]));

    let first = backend.emergency_gate(package.package_name()).unwrap();
    let second = backend.emergency_gate(package.package_name()).unwrap();

    assert_eq!((first, second), (original, original));
    assert_eq!(
        backend
            .gate_lease_store()
            .events
            .borrow()
            .iter()
            .filter(|event| **event == "persist-emergency")
            .count(),
        1
    );
}
