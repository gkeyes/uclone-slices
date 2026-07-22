#![doc = "Fixed-root `UClone` Slots Preview daemon entry point."]

use clap::Parser;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use uclone_slot_runtime::android::{
    AndroidBackend, AndroidMaterializer, FileGateLeaseStore, GateLeaseStore, SystemCommandRunner,
    SystemMaterializerExecutor, SystemPackageProbe,
};
use uclone_slot_runtime::daemon::{DaemonError, DaemonServer, RuntimeLock, RuntimeLockError};
use uclone_slot_runtime::domain::{PackageKey, PackageName, UserId};
use uclone_slot_runtime::enrollment::EnrollmentStore;
use uclone_slot_runtime::enrollment_attempt::EnrollmentAttemptStore;
use uclone_slot_runtime::journal::JournalStore;
use uclone_slot_runtime::layout::RuntimeLayout;
use uclone_slot_runtime::production::{ProductionPlatform, SystemMetadataSource};
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
    let (keys, corrupt_discovery) = startup_keys();
    let runtime_probe = SystemPackageProbe::new();
    let runtime = AndroidBackend::new(SystemCommandRunner::new(), runtime_probe);
    let metadata = SystemMetadataSource::new();
    let mut rescue = OfflineRescuePlatform::open_fixed(runtime, metadata);
    let mut held = 0_usize;
    for key in &keys {
        if rescue
            .hold_startup_gate(key)
            .map_err(|error| UclonedError::Reconcile(error.to_string()))?
        {
            held += 1;
        }
    }
    if startup_gate {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{}", startup_gate_value(held, corrupt_discovery)?)
            .map_err(|error| UclonedError::Reconcile(error.to_string()))?;
        return Ok(());
    }
    if corrupt_discovery {
        return serve_with_lock(rescue, lock);
    }
    for key in &keys {
        match rescue.reconcile_startup(key) {
            RescueStartup::OpenOrdinary => {}
            RescueStartup::BaseRetired
            | RescueStartup::RecoveryRequired
            | RescueStartup::Quarantined => return serve_with_lock(rescue, lock),
            RescueStartup::ContainmentFailed => {
                return Err(UclonedError::Reconcile(
                    "package containment could not be proved".to_owned(),
                ));
            }
        }
    }
    run_ordinary(rescue, &keys, lock)
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
    lock: RuntimeLock,
) -> Result<(), UclonedError> {
    let (runtime, metadata, _) = rescue.into_dependencies();
    let materializer_probe = SystemPackageProbe::new();
    let materializer = AndroidMaterializer::new(SystemMaterializerExecutor, materializer_probe);
    let service_probe = SystemPackageProbe::new();
    let mut production =
        ProductionPlatform::open_fixed(runtime, materializer, service_probe, metadata)?;
    for key in keys {
        production
            .reconcile_two_phase(key)
            .map_err(|error| UclonedError::Reconcile(error.to_string()))?;
    }
    serve_with_lock(production, lock)
}

fn startup_keys() -> (Vec<PackageKey>, bool) {
    let mut packages = BTreeMap::<String, PackageName>::new();
    let mut corrupt_discovery = false;
    for root in management_package_roots() {
        corrupt_discovery |= scan_package_root(&root, &mut packages);
    }
    match EnrollmentStore::new(RuntimeLayout::enrollment_root())
        .and_then(|store| store.package_names())
    {
        Ok(scan) => {
            corrupt_discovery |= scan.corrupt_artifact();
            for package in scan.package_names() {
                insert_package(&mut packages, package.clone());
            }
        }
        Err(_) => corrupt_discovery = true,
    }
    match EnrollmentAttemptStore::fixed().and_then(|store| store.list()) {
        Ok(attempts) => {
            for attempt in attempts {
                insert_package(&mut packages, attempt.package_key().package_name().clone());
            }
        }
        Err(_) => corrupt_discovery = true,
    }
    match JournalStore::new(RuntimeLayout::journal_root()).and_then(|store| store.list()) {
        Ok(transactions) => {
            for transaction in transactions {
                insert_package(&mut packages, transaction.spec().package_name().clone());
            }
        }
        Err(_) => corrupt_discovery = true,
    }
    let mut lease_store = FileGateLeaseStore;
    match lease_store.package_names() {
        Ok(lease_packages) => {
            for package in lease_packages {
                insert_package(&mut packages, package);
            }
        }
        Err(_) => corrupt_discovery = true,
    }
    (
        packages
            .into_values()
            .map(|package| PackageKey::new(package, UserId::PRIMARY))
            .collect(),
        corrupt_discovery,
    )
}

fn management_package_roots() -> Vec<std::path::PathBuf> {
    vec![
        RuntimeLayout::enrollment_root().join("packages"),
        RuntimeLayout::compatibility_policy_root().join("packages"),
        RuntimeLayout::catalog_root().join("packages"),
        RuntimeLayout::registry_root().join("packages"),
        RuntimeLayout::package_state_root().join("packages"),
        RuntimeLayout::slot_metadata_root().join("packages"),
        RuntimeLayout::enrollment_attempt_root().join("attempts"),
        RuntimeLayout::rescue_journal_root().join("packages"),
        Path::new(uclone_slot_runtime::target::DE_SLOT_ROOT).to_path_buf(),
    ]
}

fn scan_package_root(root: &Path, packages: &mut BTreeMap<String, PackageName>) -> bool {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(_) => return true,
    };
    let mut corrupt = false;
    for entry in entries {
        let Ok(entry) = entry else {
            corrupt = true;
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            corrupt = true;
            continue;
        };
        match PackageName::parse(&name) {
            Ok(package) => {
                if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    corrupt = true;
                }
                insert_package(packages, package);
            }
            Err(_) => corrupt = true,
        }
    }
    corrupt
}

fn insert_package(packages: &mut BTreeMap<String, PackageName>, package: PackageName) {
    packages.insert(package.as_str().to_owned(), package);
}

#[cfg(test)]
#[path = "ucloned/tests.rs"]
mod tests;
