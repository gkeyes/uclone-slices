use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::launch::{LaunchGate, PROCESS_VIEW_WORKER, ProcessIdentity, tracked_processes};
use crate::layout;
use crate::model::{
    AccountIoToken, Capabilities, ObservedView, PackageEnabledState, PackageIdentity,
    PackageInspection, PackageName, SlotId,
};
use crate::ports::{AdapterError, AndroidOps};

const PROCESS_EXIT_CONFIRMATION_WINDOW: Duration = Duration::from_secs(10);
const PROCESS_EXIT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// How `force_stop` confirms that a package has stopped.
///
/// Most packages use the generic confirmation (one `am force-stop`, then wait until two
/// consecutive scans find no process for the UID). Packages listed in
/// [`FORCE_STOP_POLICIES`] restart themselves right after a force-stop (for example
/// WeChat's FastRestart), so they get a bounded retry loop instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ForceStopPolicy {
    /// Name used in diagnostics.
    label: &'static str,
    /// Maximum `am force-stop` requests, counting the first one.
    force_stop_limit: usize,
    /// Total time allowed for the package to quiesce.
    confirmation_window: Duration,
    /// How long the UID must stay without processes to count as stopped.
    quiet_window: Duration,
}

/// Packages that need a fast-restart force-stop policy, keyed by package name.
const FORCE_STOP_POLICIES: &[(&str, ForceStopPolicy)] = &[(
    "com.tencent.mm",
    ForceStopPolicy {
        label: "WeChat FastRestart",
        force_stop_limit: 5,
        confirmation_window: Duration::from_secs(2),
        quiet_window: Duration::from_millis(500),
    },
)];

fn force_stop_policy(package: &PackageName) -> Option<&'static ForceStopPolicy> {
    FORCE_STOP_POLICIES
        .iter()
        .find(|(name, _)| *name == package.as_str())
        .map(|(_, policy)| policy)
}

#[derive(Debug, Clone)]
struct CommandOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

trait CommandRunner: core::fmt::Debug {
    fn run(&mut self, program: &str, arguments: &[&str]) -> Result<CommandOutput, AdapterError>;
}

#[derive(Debug, Default)]
struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    fn run(&mut self, program: &str, arguments: &[&str]) -> Result<CommandOutput, AdapterError> {
        let output = Command::new(program)
            .args(arguments)
            .output()
            .map_err(|error| AdapterError::new(error.to_string()))?;
        Ok(CommandOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

#[derive(Debug)]
pub(crate) struct SystemAndroidOps {
    build_id: String,
    ce_slots_root: PathBuf,
    de_slots_root: PathBuf,
    canonical_ce_root: PathBuf,
    canonical_de_root: PathBuf,
    mountinfo: PathBuf,
    proc_root: PathBuf,
    runner: Box<dyn CommandRunner>,
    // One SystemAndroidOps serves one request (ucloned recomposes the Runtime per request),
    // so these answers cannot go stale across requests. Within a switch they replace two
    // repeated full package listings and a repeated Launcher resolution.
    package_rows: Option<Vec<(String, u32)>>,
    launchers: BTreeMap<PackageName, String>,
    launch_gate: Option<Arc<LaunchGate>>,
    runtime_root: PathBuf,
}

impl SystemAndroidOps {
    pub(crate) fn new(
        build_id: impl Into<String>,
        ce_slots_root: impl Into<PathBuf>,
        de_slots_root: impl Into<PathBuf>,
        canonical_ce_root: impl Into<PathBuf>,
        canonical_de_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            build_id: build_id.into(),
            ce_slots_root: ce_slots_root.into(),
            de_slots_root: de_slots_root.into(),
            canonical_ce_root: canonical_ce_root.into(),
            canonical_de_root: canonical_de_root.into(),
            mountinfo: PathBuf::from("/proc/self/mountinfo"),
            proc_root: PathBuf::from("/proc"),
            runner: Box::<ProcessRunner>::default(),
            package_rows: None,
            launchers: BTreeMap::new(),
            launch_gate: None,
            runtime_root: PathBuf::from(layout::RUNTIME_ROOT),
        }
    }

    pub(crate) fn set_launch_gate(&mut self, root: &Path, gate: Option<Arc<LaunchGate>>) {
        self.runtime_root = root.to_path_buf();
        self.launch_gate = gate;
    }

    fn canonical_paths(&self, package: &PackageName) -> (PathBuf, PathBuf) {
        (
            self.canonical_ce_root.join(package.as_str()),
            self.canonical_de_root.join(package.as_str()),
        )
    }

    fn slot_paths(&self, package: &PackageName, slot: &SlotId) -> (PathBuf, PathBuf) {
        (
            self.ce_slots_root
                .join(package.as_str())
                .join(slot.as_str()),
            self.de_slots_root
                .join(package.as_str())
                .join(slot.as_str()),
        )
    }

    fn maintenance_paths(
        &self,
        package: &PackageName,
        token: &AccountIoToken,
    ) -> Result<(PathBuf, PathBuf), AdapterError> {
        layout::storage_paths(
            &self.ce_slots_root,
            &self.de_slots_root,
            layout::MAINTENANCE_DIR,
            &[token.as_str(), package.as_str()],
        )
        .ok_or_else(|| AdapterError::new("slots root has no storage parent"))
    }

    fn package_enabled_state(
        &mut self,
        package: &PackageName,
    ) -> Result<PackageEnabledState, AdapterError> {
        let output = self.required("/system/bin/dumpsys", &["package", package.as_str()])?;
        parse_user0_enabled_state(&output.stdout)
            .ok_or_else(|| AdapterError::new("package user0 enabled state is unavailable"))
    }

    fn run(&mut self, program: &str, arguments: &[&str]) -> Result<CommandOutput, AdapterError> {
        self.runner.run(program, arguments)
    }

    fn required(
        &mut self,
        program: &str,
        arguments: &[&str],
    ) -> Result<CommandOutput, AdapterError> {
        let output = self.run(program, arguments)?;
        if output.success {
            Ok(output)
        } else {
            Err(AdapterError::new(format!(
                "{program} failed: {}",
                output.stderr.trim()
            )))
        }
    }

    fn package_rows(&mut self) -> Result<&[(String, u32)], AdapterError> {
        if self.package_rows.is_none() {
            let output = self.required(
                "/system/bin/cmd",
                &["package", "list", "packages", "-3", "-U", "--user", "0"],
            )?;
            let rows = output
                .stdout
                .lines()
                .filter_map(parse_package_uid_row)
                .map(|(name, uid)| (name.to_owned(), uid))
                .collect::<Vec<_>>();
            self.package_rows = Some(rows);
        }
        Ok(self.package_rows.as_deref().unwrap_or_default())
    }

    fn package_uid(&mut self, package: &PackageName) -> Result<u32, AdapterError> {
        package_uid_from_rows(self.package_rows()?, package)
    }

    /// Every process in `/proc` with its owning uid.
    fn process_uids(&self) -> Result<Vec<(PathBuf, u32)>, AdapterError> {
        let entries =
            fs::read_dir(&self.proc_root).map_err(|error| AdapterError::new(error.to_string()))?;
        let mut processes = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(AdapterError::new(error.to_string())),
            };
            let name = entry.file_name();
            let Some(pid) = name.to_str() else {
                continue;
            };
            if !pid.bytes().all(|byte| byte.is_ascii_digit()) {
                continue;
            }
            if let Some(uid) = process_uid(&entry.path())? {
                processes.push((entry.path(), uid));
            }
        }
        Ok(processes)
    }

    fn running_uids(&self) -> Result<HashSet<u32>, AdapterError> {
        Ok(self
            .process_uids()?
            .into_iter()
            .map(|(_process, uid)| uid)
            .collect())
    }

    fn apk_path(&mut self, package: &PackageName) -> Result<PathBuf, AdapterError> {
        let output = self.required(
            "/system/bin/cmd",
            &["package", "path", "--user", "0", package.as_str()],
        )?;
        output
            .stdout
            .lines()
            .filter_map(|line| line.trim().strip_prefix("package:"))
            .map(PathBuf::from)
            .find(|path| path.file_name().is_some_and(|name| name == "base.apk"))
            .ok_or_else(|| AdapterError::new("package base APK was not found"))
    }

    fn launcher_component(&mut self, package: &PackageName) -> Result<String, AdapterError> {
        if let Some(component) = self.launchers.get(package) {
            return Ok(component.clone());
        }
        let output = self.required(
            "/system/bin/cmd",
            &[
                "package",
                "resolve-activity",
                "--brief",
                "--user",
                "0",
                "-a",
                "android.intent.action.MAIN",
                "-c",
                "android.intent.category.LAUNCHER",
                package.as_str(),
            ],
        )?;
        let component = parse_launcher_component(&output.stdout, package)
            .ok_or_else(|| AdapterError::new("package has no Launcher activity"))?;
        self.launchers.insert(package.clone(), component.clone());
        Ok(component)
    }

    fn read_self_view(&self, package: &PackageName) -> Result<ObservedView, AdapterError> {
        let content = fs::read_to_string(&self.mountinfo)
            .map_err(|error| AdapterError::new(error.to_string()))?;
        Ok(runtime_view_from_mountinfo(
            &content,
            package,
            &self.canonical_ce_root,
            &self.canonical_de_root,
        ))
    }

    fn package_processes(&self, uid: u32) -> Result<Vec<PathBuf>, AdapterError> {
        let mut processes = self
            .process_uids()?
            .into_iter()
            .filter(|(_process, owner)| *owner == uid)
            .map(|(process, _owner)| process)
            .collect::<Vec<_>>();
        if self.launch_gate.is_some() {
            processes.extend(
                tracked_processes(&self.runtime_root, uid)
                    .map_err(|error| AdapterError::new(error.to_string()))?,
            );
        }
        processes.sort();
        processes.dedup();
        Ok(processes)
    }

    /// The installed identity of `package` running as `uid` from `apk_path`; both data
    /// directories must exist.
    fn package_identity(
        &self,
        package: &PackageName,
        uid: u32,
        apk_path: &Path,
    ) -> Result<PackageIdentity, AdapterError> {
        let (canonical_ce, canonical_de) = self.canonical_paths(package);
        if !canonical_ce.is_dir() || !canonical_de.is_dir() {
            return Err(AdapterError::new(
                "package CE/DE directories are not both available",
            ));
        }
        let metadata =
            fs::metadata(apk_path).map_err(|error| AdapterError::new(error.to_string()))?;
        PackageIdentity::new(
            uid,
            path_text(apk_path)?.to_owned(),
            metadata.dev(),
            metadata.ino(),
        )
        .map_err(|error| AdapterError::new(error.to_string()))
    }

    fn confirm_package_processes_stopped(&self, uid: u32) -> Result<(), AdapterError> {
        confirm_processes_stably_stopped(
            uid,
            PROCESS_EXIT_CONFIRMATION_WINDOW,
            PROCESS_EXIT_POLL_INTERVAL,
            || self.package_processes(uid),
            |known| {
                let mut survivors = surviving_processes(known, uid)?;
                if self.launch_gate.is_some() {
                    survivors.extend(
                        tracked_processes(&self.runtime_root, uid)
                            .map_err(|error| AdapterError::new(error.to_string()))?,
                    );
                    survivors.sort();
                    survivors.dedup();
                }
                Ok(survivors)
            },
        )
    }

    fn request_force_stop(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        self.required(
            "/system/bin/am",
            &["force-stop", "--user", "0", package.as_str()],
        )?;
        Ok(())
    }

    fn force_stop_with_policy(
        &mut self,
        package: &PackageName,
        uid: u32,
        policy: &ForceStopPolicy,
    ) -> Result<(), AdapterError> {
        let deadline = Instant::now() + policy.confirmation_window;
        let mut previous_generation = self.package_processes(uid)?;
        let mut force_stop_attempts = 0_usize;

        loop {
            self.request_force_stop(package)?;
            force_stop_attempts += 1;
            let mut empty_since = None;

            loop {
                let remaining = self.package_processes(uid)?;
                let now = Instant::now();
                if remaining.is_empty() {
                    let quiet_start = *empty_since.get_or_insert(now);
                    if now.duration_since(quiet_start) >= policy.quiet_window {
                        return Ok(());
                    }
                } else {
                    empty_since = None;
                    let has_new_generation = remaining
                        .iter()
                        .any(|process| !previous_generation.contains(process));
                    if has_new_generation && force_stop_attempts < policy.force_stop_limit {
                        previous_generation = remaining;
                        break;
                    }
                }

                if now >= deadline {
                    return Err(AdapterError::new(format!(
                        "{} did not quiesce within {} ms after {force_stop_attempts} force-stop attempt(s); {} process(es) remain for UID {uid}",
                        policy.label,
                        policy.confirmation_window.as_millis(),
                        remaining.len()
                    )));
                }
                thread::sleep(
                    PROCESS_EXIT_POLL_INTERVAL.min(deadline.saturating_duration_since(now)),
                );
            }
        }
    }

    fn ensure_unmounted(&mut self, mount_point: &Path) -> Result<(), AdapterError> {
        loop {
            let content = fs::read_to_string(&self.mountinfo)
                .map_err(|error| AdapterError::new(error.to_string()))?;
            let before = mount_roots(&content, mount_point).len();
            if before == 0 {
                return Ok(());
            }
            let mount_point_text = path_text(mount_point)?;
            let output = self.run("/system/bin/umount", &[mount_point_text])?;
            if !output.success {
                let busy_error = format!("umount: {mount_point_text}: Device or resource busy");
                if output.stderr.trim() == busy_error {
                    self.required("/system/bin/umount", &["-l", mount_point_text])?;
                } else {
                    return Err(AdapterError::new(format!(
                        "/system/bin/umount failed: {}",
                        output.stderr.trim()
                    )));
                }
            }
            let content = fs::read_to_string(&self.mountinfo)
                .map_err(|error| AdapterError::new(error.to_string()))?;
            let after = mount_roots(&content, mount_point).len();
            if after >= before {
                return Err(AdapterError::new(
                    "unmount made no progress in the Runtime namespace",
                ));
            }
        }
    }

    fn cleanup_mounts(&mut self, canonical_ce: &Path, canonical_de: &Path) {
        if let Err(error) = self.ensure_unmounted(canonical_ce) {
            eprintln!(
                "op=apply_view step=cleanup_ce path={} error={error}",
                canonical_ce.display()
            );
        }
        if let Err(error) = self.ensure_unmounted(canonical_de) {
            eprintln!(
                "op=apply_view step=cleanup_de path={} error={error}",
                canonical_de.display()
            );
        }
    }

    fn contain_launch_failure(
        &mut self,
        package: &PackageName,
        verification_error: AdapterError,
    ) -> AdapterError {
        match self.force_stop(package) {
            Ok(()) => AdapterError::new(format!(
                "{verification_error}; App was force-stopped and UID processes are zero"
            )),
            Err(containment_error) => AdapterError::new(format!(
                "{verification_error}; containment failed: {containment_error}"
            )),
        }
    }
}

/// Polls until two consecutive full scans find no process for `uid`, or the window ends.
/// While processes from the last full scan are still alive, only those are rechecked: the
/// package cannot have stopped yet, so a full `/proc` scan would not change the answer.
fn confirm_processes_stably_stopped<F, G>(
    uid: u32,
    confirmation_window: Duration,
    poll_interval: Duration,
    mut scan_all: F,
    mut recheck: G,
) -> Result<(), AdapterError>
where
    F: FnMut() -> Result<Vec<PathBuf>, AdapterError>,
    G: FnMut(&[PathBuf]) -> Result<Vec<PathBuf>, AdapterError>,
{
    let deadline = Instant::now() + confirmation_window;
    let mut previous_scan_was_empty = false;
    let mut remaining = Vec::new();
    loop {
        if !remaining.is_empty() {
            remaining = recheck(&remaining)?;
        }
        if remaining.is_empty() {
            remaining = scan_all()?;
        }
        if remaining.is_empty() {
            if previous_scan_was_empty {
                return Ok(());
            }
            previous_scan_was_empty = true;
        } else {
            previous_scan_was_empty = false;
        }
        let now = Instant::now();
        if now >= deadline {
            if remaining.is_empty() {
                return Err(AdapterError::new(format!(
                    "force-stopped package process state did not remain empty for UID {uid}"
                )));
            }
            return Err(AdapterError::new(format!(
                "force-stopped package still has {} process(es) for UID {uid}",
                remaining.len()
            )));
        }
        thread::sleep(poll_interval.min(deadline.saturating_duration_since(now)));
    }
}

impl AndroidOps for SystemAndroidOps {
    fn probe(&mut self) -> Result<Capabilities, AdapterError> {
        let user = self.required("/system/bin/cmd", &["activity", "get-current-user"])?;
        if user.stdout.trim() != "0" {
            return Err(AdapterError::new("current Android user is not user0"));
        }
        let unlocked = self.required(
            "/system/bin/cmd",
            &["activity", "get-started-user-state", "0"],
        )?;
        if unlocked.stdout.trim() != "RUNNING_UNLOCKED" {
            return Err(AdapterError::new("user0 is not unlocked"));
        }
        let (launch_gate_ready, process_view_ready) = self
            .launch_gate
            .as_ref()
            .map_or((false, false), |gate| gate.readiness());
        Ok(Capabilities {
            build_id: self.build_id.clone(),
            binary_build_id: crate::BINARY_BUILD_ID.to_owned(),
            launch_gate_ready,
            process_view_ready,
        })
    }

    fn inspect(&mut self, package: &PackageName) -> Result<PackageInspection, AdapterError> {
        let uid = self.package_uid(package)?;
        let apk_path = self.apk_path(package)?;
        let _launcher = self.launcher_component(package)?;
        let identity = self.package_identity(package, uid, &apk_path)?;
        Ok(PackageInspection {
            identity,
            was_running: !self.package_processes(uid)?.is_empty(),
        })
    }

    fn inspect_many(
        &mut self,
        packages: &[PackageName],
    ) -> Vec<(PackageName, Result<PackageInspection, AdapterError>)> {
        if packages.is_empty() {
            return Vec::new();
        }
        let shared = self
            .package_rows()
            .map(<[_]>::to_vec)
            .and_then(|rows| Ok((rows, self.running_uids()?)));
        let (rows, running_uids) = match shared {
            Ok(shared) => shared,
            Err(error) => {
                return packages
                    .iter()
                    .cloned()
                    .map(|package| (package, Err(error.clone())))
                    .collect();
            }
        };
        packages
            .iter()
            .cloned()
            .map(|package| {
                let inspection = package_uid_from_rows(&rows, &package).and_then(|uid| {
                    let apk_path = self.apk_path(&package)?;
                    Ok(PackageInspection {
                        identity: self.package_identity(&package, uid, &apk_path)?,
                        was_running: running_uids.contains(&uid),
                    })
                });
                (package, inspection)
            })
            .collect()
    }

    fn force_stop(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        if let Some(gate) = self.launch_gate.as_ref() {
            gate.block_new_processes(package.as_str())
                .map_err(|error| AdapterError::new(error.to_string()))?;
        }
        if let Some(policy) = force_stop_policy(package) {
            let uid = self.package_uid(package)?;
            return self.force_stop_with_policy(package, uid, policy);
        }
        self.request_force_stop(package)?;
        let uid = self.package_uid(package)?;
        self.confirm_package_processes_stopped(uid)
    }

    fn observe_view(&mut self, package: &PackageName) -> Result<ObservedView, AdapterError> {
        self.read_self_view(package)
    }

    fn apply_view(&mut self, package: &PackageName, slot: &SlotId) -> Result<(), AdapterError> {
        let (canonical_ce, canonical_de) = self.canonical_paths(package);
        self.ensure_unmounted(&canonical_ce)?;
        self.ensure_unmounted(&canonical_de)?;
        if !slot.is_base() {
            let (slot_ce, slot_de) = self.slot_paths(package, slot);
            if !slot_ce.is_dir() || !slot_de.is_dir() {
                return Err(AdapterError::new("slot CE/DE pair is incomplete"));
            }
            let mount_ce = self.required(
                "/system/bin/mount",
                &["--bind", path_text(&slot_ce)?, path_text(&canonical_ce)?],
            );
            if let Err(error) = mount_ce {
                self.cleanup_mounts(&canonical_ce, &canonical_de);
                return Err(error);
            }
            let mount_de = self.required(
                "/system/bin/mount",
                &["--bind", path_text(&slot_de)?, path_text(&canonical_de)?],
            );
            if let Err(error) = mount_de {
                self.cleanup_mounts(&canonical_ce, &canonical_de);
                return Err(error);
            }
        }
        let observed = self.read_self_view(package)?;
        if observed.matches(slot) {
            Ok(())
        } else {
            self.cleanup_mounts(&canonical_ce, &canonical_de);
            Err(AdapterError::new(
                "Runtime mountinfo did not show the requested CE/DE view",
            ))
        }
    }

    fn apply_maintenance_view(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
    ) -> Result<(), AdapterError> {
        let (canonical_ce, canonical_de) = self.canonical_paths(package);
        let (maintenance_ce, maintenance_de) = self.maintenance_paths(package, token)?;
        if !maintenance_ce.is_dir() || !maintenance_de.is_dir() {
            return Err(AdapterError::new("maintenance CE/DE pair is incomplete"));
        }
        self.ensure_unmounted(&canonical_ce)?;
        self.ensure_unmounted(&canonical_de)?;
        let mount_ce = self.required(
            "/system/bin/mount",
            &[
                "--bind",
                path_text(&maintenance_ce)?,
                path_text(&canonical_ce)?,
            ],
        );
        if let Err(error) = mount_ce {
            self.cleanup_mounts(&canonical_ce, &canonical_de);
            return Err(error);
        }
        let mount_de = self.required(
            "/system/bin/mount",
            &[
                "--bind",
                path_text(&maintenance_de)?,
                path_text(&canonical_de)?,
            ],
        );
        if let Err(error) = mount_de {
            self.cleanup_mounts(&canonical_ce, &canonical_de);
            return Err(error);
        }
        let observed = self.read_self_view(package)?;
        if observed == ObservedView::Maintenance(token.clone()) {
            Ok(())
        } else {
            self.cleanup_mounts(&canonical_ce, &canonical_de);
            Err(AdapterError::new(
                "Runtime mountinfo did not show the maintenance CE/DE view",
            ))
        }
    }

    fn read_enabled_state(
        &mut self,
        package: &PackageName,
    ) -> Result<PackageEnabledState, AdapterError> {
        self.package_enabled_state(package)
    }

    fn block_launch(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        self.required(
            "/system/bin/pm",
            &["disable-user", "--user", "0", package.as_str()],
        )?;
        self.force_stop(package)?;
        Ok(())
    }

    fn restore_enabled_state(
        &mut self,
        package: &PackageName,
        state: PackageEnabledState,
    ) -> Result<(), AdapterError> {
        let action = match state {
            PackageEnabledState::Default => "default-state",
            PackageEnabledState::Enabled => "enable",
            PackageEnabledState::Disabled => "disable",
            PackageEnabledState::DisabledUser => "disable-user",
            PackageEnabledState::DisabledUntilUsed => "disable-until-used",
        };
        self.required("/system/bin/pm", &[action, "--user", "0", package.as_str()])?;
        Ok(())
    }

    fn launch_verified(
        &mut self,
        package: &PackageName,
        expected: &SlotId,
    ) -> Result<(), AdapterError> {
        let component = self.launcher_component(package)?;
        if let Some(gate) = self.launch_gate.as_ref() {
            gate.permit_explicit(package.as_str(), expected.as_str())
                .map_err(|error| AdapterError::new(error.to_string()))?;
        }
        if let Err(error) = self.required(
            "/system/bin/am",
            &["start", "-W", "--user", "0", "-n", &component],
        ) {
            return Err(self.contain_launch_failure(package, error));
        }
        let verification = (|| {
            let uid = self.package_uid(package)?;
            let processes = self.package_processes(uid)?;
            if processes.is_empty() {
                return Err(AdapterError::new("launched package has no App process"));
            }
            let mut verified = 0_usize;
            for process in processes {
                let identity = match ProcessIdentity::read_from(&process) {
                    Ok(identity) => identity,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(AdapterError::new(error.to_string())),
                };
                // A UID-changing PID reuse cannot opt itself into the isolated path.
                // Only Root-tracked descendants belong to this package's process group.
                let isolated = identity.uid != uid;
                if isolated
                    && !tracked_processes(&self.runtime_root, uid)
                        .map_err(|error| AdapterError::new(error.to_string()))?
                        .contains(&process)
                {
                    return Err(AdapterError::new(
                        "App process identity changed before verify",
                    ));
                }
                let pid = identity.pid.to_string();
                let owner = identity.uid.to_string();
                let started = identity.started.to_string();
                let mut arguments = vec![
                    pid.as_str(),
                    owner.as_str(),
                    started.as_str(),
                    package.as_str(),
                    expected.as_str(),
                    "--install",
                    "--verify-inherited",
                ];
                if isolated {
                    arguments.push("--isolated");
                }
                // Resolve canonical/legacy/mirror aliases inside the App's actual root.
                // A mountinfo string at /data/user/0 misses Android's /data/data symlink.
                self.required(PROCESS_VIEW_WORKER, &arguments)
                    .map_err(|error| {
                        AdapterError::new(format!(
                            "App PID {pid} did not observe the requested CE/DE view: {error}"
                        ))
                    })?;
                match ProcessIdentity::read_from(&process) {
                    Ok(after) if after == identity => verified += 1,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    _ => {
                        return Err(AdapterError::new(
                            "App process identity changed during verify",
                        ));
                    }
                }
            }
            (verified != 0)
                .then_some(())
                .ok_or_else(|| AdapterError::new("launched package processes exited before verify"))
        })();
        match verification {
            Ok(()) => Ok(()),
            Err(error) => Err(self.contain_launch_failure(package, error)),
        }
    }
}

fn parse_package_uid_row(line: &str) -> Option<(&str, u32)> {
    let line = line.trim().strip_prefix("package:")?;
    let (package, uid) = line.rsplit_once(" uid:")?;
    Some((package, uid.parse().ok()?))
}

fn package_uid_from_rows(
    rows: &[(String, u32)],
    package: &PackageName,
) -> Result<u32, AdapterError> {
    let uid = rows
        .iter()
        .find_map(|(name, uid)| (name == package.as_str()).then_some(*uid))
        .ok_or_else(|| AdapterError::new("package is not an ordinary user0 third-party app"))?;
    if rows
        .iter()
        .any(|(name, owner_uid)| name != package.as_str() && *owner_uid == uid)
    {
        return Err(AdapterError::new("package UID is shared by another app"));
    }
    Ok(uid)
}

/// Reads the real UID of one `/proc/<pid>` entry; `None` once the process has exited.
fn process_uid(process: &Path) -> Result<Option<u32>, AdapterError> {
    match fs::read_to_string(process.join("status")) {
        Ok(status) => Ok(status_uid(&status)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AdapterError::new(error.to_string())),
    }
}

fn surviving_processes(known: &[PathBuf], uid: u32) -> Result<Vec<PathBuf>, AdapterError> {
    let mut surviving = Vec::with_capacity(known.len());
    for process in known {
        if process_uid(process)? == Some(uid) {
            surviving.push(process.clone());
        }
    }
    Ok(surviving)
}

fn status_uid(content: &str) -> Option<u32> {
    content.lines().find_map(|line| {
        let values = line.strip_prefix("Uid:")?;
        values.split_whitespace().next()?.parse().ok()
    })
}

fn parse_user0_enabled_state(content: &str) -> Option<PackageEnabledState> {
    let line = content
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("User 0:") && line.contains("enabled="))?;
    let value = line
        .split_whitespace()
        .find_map(|field| field.strip_prefix("enabled="))?
        .trim_end_matches(',');
    match value {
        "0" => Some(PackageEnabledState::Default),
        "1" => Some(PackageEnabledState::Enabled),
        "2" => Some(PackageEnabledState::Disabled),
        "3" => Some(PackageEnabledState::DisabledUser),
        "4" => Some(PackageEnabledState::DisabledUntilUsed),
        _ => None,
    }
}

fn parse_launcher_component(content: &str, package: &PackageName) -> Option<String> {
    content.lines().rev().find_map(|line| {
        let line = line.trim();
        let (owner, activity) = line.split_once('/')?;
        (owner == package.as_str() && !activity.is_empty()).then(|| line.to_owned())
    })
}

fn runtime_view_from_mountinfo(
    content: &str,
    package: &PackageName,
    canonical_ce_root: &Path,
    canonical_de_root: &Path,
) -> ObservedView {
    let ce = runtime_domain_view(
        &mount_roots(content, &canonical_ce_root.join(package.as_str())),
        package,
    );
    let de = runtime_domain_view(
        &mount_roots(content, &canonical_de_root.join(package.as_str())),
        package,
    );
    match (ce, de) {
        (Some(RuntimeDomainView::Base), Some(RuntimeDomainView::Base)) => ObservedView::Base,
        (Some(RuntimeDomainView::Slot(ce)), Some(RuntimeDomainView::Slot(de))) if ce == de => {
            ObservedView::Slot(ce)
        }
        (Some(RuntimeDomainView::Maintenance(ce)), Some(RuntimeDomainView::Maintenance(de)))
            if ce == de =>
        {
            ObservedView::Maintenance(ce)
        }
        _ => ObservedView::Inconsistent,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeDomainView {
    Base,
    Slot(SlotId),
    Maintenance(AccountIoToken),
}

fn runtime_domain_view(roots: &[String], package: &PackageName) -> Option<RuntimeDomainView> {
    match roots {
        [] => Some(RuntimeDomainView::Base),
        [root] => slot_from_mount_root(root, package)
            .map(RuntimeDomainView::Slot)
            .or_else(|| {
                maintenance_from_mount_root(root, package).map(RuntimeDomainView::Maintenance)
            }),
        _ => None,
    }
}

fn mount_roots(content: &str, mount_point: &Path) -> Vec<String> {
    let Some(mount_point) = mount_point.to_str() else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            (fields.get(4).copied() == Some(mount_point))
                .then(|| fields.get(3).map(|value| (*value).to_owned()))
                .flatten()
        })
        .collect()
}

fn slot_from_mount_root(root: &str, package: &PackageName) -> Option<SlotId> {
    let marker = format!("{}{}/", layout::SLOTS_MOUNT_MARKER, package.as_str());
    let slot = root.split_once(&marker)?.1;
    (!slot.contains('/'))
        .then(|| SlotId::new(slot.to_owned()).ok())
        .flatten()
}

fn maintenance_from_mount_root(root: &str, package: &PackageName) -> Option<AccountIoToken> {
    let tail = root.split_once(layout::MAINTENANCE_MOUNT_MARKER)?.1;
    let (token, owner) = tail.split_once('/')?;
    (owner == package.as_str() && !token.contains('/'))
        .then(|| AccountIoToken::new(token.to_owned()).ok())
        .flatten()
}

fn path_text(path: &Path) -> Result<&str, AdapterError> {
    path.to_str()
        .ok_or_else(|| AdapterError::new("path is not UTF-8"))
}

#[cfg(test)]
mod tests;
