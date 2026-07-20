use std::cell::RefCell;
use std::rc::Rc;

use uclone_slot_runtime::android::{
    AndroidBackend, AndroidCommand, CommandError, CommandKind, CommandRunner, DataDomain,
    MountNamespaceProof, PackageProbe, ProbeError, ViewProof,
};
use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey,
    PackageName, PackageObservation, SlotId, SlotView, UserId,
};
use uclone_slot_runtime::lifecycle::LifecycleState;

#[path = "support/fixture.rs"]
mod fixture;
#[path = "support/model.rs"]
mod model;

use model::{apply_unmount, coherent};

#[derive(Debug, Clone, Copy)]
pub(super) enum DomainView {
    Base,
    Preview,
}

#[derive(Debug)]
struct World {
    identity: AppIdentity,
    base: DataInodes,
    preview: DataInodes,
    package_manager: DataInodes,
    view: ViewProof,
    gate: GateSnapshot,
    processes: u32,
    namespace: MountNamespaceProof,
    pending_install: bool,
    user0_unlocked: bool,
    preview_present: bool,
    fail_domain: Option<DataDomain>,
    commands: Vec<AndroidCommand>,
}

#[derive(Debug, Clone)]
pub(super) struct Fixture(Rc<RefCell<World>>);

#[derive(Debug, Clone)]
pub(super) struct Runner(Rc<RefCell<World>>);

#[derive(Debug, Clone)]
pub(super) struct Probe(Rc<RefCell<World>>);

pub(super) type Backend = AndroidBackend<Runner, Probe>;

impl CommandRunner for Runner {
    fn run(&mut self, command: &AndroidCommand) -> Result<(), CommandError> {
        let mut world = self.0.borrow_mut();
        world.commands.push(command.clone());
        let CommandKind::Unmount(domain) = command.kind() else {
            return Err(CommandError::Rejected);
        };
        if world.fail_domain == Some(domain) {
            return Err(CommandError::Rejected);
        }
        apply_unmount(&mut world, domain);
        Ok(())
    }
}

impl PackageProbe for Probe {
    fn mount_namespace_proof(&mut self) -> Result<MountNamespaceProof, ProbeError> {
        Ok(self.0.borrow().namespace)
    }

    fn observe_package(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<PackageObservation, ProbeError> {
        let world = self.0.borrow();
        Ok(PackageObservation::new(
            world.identity.clone(),
            world.package_manager,
            world.view.canonical().inodes(),
            world.view.zygote_inodes(),
            world.pending_install,
        ))
    }

    fn gate_snapshot(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<GateSnapshot, ProbeError> {
        Ok(self.0.borrow().gate)
    }

    fn running_process_count(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<u32, ProbeError> {
        Ok(self.0.borrow().processes)
    }

    fn view_proof(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<ViewProof, ProbeError> {
        Ok(self.0.borrow().view)
    }

    fn slot_inodes(
        &mut self,
        _package: &PackageName,
        slot_id: &SlotId,
    ) -> Result<Option<DataInodes>, ProbeError> {
        let world = self.0.borrow();
        if slot_id.as_str() != "preview" {
            return Err(ProbeError::InvalidResponse);
        }
        Ok(world.preview_present.then_some(world.preview))
    }

    fn user0_unlocked(&mut self) -> Result<bool, ProbeError> {
        Ok(self.0.borrow().user0_unlocked)
    }
}

pub(super) fn fixture(ce: DomainView, de: DomainView) -> (Backend, Fixture, ManagedPackage) {
    let package = managed("com.uclone.slotprobe");
    let base = package.base_inodes();
    let preview = DataInodes::new(303, 404).unwrap();
    let view = coherent(base, preview, ce, de);
    let world = Rc::new(RefCell::new(World {
        identity: package.identity().clone(),
        base,
        preview,
        package_manager: base,
        view,
        gate: GateSnapshot::new(PackageEnabledState::DisabledUser, false),
        processes: 0,
        namespace: MountNamespaceProof::new(7, 7),
        pending_install: false,
        user0_unlocked: true,
        preview_present: true,
        fail_domain: None,
        commands: Vec::new(),
    }));
    let fixture = Fixture(Rc::clone(&world));
    (
        AndroidBackend::new(Runner(Rc::clone(&world)), Probe(world)),
        fixture,
        package,
    )
}

pub(super) fn managed(name: &str) -> ManagedPackage {
    let base = DataInodes::new(101, 202).unwrap();
    ManagedPackage::new(
        PackageKey::new(PackageName::parse(name).unwrap(), UserId::PRIMARY),
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
