use uclone_slot_runtime::android::{
    AndroidCommand, CanonicalView, DataDomain, MountCounts, MountNamespaceProof, ViewProof,
};
use uclone_slot_runtime::domain::{AppIdentity, DataInodes, GateSnapshot};

use super::Fixture;

impl Fixture {
    pub(crate) fn commands(&self) -> Vec<AndroidCommand> {
        self.0.borrow().commands.clone()
    }

    pub(crate) fn view(&self) -> ViewProof {
        self.0.borrow().view
    }

    pub(crate) fn base(&self) -> DataInodes {
        self.0.borrow().base
    }

    pub(crate) fn preview(&self) -> DataInodes {
        self.0.borrow().preview
    }

    pub(crate) fn set_counts(&self, counts: MountCounts) {
        let mut world = self.0.borrow_mut();
        let canonical = world.view.canonical().inodes();
        world.view = ViewProof::new(
            CanonicalView::new(canonical, counts),
            world.view.mirror_inodes(),
            world.view.zygote_inodes(),
        );
    }

    pub(crate) fn set_view(&self, canonical: DataInodes, mirror: DataInodes, zygote: DataInodes) {
        let mut world = self.0.borrow_mut();
        let counts = world.view.canonical().mount_counts();
        world.view = ViewProof::new(CanonicalView::new(canonical, counts), mirror, zygote);
    }

    pub(crate) fn set_namespace(&self, proof: MountNamespaceProof) {
        self.0.borrow_mut().namespace = proof;
    }

    pub(crate) fn set_gate(&self, snapshot: GateSnapshot) {
        self.0.borrow_mut().gate = snapshot;
    }

    pub(crate) fn set_processes(&self, count: u32) {
        self.0.borrow_mut().processes = count;
    }

    pub(crate) fn set_identity(&self, identity: AppIdentity) {
        self.0.borrow_mut().identity = identity;
    }

    pub(crate) fn set_package_manager(&self, inodes: DataInodes) {
        self.0.borrow_mut().package_manager = inodes;
    }

    pub(crate) fn set_pending_install(&self, pending: bool) {
        self.0.borrow_mut().pending_install = pending;
    }

    pub(crate) fn set_user0_unlocked(&self, unlocked: bool) {
        self.0.borrow_mut().user0_unlocked = unlocked;
    }

    pub(crate) fn fail_on(&self, domain: Option<DataDomain>) {
        self.0.borrow_mut().fail_domain = domain;
    }
}
