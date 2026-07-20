use std::collections::VecDeque;

use uclone_slot_runtime::catalog::{PathSecurityProof, SecurityProfileProof};
use uclone_slot_runtime::domain::{DataInodes, SlotId};
use uclone_slot_runtime::materializer::{
    ArtifactState, DataDomain, MaterializationCoordinator, MaterializationError, NoFault,
    TreeSafetyProof, UnsafeArtifact,
};

use super::support::{Call, FakeBackend, base_anchor, managed_base, preview, security, slot_proof};

#[test]
fn gate_or_quiescence_failure_performs_no_filesystem_write() {
    for failed in [Call::Gate, Call::Quiet, Call::Capacity] {
        let mut backend = FakeBackend::healthy();
        backend.fail_call = Some(failed);
        let mut faults = NoFault;
        let error = MaterializationCoordinator::new(&mut backend, &mut faults)
            .materialize(&managed_base(), &preview())
            .unwrap_err();

        assert!(matches!(error, MaterializationError::Backend { .. }));
        assert_eq!(backend.artifacts, ArtifactState::Absent);
        assert!(!backend.calls.contains(&Call::Create));
        assert!(!backend.calls.contains(&Call::Cleanup));
    }
}

#[test]
fn ce_success_de_failure_removes_every_artifact_and_keeps_gate_held() {
    let mut backend = FakeBackend::healthy();
    backend.fail_call = Some(Call::Copy(DataDomain::De));
    let mut faults = NoFault;
    let error = MaterializationCoordinator::new(&mut backend, &mut faults)
        .materialize(&managed_base(), &preview())
        .unwrap_err();

    assert!(matches!(error, MaterializationError::Backend { .. }));
    assert_eq!(backend.artifacts, ArtifactState::Absent);
    assert_eq!(
        backend
            .calls
            .iter()
            .filter(|call| **call == Call::Gate)
            .count(),
        1
    );
    assert!(
        !backend
            .calls
            .iter()
            .any(|call| matches!(call, Call::Publish))
    );
}

#[test]
fn rejects_symlink_device_and_hardlink_trees_before_publication() {
    for (safety, expected) in [
        (TreeSafetyProof::new(1, 0, 0), UnsafeArtifact::Symlink),
        (TreeSafetyProof::new(0, 1, 0), UnsafeArtifact::DeviceNode),
        (TreeSafetyProof::new(0, 0, 1), UnsafeArtifact::Hardlink),
    ] {
        let mut backend = FakeBackend::healthy();
        backend.ce_copy = uclone_slot_runtime::materializer::DomainCopyProof::new(
            DataDomain::Ce,
            super::support::CE_DIGEST,
            safety,
        )
        .unwrap();
        let mut faults = NoFault;
        let error = MaterializationCoordinator::new(&mut backend, &mut faults)
            .materialize(&managed_base(), &preview())
            .unwrap_err();
        assert!(
            matches!(error, MaterializationError::UnsafeTree { artifact, .. } if artifact == expected)
        );
        assert_eq!(backend.artifacts, ArtifactState::Absent);
    }
}

#[test]
fn rejects_security_fscrypt_device_and_base_anchor_mismatch() {
    let changed_security = SecurityProfileProof::new(
        PathSecurityProof::new(
            10_321,
            10_999,
            0o700,
            "u:object_r:app_data_file:s0:c1,c2",
            security().ce().fscrypt_policy_sha256(),
        )
        .unwrap(),
        security().de().clone(),
    );
    let mut cases = Vec::new();
    let mut security_backend = FakeBackend::healthy();
    security_backend.staging = slot_proof().with_security(changed_security);
    cases.push((security_backend, "security"));
    let mut fscrypt_backend = FakeBackend::healthy();
    let bad_policy = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    let bad_security = SecurityProfileProof::new(
        PathSecurityProof::new(
            10_321,
            10_321,
            0o700,
            "u:object_r:app_data_file:s0:c1,c2",
            bad_policy,
        )
        .unwrap(),
        security().de().clone(),
    );
    fscrypt_backend.staging = slot_proof().with_security(bad_security);
    cases.push((fscrypt_backend, "fscrypt"));
    let mut device_backend = FakeBackend::healthy();
    device_backend.staging = slot_proof().with_devices(99, 12).unwrap();
    cases.push((device_backend, "device"));
    let mut base_backend = FakeBackend::healthy();
    base_backend.base_samples = VecDeque::from([
        base_anchor(),
        base_anchor().with_inodes(DataInodes::new(101, 200).unwrap()),
    ]);
    cases.push((base_backend, "base"));

    for (mut backend, kind) in cases {
        let mut faults = NoFault;
        let error = MaterializationCoordinator::new(&mut backend, &mut faults)
            .materialize(&managed_base(), &preview())
            .unwrap_err();
        assert_eq!(error.kind(), kind);
        assert_eq!(backend.artifacts, ArtifactState::Absent);
    }
}

#[test]
fn rejects_base_and_repeated_slot_without_rejecting_runtime_ids() {
    let mut backend = FakeBackend::healthy();
    let mut faults = NoFault;
    let error = MaterializationCoordinator::new(&mut backend, &mut faults)
        .materialize(&managed_base(), &SlotId::base())
        .unwrap_err();
    assert!(matches!(error, MaterializationError::UnsupportedSlot(_)));
    assert!(backend.calls.is_empty());

    let mut backend = FakeBackend::healthy();
    let mut faults = NoFault;
    let work = SlotId::parse("work").unwrap();
    let result = MaterializationCoordinator::new(&mut backend, &mut faults)
        .materialize(&managed_base(), &work)
        .unwrap();
    assert_eq!(result.slot_id(), &work);

    let mut backend = FakeBackend::healthy();
    backend.artifacts = ArtifactState::ReadyOnly;
    let mut faults = NoFault;
    let error = MaterializationCoordinator::new(&mut backend, &mut faults)
        .materialize(&managed_base(), &preview())
        .unwrap_err();
    assert!(matches!(error, MaterializationError::ReadyAlreadyExists));
    assert_eq!(backend.artifacts, ArtifactState::ReadyOnly);
    assert!(!backend.calls.contains(&Call::Cleanup));
}
