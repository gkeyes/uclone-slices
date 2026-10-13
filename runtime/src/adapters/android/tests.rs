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
        for entry in
            fs::read_dir(&self.proc_root).map_err(|error| AdapterError::new(error.to_string()))?
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
    fn run(&mut self, program: &str, arguments: &[&str]) -> Result<CommandOutput, AdapterError> {
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
    fn run(&mut self, program: &str, arguments: &[&str]) -> Result<CommandOutput, AdapterError> {
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
    fn run(&mut self, program: &str, arguments: &[&str]) -> Result<CommandOutput, AdapterError> {
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
    fn run(&mut self, program: &str, arguments: &[&str]) -> Result<CommandOutput, AdapterError> {
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
            package_rows: None,
            launchers: BTreeMap::new(),
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
            package_rows: None,
            launchers: BTreeMap::new(),
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

    confirm_processes_stably_stopped(
        10_123,
        Duration::from_secs(1),
        Duration::ZERO,
        || {
            scan_count += 1;
            Ok(scans.pop_front().unwrap_or_default())
        },
        |_known| Ok(Vec::new()),
    )
    .unwrap();

    assert_eq!(scan_count, 4);
}

#[test]
fn process_stop_confirmation_rechecks_only_known_processes_until_they_exit() {
    let mut full_scans = 0_usize;
    let mut rechecks = 0_usize;

    confirm_processes_stably_stopped(
        10_123,
        Duration::from_secs(1),
        Duration::ZERO,
        || {
            full_scans += 1;
            Ok(if full_scans == 1 {
                vec![PathBuf::from("/proc/123")]
            } else {
                Vec::new()
            })
        },
        |known| {
            rechecks += 1;
            Ok(if rechecks < 5 {
                known.to_vec()
            } else {
                Vec::new()
            })
        },
    )
    .unwrap();

    // One scan finds the process, rechecks follow it until it exits, then two consecutive
    // empty full scans confirm that nothing new took its place.
    assert_eq!(rechecks, 5);
    assert_eq!(full_scans, 3);
}

#[test]
fn process_stop_recheck_drops_exited_and_reused_pids() {
    let root = tempfile::tempdir().unwrap();
    let alive = root.path().join("123");
    let reused = root.path().join("456");
    fs::create_dir_all(&alive).unwrap();
    fs::create_dir_all(&reused).unwrap();
    fs::write(alive.join("status"), "Uid:\t10123\t10123\t10123\t10123\n").unwrap();
    fs::write(reused.join("status"), "Uid:\t10999\t10999\t10999\t10999\n").unwrap();
    let known = [alive.clone(), reused, root.path().join("789")];

    assert_eq!(surviving_processes(&known, 10_123).unwrap(), vec![alive]);
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
    let base =
        "1 0 0:1 /user_de/0/com.example.app /data/user_de/0/com.example.app rw - ext4 none rw\n";
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
fn one_request_lists_packages_and_resolves_the_launcher_once() {
    let root = tempfile::tempdir().unwrap();
    let package = PackageName::new("com.example.app").unwrap();
    let apk = root.path().join("base.apk");
    fs::write(&apk, b"apk").unwrap();
    fs::create_dir_all(root.path().join("canonical-ce").join(package.as_str())).unwrap();
    fs::create_dir_all(root.path().join("canonical-de").join(package.as_str())).unwrap();
    let process = root.path().join("proc").join("123");
    fs::create_dir_all(&process).unwrap();
    fs::write(
        process.join("status"),
        "Name:\tapp\nUid:\t10123\t10123\t10123\t10123\n",
    )
    .unwrap();
    fs::write(process.join("mountinfo"), b"").unwrap();
    fs::write(root.path().join("mountinfo"), b"").unwrap();
    let commands = Rc::new(RefCell::new(Vec::new()));
    let runner = RecordingRunner {
        outputs: VecDeque::from([
            output(format!("package:{package} uid:10123\n")),
            output(format!("package:{}\n", apk.display())),
            output(format!("{package}/.MainActivity\n")),
            output("Status: ok\n"),
        ]),
        commands: Rc::clone(&commands),
    };
    let (mut android, recorded) = system_with_runner(root.path(), runner);

    let inspection = android.inspect(&package).unwrap();
    android.launch_verified(&package, &SlotId::base()).unwrap();

    assert!(inspection.was_running);
    let recorded = recorded.borrow();
    let listings = recorded
        .iter()
        .filter(|(_, arguments)| arguments.get(1).map(String::as_str) == Some("list"))
        .count();
    let resolutions = recorded
        .iter()
        .filter(|(_, arguments)| arguments.get(1).map(String::as_str) == Some("resolve-activity"))
        .count();
    assert_eq!((listings, resolutions), (1, 1));
    assert_eq!(recorded.len(), 4);
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
        program == "/system/bin/am" && arguments.first().map(String::as_str) == Some("force-stop")
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
            package_rows: None,
            launchers: BTreeMap::new(),
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
        package_rows: None,
        launchers: BTreeMap::new(),
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
        package_rows: None,
        launchers: BTreeMap::new(),
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
            package_rows: None,
            launchers: BTreeMap::new(),
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
        package_rows: None,
        launchers: BTreeMap::new(),
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
        package_rows: None,
        launchers: BTreeMap::new(),
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
        package_rows: None,
        launchers: BTreeMap::new(),
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
        package_rows: None,
        launchers: BTreeMap::new(),
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
