use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::model::{
    Capabilities, ObservedView, PackageIdentity, PackageInspection, PackageName, SlotId,
};
use crate::ports::{AdapterError, AndroidOps};

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
        parse_package_uid(&output.stdout, package)
            .ok_or_else(|| AdapterError::new("package is not an ordinary user0 third-party app"))
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

    fn ensure_unmounted(&mut self, mount_point: &Path) -> Result<(), AdapterError> {
        loop {
            let content = fs::read_to_string(&self.mountinfo)
                .map_err(|error| AdapterError::new(error.to_string()))?;
            let before = mount_roots(&content, mount_point).len();
            if before == 0 {
                return Ok(());
            }
            let mount_point_text = path_text(mount_point)?;
            self.required("/system/bin/umount", &[mount_point_text])?;
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
        let running = self.run("/system/bin/pidof", &[package.as_str()])?;
        Ok(PackageInspection {
            identity,
            was_running: running.success && !running.stdout.trim().is_empty(),
        })
    }

    fn force_stop(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        self.required(
            "/system/bin/am",
            &["force-stop", "--user", "0", package.as_str()],
        )?;
        Ok(())
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

    fn launch_verified(
        &mut self,
        package: &PackageName,
        expected: &SlotId,
    ) -> Result<(), AdapterError> {
        let component = self.launcher_component(package)?;
        self.required(
            "/system/bin/am",
            &["start", "-W", "--user", "0", "-n", &component],
        )?;
        let pid_output = self.required("/system/bin/pidof", &[package.as_str()])?;
        let pids: Vec<_> = pid_output.stdout.split_whitespace().collect();
        if pids.is_empty() {
            return Err(AdapterError::new("launched package has no App process"));
        }
        for pid in pids {
            if !pid.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(AdapterError::new("pidof returned an invalid PID"));
            }
            let mountinfo = fs::read_to_string(self.proc_root.join(pid).join("mountinfo"))
                .map_err(|error| AdapterError::new(error.to_string()))?;
            let observed = app_view_from_mountinfo(
                &mountinfo,
                package,
                &self.canonical_ce_root,
                &self.canonical_de_root,
            );
            if !observed.matches(expected) {
                return Err(AdapterError::new(format!(
                    "App PID {pid} did not observe the requested CE/DE view"
                )));
            }
        }
        Ok(())
    }
}

fn parse_package_uid(content: &str, package: &PackageName) -> Option<u32> {
    let prefix = format!("package:{} uid:", package.as_str());
    content
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix)?.parse().ok())
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
    let ce = domain_view(
        &mount_roots(content, &canonical_ce_root.join(package.as_str())),
        package,
    );
    let de = domain_view(
        &mount_roots(content, &canonical_de_root.join(package.as_str())),
        package,
    );
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

fn domain_view(roots: &[String], package: &PackageName) -> Option<Option<SlotId>> {
    match roots {
        [] => Some(None),
        [root] => slot_from_mount_root(root, package).map(Some),
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
            } else if program == "/system/bin/umount" && !self.ignore_unmount {
                let target = arguments.first().copied().unwrap_or_default();
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

    #[test]
    fn inspect_uses_user0_third_party_launcher_identity_commands() {
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
                output(format!("package:{package} uid:10123\n")),
                output(format!("package:{}\n", apk.display())),
                output(format!("{package}/.MainActivity\n")),
                CommandOutput {
                    success: false,
                    stdout: String::new(),
                    stderr: String::new(),
                },
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
                (
                    "/system/bin/pidof".to_owned(),
                    vec![package.as_str().to_owned()],
                ),
            ]
        );
    }

    #[test]
    fn force_stop_does_not_change_enabled_state() {
        let root = tempfile::tempdir().unwrap();
        let package = PackageName::new("com.example.app").unwrap();
        let commands = Rc::new(RefCell::new(Vec::new()));
        let runner = RecordingRunner {
            outputs: VecDeque::new(),
            commands: Rc::clone(&commands),
        };
        let (mut android, recorded) = system_with_runner(root.path(), runner);

        android.force_stop(&package).unwrap();

        assert_eq!(
            *recorded.borrow(),
            vec![(
                "/system/bin/am".to_owned(),
                vec![
                    "force-stop".to_owned(),
                    "--user".to_owned(),
                    "0".to_owned(),
                    package.as_str().to_owned(),
                ],
            )]
        );
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
                output("123 456\n"),
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
}
