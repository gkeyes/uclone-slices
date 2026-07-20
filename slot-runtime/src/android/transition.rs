use crate::domain::{ManagedPackage, SlotView};
use crate::layout::SlotPaths;
use crate::runtime::PlatformError;

use super::backend::AndroidBackend;
use super::command::{AndroidCommand, CommandKind, CommandRunner, DataDomain};
use super::lease::GateLeaseStore;
use super::policy::{Stage, failure};
use super::probe::PackageProbe;
use super::view::verify_observed_view;

pub(super) struct TransitionPlan {
    pub(super) previous: SlotView,
    pub(super) canonical: SlotPaths,
    pub(super) target_source: Option<SlotPaths>,
    pub(super) previous_source: Option<SlotPaths>,
    pub(super) current_mounted: bool,
}

struct Compensation<'a> {
    package: &'a ManagedPackage,
    plan: &'a TransitionPlan,
    failure_detail: &'static str,
}

impl<R: CommandRunner, P: PackageProbe, L: GateLeaseStore> AndroidBackend<R, P, L> {
    pub(super) fn execute_transition(
        &mut self,
        package: &ManagedPackage,
        plan: &TransitionPlan,
    ) -> Result<(), PlatformError> {
        self.ensure_mount_master(Stage::ApplyView)?;
        if plan.current_mounted {
            if self
                .runner
                .run(&AndroidCommand::unmount(
                    DataDomain::Ce,
                    package.package_name(),
                    plan.canonical.ce(),
                ))
                .is_err()
            {
                return Err(self.compensated_failure(&Compensation {
                    package,
                    plan,
                    failure_detail: "unmount_ce_failed",
                }));
            }
            if self
                .runner
                .run(&AndroidCommand::unmount(
                    DataDomain::De,
                    package.package_name(),
                    plan.canonical.de(),
                ))
                .is_err()
            {
                return Err(self.compensated_failure(&Compensation {
                    package,
                    plan,
                    failure_detail: "partial_unmount_de_failed",
                }));
            }
        }
        let Some(source) = &plan.target_source else {
            return Ok(());
        };
        if self
            .runner
            .run(&AndroidCommand::bind_ce(
                package.package_name(),
                source.ce(),
                plan.canonical.ce(),
            ))
            .is_err()
        {
            return Err(self.compensated_failure(&Compensation {
                package,
                plan,
                failure_detail: "bind_ce_failed",
            }));
        }
        if self
            .runner
            .run(&AndroidCommand::bind_de(
                package.package_name(),
                source.de(),
                plan.canonical.de(),
            ))
            .is_err()
        {
            return Err(self.compensated_failure(&Compensation {
                package,
                plan,
                failure_detail: "partial_bind_de_failed",
            }));
        }
        Ok(())
    }

    fn compensated_failure(&mut self, context: &Compensation<'_>) -> PlatformError {
        if self.compensate(context).is_ok() {
            failure(Stage::ApplyView, context.failure_detail)
        } else {
            failure(
                Stage::ApplyView,
                &format!("{}_compensation_failed", context.failure_detail),
            )
        }
    }

    fn compensate(&mut self, context: &Compensation<'_>) -> Result<(), ()> {
        self.ensure_containment(context.package)?;
        self.ensure_mount_master(Stage::ApplyView).map_err(|_| ())?;
        let observed = self
            .probe
            .view_proof(context.package.package_name(), context.package.user_id())
            .map_err(|_| ())?;
        let counts = observed.canonical().mount_counts();
        if counts.ce() > 1 || counts.de() > 1 {
            return Err(());
        }
        if counts.ce() == 1 {
            self.runner
                .run(&AndroidCommand::unmount(
                    DataDomain::Ce,
                    context.package.package_name(),
                    context.plan.canonical.ce(),
                ))
                .map_err(|_| ())?;
        }
        if counts.de() == 1 {
            self.runner
                .run(&AndroidCommand::unmount(
                    DataDomain::De,
                    context.package.package_name(),
                    context.plan.canonical.de(),
                ))
                .map_err(|_| ())?;
        }
        if let Some(source) = &context.plan.previous_source {
            self.runner
                .run(&AndroidCommand::bind_ce(
                    context.package.package_name(),
                    source.ce(),
                    context.plan.canonical.ce(),
                ))
                .map_err(|_| ())?;
            self.runner
                .run(&AndroidCommand::bind_de(
                    context.package.package_name(),
                    source.de(),
                    context.plan.canonical.de(),
                ))
                .map_err(|_| ())?;
        }
        self.ensure_mount_master(Stage::ApplyView).map_err(|_| ())?;
        let restored = self
            .probe
            .view_proof(context.package.package_name(), context.package.user_id())
            .map_err(|_| ())?;
        verify_observed_view(restored, &context.plan.previous, Stage::ApplyView).map_err(|_| ())
    }

    fn ensure_containment(&mut self, package: &ManagedPackage) -> Result<(), ()> {
        if self.ensure_gate(package, Stage::ApplyView).is_ok() {
            return Ok(());
        }
        self.runner
            .run(&AndroidCommand::package(
                CommandKind::DisableUser,
                package.package_name(),
            ))
            .map_err(|_| ())?;
        self.ensure_gate(package, Stage::ApplyView)
            .map_err(|_| ())?;
        self.runner
            .run(&AndroidCommand::package(
                CommandKind::ForceStop,
                package.package_name(),
            ))
            .map_err(|_| ())?;
        let count = self
            .probe
            .running_process_count(package.package_name(), package.user_id())
            .map_err(|_| ())?;
        if count == 0 { Ok(()) } else { Err(()) }
    }
}
