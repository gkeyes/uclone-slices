use crate::domain::{DataInodes, ManagedPackage, PackageObservation, SlotId};
use crate::layout::RuntimeLayout;
use crate::runtime::PlatformError;

use super::backend::AndroidBackend;
use super::command::{AndroidCommand, CommandRunner, DataDomain};
use super::lease::GateLeaseStore;
use super::policy::{Stage, ensure_supported, failure};
use super::probe::{PackageProbe, ViewProof};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DomainState {
    NativeBase,
    PreviewBound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RescueState {
    ce: DomainState,
    de: DomainState,
}

#[derive(Debug, Clone, Copy)]
struct DomainProof {
    name: &'static str,
    count: u32,
    canonical_inode: u64,
    mirror_inode: u64,
    zygote_inode: u64,
    base_inode: u64,
}

impl RescueState {
    const BASE: Self = Self {
        ce: DomainState::NativeBase,
        de: DomainState::NativeBase,
    };
}

impl<R: CommandRunner, P: PackageProbe, L: GateLeaseStore> AndroidBackend<R, P, L> {
    pub(super) fn restore_native_base(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<(), PlatformError> {
        ensure_supported(package, Stage::ApplyView)?;
        if !package.active_slot().is_base() || package.active_inodes() != package.base_inodes() {
            return Err(failure(Stage::ApplyView, "enrolled_base_not_native"));
        }
        let initial = self.prove_rescue_state(package)?;
        let canonical = RuntimeLayout::slot_paths(package.package_name(), &SlotId::base());

        if initial.ce == DomainState::PreviewBound {
            self.runner
                .run(&AndroidCommand::unmount(
                    DataDomain::Ce,
                    package.package_name(),
                    canonical.ce(),
                ))
                .map_err(|_| failure(Stage::ApplyView, "unmount_ce_failed"))?;
            self.prove_expected(
                package,
                RescueState {
                    ce: DomainState::NativeBase,
                    de: initial.de,
                },
            )?;
        }

        if initial.de == DomainState::PreviewBound {
            self.runner
                .run(&AndroidCommand::unmount(
                    DataDomain::De,
                    package.package_name(),
                    canonical.de(),
                ))
                .map_err(|_| failure(Stage::ApplyView, "partial_unmount_de_failed"))?;
        }
        self.prove_expected(package, RescueState::BASE)
    }

    fn prove_expected(
        &mut self,
        package: &ManagedPackage,
        expected: RescueState,
    ) -> Result<(), PlatformError> {
        let actual = self.prove_rescue_state(package)?;
        if actual == expected {
            Ok(())
        } else {
            Err(failure(Stage::ApplyView, "rescue_transition_mismatch"))
        }
    }

    fn prove_rescue_state(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<RescueState, PlatformError> {
        self.ensure_rescue_containment(package)?;
        let unlocked = self
            .probe
            .user0_unlocked()
            .map_err(|error| failure(Stage::ApplyView, error.code()))?;
        if !unlocked {
            return Err(failure(Stage::ApplyView, "user0_locked"));
        }
        let observation = self
            .probe
            .observe_package(package.package_name(), package.user_id())
            .map_err(|error| failure(Stage::ApplyView, error.code()))?;
        verify_identity_and_base(package, &observation)?;
        let view = self
            .probe
            .view_proof(package.package_name(), package.user_id())
            .map_err(|error| failure(Stage::ApplyView, error.code()))?;
        verify_samples_match(&observation, view)?;
        let counts = view.canonical().mount_counts();
        if counts.ce() > 1 || counts.de() > 1 {
            return Err(failure(Stage::ApplyView, "excess_mount_layers"));
        }
        let state = classify_view(package.base_inodes(), view)?;
        self.ensure_rescue_containment(package)?;
        Ok(state)
    }

    fn ensure_rescue_containment(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        self.ensure_gate(package, Stage::ApplyView)?;
        let processes = self
            .probe
            .running_process_count(package.package_name(), package.user_id())
            .map_err(|error| failure(Stage::ApplyView, error.code()))?;
        if processes != 0 {
            return Err(failure(Stage::ApplyView, "processes_still_running"));
        }
        self.ensure_mount_master(Stage::ApplyView)
    }
}

fn verify_identity_and_base(
    package: &ManagedPackage,
    observed: &PackageObservation,
) -> Result<(), PlatformError> {
    if observed.identity() != package.identity() {
        return Err(failure(Stage::ApplyView, "package_identity_mismatch"));
    }
    if observed.package_manager_inodes() != package.base_inodes() {
        return Err(failure(
            Stage::ApplyView,
            "package_manager_base_inode_mismatch",
        ));
    }
    if observed.pending_install() {
        return Err(failure(Stage::ApplyView, "pending_install"));
    }
    Ok(())
}

fn verify_samples_match(
    observed: &PackageObservation,
    view: ViewProof,
) -> Result<(), PlatformError> {
    if observed.canonical_inodes() != view.canonical().inodes() {
        return Err(failure(Stage::ApplyView, "canonical_sample_changed"));
    }
    if observed.active_process_inodes() != view.zygote_inodes() {
        return Err(failure(Stage::ApplyView, "zygote_sample_changed"));
    }
    Ok(())
}

fn classify_view(base: DataInodes, view: ViewProof) -> Result<RescueState, PlatformError> {
    let canonical = view.canonical().inodes();
    let mirror = view.mirror_inodes();
    let zygote = view.zygote_inodes();
    let counts = view.canonical().mount_counts();
    Ok(RescueState {
        ce: classify_domain(DomainProof {
            name: "ce",
            count: counts.ce(),
            canonical_inode: canonical.ce().get(),
            mirror_inode: mirror.ce().get(),
            zygote_inode: zygote.ce().get(),
            base_inode: base.ce().get(),
        })?,
        de: classify_domain(DomainProof {
            name: "de",
            count: counts.de(),
            canonical_inode: canonical.de().get(),
            mirror_inode: mirror.de().get(),
            zygote_inode: zygote.de().get(),
            base_inode: base.de().get(),
        })?,
    })
}

fn classify_domain(proof: DomainProof) -> Result<DomainState, PlatformError> {
    if proof.canonical_inode != proof.mirror_inode || proof.canonical_inode != proof.zygote_inode {
        return Err(failure(
            Stage::ApplyView,
            &format!("{}_view_split", proof.name),
        ));
    }
    match proof.count {
        0 if proof.canonical_inode == proof.base_inode => Ok(DomainState::NativeBase),
        0 => Err(failure(
            Stage::ApplyView,
            &format!("{}_native_base_inode_mismatch", proof.name),
        )),
        1 if proof.canonical_inode != proof.base_inode => Ok(DomainState::PreviewBound),
        1 => Err(failure(
            Stage::ApplyView,
            &format!("{}_bound_to_base", proof.name),
        )),
        _ => Err(failure(Stage::ApplyView, "excess_mount_layers")),
    }
}
