#![doc = "Bridge-backed Android system package probe tests."]
#![allow(
    clippy::redundant_pub_crate,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "included production module keeps its original crate-relative visibility"
)]
#![allow(
    unreachable_pub,
    reason = "production module is included as a test fixture"
)]

use std::collections::VecDeque;
use std::io;

pub use uclone_slot_runtime::{android, bridge, domain, layout};

#[path = "../src/android/system/mod.rs"]
mod system;

use android::{MountCounts, PackageProbe, ProbeError};
use bridge::{BridgeCommand, BridgeCommandRunner, BridgeRunnerError};
use domain::{DataInodes, ManagedPackage, PackageKey, PackageName, SlotId, SlotView, UserId};
use system::{
    FactError, ProcessExecutor, ProcessFailure, ProcessInvocation, ProcessOutput, ProcessSet,
    SystemFacts, SystemPackageProbe,
};
use uclone_slot_runtime::lifecycle::{GuardDecision, LifecycleState, PackageLifecycleGuard};

#[derive(Debug)]
struct FakeBridge {
    package: Vec<u8>,
    calls: usize,
}

impl BridgeCommandRunner for FakeBridge {
    fn run(&mut self, command: &BridgeCommand) -> Result<Vec<u8>, BridgeRunnerError> {
        self.calls += 1;
        if matches!(command, BridgeCommand::PackageStatus(_)) {
            Ok(self.package.clone())
        } else {
            Err(BridgeRunnerError::Io(io::Error::other(
                "unexpected bridge command",
            )))
        }
    }
}

#[derive(Debug)]
struct FakeExecutor {
    outputs: VecDeque<ProcessOutput>,
    seen: Vec<ProcessInvocation>,
}

impl ProcessExecutor for FakeExecutor {
    fn execute(&mut self, invocation: &ProcessInvocation) -> Result<ProcessOutput, ProcessFailure> {
        self.seen.push(invocation.clone());
        self.outputs
            .pop_front()
            .ok_or_else(|| ProcessFailure::Io(io::Error::other("missing fake output")))
    }
}

#[derive(Debug)]
struct FakeFacts {
    canonical: DataInodes,
    mirror: DataInodes,
    counts: MountCounts,
    package_processes: Result<ProcessSet, FactError>,
    zygotes: ProcessSet,
}

impl SystemFacts for FakeFacts {
    fn mount_namespace_ids(&mut self) -> Result<(u64, u64), FactError> {
        Ok((77, 77))
    }

    fn canonical_inodes(&mut self, _package: &PackageName) -> Result<DataInodes, FactError> {
        Ok(self.canonical)
    }

    fn mirror_inodes(&mut self, _package: &PackageName) -> Result<DataInodes, FactError> {
        Ok(self.mirror)
    }

    fn canonical_mount_counts(&mut self, _package: &PackageName) -> Result<MountCounts, FactError> {
        Ok(self.counts)
    }

    fn slot_inodes(
        &mut self,
        _package: &PackageName,
        _slot: &SlotId,
    ) -> Result<Option<DataInodes>, FactError> {
        Ok(None)
    }

    fn package_processes(
        &mut self,
        _package: &PackageName,
        _uid: u32,
    ) -> Result<ProcessSet, FactError> {
        self.package_processes.clone()
    }

    fn arm64_zygote_processes(&mut self) -> Result<ProcessSet, FactError> {
        Ok(self.zygotes.clone())
    }
}

fn package() -> PackageName {
    PackageName::parse("com.uclone.slotprobe").unwrap()
}

fn package_response(pm: DataInodes) -> Vec<u8> {
    package_response_for(pm, "com.uclone.slotprobe", 12345)
}

fn package_response_for(pm: DataInodes, package: &str, uid: u32) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schemaVersion": 1,
        "requestId": "package",
        "ok": true,
        "payload": {
            "type": "package",
            "packageName": package,
            "userId": 0,
            "uid": uid,
            "signatureSha256": "aa".repeat(32),
            "versionCode": 7,
            "versionName": "preview",
            "codePath": format!("/data/app/{package}/base.apk"),
            "ceDataPath": format!("/data/user/0/{package}"),
            "deDataPath": format!("/data/user_de/0/{package}"),
            "packageManagerCeInode": pm.ce().get(),
            "packageManagerDeInode": pm.de().get(),
            "enabledState": "enabled",
            "suspended": false,
            "pendingInstall": false,
            "systemApp": false,
            "sharedUid": false,
            "directBootAware": false
        }
    }))
    .unwrap()
}

fn incompatible_package_response(pm: DataInodes) -> Vec<u8> {
    let mut value: serde_json::Value =
        serde_json::from_slice(&package_response(pm)).expect("package fixture");
    value["payload"]["sharedUid"] = serde_json::json!(true);
    serde_json::to_vec(&value).expect("package fixture encoding")
}

fn fake_facts(canonical: DataInodes, processes: Vec<u32>) -> FakeFacts {
    FakeFacts {
        canonical,
        mirror: canonical,
        counts: MountCounts::new(1, 1),
        package_processes: Ok(ProcessSet::new(processes).unwrap()),
        zygotes: ProcessSet::new(vec![41]).unwrap(),
    }
}

#[test]
fn observation_uses_persisted_pm_inodes_while_preview_is_active() {
    let base = DataInodes::new(101, 202).unwrap();
    let preview = DataInodes::new(303, 404).unwrap();
    let bridge = FakeBridge {
        package: package_response(base),
        calls: 0,
    };
    let executor = FakeExecutor {
        outputs: VecDeque::from([ProcessOutput::new(Some(0), b"303\n404\n".to_vec())]),
        seen: Vec::new(),
    };
    let mut probe =
        SystemPackageProbe::with_dependencies(bridge, executor, fake_facts(preview, vec![81]));

    let observed = probe.observe_package(&package(), UserId::PRIMARY).unwrap();

    assert_eq!(observed.package_manager_inodes(), base);
    assert_eq!(observed.canonical_inodes(), preview);
    assert_eq!(observed.active_process_inodes(), preview);
    let managed = ManagedPackage::new(
        PackageKey::new(package(), UserId::PRIMARY),
        observed.identity().clone(),
        base,
        SlotView::new(SlotId::parse("preview").unwrap(), preview),
        LifecycleState::Normal,
    )
    .unwrap();
    assert_eq!(
        PackageLifecycleGuard::assess(&managed, &observed),
        GuardDecision::AllowSlot
    );
}

#[test]
fn package_manager_compatibility_flags_reach_the_runtime_guard() {
    let base = DataInodes::new(101, 202).unwrap();
    let bridge = FakeBridge {
        package: incompatible_package_response(base),
        calls: 0,
    };
    let executor = FakeExecutor {
        outputs: VecDeque::from([ProcessOutput::new(Some(0), b"101\n202\n".to_vec())]),
        seen: Vec::new(),
    };
    let mut probe =
        SystemPackageProbe::with_dependencies(bridge, executor, fake_facts(base, vec![81]));

    let observed = probe.observe_package(&package(), UserId::PRIMARY).unwrap();

    assert!(!observed.compatibility().is_supported());
    assert!(observed.compatibility().shared_uid());
}

#[test]
fn lightweight_inspection_does_not_enter_a_running_app_namespace() {
    let base = DataInodes::new(101, 202).unwrap();
    let mut facts = fake_facts(base, vec![81]);
    facts.package_processes = Err(FactError::Invalid);
    let mut response: serde_json::Value = serde_json::from_slice(&package_response(base)).unwrap();
    response["payload"]["directBootAware"] = serde_json::json!(true);
    let mut probe = SystemPackageProbe::with_dependencies(
        FakeBridge {
            package: serde_json::to_vec(&response).unwrap(),
            calls: 0,
        },
        FakeExecutor {
            outputs: VecDeque::new(),
            seen: Vec::new(),
        },
        facts,
    );

    let inspected = probe.inspect_package(&package(), UserId::PRIMARY).unwrap();

    assert_eq!(inspected.package_manager_inodes(), base);
    assert_eq!(
        inspected.compatibility().support_level(),
        domain::PackageSupportLevel::DirectBootConditional,
    );
    assert!(probe.executor().seen.is_empty());
}

#[test]
fn zygote_namespace_disagreement_fails_closed() {
    let base = DataInodes::new(101, 202).unwrap();
    let bridge = FakeBridge {
        package: package_response(base),
        calls: 0,
    };
    let executor = FakeExecutor {
        outputs: VecDeque::from([
            ProcessOutput::new(Some(0), b"101\n202\n".to_vec()),
            ProcessOutput::new(Some(0), b"303\n404\n".to_vec()),
        ]),
        seen: Vec::new(),
    };
    let mut facts = fake_facts(base, Vec::new());
    facts.zygotes = ProcessSet::new(vec![41, 42]).unwrap();
    let mut probe = SystemPackageProbe::with_dependencies(bridge, executor, facts);

    assert_eq!(
        probe.view_proof(&package(), UserId::PRIMARY),
        Err(ProbeError::InvalidResponse)
    );
}

#[test]
fn shared_uid_fact_rejection_propagates_fail_closed() {
    let base = DataInodes::new(101, 202).unwrap();
    let mut facts = fake_facts(base, Vec::new());
    facts.package_processes = Err(FactError::Invalid);
    let mut probe = SystemPackageProbe::with_dependencies(
        FakeBridge {
            package: package_response(base),
            calls: 0,
        },
        FakeExecutor {
            outputs: VecDeque::new(),
            seen: Vec::new(),
        },
        facts,
    );

    assert_eq!(
        probe.running_process_count(&package(), UserId::PRIMARY),
        Err(ProbeError::InvalidResponse)
    );
}

#[test]
fn running_process_check_reuses_the_fresh_observation_identity() {
    let base = DataInodes::new(101, 202).unwrap();
    let bridge = FakeBridge {
        package: package_response(base),
        calls: 0,
    };
    let executor = FakeExecutor {
        outputs: VecDeque::from([ProcessOutput::new(Some(0), b"101\n202\n".to_vec())]),
        seen: Vec::new(),
    };
    let mut probe =
        SystemPackageProbe::with_dependencies(bridge, executor, fake_facts(base, Vec::new()));

    probe.observe_package(&package(), UserId::PRIMARY).unwrap();
    assert_eq!(
        probe
            .running_process_count(&package(), UserId::PRIMARY)
            .unwrap(),
        0
    );

    let (bridge, _, _) = probe.into_dependencies();
    assert_eq!(bridge.calls, 1);
}

#[derive(Debug)]
struct MultiPackageBridge {
    responses: Vec<(PackageName, Vec<u8>)>,
    calls: usize,
}

impl BridgeCommandRunner for MultiPackageBridge {
    fn run(&mut self, command: &BridgeCommand) -> Result<Vec<u8>, BridgeRunnerError> {
        self.calls += 1;
        let BridgeCommand::PackageStatus(package) = command else {
            return Err(BridgeRunnerError::Io(io::Error::other(
                "unexpected bridge command",
            )));
        };
        self.responses
            .iter()
            .find(|(candidate, _)| candidate == package)
            .map(|(_, response)| response.clone())
            .ok_or_else(|| BridgeRunnerError::Io(io::Error::other("missing package response")))
    }
}

#[derive(Debug)]
struct UidAwareFacts {
    base: DataInodes,
    running_uid: u32,
    process_queries: Vec<(PackageName, u32)>,
}

impl SystemFacts for UidAwareFacts {
    fn mount_namespace_ids(&mut self) -> Result<(u64, u64), FactError> {
        Ok((77, 77))
    }

    fn canonical_inodes(&mut self, _package: &PackageName) -> Result<DataInodes, FactError> {
        Ok(self.base)
    }

    fn mirror_inodes(&mut self, _package: &PackageName) -> Result<DataInodes, FactError> {
        Ok(self.base)
    }

    fn canonical_mount_counts(&mut self, _package: &PackageName) -> Result<MountCounts, FactError> {
        Ok(MountCounts::new(0, 0))
    }

    fn slot_inodes(
        &mut self,
        _package: &PackageName,
        _slot: &SlotId,
    ) -> Result<Option<DataInodes>, FactError> {
        Ok(None)
    }

    fn package_processes(
        &mut self,
        package: &PackageName,
        uid: u32,
    ) -> Result<ProcessSet, FactError> {
        self.process_queries.push((package.clone(), uid));
        ProcessSet::new(if uid == self.running_uid {
            vec![91]
        } else {
            Vec::new()
        })
    }

    fn arm64_zygote_processes(&mut self) -> Result<ProcessSet, FactError> {
        ProcessSet::new(vec![41])
    }
}

#[test]
fn process_check_never_reuses_another_packages_cached_uid() {
    let base = DataInodes::new(101, 202).unwrap();
    let package_a = package();
    let package_b = PackageName::parse("com.asksky.fitness").unwrap();
    let uid_a = 12_345;
    let uid_b = 23_456;
    let bridge = MultiPackageBridge {
        responses: vec![
            (
                package_a.clone(),
                package_response_for(base, package_a.as_str(), uid_a),
            ),
            (
                package_b.clone(),
                package_response_for(base, package_b.as_str(), uid_b),
            ),
        ],
        calls: 0,
    };
    let facts = UidAwareFacts {
        base,
        running_uid: uid_b,
        process_queries: Vec::new(),
    };
    let mut probe = SystemPackageProbe::with_dependencies(
        bridge,
        FakeExecutor {
            outputs: VecDeque::new(),
            seen: Vec::new(),
        },
        facts,
    );

    probe.inspect_package(&package_a, UserId::PRIMARY).unwrap();
    assert_eq!(
        probe
            .running_process_count(&package_b, UserId::PRIMARY)
            .unwrap(),
        1,
        "package B must not be declared quiescent using package A's cached UID",
    );

    let (bridge, _, facts) = probe.into_dependencies();
    assert_eq!(bridge.calls, 2);
    assert_eq!(facts.process_queries, vec![(package_b, uid_b)]);
}
