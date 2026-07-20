#![doc = "Fixed `app_process` runner contract tests."]
#![allow(
    clippy::unwrap_used,
    reason = "the fixed valid package literal is a test fixture invariant"
)]

use uclone_slot_runtime::bridge::{
    APP_PROCESS_PATH, AppProcessRunner, BRIDGE_CLASSPATH, BRIDGE_MAIN_CLASS, BridgeCommand,
    PackageEnabledState,
};
use uclone_slot_runtime::domain::PackageName;

fn package() -> PackageName {
    PackageName::parse("com.example.dynamic").unwrap()
}

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
    let commands = vec![
        BridgeCommand::DeviceStatus,
        BridgeCommand::PackageStatus(package()),
        BridgeCommand::GateStatus(package()),
        BridgeCommand::SetEnabled(package(), PackageEnabledState::Enabled),
        BridgeCommand::SetSuspended(package(), false),
    ];
    for command in commands {
        let argv = command.argv();
        assert_eq!(argv.first().map(String::as_str), Some("/system/bin"));
        assert_eq!(argv.get(1).map(String::as_str), Some(BRIDGE_MAIN_CLASS));
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
        BridgeCommand::PackageStatus(package()).argv(),
        vec![
            "/system/bin",
            BRIDGE_MAIN_CLASS,
            "probe-package",
            "com.example.dynamic"
        ]
    );
    assert_eq!(
        BridgeCommand::GateStatus(package()).argv(),
        vec![
            "/system/bin",
            BRIDGE_MAIN_CLASS,
            "probe-gate",
            "com.example.dynamic"
        ]
    );
}
