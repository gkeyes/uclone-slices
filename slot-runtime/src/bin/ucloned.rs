#![doc = "Fixed-root `UClone` Slots Preview daemon entry point."]

use clap::Parser;
use std::collections::BTreeSet;
use std::io::{self, Write};
use std::process::ExitCode;
use uclone_slot_runtime::android::{
    AndroidBackend, AndroidMaterializer, SystemCommandRunner, SystemMaterializerExecutor,
    SystemPackageProbe,
};
use uclone_slot_runtime::daemon::{DaemonError, DaemonServer, RuntimeLock, RuntimeLockError};
use uclone_slot_runtime::domain::{PackageKey, PackageName};
use uclone_slot_runtime::layout::RuntimeLayout;
use uclone_slot_runtime::production::{ProductionPlatform, SystemMetadataSource};
use uclone_slot_runtime::reconcile::ReconcileOutcome;
use uclone_slot_runtime::rescue::{
    OfflineRescuePlatform, RescueStartup, RescueStatus, StartupGateOutcome,
};
use uclone_slot_runtime::service::{PreviewService, ServiceError, ServicePlatform};

#[path = "ucloned/discovery.rs"]
pub(crate) mod discovery;
use discovery::startup_keys;
#[derive(Debug, thiserror::Error)]
enum UclonedError {
    #[error("reconciliation failed closed: {0}")]
    Reconcile(String),
    #[error("daemon socket failed: {0}")]
    Daemon(#[from] DaemonError),
    #[error("production composition failed: {0}")]
    Composition(#[from] ServiceError),
    #[error("runtime lock unavailable: {0}")]
    Lock(#[from] RuntimeLockError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupPackageOutcome {
    Ordinary,
    RecoveryRequired,
    BaseRetired,
}

fn record_startup_outcome(
    key: &PackageKey,
    outcome: StartupPackageOutcome,
    ordinary: &mut Vec<PackageKey>,
    recovery: &mut BTreeSet<PackageName>,
    retired: &mut BTreeSet<PackageName>,
) {
    match outcome {
        StartupPackageOutcome::Ordinary => ordinary.push(key.clone()),
        StartupPackageOutcome::RecoveryRequired => {
            recovery.insert(key.package_name().clone());
        }
        StartupPackageOutcome::BaseRetired => {
            retired.insert(key.package_name().clone());
        }
    }
}
fn serve_with_lock<P: ServicePlatform>(platform: P, lock: RuntimeLock) -> Result<(), UclonedError> {
    let service = PreviewService::new(platform);
    let mut server = DaemonServer::bind_with_lock(service, lock)?;
    server.run().map_err(UclonedError::Daemon)
}
fn serve_recovery_with_lock<P: ServicePlatform>(
    platform: P,
    lock: RuntimeLock,
) -> Result<(), UclonedError> {
    let service = PreviewService::new_recovery_only(platform);
    let mut server = DaemonServer::bind_with_lock(service, lock)?;
    server.run().map_err(UclonedError::Daemon)
}

#[derive(Debug, Parser)]
#[command(name = "ucloned", disable_help_subcommand = true)]
struct Cli {
    #[arg(long)]
    startup_gate: bool,
}
fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.startup_gate) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let mut stderr = io::stderr().lock();
            let _ = writeln!(stderr, "ucloned: {error}");
            ExitCode::FAILURE
        }
    }
}
fn run(startup_gate: bool) -> Result<(), UclonedError> {
    let lock = RuntimeLock::acquire(RuntimeLayout::lock(), "ucloned")?;
    let (keys, corrupt_discovery) = startup_keys();
    let runtime_probe = SystemPackageProbe::new();
    let runtime = AndroidBackend::new(SystemCommandRunner::new(), runtime_probe);
    let metadata = SystemMetadataSource::new();
    let mut rescue = OfflineRescuePlatform::open_fixed(runtime, metadata);
    let mut held = 0_usize;
    let mut recovery = BTreeSet::new();
    let mut retired = BTreeSet::new();
    for key in &keys {
        let rescue_status = rescue.startup_status(key);
        if matches!(rescue_status, Ok(Some(RescueStatus::BaseRetired))) {
            match rescue.reconcile_startup(key) {
                RescueStartup::BaseRetired => {
                    retired.insert(key.package_name().clone());
                    continue;
                }
                RescueStartup::RecoveryRequired | RescueStartup::Quarantined => {
                    recovery.insert(key.package_name().clone());
                    held += 1;
                    continue;
                }
                RescueStartup::ContainmentFailed => {
                    return Err(UclonedError::Reconcile(
                        "retired package containment could not be proved".to_owned(),
                    ));
                }
                RescueStartup::OpenOrdinary => {}
            }
        }
        match rescue
            .hold_startup_gate(key)
            .map_err(|error| UclonedError::Reconcile(error.to_string()))?
        {
            StartupGateOutcome::NotManaged => {}
            StartupGateOutcome::Held => held += 1,
            StartupGateOutcome::HeldRecovery => {
                held += 1;
                recovery.insert(key.package_name().clone());
            }
        }
    }
    if startup_gate {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{}", startup_gate_value(held, corrupt_discovery)?)
            .map_err(|error| UclonedError::Reconcile(error.to_string()))?;
        return Ok(());
    }
    authorize_only_typed_recovery_targets(&mut rescue, &keys, &retired);
    if corrupt_discovery {
        return serve_recovery_with_lock(rescue, lock);
    }
    let mut ordinary = Vec::with_capacity(keys.len());
    for key in &keys {
        if retired.contains(key.package_name()) || recovery.contains(key.package_name()) {
            continue;
        }
        match rescue.reconcile_startup(key) {
            RescueStartup::OpenOrdinary => record_startup_outcome(
                key,
                StartupPackageOutcome::Ordinary,
                &mut ordinary,
                &mut recovery,
                &mut retired,
            ),
            RescueStartup::RecoveryRequired | RescueStartup::Quarantined => record_startup_outcome(
                key,
                StartupPackageOutcome::RecoveryRequired,
                &mut ordinary,
                &mut recovery,
                &mut retired,
            ),
            RescueStartup::BaseRetired => record_startup_outcome(
                key,
                StartupPackageOutcome::BaseRetired,
                &mut ordinary,
                &mut recovery,
                &mut retired,
            ),
            RescueStartup::ContainmentFailed => {
                return Err(UclonedError::Reconcile(
                    "package containment could not be proved".to_owned(),
                ));
            }
        }
    }
    run_ordinary(rescue, &ordinary, &recovery, lock)
}

fn authorize_only_typed_recovery_targets<B, T, F>(
    rescue: &mut OfflineRescuePlatform<B, T, F>,
    keys: &[PackageKey],
    retired: &BTreeSet<PackageName>,
) {
    for key in keys {
        if retired.contains(key.package_name()) {
            continue;
        }
        let _authorization = rescue.authorize_existing_target(key);
    }
}
fn startup_gate_value(held: usize, corrupt_discovery: bool) -> Result<&'static str, UclonedError> {
    if corrupt_discovery {
        return Err(UclonedError::Reconcile(
            "management artifacts are corrupt; boot containment cannot be proved".to_owned(),
        ));
    }
    Ok(if held > 0 { "held" } else { "not-managed" })
}

fn run_ordinary(
    rescue: OfflineRescuePlatform<
        AndroidBackend<SystemCommandRunner, SystemPackageProbe>,
        SystemMetadataSource,
    >,
    keys: &[PackageKey],
    recovery: &BTreeSet<PackageName>,
    lock: RuntimeLock,
) -> Result<(), UclonedError> {
    let (runtime, metadata, _) = rescue.into_dependencies();
    let materializer_probe = SystemPackageProbe::new();
    let materializer = AndroidMaterializer::new(SystemMaterializerExecutor, materializer_probe);
    let service_probe = SystemPackageProbe::new();
    let mut production =
        ProductionPlatform::open_fixed(runtime, materializer, service_probe, metadata)?;
    for key in discovery::reconciliation_keys(keys, recovery) {
        reconcile_startup_result(&key, production.reconcile_two_phase(&key))?;
    }
    serve_with_lock(production, lock)
}

fn reconcile_startup_result(
    key: &PackageKey,
    result: Result<ReconcileOutcome, ServiceError>,
) -> Result<(), UclonedError> {
    result.map(|_| ()).map_err(|error| {
        UclonedError::Reconcile(format!(
            "package {} reconciliation containment failed: {error}",
            key.package_name()
        ))
    })
}

#[cfg(test)]
#[path = "ucloned/tests.rs"]
mod tests;
