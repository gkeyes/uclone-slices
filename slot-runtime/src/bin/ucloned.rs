#![doc = "Fixed-root `UClone` Slots Preview daemon entry point."]

use std::io::{self, Write};
use std::process::ExitCode;

use clap::Parser;
use uclone_slot_runtime::android::{
    AndroidBackend, AndroidMaterializer, SystemCommandRunner, SystemMaterializerExecutor,
    SystemPackageProbe,
};
use uclone_slot_runtime::daemon::{DaemonError, DaemonServer, RuntimeLock, RuntimeLockError};
use uclone_slot_runtime::domain::{PackageKey, PackageName, UserId};
use uclone_slot_runtime::layout::RuntimeLayout;
use uclone_slot_runtime::production::{ProductionPlatform, SystemMetadataSource};
use uclone_slot_runtime::protocol::ALLOWED_PACKAGE;
use uclone_slot_runtime::rescue::{OfflineRescuePlatform, RescueStartup};
use uclone_slot_runtime::service::{PreviewService, ServiceError, ServicePlatform};

/// Errors raised before the fixed root daemon socket can serve requests.
#[derive(Debug, thiserror::Error)]
enum UclonedError {
    /// Early-boot and user-unlock reconciliation could not establish a safe state.
    #[error("reconciliation failed closed: {0}")]
    Reconcile(String),
    /// The fixed 0600 Unix socket could not be prepared or served.
    #[error("daemon socket failed: {0}")]
    Daemon(#[from] DaemonError),
    #[error("production composition failed: {0}")]
    Composition(#[from] ServiceError),
    #[error("runtime lock unavailable: {0}")]
    Lock(#[from] RuntimeLockError),
}

fn serve_with_lock<P: ServicePlatform>(platform: P, lock: RuntimeLock) -> Result<(), UclonedError> {
    let service = PreviewService::new(platform);
    let mut server = DaemonServer::bind_with_lock(service, lock)?;
    server.run().map_err(UclonedError::Daemon)
}

/// Parses only the fixed daemon mode or the fixed early-boot gate probe.
#[derive(Debug, Parser)]
#[command(name = "ucloned", disable_help_subcommand = true)]
struct Cli {
    /// Capture/reuse the exact gate lease and exit without opening ordinary stores.
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
    let package = PackageName::parse(ALLOWED_PACKAGE)
        .map_err(|error| UclonedError::Reconcile(error.to_string()))?;
    let key = PackageKey::new(package, UserId::PRIMARY);
    let runtime_probe = SystemPackageProbe::new();
    let runtime = AndroidBackend::new(SystemCommandRunner::new(), runtime_probe);
    let metadata = SystemMetadataSource::new();
    let mut rescue = OfflineRescuePlatform::open_fixed(runtime, metadata);
    if startup_gate {
        let held = rescue
            .hold_startup_gate(&key)
            .map_err(|error| UclonedError::Reconcile(error.to_string()))?;
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{}", if held { "held" } else { "not-managed" })
            .map_err(|error| UclonedError::Reconcile(error.to_string()))?;
        return Ok(());
    }
    rescue
        .hold_startup_gate(&key)
        .map_err(|error| UclonedError::Reconcile(error.to_string()))?;
    match rescue.reconcile_startup(&key) {
        RescueStartup::OpenOrdinary => run_ordinary(rescue, &key, lock),
        RescueStartup::BaseRetired
        | RescueStartup::RecoveryRequired
        | RescueStartup::Quarantined => serve_with_lock(rescue, lock),
        RescueStartup::ContainmentFailed => Err(UclonedError::Reconcile(
            "package containment could not be proved".to_owned(),
        )),
    }
}

fn run_ordinary(
    rescue: OfflineRescuePlatform<
        AndroidBackend<SystemCommandRunner, SystemPackageProbe>,
        SystemMetadataSource,
    >,
    key: &PackageKey,
    lock: RuntimeLock,
) -> Result<(), UclonedError> {
    let (runtime, metadata, _) = rescue.into_dependencies();
    let materializer_probe = SystemPackageProbe::new();
    let materializer = AndroidMaterializer::new(SystemMaterializerExecutor, materializer_probe);
    let service_probe = SystemPackageProbe::new();
    let mut production =
        ProductionPlatform::open_fixed(runtime, materializer, service_probe, metadata)?;
    production
        .reconcile_two_phase(key)
        .map_err(|error| UclonedError::Reconcile(error.to_string()))?;
    serve_with_lock(production, lock)
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::Cli;

    #[test]
    fn accepts_only_the_fixed_startup_gate_flag() {
        assert!(Cli::try_parse_from(["ucloned", "--startup-gate"]).is_ok());
        assert!(Cli::try_parse_from(["ucloned", "--package", "other"]).is_err());
    }
}
