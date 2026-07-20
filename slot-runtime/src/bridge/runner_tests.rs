use super::{
    ANDROID_ART_ROOT, ANDROID_I18N_ROOT, ANDROID_TZDATA_ROOT, BRIDGE_CLASSPATH,
    MAX_BOOT_CLASSPATH_BYTES, ValidatedBootEnvironment,
};
use crate::bridge::{ALLOWED_PACKAGE, BridgeCommand, PackageEnabledState};

const CURRENT_BOOTCLASSPATH: &str =
    "/apex/com.android.art/javalib/core-oj.jar:/system/framework/framework.jar";
const CURRENT_DEX2OATBOOTCLASSPATH: &str =
    "/apex/com.android.art/javalib/core-oj.jar:/system/framework/core-libart.jar";

#[test]
fn uses_fixed_android16_art_environment_roots() {
    assert_eq!(ANDROID_ART_ROOT, "/apex/com.android.art");
    assert_eq!(ANDROID_I18N_ROOT, "/apex/com.android.i18n");
    assert_eq!(ANDROID_TZDATA_ROOT, "/apex/com.android.tzdata");
}

#[test]
fn builds_only_the_fixed_and_validated_app_process_environment() {
    let environment = ValidatedBootEnvironment {
        bootclasspath: CURRENT_BOOTCLASSPATH.to_owned(),
        dex2oatbootclasspath: CURRENT_DEX2OATBOOTCLASSPATH.to_owned(),
    };
    assert_eq!(
        environment.environment_entries(),
        [
            ("CLASSPATH", BRIDGE_CLASSPATH),
            ("ANDROID_ROOT", "/system"),
            ("ANDROID_DATA", "/data"),
            ("ANDROID_ART_ROOT", ANDROID_ART_ROOT),
            ("ANDROID_I18N_ROOT", ANDROID_I18N_ROOT),
            ("ANDROID_TZDATA_ROOT", ANDROID_TZDATA_ROOT),
            ("BOOTCLASSPATH", CURRENT_BOOTCLASSPATH),
            ("DEX2OATBOOTCLASSPATH", CURRENT_DEX2OATBOOTCLASSPATH),
        ]
    );
}

#[test]
fn accepts_current_like_framework_and_apex_classpaths() {
    let result = ValidatedBootEnvironment::from_values(
        Some(CURRENT_BOOTCLASSPATH.to_owned()),
        Some(CURRENT_DEX2OATBOOTCLASSPATH.to_owned()),
    );
    assert!(result.is_ok());
}

#[test]
fn rejects_missing_classpaths() {
    let missing_boot =
        ValidatedBootEnvironment::from_values(None, Some(CURRENT_DEX2OATBOOTCLASSPATH.to_owned()));
    assert!(matches!(
        missing_boot,
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput
    ));

    let missing_dex =
        ValidatedBootEnvironment::from_values(Some(CURRENT_BOOTCLASSPATH.to_owned()), None);
    assert!(matches!(
        missing_dex,
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput
    ));
}

#[test]
fn rejects_untrusted_empty_traversal_and_control_paths() {
    let rejected = [
        "/tmp/evil.jar",
        "/system/framework/good.jar:",
        "/system/framework/../evil.jar",
        "/system/framework/evil\n.jar",
        "/apex/com.android.art/lib/core-oj.jar",
        "/apex/com.android.art/javalib/",
    ];
    for path in rejected {
        let result = ValidatedBootEnvironment::from_values(
            Some(path.to_owned()),
            Some(CURRENT_DEX2OATBOOTCLASSPATH.to_owned()),
        );
        assert!(result.is_err(), "path should be rejected: {path:?}");
    }
}

#[test]
fn rejects_classpath_larger_than_64_kibibytes() {
    let oversized = format!(
        "/system/framework/{}.jar",
        "a".repeat(MAX_BOOT_CLASSPATH_BYTES)
    );
    let result = ValidatedBootEnvironment::from_values(
        Some(oversized),
        Some(CURRENT_DEX2OATBOOTCLASSPATH.to_owned()),
    );
    assert!(matches!(
        result,
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput
    ));
}

#[test]
fn persistent_session_requests_keep_only_fixed_bridge_arguments() {
    assert_eq!(
        BridgeCommand::GateStatus.session_request(),
        format!("probe-gate\t{ALLOWED_PACKAGE}\n").into_bytes()
    );
    assert_eq!(
        BridgeCommand::SetEnabled(PackageEnabledState::DisabledUser).session_request(),
        format!("set-enabled\t{ALLOWED_PACKAGE}\tdisabled_user\n").into_bytes()
    );
}
