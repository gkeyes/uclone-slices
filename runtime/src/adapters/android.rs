use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::model::{
    AccountIoToken, Capabilities, ObservedView, PackageEnabledState, PackageIdentity,
    PackageInspection, PackageName, SlotId,
};
use crate::ports::{AdapterError, AndroidOps};

const PROCESS_EXIT_CONFIRMATION_WINDOW: Duration = Duration::from_secs(10);
const PROCESS_EXIT_POLL_INTERVAL: Duration = Duration::from_millis(10);
const WECHAT_PACKAGE: &str = "com.tencent.mm";
const WECHAT_FAST_RESTART_FORCE_STOP_LIMIT: usize = 5;
const WECHAT_FAST_RESTART_CONFIRMATION_WINDOW: Duration = Duration::from_secs(2);
const WECHAT_FAST_RESTART_QUIET_WINDOW: Duration = Duration::from_millis(500);

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
        }
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
        let ce_root = self
            .ce_slots_root
            .parent()
            .ok_or_else(|| AdapterError::new("CE slots root has no storage parent"))?;
        let de_root = self
            .de_slots_root
            .parent()
            .ok_or_else(|| AdapterError::new("DE slots root has no storage parent"))?;
        Ok((
            ce_root
                .join("maintenance")
                .join(token.as_str())
                .join(package.as_str()),
            de_root
                .join("maintenance")
                .join(token.as_str())
                .join(package.as_str()),
        ))
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

    fn package_uid(&mut self, package: &PackageName) -> Result<u32, AdapterError> {
        let output = self.required(
            "/system/bin/cmd",
            &["package", "list", "packages", "-3", "-U", "--user", "0"],
        )?;
        let rows: Vec<_> = output
            .stdout
            .lines()
            .filter_map(parse_package_uid_row)
            .collect();
        let uid = rows
            .iter()
            .find_map(|(name, uid)| (*name == package.as_str()).then_some(*uid))
            .ok_or_else(|| AdapterError::new("package is not an ordinary user0 third-party app"))?;
        if rows
            .iter()
            .any(|(name, owner_uid)| *name != package.as_str() && *owner_uid == uid)
        {
            return Err(AdapterError::new("package UID is shared by another app"));
        }
        Ok(uid)
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
        parse_launcher_component(&output.stdout, package)
            .ok_or_else(|| AdapterError::new("package has no Launcher activity"))
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
            let status = match fs::read_to_string(entry.path().join("status")) {
                Ok(status) => status,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(AdapterError::new(error.to_string())),
            };
            if status_uid(&status) == Some(uid) {
                processes.push(entry.path());
            }
        }
        processes.sort();
        Ok(processes)
    }

    fn confirm_package_processes_stopped(&self, uid: u32) -> Result<(), AdapterError> {
        confirm_processes_stably_stopped(
            uid,
            PROCESS_EXIT_CONFIRMATION_WINDOW,
            PROCESS_EXIT_POLL_INTERVAL,
            || self.package_processes(uid),
        )
    }

    fn request_force_stop(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        self.required(
            "/system/bin/am",
            &["force-stop", "--user", "0", package.as_str()],
        )?;
        Ok(())
    }

    fn force_stop_wechat(&mut self, package: &PackageName, uid: u32) -> Result<(), AdapterError> {
        let deadline = Instant::now() + WECHAT_FAST_RESTART_CONFIRMATION_WINDOW;
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
                    if now.duration_since(quiet_start) >= WECHAT_FAST_RESTART_QUIET_WINDOW {
                        return Ok(());
                    }
                } else {
                    empty_since = None;
                    let has_new_generation = remaining
                        .iter()
                        .any(|process| !previous_generation.contains(process));
                    if has_new_generation
                        && force_stop_attempts < WECHAT_FAST_RESTART_FORCE_STOP_LIMIT
                    {
                        previous_generation = remaining;
                        break;
                    }
                }

                if now >= deadline {
                    return Err(AdapterError::new(format!(
                        "WeChat FastRestart did not quiesce within {} ms after {force_stop_attempts} force-stop attempt(s); {} process(es) remain for UID {uid}",
                        WECHAT_FAST_RESTART_CONFIRMATION_WINDOW.as_millis(),
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

fn confirm_processes_stably_stopped<F>(
    uid: u32,
    confirmation_window: Duration,
    poll_interval: Duration,
    mut processes: F,
) -> Result<(), AdapterError>
where
    F: FnMut() -> Result<Vec<PathBuf>, AdapterError>,
{
    let deadline = Instant::now() + confirmation_window;
    let mut previous_scan_was_empty = false;
    loop {
        let remaining = processes()?;
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
        self.required(
            "/system/bin/cmd",
            &["package", "list", "packages", "-3", "-U", "--user", "0"],
        )?;
        Ok(Capabilities {
            build_id: self.build_id.clone(),
        })
    }

    fn inspect(&mut self, package: &PackageName) -> Result<PackageInspection, AdapterError> {
        let uid = self.package_uid(package)?;
        let apk_path = self.apk_path(package)?;
        let _launcher = self.launcher_component(package)?;
        let (canonical_ce, canonical_de) = self.canonical_paths(package);
        if !canonical_ce.is_dir() || !canonical_de.is_dir() {
            return Err(AdapterError::new(
                "package CE/DE directories are not both available",
            ));
        }
        let metadata =
            fs::metadata(&apk_path).map_err(|error| AdapterError::new(error.to_string()))?;
        let identity = PackageIdentity::new(
            uid,
            path_text(&apk_path)?.to_owned(),
            metadata.dev(),
            metadata.ino(),
        )
        .map_err(|error| AdapterError::new(error.to_string()))?;
        Ok(PackageInspection {
            identity,
            was_running: !self.package_processes(uid)?.is_empty(),
        })
    }

    fn force_stop(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        if package.as_str() == WECHAT_PACKAGE {
            let uid = self.package_uid(package)?;
            return self.force_stop_wechat(package, uid);
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
                let mountinfo = match fs::read_to_string(process.join("mountinfo")) {
                    Ok(mountinfo) => mountinfo,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(AdapterError::new(error.to_string())),
                };
                verified += 1;
                let observed = app_view_from_mountinfo(
                    &mountinfo,
                    package,
                    &self.canonical_ce_root,
                    &self.canonical_de_root,
                );
                if !observed.matches(expected) {
                    let pid = process
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("unknown");
                    return Err(AdapterError::new(format!(
                        "App PID {pid} did not observe the requested CE/DE view"
                    )));
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

fn app_view_from_mountinfo(
    content: &str,
    package: &PackageName,
    canonical_ce_root: &Path,
    canonical_de_root: &Path,
) -> ObservedView {
    let ce = app_domain_view(
        &mount_roots(content, &canonical_ce_root.join(package.as_str())),
        package,
    );
    let de = app_domain_view(
        &mount_roots(content, &canonical_de_root.join(package.as_str())),
        package,
    );
    paired_view(ce, de)
}

fn paired_view(ce: Option<Option<SlotId>>, de: Option<Option<SlotId>>) -> ObservedView {
    match (ce, de) {
        (Some(None), Some(None)) => ObservedView::Base,
        (Some(Some(ce)), Some(Some(de))) if ce == de => ObservedView::Slot(ce),
        (Some(None), Some(Some(_)))
        | (Some(Some(_)), Some(None))
        | (Some(Some(_)), Some(Some(_)))
        | (None, _)
        | (_, None) => ObservedView::Inconsistent,
    }
}

fn app_domain_view(roots: &[String], package: &PackageName) -> Option<Option<SlotId>> {
    if roots
        .iter()
        .any(|root| root.contains("/uclone-slices-v2/maintenance/"))
    {
        return None;
    }
    let v2_roots: Vec<_> = roots
        .iter()
        .filter(|root| root.contains("/uclone-slices-v2/slots/"))
        .collect();
    if v2_roots.is_empty() {
        return Some(None);
    }
    let first = slot_from_mount_root(v2_roots[0], package)?;
    v2_roots
        .iter()
        .all(|root| slot_from_mount_root(root, package).as_ref() == Some(&first))
        .then_some(Some(first))
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
    let marker = format!("/uclone-slices-v2/slots/{}/", package.as_str());
    let slot = root.split_once(&marker)?.1;
    (!slot.contains('/'))
        .then(|| SlotId::new(slot.to_owned()).ok())
        .flatten()
}

fn maintenance_from_mount_root(root: &str, package: &PackageName) -> Option<AccountIoToken> {
    let marker = "/uclone-slices-v2/maintenance/";
    let tail = root.split_once(marker)?.1;
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
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    type RecordedCommands = Rc<RefCell<Vec<(String, Vec<String>)>>>;

    #[derive(Debug)]
    struct RecordingRunner {
        outputs: VecDeque<CommandOutput>,
        commands: RecordedCommands,
    }

    #[derive(Debug)]
    struct ContainmentRunner {
        outputs: VecDeque<CommandOutput>,
        commands: RecordedCommands,
        proc_root: PathBuf,
    }

    #[derive(Debug)]
    struct FastRestartRunner {
        commands: RecordedCommands,
        proc_root: PathBuf,
        package: PackageName,
        uid: u32,
        respawn_attempts: usize,
        force_stop_attempts: usize,
    }

    impl FastRestartRunner {
        fn replace_process_generation(&self, attempt: usize) -> Result<(), AdapterError> {
            for entry in fs::read_dir(&self.proc_root)
                .map_err(|error| AdapterError::new(error.to_string()))?
            {
                let entry = entry.map_err(|error| AdapterError::new(error.to_string()))?;
                fs::remove_dir_all(entry.path())
                    .map_err(|error| AdapterError::new(error.to_string()))?;
            }
            if attempt <= self.respawn_attempts {
                let process = self.proc_root.join((10_000 + attempt).to_string());
                fs::create_dir(&process).map_err(|error| AdapterError::new(error.to_string()))?;
                fs::write(
                    process.join("status"),
                    format!("Name:\tapp\nUid:\t{0}\t{0}\t{0}\t{0}\n", self.uid),
                )
                .map_err(|error| AdapterError::new(error.to_string()))?;
            }
            Ok(())
        }
    }

    impl CommandRunner for FastRestartRunner {
        fn run(
            &mut self,
            program: &str,
            arguments: &[&str],
        ) -> Result<CommandOutput, AdapterError> {
            self.commands.borrow_mut().push((
                program.to_owned(),
                arguments.iter().map(|value| (*value).to_owned()).collect(),
            ));
            if program == "/system/bin/am" && arguments.first().copied() == Some("force-stop") {
                self.force_stop_attempts += 1;
                self.replace_process_generation(self.force_stop_attempts)?;
                return Ok(output(""));
            }
            if program == "/system/bin/cmd" {
                return Ok(output(format!(
                    "package:{} uid:{}\n",
                    self.package, self.uid
                )));
            }
            Ok(output(""))
        }
    }

    impl CommandRunner for ContainmentRunner {
        fn run(
            &mut self,
            program: &str,
            arguments: &[&str],
        ) -> Result<CommandOutput, AdapterError> {
            self.commands.borrow_mut().push((
                program.to_owned(),
                arguments.iter().map(|value| (*value).to_owned()).collect(),
            ));
            if program == "/system/bin/am" && arguments.first().copied() == Some("force-stop") {
                fs::remove_dir_all(&self.proc_root)
                    .map_err(|error| AdapterError::new(error.to_string()))?;
                fs::create_dir(&self.proc_root)
                    .map_err(|error| AdapterError::new(error.to_string()))?;
            }
            Ok(self.outputs.pop_front().unwrap_or_else(|| output("")))
        }
    }

    impl CommandRunner for RecordingRunner {
        fn run(
            &mut self,
            program: &str,
            arguments: &[&str],
        ) -> Result<CommandOutput, AdapterError> {
            self.commands.borrow_mut().push((
                program.to_owned(),
                arguments.iter().map(|value| (*value).to_owned()).collect(),
            ));
            Ok(self.outputs.pop_front().unwrap_or(CommandOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            }))
        }
    }

    fn output(stdout: impl Into<String>) -> CommandOutput {
        CommandOutput {
            success: true,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    #[derive(Debug)]
    struct MountRunner {
        commands: RecordedCommands,
        mountinfo: PathBuf,
        fail_mount_containing: Option<String>,
        normal_unmount_error: Option<String>,
        ignore_unmount: bool,
    }

    impl CommandRunner for MountRunner {
        fn run(
            &mut self,
            program: &str,
            arguments: &[&str],
        ) -> Result<CommandOutput, AdapterError> {
            self.commands.borrow_mut().push((
                program.to_owned(),
                arguments.iter().map(|value| (*value).to_owned()).collect(),
            ));
            if program == "/system/bin/mount" {
                let source = arguments.get(1).copied().unwrap_or_default();
                let target = arguments.get(2).copied().unwrap_or_default();
                if self
                    .fail_mount_containing
                    .as_deref()
                    .is_some_and(|marker| source.contains(marker))
                {
                    return Ok(CommandOutput {
                        success: false,
                        stdout: String::new(),
                        stderr: "injected mount failure".to_owned(),
                    });
                }
                let mut content = fs::read_to_string(&self.mountinfo).unwrap_or_default();
                let id = content.lines().count() + 1;
                content.push_str(&format!("{id} 0 0:1 {source} {target} rw - ext4 none rw\n"));
                fs::write(&self.mountinfo, content)
                    .map_err(|error| AdapterError::new(error.to_string()))?;
            } else if program == "/system/bin/umount" {
                let lazy = arguments.first().copied() == Some("-l");
                if !lazy && let Some(error) = &self.normal_unmount_error {
                    let target = arguments.last().copied().unwrap_or_default();
                    return Ok(CommandOutput {
                        success: false,
                        stdout: String::new(),
                        stderr: error.replace("{mount_point}", target),
                    });
                }
                if self.ignore_unmount {
                    return Ok(output(""));
                }
                let target = arguments.last().copied().unwrap_or_default();
                let content = fs::read_to_string(&self.mountinfo).unwrap_or_default();
                let mut removed = false;
                let mut lines: Vec<_> = content.lines().collect();
                for index in (0..lines.len()).rev() {
                    let fields: Vec<_> = lines[index].split_whitespace().collect();
                    if fields.get(4).copied() == Some(target) {
                        lines.remove(index);
                        removed = true;
                        break;
                    }
                }
                if removed {
                    let rewritten = if lines.is_empty() {
                        String::new()
                    } else {
                        format!("{}\n", lines.join("\n"))
                    };
                    fs::write(&self.mountinfo, rewritten)
                        .map_err(|error| AdapterError::new(error.to_string()))?;
                }
            }
            Ok(output(""))
        }
    }

    fn system_with_runner(
        root: &Path,
        runner: RecordingRunner,
    ) -> (SystemAndroidOps, RecordedCommands) {
        let commands = Rc::clone(&runner.commands);
        (
            SystemAndroidOps {
                build_id: "test".to_owned(),
                ce_slots_root: root.join("slots-ce"),
                de_slots_root: root.join("slots-de"),
                canonical_ce_root: root.join("canonical-ce"),
                canonical_de_root: root.join("canonical-de"),
                mountinfo: root.join("mountinfo"),
                proc_root: root.join("proc"),
                runner: Box::new(runner),
            },
            commands,
        )
    }

    fn system_with_fast_restart_runner(
        root: &Path,
        package: PackageName,
        uid: u32,
        respawn_attempts: usize,
    ) -> (SystemAndroidOps, RecordedCommands) {
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = FastRestartRunner {
            commands: Rc::clone(&commands),
            proc_root: root.join("proc"),
            package,
            uid,
            respawn_attempts,
            force_stop_attempts: 0,
        };
        (
            SystemAndroidOps {
                build_id: "test".to_owned(),
                ce_slots_root: root.join("slots-ce"),
                de_slots_root: root.join("slots-de"),
                canonical_ce_root: root.join("canonical-ce"),
                canonical_de_root: root.join("canonical-de"),
                mountinfo: root.join("mountinfo"),
                proc_root: root.join("proc"),
                runner: Box::new(runner),
            },
            commands,
        )
    }

    #[test]
    fn inspect_uses_user0_third_party_launcher_identity_commands() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let apk = root.path().join("base.apk");
        fs::write(&apk, b"apk").unwrap();
        fs::create_dir_all(root.path().join("canonical-ce").join(package.as_str())).unwrap();
        fs::create_dir_all(root.path().join("canonical-de").join(package.as_str())).unwrap();
        fs::create_dir_all(root.path().join("proc")).unwrap();
        fs::write(root.path().join("mountinfo"), b"").unwrap();
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = RecordingRunner {
            outputs: VecDeque::from([
                output(format!("package:{package} uid:10123\n")),
                output(format!("package:{}\n", apk.display())),
                output(format!("{package}/.MainActivity\n")),
            ]),
            commands: Rc::clone(&commands),
        };
        let (mut android, recorded) = system_with_runner(root.path(), runner);

        let inspection = android.inspect(&package).unwrap();

        assert!(!inspection.was_running);
        assert_eq!(
            *recorded.borrow(),
            vec![
                (
                    "/system/bin/cmd".to_owned(),
                    vec!["package", "list", "packages", "-3", "-U", "--user", "0",]
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                ),
                (
                    "/system/bin/cmd".to_owned(),
                    vec!["package", "path", "--user", "0", package.as_str()]
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                ),
                (
                    "/system/bin/cmd".to_owned(),
                    vec![
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
                    ]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                ),
            ]
        );
    }

    #[test]
    fn inspect_rejects_a_uid_shared_by_another_package() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let apk = root.path().join("base.apk");
        fs::write(&apk, b"apk").unwrap();
        fs::create_dir_all(root.path().join("canonical-ce").join(package.as_str())).unwrap();
        fs::create_dir_all(root.path().join("canonical-de").join(package.as_str())).unwrap();
        fs::write(root.path().join("mountinfo"), b"").unwrap();
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = RecordingRunner {
            outputs: VecDeque::from([
                output(format!(
                    "package:{package} uid:10123\npackage:com.example.shared uid:10123\n"
                )),
                output(format!("package:{}\n", apk.display())),
                output(format!("{package}/.MainActivity\n")),
                CommandOutput {
                    success: false,
                    stdout: String::new(),
                    stderr: String::new(),
                },
            ]),
            commands,
        };
        let (mut android, _recorded) = system_with_runner(root.path(), runner);

        let result = android.inspect(&package);

        assert!(result.is_err());
    }

    #[test]
    fn force_stop_does_not_change_enabled_state() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        fs::create_dir(root.path().join("proc")).unwrap();
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = RecordingRunner {
            outputs: VecDeque::from([output(""), output(format!("package:{package} uid:10123\n"))]),
            commands: Rc::clone(&commands),
        };
        let (mut android, recorded) = system_with_runner(root.path(), runner);

        android.force_stop(&package).unwrap();

        assert_eq!(
            *recorded.borrow(),
            vec![
                (
                    "/system/bin/am".to_owned(),
                    vec![
                        "force-stop".to_owned(),
                        "--user".to_owned(),
                        "0".to_owned(),
                        package.as_str().to_owned(),
                    ],
                ),
                (
                    "/system/bin/cmd".to_owned(),
                    vec!["package", "list", "packages", "-3", "-U", "--user", "0",]
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                ),
            ]
        );
    }

    #[test]
    fn force_stop_rejects_a_lingering_target_uid_process() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let process = root.path().join("proc/123");
        fs::create_dir_all(&process).unwrap();
        fs::write(
            process.join("status"),
            "Name:\tapp\nUid:\t10123\t10123\t10123\t10123\n",
        )
        .unwrap();
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = RecordingRunner {
            outputs: VecDeque::from([output(""), output(format!("package:{package} uid:10123\n"))]),
            commands,
        };
        let (mut android, _recorded) = system_with_runner(root.path(), runner);

        let error = android.force_stop(&package).unwrap_err();

        assert!(error.to_string().contains("still has 1 process"));
    }

    #[test]
    fn force_stop_waits_for_kernel_process_exit_before_succeeding() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let proc_root = root.path().join("proc");
        let process = proc_root.join("123");
        fs::create_dir_all(&process).unwrap();
        fs::write(
            process.join("status"),
            "Name:\tapp\nUid:\t10123\t10123\t10123\t10123\n",
        )
        .unwrap();
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = RecordingRunner {
            outputs: VecDeque::from([output(""), output(format!("package:{package} uid:10123\n"))]),
            commands,
        };
        let (mut android, _recorded) = system_with_runner(root.path(), runner);
        let process_exit = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            fs::remove_dir_all(&process).unwrap();
        });

        android.force_stop(&package).unwrap();

        process_exit.join().unwrap();
    }

    #[test]
    fn force_stop_waits_for_a_slow_multi_process_handoff() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let proc_root = root.path().join("proc");
        let main_process = proc_root.join("123");
        let push_process = proc_root.join("456");
        fs::create_dir_all(&main_process).unwrap();
        fs::write(
            main_process.join("status"),
            "Name:\tapp_main\nUid:\t10123\t10123\t10123\t10123\n",
        )
        .unwrap();
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = RecordingRunner {
            outputs: VecDeque::from([output(""), output(format!("package:{package} uid:10123\n"))]),
            commands,
        };
        let (mut android, _recorded) = system_with_runner(root.path(), runner);
        let remover = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            fs::create_dir_all(&push_process).unwrap();
            fs::write(
                push_process.join("status"),
                "Name:\tapp_push\nUid:\t10123\t10123\t10123\t10123\n",
            )
            .unwrap();
            fs::remove_dir_all(main_process).unwrap();
            std::thread::sleep(Duration::from_millis(800));
            fs::remove_dir_all(push_process).unwrap();
        });

        let result = android.force_stop(&package);
        remover.join().unwrap();

        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn wechat_force_stop_consumes_bounded_fast_restart_generations() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.tencent.mm").unwrap();
        let proc_root = root.path().join("proc");
        let initial_process = proc_root.join("123");
        fs::create_dir_all(&initial_process).unwrap();
        fs::write(
            initial_process.join("status"),
            "Name:\tapp\nUid:\t10123\t10123\t10123\t10123\n",
        )
        .unwrap();
        let (android, commands) =
            system_with_fast_restart_runner(root.path(), package.clone(), 10_123, 4);
        let fallback_cleanup = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            for entry in fs::read_dir(proc_root).unwrap() {
                fs::remove_dir_all(entry.unwrap().path()).unwrap();
            }
        });
        let mut android = android;

        let result = android.force_stop(&package);
        fallback_cleanup.join().unwrap();

        assert!(result.is_ok(), "{result:?}");
        let force_stop_count = commands
            .borrow()
            .iter()
            .filter(|(program, arguments)| {
                program == "/system/bin/am"
                    && arguments.first().map(String::as_str) == Some("force-stop")
            })
            .count();
        assert_eq!(force_stop_count, 5);
    }

    #[test]
    fn wechat_force_stop_fails_closed_after_the_platform_restart_limit() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.tencent.mm").unwrap();
        let initial_process = root.path().join("proc/123");
        fs::create_dir_all(&initial_process).unwrap();
        fs::write(
            initial_process.join("status"),
            "Name:\tapp\nUid:\t10123\t10123\t10123\t10123\n",
        )
        .unwrap();
        let (mut android, commands) =
            system_with_fast_restart_runner(root.path(), package.clone(), 10_123, 5);

        let error = android.force_stop(&package).unwrap_err();

        assert!(error.to_string().contains("WeChat FastRestart"));
        let force_stop_count = commands
            .borrow()
            .iter()
            .filter(|(program, arguments)| {
                program == "/system/bin/am"
                    && arguments.first().map(String::as_str) == Some("force-stop")
            })
            .count();
        assert_eq!(force_stop_count, 5);
    }

    #[test]
    fn wechat_force_stop_does_not_retry_a_lingering_old_pid() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.tencent.mm").unwrap();
        let process = root.path().join("proc/123");
        fs::create_dir_all(&process).unwrap();
        fs::write(
            process.join("status"),
            "Name:\tapp\nUid:\t10123\t10123\t10123\t10123\n",
        )
        .unwrap();
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = RecordingRunner {
            outputs: VecDeque::from([output(format!("package:{package} uid:10123\n")), output("")]),
            commands: Rc::clone(&commands),
        };
        let (mut android, _recorded) = system_with_runner(root.path(), runner);
        let process_exit = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            fs::remove_dir_all(process).unwrap();
        });

        android.force_stop(&package).unwrap();
        process_exit.join().unwrap();

        let force_stop_count = commands
            .borrow()
            .iter()
            .filter(|(program, arguments)| {
                program == "/system/bin/am"
                    && arguments.first().map(String::as_str) == Some("force-stop")
            })
            .count();
        assert_eq!(force_stop_count, 1);
    }

    #[test]
    fn process_stop_confirmation_rejects_a_single_empty_scan_between_processes() {
        let mut scans = VecDeque::from([
            Vec::<PathBuf>::new(),
            vec![PathBuf::from("/proc/456")],
            Vec::<PathBuf>::new(),
            Vec::<PathBuf>::new(),
        ]);
        let mut scan_count = 0_usize;

        confirm_processes_stably_stopped(10_123, Duration::from_secs(1), Duration::ZERO, || {
            scan_count += 1;
            Ok(scans.pop_front().unwrap_or_default())
        })
        .unwrap();

        assert_eq!(scan_count, 4);
    }

    #[test]
    fn mountinfo_requires_exactly_one_paired_v2_slot() {
        let package = PackageName::new("com.example.app").unwrap();
        let ce_root = Path::new("/data/user/0");
        let de_root = Path::new("/data/user_de/0");
        let paired = "1 0 0:1 /misc_ce/0/uclone-slices-v2/slots/com.example.app/slot-2 /data/user/0/com.example.app rw - ext4 /dev/block/data rw\n2 0 0:1 /misc_de/0/uclone-slices-v2/slots/com.example.app/slot-2 /data/user_de/0/com.example.app rw - ext4 /dev/block/data rw\n";
        let stacked = format!(
            "{paired}3 0 0:1 /misc_ce/0/uclone-slices-v2/slots/com.example.app/slot-1 /data/user/0/com.example.app rw - ext4 /dev/block/data rw\n"
        );
        let foreign =
            "1 0 0:1 /foreign/slot /data/user/0/com.example.app rw - ext4 /dev/block/data rw\n";

        assert_eq!(
            runtime_view_from_mountinfo(paired, &package, ce_root, de_root),
            ObservedView::Slot(SlotId::numbered(2))
        );
        assert_eq!(
            runtime_view_from_mountinfo(&stacked, &package, ce_root, de_root),
            ObservedView::Inconsistent
        );
        assert_eq!(
            runtime_view_from_mountinfo(foreign, &package, ce_root, de_root),
            ObservedView::Inconsistent
        );
        assert_eq!(
            runtime_view_from_mountinfo("", &package, ce_root, de_root),
            ObservedView::Base
        );
    }

    #[test]
    fn app_mountinfo_accepts_android_base_mapping_and_equivalent_slot_duplicates() {
        let package = PackageName::new("com.example.app").unwrap();
        let ce_root = Path::new("/data/user/0");
        let de_root = Path::new("/data/user_de/0");
        let base = "1 0 0:1 /user_de/0/com.example.app /data/user_de/0/com.example.app rw - ext4 none rw\n";
        let slot = "1 0 0:1 /misc_ce/0/uclone-slices-v2/slots/com.example.app/slot-2 /data/user/0/com.example.app rw - ext4 none rw\n2 0 0:1 /misc_de/0/uclone-slices-v2/slots/com.example.app/slot-2 /data/user_de/0/com.example.app rw - ext4 none rw\n3 0 0:1 /misc_de/0/uclone-slices-v2/slots/com.example.app/slot-2 /data/user_de/0/com.example.app rw - ext4 none rw\n";

        assert_eq!(
            app_view_from_mountinfo(base, &package, ce_root, de_root),
            ObservedView::Base
        );
        assert_eq!(
            app_view_from_mountinfo(slot, &package, ce_root, de_root),
            ObservedView::Slot(SlotId::numbered(2))
        );
    }

    #[test]
    fn launch_uses_resolved_activity_and_checks_every_pid_mountinfo() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let slot = SlotId::numbered(3);
        for pid in ["123", "456"] {
            let directory = root.path().join("proc").join(pid);
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                directory.join("status"),
                "Name:\tapp\nUid:\t10123\t10123\t10123\t10123\n",
            )
            .unwrap();
            fs::write(
                directory.join("mountinfo"),
                format!(
                    "1 0 0:1 /uclone-slices-v2/slots/{package}/{slot} {}/{} rw - ext4 none rw\n2 0 0:1 /uclone-slices-v2/slots/{package}/{slot} {}/{} rw - ext4 none rw\n",
                    root.path().join("canonical-ce").display(),
                    package,
                    root.path().join("canonical-de").display(),
                    package,
                ),
            )
            .unwrap();
        }
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = RecordingRunner {
            outputs: VecDeque::from([
                output(format!("{package}/.MainActivity\n")),
                output("Status: ok\n"),
                output(format!("package:{package} uid:10123\n")),
            ]),
            commands: Rc::clone(&commands),
        };
        let (mut android, recorded) = system_with_runner(root.path(), runner);

        android.launch_verified(&package, &slot).unwrap();

        assert_eq!(
            recorded.borrow()[1],
            (
                "/system/bin/am".to_owned(),
                vec![
                    "start".to_owned(),
                    "-W".to_owned(),
                    "--user".to_owned(),
                    "0".to_owned(),
                    "-n".to_owned(),
                    format!("{package}/.MainActivity"),
                ],
            )
        );
    }

    #[test]
    fn launch_rejects_a_remote_process_with_the_wrong_view() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let slot = SlotId::numbered(3);
        for (pid, root_path) in [
            ("123", format!("/uclone-slices-v2/slots/{package}/{slot}")),
            ("456", "/wrong-view".to_owned()),
        ] {
            let directory = root.path().join("proc").join(pid);
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                directory.join("status"),
                "Name:\tapp\nUid:\t10123\t10123\t10123\t10123\n",
            )
            .unwrap();
            fs::write(
                directory.join("mountinfo"),
                format!(
                    "1 0 0:1 {root_path} {}/{} rw - ext4 none rw\n2 0 0:1 {root_path} {}/{} rw - ext4 none rw\n",
                    root.path().join("canonical-ce").display(),
                    package,
                    root.path().join("canonical-de").display(),
                    package,
                ),
            )
            .unwrap();
        }
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = RecordingRunner {
            outputs: VecDeque::from([
                output(format!("{package}/.MainActivity\n")),
                output("Status: ok\n"),
                output(format!("package:{package} uid:10123\n")),
                output(""),
                output(format!("package:{package} uid:10123\n")),
            ]),
            commands: Rc::clone(&commands),
        };
        let (mut android, recorded) = system_with_runner(root.path(), runner);

        let error = android.launch_verified(&package, &slot).unwrap_err();

        assert!(error.to_string().contains("PID 456"));
        assert!(error.to_string().contains("containment failed"));
        assert!(recorded.borrow().iter().any(|(program, arguments)| {
            program == "/system/bin/am"
                && arguments.first().map(String::as_str) == Some("force-stop")
        }));
    }

    #[test]
    fn launch_verification_failures_are_contained_and_leave_no_uid_processes() {
        for failure in ["wrong_view", "pid_disappeared", "mountinfo_unreadable"] {
            let root = tempfile::tempdir().unwrap();
            let package = PackageName::new("com.example.app").unwrap();
            let slot = SlotId::numbered(3);
            let process = root.path().join("proc/123");
            fs::create_dir_all(&process).unwrap();
            fs::write(
                process.join("status"),
                "Name:\tapp\nUid:\t10123\t10123\t10123\t10123\n",
            )
            .unwrap();
            match failure {
                "wrong_view" => fs::write(
                    process.join("mountinfo"),
                    format!(
                        "1 0 0:1 /wrong {}/{} rw - ext4 none rw\n2 0 0:1 /wrong {}/{} rw - ext4 none rw\n",
                        root.path().join("canonical-ce").display(),
                        package,
                        root.path().join("canonical-de").display(),
                        package,
                    ),
                )
                .unwrap(),
                "pid_disappeared" => {}
                "mountinfo_unreadable" => {
                    fs::create_dir(process.join("mountinfo")).unwrap();
                }
                _ => unreachable!(),
            }
            let commands = Rc::new(RefCell::new(Vec::new()));
            let runner = ContainmentRunner {
                outputs: VecDeque::from([
                    output(format!("{package}/.MainActivity\n")),
                    output("Status: ok\n"),
                    output(format!("package:{package} uid:10123\n")),
                    output(""),
                    output(format!("package:{package} uid:10123\n")),
                ]),
                commands: Rc::clone(&commands),
                proc_root: root.path().join("proc"),
            };
            let mut android = SystemAndroidOps {
                build_id: "test".to_owned(),
                ce_slots_root: root.path().join("slots-ce"),
                de_slots_root: root.path().join("slots-de"),
                canonical_ce_root: root.path().join("canonical-ce"),
                canonical_de_root: root.path().join("canonical-de"),
                mountinfo: root.path().join("mountinfo"),
                proc_root: root.path().join("proc"),
                runner: Box::new(runner),
            };

            let error = android.launch_verified(&package, &slot).unwrap_err();

            assert!(
                error.to_string().contains("force-stopped"),
                "failure={failure} error={error}"
            );
            assert!(
                fs::read_dir(root.path().join("proc"))
                    .unwrap()
                    .next()
                    .is_none()
            );
            assert!(commands.borrow().iter().any(|(program, arguments)| {
                program == "/system/bin/am"
                    && arguments.first().map(String::as_str) == Some("force-stop")
            }));
        }
    }

    #[test]
    fn apply_view_mounts_and_unmounts_the_ce_de_pair_in_order() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let slot = SlotId::numbered(1);
        let ce_slots = root.path().join("misc_ce/uclone-slices-v2/slots");
        let de_slots = root.path().join("misc_de/uclone-slices-v2/slots");
        let canonical_ce = root.path().join("data/user/0");
        let canonical_de = root.path().join("data/user_de/0");
        let slot_ce = ce_slots.join(package.as_str()).join(slot.as_str());
        let slot_de = de_slots.join(package.as_str()).join(slot.as_str());
        fs::create_dir_all(&slot_ce).unwrap();
        fs::create_dir_all(&slot_de).unwrap();
        fs::create_dir_all(canonical_ce.join(package.as_str())).unwrap();
        fs::create_dir_all(canonical_de.join(package.as_str())).unwrap();
        let mountinfo = root.path().join("mountinfo");
        fs::write(&mountinfo, "").unwrap();
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = MountRunner {
            commands: Rc::clone(&commands),
            mountinfo: mountinfo.clone(),
            fail_mount_containing: None,
            normal_unmount_error: None,
            ignore_unmount: false,
        };
        let mut android = SystemAndroidOps {
            build_id: "test".to_owned(),
            ce_slots_root: ce_slots,
            de_slots_root: de_slots,
            canonical_ce_root: canonical_ce.clone(),
            canonical_de_root: canonical_de.clone(),
            mountinfo,
            proc_root: root.path().join("proc"),
            runner: Box::new(runner),
        };

        android.apply_view(&package, &slot).unwrap();
        assert_eq!(
            android.observe_view(&package).unwrap(),
            ObservedView::Slot(slot.clone())
        );
        android.apply_view(&package, &SlotId::base()).unwrap();

        assert_eq!(android.observe_view(&package).unwrap(), ObservedView::Base);
        assert_eq!(
            *commands.borrow(),
            vec![
                (
                    "/system/bin/mount".to_owned(),
                    vec![
                        "--bind".to_owned(),
                        slot_ce.display().to_string(),
                        canonical_ce.join(package.as_str()).display().to_string(),
                    ],
                ),
                (
                    "/system/bin/mount".to_owned(),
                    vec![
                        "--bind".to_owned(),
                        slot_de.display().to_string(),
                        canonical_de.join(package.as_str()).display().to_string(),
                    ],
                ),
                (
                    "/system/bin/umount".to_owned(),
                    vec![canonical_ce.join(package.as_str()).display().to_string()],
                ),
                (
                    "/system/bin/umount".to_owned(),
                    vec![canonical_de.join(package.as_str()).display().to_string()],
                ),
            ]
        );
    }

    #[test]
    fn maintenance_view_mounts_the_private_empty_pair_and_is_observable() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
        let ce_slots = root.path().join("misc_ce/uclone-slices-v2/slots");
        let de_slots = root.path().join("misc_de/uclone-slices-v2/slots");
        let maintenance_ce = ce_slots
            .parent()
            .unwrap()
            .join("maintenance")
            .join(token.as_str())
            .join(package.as_str());
        let maintenance_de = de_slots
            .parent()
            .unwrap()
            .join("maintenance")
            .join(token.as_str())
            .join(package.as_str());
        let canonical_ce = root.path().join("data/user/0");
        let canonical_de = root.path().join("data/user_de/0");
        for path in [
            &maintenance_ce,
            &maintenance_de,
            &canonical_ce.join(package.as_str()),
            &canonical_de.join(package.as_str()),
        ] {
            fs::create_dir_all(path).unwrap();
        }
        let mountinfo = root.path().join("mountinfo");
        fs::write(&mountinfo, "").unwrap();
        let runner = MountRunner {
            commands: Rc::new(RefCell::new(Vec::new())),
            mountinfo: mountinfo.clone(),
            fail_mount_containing: None,
            normal_unmount_error: None,
            ignore_unmount: false,
        };
        let mut android = SystemAndroidOps {
            build_id: "test".to_owned(),
            ce_slots_root: ce_slots,
            de_slots_root: de_slots,
            canonical_ce_root: canonical_ce,
            canonical_de_root: canonical_de,
            mountinfo,
            proc_root: root.path().join("proc"),
            runner: Box::new(runner),
        };

        android.apply_maintenance_view(&package, &token).unwrap();

        assert_eq!(
            android.observe_view(&package).unwrap(),
            ObservedView::Maintenance(token),
        );
    }

    #[test]
    fn enabled_state_round_trips_through_the_exact_user0_pm_action() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = RecordingRunner {
            outputs: VecDeque::from([
                output("User 0: ceDataInode=1 installed=true hidden=false enabled=3\n"),
                output(""),
            ]),
            commands: Rc::clone(&commands),
        };
        let (mut android, recorded) = system_with_runner(root.path(), runner);

        let state = android.read_enabled_state(&package).unwrap();
        android.restore_enabled_state(&package, state).unwrap();

        assert_eq!(state, PackageEnabledState::DisabledUser);
        assert_eq!(
            *recorded.borrow(),
            vec![
                (
                    "/system/bin/dumpsys".to_owned(),
                    vec!["package".to_owned(), package.as_str().to_owned()],
                ),
                (
                    "/system/bin/pm".to_owned(),
                    vec![
                        "disable-user".to_owned(),
                        "--user".to_owned(),
                        "0".to_owned(),
                        package.as_str().to_owned(),
                    ],
                ),
            ],
        );
    }

    #[test]
    fn either_domain_mount_failure_cleans_every_published_mount() {
        for failing_domain in ["misc_ce", "misc_de"] {
            let root = tempfile::tempdir().unwrap();
            let package = PackageName::new("com.example.app").unwrap();
            let slot = SlotId::numbered(1);
            let ce_slots = root.path().join("misc_ce/uclone-slices-v2/slots");
            let de_slots = root.path().join("misc_de/uclone-slices-v2/slots");
            let canonical_ce = root.path().join("data/user/0");
            let canonical_de = root.path().join("data/user_de/0");
            fs::create_dir_all(ce_slots.join(package.as_str()).join(slot.as_str())).unwrap();
            fs::create_dir_all(de_slots.join(package.as_str()).join(slot.as_str())).unwrap();
            fs::create_dir_all(canonical_ce.join(package.as_str())).unwrap();
            fs::create_dir_all(canonical_de.join(package.as_str())).unwrap();
            let mountinfo = root.path().join("mountinfo");
            fs::write(&mountinfo, "").unwrap();
            let runner = MountRunner {
                commands: Rc::new(RefCell::new(Vec::new())),
                mountinfo: mountinfo.clone(),
                fail_mount_containing: Some(failing_domain.to_owned()),
                normal_unmount_error: None,
                ignore_unmount: false,
            };
            let mut android = SystemAndroidOps {
                build_id: "test".to_owned(),
                ce_slots_root: ce_slots,
                de_slots_root: de_slots,
                canonical_ce_root: canonical_ce,
                canonical_de_root: canonical_de,
                mountinfo,
                proc_root: root.path().join("proc"),
                runner: Box::new(runner),
            };

            assert!(android.apply_view(&package, &slot).is_err());
            assert_eq!(android.observe_view(&package).unwrap(), ObservedView::Base);
        }
    }

    #[test]
    fn an_unmount_that_makes_no_progress_fails_without_a_retry_limit() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let canonical_ce = root.path().join("data/user/0");
        let canonical_de = root.path().join("data/user_de/0");
        fs::create_dir_all(canonical_ce.join(package.as_str())).unwrap();
        fs::create_dir_all(canonical_de.join(package.as_str())).unwrap();
        let mountinfo = root.path().join("mountinfo");
        fs::write(
            &mountinfo,
            format!(
                "1 0 0:1 /uclone-slices-v2/slots/{package}/slot-1 {}/{} rw - ext4 none rw\n",
                canonical_ce.display(),
                package,
            ),
        )
        .unwrap();
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = MountRunner {
            commands: Rc::clone(&commands),
            mountinfo: mountinfo.clone(),
            fail_mount_containing: None,
            normal_unmount_error: None,
            ignore_unmount: true,
        };
        let mut android = SystemAndroidOps {
            build_id: "test".to_owned(),
            ce_slots_root: root.path().join("misc_ce/uclone-slices-v2/slots"),
            de_slots_root: root.path().join("misc_de/uclone-slices-v2/slots"),
            canonical_ce_root: canonical_ce,
            canonical_de_root: canonical_de,
            mountinfo,
            proc_root: root.path().join("proc"),
            runner: Box::new(runner),
        };

        assert!(android.apply_view(&package, &SlotId::base()).is_err());
        assert_eq!(commands.borrow().len(), 1);
    }

    #[test]
    fn busy_managed_mount_uses_lazy_detach_and_still_verifies_progress() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let canonical_ce = root.path().join("data/user/0");
        let canonical_de = root.path().join("data/user_de/0");
        fs::create_dir_all(canonical_ce.join(package.as_str())).unwrap();
        fs::create_dir_all(canonical_de.join(package.as_str())).unwrap();
        let mountinfo = root.path().join("mountinfo");
        fs::write(
            &mountinfo,
            format!(
                "1 0 0:1 /uclone-slices-v2/slots/{package}/slot-1 {}/{} rw - ext4 none rw\n\
                 2 0 0:1 /uclone-slices-v2/slots/{package}/slot-1 {}/{} rw - ext4 none rw\n",
                canonical_ce.display(),
                package,
                canonical_de.display(),
                package,
            ),
        )
        .unwrap();
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = MountRunner {
            commands: Rc::clone(&commands),
            mountinfo: mountinfo.clone(),
            fail_mount_containing: None,
            normal_unmount_error: Some("umount: {mount_point}: Device or resource busy".to_owned()),
            ignore_unmount: false,
        };
        let mut android = SystemAndroidOps {
            build_id: "test".to_owned(),
            ce_slots_root: root.path().join("misc_ce/uclone-slices-v2/slots"),
            de_slots_root: root.path().join("misc_de/uclone-slices-v2/slots"),
            canonical_ce_root: canonical_ce.clone(),
            canonical_de_root: canonical_de.clone(),
            mountinfo,
            proc_root: root.path().join("proc"),
            runner: Box::new(runner),
        };

        android.apply_view(&package, &SlotId::base()).unwrap();

        assert_eq!(android.observe_view(&package).unwrap(), ObservedView::Base);
        assert_eq!(
            *commands.borrow(),
            vec![
                (
                    "/system/bin/umount".to_owned(),
                    vec![canonical_ce.join(package.as_str()).display().to_string()],
                ),
                (
                    "/system/bin/umount".to_owned(),
                    vec![
                        "-l".to_owned(),
                        canonical_ce.join(package.as_str()).display().to_string(),
                    ],
                ),
                (
                    "/system/bin/umount".to_owned(),
                    vec![canonical_de.join(package.as_str()).display().to_string()],
                ),
                (
                    "/system/bin/umount".to_owned(),
                    vec![
                        "-l".to_owned(),
                        canonical_de.join(package.as_str()).display().to_string(),
                    ],
                ),
            ]
        );
    }

    #[test]
    fn non_exact_busy_error_does_not_use_lazy_detach() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let canonical_ce = root.path().join("data/user/0");
        let canonical_de = root.path().join("data/user_de/0");
        fs::create_dir_all(canonical_ce.join(package.as_str())).unwrap();
        fs::create_dir_all(canonical_de.join(package.as_str())).unwrap();
        let mountinfo = root.path().join("mountinfo");
        fs::write(
            &mountinfo,
            format!(
                "1 0 0:1 /uclone-slices-v2/slots/{package}/slot-1 {}/{} rw - ext4 none rw\n",
                canonical_ce.display(),
                package,
            ),
        )
        .unwrap();
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = MountRunner {
            commands: Rc::clone(&commands),
            mountinfo: mountinfo.clone(),
            fail_mount_containing: None,
            normal_unmount_error: Some("helper: Device or resource busy".to_owned()),
            ignore_unmount: false,
        };
        let mut android = SystemAndroidOps {
            build_id: "test".to_owned(),
            ce_slots_root: root.path().join("misc_ce/uclone-slices-v2/slots"),
            de_slots_root: root.path().join("misc_de/uclone-slices-v2/slots"),
            canonical_ce_root: canonical_ce.clone(),
            canonical_de_root: canonical_de,
            mountinfo,
            proc_root: root.path().join("proc"),
            runner: Box::new(runner),
        };

        let error = android
            .apply_view(&package, &SlotId::base())
            .unwrap_err()
            .to_string();

        assert!(error.contains("helper: Device or resource busy"));
        assert_eq!(
            *commands.borrow(),
            vec![(
                "/system/bin/umount".to_owned(),
                vec![canonical_ce.join(package.as_str()).display().to_string()],
            )]
        );
    }

    #[test]
    fn lazy_detach_that_makes_no_progress_still_fails() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let canonical_ce = root.path().join("data/user/0");
        let canonical_de = root.path().join("data/user_de/0");
        fs::create_dir_all(canonical_ce.join(package.as_str())).unwrap();
        fs::create_dir_all(canonical_de.join(package.as_str())).unwrap();
        let mountinfo = root.path().join("mountinfo");
        fs::write(
            &mountinfo,
            format!(
                "1 0 0:1 /uclone-slices-v2/slots/{package}/slot-1 {}/{} rw - ext4 none rw\n",
                canonical_ce.display(),
                package,
            ),
        )
        .unwrap();
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = MountRunner {
            commands: Rc::clone(&commands),
            mountinfo: mountinfo.clone(),
            fail_mount_containing: None,
            normal_unmount_error: Some("umount: {mount_point}: Device or resource busy".to_owned()),
            ignore_unmount: true,
        };
        let mut android = SystemAndroidOps {
            build_id: "test".to_owned(),
            ce_slots_root: root.path().join("misc_ce/uclone-slices-v2/slots"),
            de_slots_root: root.path().join("misc_de/uclone-slices-v2/slots"),
            canonical_ce_root: canonical_ce.clone(),
            canonical_de_root: canonical_de,
            mountinfo,
            proc_root: root.path().join("proc"),
            runner: Box::new(runner),
        };

        let error = android
            .apply_view(&package, &SlotId::base())
            .unwrap_err()
            .to_string();

        assert!(error.contains("unmount made no progress"));
        assert_eq!(
            *commands.borrow(),
            vec![
                (
                    "/system/bin/umount".to_owned(),
                    vec![canonical_ce.join(package.as_str()).display().to_string()],
                ),
                (
                    "/system/bin/umount".to_owned(),
                    vec![
                        "-l".to_owned(),
                        canonical_ce.join(package.as_str()).display().to_string(),
                    ],
                ),
            ]
        );
    }
}
