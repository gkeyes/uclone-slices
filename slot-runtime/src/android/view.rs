use crate::domain::{ManagedPackage, SlotId, SlotView};
use crate::layout::{RuntimeLayout, SlotPaths};
use crate::runtime::PlatformError;

use super::backend::AndroidBackend;
use super::command::CommandRunner;
use super::lease::GateLeaseStore;
use super::policy::{Stage, failure};
use super::probe::{MountCounts, PackageProbe, ViewProof};
use super::transition::TransitionPlan;

impl<R: CommandRunner, P: PackageProbe, L: GateLeaseStore> AndroidBackend<R, P, L> {
    pub(super) fn apply_view(
        &mut self,
        package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), PlatformError> {
        self.ensure_gate(package, Stage::ApplyView)?;
        self.ensure_mount_master(Stage::ApplyView)?;
        let current = self
            .probe
            .view_proof(package.package_name(), package.user_id())
            .map_err(|error| failure(Stage::ApplyView, error.code()))?;
        verify_namespace_inodes(current, current.canonical().inodes(), Stage::ApplyView)?;
        let counts = current.canonical().mount_counts();
        if counts.ce() != counts.de() {
            return Err(failure(Stage::ApplyView, "unpaired_mount_state"));
        }
        if counts.ce() > 1 {
            return Err(failure(Stage::ApplyView, "excess_mount_layers"));
        }

        let previous = SlotView::new(package.active_slot().clone(), package.active_inodes());
        verify_observed_view(current, &previous, Stage::ApplyView)?;
        let target_source = self.preflight_source(package, view)?;
        let previous_source = self.preflight_source(package, &previous)?;
        let canonical = RuntimeLayout::slot_paths(package.package_name(), &SlotId::base());
        let plan = TransitionPlan {
            previous,
            canonical,
            target_source,
            previous_source,
            current_mounted: counts.ce() == 1,
        };
        self.execute_transition(package, &plan)
    }

    pub(super) fn verify_view(
        &mut self,
        package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), PlatformError> {
        self.ensure_gate(package, Stage::VerifyView)?;
        self.ensure_mount_master(Stage::VerifyView)?;
        let observed = self
            .probe
            .view_proof(package.package_name(), package.user_id())
            .map_err(|error| failure(Stage::VerifyView, error.code()))?;
        verify_observed_view(observed, view, Stage::VerifyView)
    }

    pub(super) fn verify_base_without_mutation(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<(), PlatformError> {
        super::policy::ensure_supported(package, Stage::VerifyView)?;
        self.ensure_mount_master(Stage::VerifyView)?;
        let observed = self
            .probe
            .view_proof(package.package_name(), package.user_id())
            .map_err(|error| failure(Stage::VerifyView, error.code()))?;
        let base = SlotView::new(SlotId::base(), package.base_inodes());
        verify_observed_view(observed, &base, Stage::VerifyView)
    }

    fn preflight_source(
        &mut self,
        package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<Option<SlotPaths>, PlatformError> {
        if view.slot_id().is_base() {
            return Ok(None);
        }
        let source_inodes = self
            .probe
            .slot_inodes(package.package_name(), view.slot_id())
            .map_err(|error| failure(Stage::ApplyView, error.code()))?
            .ok_or_else(|| failure(Stage::ApplyView, "slot_source_missing"))?;
        if source_inodes.ce() != view.inodes().ce() {
            return Err(failure(Stage::ApplyView, "slot_source_ce_inode_mismatch"));
        }
        if source_inodes.de() != view.inodes().de() {
            return Err(failure(Stage::ApplyView, "slot_source_de_inode_mismatch"));
        }
        Ok(Some(RuntimeLayout::slot_paths(
            package.package_name(),
            view.slot_id(),
        )))
    }
}

pub(super) fn verify_observed_view(
    observed: ViewProof,
    view: &SlotView,
    stage: Stage,
) -> Result<(), PlatformError> {
    let actual_inodes = observed.canonical().inodes();
    let expected_inodes = view.inodes();
    if actual_inodes.ce() != expected_inodes.ce() {
        return Err(failure(stage, "canonical_ce_inode_mismatch"));
    }
    if actual_inodes.de() != expected_inodes.de() {
        return Err(failure(stage, "canonical_de_inode_mismatch"));
    }
    verify_namespace_inodes(observed, expected_inodes, stage)?;
    let expected_count = u32::from(!view.slot_id().is_base());
    verify_mount_counts(observed.canonical().mount_counts(), expected_count, stage)
}

fn verify_namespace_inodes(
    observed: ViewProof,
    expected: crate::domain::DataInodes,
    stage: Stage,
) -> Result<(), PlatformError> {
    if observed.mirror_inodes().ce() != expected.ce() {
        return Err(failure(stage, "mirror_ce_inode_mismatch"));
    }
    if observed.mirror_inodes().de() != expected.de() {
        return Err(failure(stage, "mirror_de_inode_mismatch"));
    }
    if observed.zygote_inodes().ce() != expected.ce() {
        return Err(failure(stage, "zygote_ce_inode_mismatch"));
    }
    if observed.zygote_inodes().de() != expected.de() {
        return Err(failure(stage, "zygote_de_inode_mismatch"));
    }
    Ok(())
}

fn verify_mount_counts(
    counts: MountCounts,
    expected: u32,
    stage: Stage,
) -> Result<(), PlatformError> {
    if counts.ce() != expected {
        return Err(failure(stage, "ce_mount_count_mismatch"));
    }
    if counts.de() != expected {
        return Err(failure(stage, "de_mount_count_mismatch"));
    }
    Ok(())
}
