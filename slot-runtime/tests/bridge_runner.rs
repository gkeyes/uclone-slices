#![doc = "Fixed `app_process` runner contract tests."]

use uclone_slot_runtime::bridge::{
    APP_PROCESS_PATH, AppProcessRunner, BRIDGE_CLASSPATH, BRIDGE_MAIN_CLASS, BridgeCommand,
};

#[test]
fn production_runner_constants_are_absolute_and_preview_owned() {
    assert_eq!(APP_PROCESS_PATH, "/system/bin/app_process");
    assert_eq!(
        BRIDGE_CLASSPATH,
        "/data/adb/modules/uclone-slices-preview/runtime/slot-bridge.apk"
    );
    assert!(BRIDGE_MAIN_CLASS.starts_with("com.uclone.slotbridge."));
    let _runner = AppProcessRunner::new();
}

#[test]
fn every_command_argv_is_fixed_and_shell_free() {
    let commands = [
        BridgeCommand::DeviceStatus,
        BridgeCommand::PackageStatus,
        BridgeCommand::GateStatus,
        BridgeCommand::SetEnabled(uclone_slot_runtime::bridge::PackageEnabledState::Enabled),
        BridgeCommand::SetSuspended(false),
    ];
    for command in commands {
        let argv = command.argv();
        assert_eq!(argv.first(), Some(&"/system/bin"));
        assert_eq!(argv.get(1), Some(&BRIDGE_MAIN_CLASS));
        assert!(argv.iter().all(|argument| {
            !argument.contains(';') && !argument.contains('|') && !argument.contains("$(")
        }));
    }
}

#[test]
fn probe_argv_literals_are_stable() {
    assert_eq!(
        BridgeCommand::DeviceStatus.argv(),
        vec!["/system/bin", BRIDGE_MAIN_CLASS, "probe-device"]
    );
    assert_eq!(
        BridgeCommand::PackageStatus.argv(),
        vec![
            "/system/bin",
            BRIDGE_MAIN_CLASS,
            "probe-package",
            "com.uclone.slotprobe"
        ]
    );
    assert_eq!(
        BridgeCommand::GateStatus.argv(),
        vec![
            "/system/bin",
            BRIDGE_MAIN_CLASS,
            "probe-gate",
            "com.uclone.slotprobe"
        ]
    );
}
