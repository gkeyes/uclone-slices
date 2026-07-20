#![allow(
    clippy::redundant_pub_crate,
    clippy::unwrap_used,
    reason = "parent-visible helpers intentionally remain inside this private test module"
)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::TempDir;
use uclone_slot_runtime::domain::{
    AppIdentity, CommitNonce, DataInodes, GateSnapshot, PackageEnabledState, PackageKey,
    PackageName, UserId,
};
use uclone_slot_runtime::rescue::{
    RescueError, RescueEvent, RescueId, RescueJournalStore, RescueSpec,
};

const SIGNATURE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const ENROLLMENT_DIGEST: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const BASE_DIGEST: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

pub(super) fn store(root: &TempDir) -> RescueJournalStore {
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    RescueJournalStore::new(root.path()).unwrap()
}

pub(super) fn spec() -> RescueSpec {
    build_spec(
        AppIdentity::new(10_321, SIGNATURE, 1, "/data/app/slotprobe/base.apk").unwrap(),
        GateSnapshot::new(PackageEnabledState::Default, false),
    )
}

pub(super) fn spec_with_gate(gate: GateSnapshot) -> RescueSpec {
    build_spec(
        AppIdentity::new(10_321, SIGNATURE, 1, "/data/app/slotprobe/base.apk").unwrap(),
        gate,
    )
}

pub(super) fn spec_with_version(version: u64) -> RescueSpec {
    build_spec(
        AppIdentity::new(10_321, SIGNATURE, version, "/data/app/slotprobe/base.apk").unwrap(),
        GateSnapshot::new(PackageEnabledState::Default, false),
    )
}

pub(super) fn spec_for_package(package: &str) -> Result<RescueSpec, RescueError> {
    RescueSpec::new(
        RescueId::parse("rescue-00000001").unwrap(),
        PackageKey::new(PackageName::parse(package).unwrap(), UserId::PRIMARY),
        AppIdentity::new(10_321, SIGNATURE, 1, "/data/app/slotprobe/base.apk").unwrap(),
        DataInodes::new(101, 202).unwrap(),
        GateSnapshot::new(PackageEnabledState::Default, false),
        ENROLLMENT_DIGEST,
        BASE_DIGEST,
        "boot-00000001",
        CommitNonce::parse("nonce-00000001").unwrap(),
    )
}

pub(super) fn append_until_verified(store: &RescueJournalStore, spec: &RescueSpec) {
    for event in [
        RescueEvent::GateHeld,
        RescueEvent::ProcessesQuiesced,
        RescueEvent::BaseApplying,
        RescueEvent::BaseVerified,
    ] {
        store.append(spec.rescue_id(), event).unwrap();
    }
}

fn build_spec(identity: AppIdentity, gate: GateSnapshot) -> RescueSpec {
    RescueSpec::new(
        RescueId::parse("rescue-00000001").unwrap(),
        PackageKey::new(
            PackageName::parse("com.uclone.slotprobe").unwrap(),
            UserId::PRIMARY,
        ),
        identity,
        DataInodes::new(101, 202).unwrap(),
        gate,
        ENROLLMENT_DIGEST,
        BASE_DIGEST,
        "boot-00000001",
        CommitNonce::parse("nonce-00000001").unwrap(),
    )
    .unwrap()
}
