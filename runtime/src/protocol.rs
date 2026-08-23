use serde::{Deserialize, Serialize};

use crate::model::{
    Capabilities, DisplayName, PackageName, PackageSnapshot, SeedMode, SigningIdentity, SlotId,
};
use crate::ports::{AndroidOps, PackageStore, SlotStorage};
use crate::usecases::{Runtime, RuntimeError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Probe,
    ListPackages,
    GetPackage {
        package: PackageName,
    },
    Enroll {
        package: PackageName,
        #[serde(default)]
        reset: bool,
        #[serde(default)]
        signing: Option<SigningIdentity>,
    },
    RebindPackage {
        package: PackageName,
        signing: SigningIdentity,
        #[serde(default)]
        trust_legacy: bool,
    },
    Unenroll {
        package: PackageName,
    },
    SetLaunchAfterReboot {
        package: PackageName,
        enabled: bool,
    },
    CreateSlot {
        package: PackageName,
        name: DisplayName,
        seed: SeedMode,
    },
    ActivateSlot {
        package: PackageName,
        slot: SlotId,
    },
    RenameSlot {
        package: PackageName,
        slot: SlotId,
        name: DisplayName,
    },
    DeleteSlot {
        package: PackageName,
        slot: SlotId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SuccessPayload {
    Capabilities(Capabilities),
    PackageList { packages: Vec<PackageSnapshot> },
    Package { package: PackageSnapshot },
    Empty {},
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    NotFound,
    StateConflict,
    IdentityMismatch,
    OperationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response {
    Ok { ok: SuccessPayload },
    Error { error: ErrorBody },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecodeError {
    InvalidRequest,
    MissingClientBuildId,
    ClientBuildMismatch,
}

pub fn handle_line<P, S, A>(
    runtime: &mut Runtime<P, S, A>,
    expected_build_id: &str,
    line: &str,
) -> String
where
    P: PackageStore,
    S: SlotStorage,
    A: AndroidOps,
{
    let response = match decode_command(line, expected_build_id) {
        Ok(command) => dispatch(runtime, command),
        Err(error) => {
            match error {
                DecodeError::MissingClientBuildId => {
                    eprintln!("op=protocol step=client_build_id error=missing")
                }
                DecodeError::ClientBuildMismatch => {
                    eprintln!("op=protocol step=client_build_id error=mismatch")
                }
                DecodeError::InvalidRequest => {}
            }
            Response::Error {
                error: ErrorBody {
                    code: ErrorCode::InvalidRequest,
                },
            }
        }
    };
    match serde_json::to_string(&response) {
        Ok(mut encoded) => {
            encoded.push('\n');
            encoded
        }
        Err(_error) => "{\"error\":{\"code\":\"operation_failed\"}}\n".to_owned(),
    }
}

fn decode_command(line: &str, expected_build_id: &str) -> Result<Command, DecodeError> {
    let mut value = serde_json::from_str::<serde_json::Value>(line)
        .map_err(|_error| DecodeError::InvalidRequest)?;
    let object = value.as_object_mut().ok_or(DecodeError::InvalidRequest)?;
    let operation = object
        .get("op")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or(DecodeError::InvalidRequest)?;
    let allowed = match operation.as_str() {
        "probe" => &["op"][..],
        "list_packages" => &["op", "client_build_id"][..],
        "get_package" => &["op", "package", "client_build_id"][..],
        "enroll" => &["op", "package", "reset", "signing", "client_build_id"][..],
        "rebind_package" => &[
            "op",
            "package",
            "signing",
            "trust_legacy",
            "client_build_id",
        ][..],
        "unenroll" => &["op", "package", "client_build_id"][..],
        "set_launch_after_reboot" => &["op", "package", "enabled", "client_build_id"][..],
        "create_slot" => &["op", "package", "name", "seed", "client_build_id"][..],
        "activate_slot" => &["op", "package", "slot", "client_build_id"][..],
        "rename_slot" => &["op", "package", "slot", "name", "client_build_id"][..],
        "delete_slot" => &["op", "package", "slot", "client_build_id"][..],
        _ => return Err(DecodeError::InvalidRequest),
    };
    if !object.keys().all(|key| allowed.contains(&key.as_str())) {
        return Err(DecodeError::InvalidRequest);
    }
    if operation != "probe" {
        let client_build_id = object
            .get("client_build_id")
            .and_then(serde_json::Value::as_str)
            .ok_or(DecodeError::MissingClientBuildId)?;
        if client_build_id != expected_build_id {
            return Err(DecodeError::ClientBuildMismatch);
        }
    }
    if operation == "enroll"
        && object
            .get("reset")
            .is_some_and(|reset| reset != &serde_json::Value::Bool(true))
    {
        return Err(DecodeError::InvalidRequest);
    }
    if operation == "rebind_package"
        && object
            .get("trust_legacy")
            .is_some_and(|trust| !trust.is_boolean())
    {
        return Err(DecodeError::InvalidRequest);
    }
    object.remove("client_build_id");
    serde_json::from_value(value).map_err(|_error| DecodeError::InvalidRequest)
}

fn dispatch<P, S, A>(runtime: &mut Runtime<P, S, A>, command: Command) -> Response
where
    P: PackageStore,
    S: SlotStorage,
    A: AndroidOps,
{
    let result = match command {
        Command::Probe => runtime.probe().map(SuccessPayload::Capabilities),
        Command::ListPackages => runtime
            .list_packages()
            .map(|packages| SuccessPayload::PackageList { packages }),
        Command::GetPackage { package } => runtime
            .get_package(&package)
            .map(|package| SuccessPayload::Package { package }),
        Command::Enroll {
            package,
            reset,
            signing,
        } => runtime
            .enroll(package, reset, signing)
            .map(|package| SuccessPayload::Package { package }),
        Command::RebindPackage {
            package,
            signing,
            trust_legacy,
        } => runtime
            .rebind_package(&package, signing, trust_legacy)
            .map(|package| SuccessPayload::Package { package }),
        Command::Unenroll { package } => runtime
            .unenroll(&package)
            .map(|()| SuccessPayload::Empty {}),
        Command::SetLaunchAfterReboot { package, enabled } => runtime
            .set_launch_after_reboot(&package, enabled)
            .map(|package| SuccessPayload::Package { package }),
        Command::CreateSlot {
            package,
            name,
            seed,
        } => runtime
            .create_slot(&package, name, seed)
            .map(|package| SuccessPayload::Package { package }),
        Command::ActivateSlot { package, slot } => runtime
            .activate_slot(&package, &slot)
            .map(|package| SuccessPayload::Package { package }),
        Command::RenameSlot {
            package,
            slot,
            name,
        } => runtime
            .rename_slot(&package, &slot, name)
            .map(|package| SuccessPayload::Package { package }),
        Command::DeleteSlot { package, slot } => runtime
            .delete_slot(&package, &slot)
            .map(|package| SuccessPayload::Package { package }),
    };
    match result {
        Ok(ok) => Response::Ok { ok },
        Err(error) => Response::Error {
            error: ErrorBody {
                code: error_code(error),
            },
        },
    }
}

fn error_code(error: RuntimeError) -> ErrorCode {
    match error {
        RuntimeError::InvalidRequest => ErrorCode::InvalidRequest,
        RuntimeError::NotFound => ErrorCode::NotFound,
        RuntimeError::StateConflict => ErrorCode::StateConflict,
        RuntimeError::IdentityMismatch => ErrorCode::IdentityMismatch,
        RuntimeError::OperationFailed => ErrorCode::OperationFailed,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::adapters::{MemoryAndroidOps, MemoryPackageStore, MemorySlotStorage};

    const CLIENT_BUILD_ID: &str = "0.1.9";

    #[test]
    fn shared_fixtures_cover_the_core_command_flow() {
        let package = PackageName::new("com.example.app").unwrap();
        let mut android = MemoryAndroidOps::default();
        android.install(package);
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            android,
        );
        for name in [
            "probe",
            "list_packages",
            "enroll",
            "get_package",
            "create_slot",
            "activate_slot",
            "set_launch_after_reboot",
        ] {
            let request = fixture(format!("{name}.request.json"));
            let expected = fixture(format!("{name}.response.json"));
            let actual = handle_line(&mut runtime, CLIENT_BUILD_ID, request.trim());
            assert_eq!(actual.trim(), expected.trim(), "fixture {name}");
        }
    }

    #[test]
    fn shared_rename_fixture_covers_the_metadata_operation() {
        let mut runtime = runtime_with_slot();
        let request = fixture("rename_slot.request.json".to_owned());
        let expected = fixture("rename_slot.response.json".to_owned());

        let actual = handle_line(&mut runtime, CLIENT_BUILD_ID, request.trim());

        assert_eq!(actual.trim(), expected.trim());
    }

    #[test]
    fn shared_delete_fixture_covers_the_storage_operation() {
        let mut runtime = runtime_with_slot();
        let request = fixture("delete_slot.request.json".to_owned());
        let expected = fixture("delete_slot.response.json".to_owned());

        let actual = handle_line(&mut runtime, CLIENT_BUILD_ID, request.trim());

        assert_eq!(actual.trim(), expected.trim());
    }

    #[test]
    fn shared_unenroll_fixture_covers_the_registration_removal() {
        let mut runtime = runtime_with_slot();
        let request = fixture("unenroll.request.json".to_owned());
        let expected = fixture("unenroll.response.json".to_owned());

        let actual = handle_line(&mut runtime, CLIENT_BUILD_ID, request.trim());
        let repeated = handle_line(&mut runtime, CLIENT_BUILD_ID, request.trim());

        assert_eq!(actual.trim(), expected.trim());
        assert_eq!(repeated.trim(), expected.trim());
    }

    #[test]
    fn unknown_field_is_an_invalid_request() {
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            MemoryAndroidOps::default(),
        );

        let response = handle_line(
            &mut runtime,
            CLIENT_BUILD_ID,
            r#"{"op":"probe","future":true}"#,
        );

        assert_eq!(response, "{\"error\":{\"code\":\"invalid_request\"}}\n");
    }

    #[test]
    fn unenroll_rejects_fields_that_have_no_consumer() {
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            MemoryAndroidOps::default(),
        );

        let response = handle_line(
            &mut runtime,
            CLIENT_BUILD_ID,
            r#"{"op":"unenroll","package":"com.example.app","slot":"base","client_build_id":"0.1.9"}"#,
        );

        assert_eq!(response, "{\"error\":{\"code\":\"invalid_request\"}}\n");
    }

    #[test]
    fn reboot_launch_setting_requires_a_boolean() {
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            MemoryAndroidOps::default(),
        );

        let response = handle_line(
            &mut runtime,
            CLIENT_BUILD_ID,
            r#"{"op":"set_launch_after_reboot","package":"com.example.app","enabled":"yes","client_build_id":"0.1.9"}"#,
        );

        assert_eq!(response, "{\"error\":{\"code\":\"invalid_request\"}}\n");
    }

    #[test]
    fn rebind_rejects_invalid_certificate_digests_and_unknown_signing_fields() {
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            MemoryAndroidOps::default(),
        );

        for request in [
            r#"{"op":"rebind_package","package":"com.example.app","signing":{"kind":"lineage","sha256":["bad"]},"trust_legacy":false,"client_build_id":"0.1.9"}"#,
            r#"{"op":"rebind_package","package":"com.example.app","signing":{"kind":"lineage","sha256":["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],"future":true},"trust_legacy":false,"client_build_id":"0.1.9"}"#,
        ] {
            assert_eq!(
                handle_line(&mut runtime, CLIENT_BUILD_ID, request),
                "{\"error\":{\"code\":\"invalid_request\"}}\n"
            );
        }
    }

    #[test]
    fn reset_marker_only_accepts_the_ui_backed_true_value() {
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            MemoryAndroidOps::default(),
        );

        let response = handle_line(
            &mut runtime,
            CLIENT_BUILD_ID,
            r#"{"op":"enroll","package":"com.example.app","reset":false,"client_build_id":"0.1.9"}"#,
        );

        assert_eq!(response, "{\"error\":{\"code\":\"invalid_request\"}}\n");
    }

    #[test]
    fn old_reset_enroll_is_rejected_without_deleting_slots() {
        let package = PackageName::new("com.example.app").unwrap();
        let mut android = MemoryAndroidOps::default();
        android.install(package.clone());
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            android,
        );
        runtime.enroll(package.clone(), false, None).unwrap();
        let created = runtime
            .create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
            .unwrap();
        let target = created.slots[1].id.clone();
        let (packages, slots, mut android) = runtime.into_parts();
        android.change_identity(&package);
        let mut runtime = Runtime::new(packages, slots, android);
        let request = fixture("enroll_reset.request.json".to_owned());
        let expected = fixture("enroll_reset.response.json".to_owned());
        let actual = handle_line(&mut runtime, CLIENT_BUILD_ID, request.trim());

        assert_eq!(actual.trim(), expected.trim());
        let (_packages, slots, _android) = runtime.into_parts();
        assert_eq!(slots.domain_state(&package, &target), Some((true, true)));
    }

    #[test]
    fn shared_rebind_fixture_preserves_the_existing_slot() {
        let package = PackageName::new("com.example.app").unwrap();
        let mut runtime = runtime_with_slot();
        let target = SlotId::numbered(1);
        runtime.activate_slot(&package, &target).unwrap();
        let (packages, slots, mut android) = runtime.into_parts();
        android.change_identity(&package);
        let mut runtime = Runtime::new(packages, slots, android);

        let request = fixture("rebind_package.request.json".to_owned());
        let expected = fixture("rebind_package.response.json".to_owned());
        let actual = handle_line(&mut runtime, CLIENT_BUILD_ID, request.trim());

        assert_eq!(actual.trim(), expected.trim());
        let (_packages, slots, _android) = runtime.into_parts();
        assert_eq!(slots.domain_state(&package, &target), Some((true, true)));
    }

    #[test]
    fn probe_keeps_the_legacy_wire_shape() {
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            MemoryAndroidOps::default(),
        );

        let response = handle_line(&mut runtime, CLIENT_BUILD_ID, r#"{"op":"probe"}"#);

        assert!(response.contains("build_id"));
    }

    #[test]
    fn every_non_probe_request_requires_the_exact_client_build_id() {
        for request in [
            r#"{"op":"list_packages"}"#,
            r#"{"op":"get_package","package":"com.example.app"}"#,
            r#"{"op":"enroll","package":"com.example.app","signing":{"kind":"lineage","sha256":["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]}}"#,
            r#"{"op":"list_packages","client_build_id":"0.1.8"}"#,
        ] {
            let mut runtime = Runtime::new(
                MemoryPackageStore::default(),
                MemorySlotStorage::default(),
                MemoryAndroidOps::default(),
            );

            assert_eq!(
                handle_line(&mut runtime, CLIENT_BUILD_ID, request),
                "{\"error\":{\"code\":\"invalid_request\"}}\n",
                "request: {request}",
            );
        }
    }

    #[test]
    fn rejected_old_write_request_has_no_side_effect() {
        let package = PackageName::new("com.example.app").unwrap();
        let mut android = MemoryAndroidOps::default();
        android.install(package.clone());
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            android,
        );

        let response = handle_line(
            &mut runtime,
            CLIENT_BUILD_ID,
            r#"{"op":"enroll","package":"com.example.app","signing":{"kind":"lineage","sha256":["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]}}"#,
        );

        assert_eq!(response, "{\"error\":{\"code\":\"invalid_request\"}}\n");
        let (packages, _slots, _android) = runtime.into_parts();
        assert!(packages.load(&package).unwrap().is_none());
    }

    fn fixture(name: String) -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../protocol/fixtures")
            .join(name);
        fs::read_to_string(path).unwrap()
    }

    fn runtime_with_slot() -> Runtime<MemoryPackageStore, MemorySlotStorage, MemoryAndroidOps> {
        let package = PackageName::new("com.example.app").unwrap();
        let mut android = MemoryAndroidOps::default();
        android.install(package.clone());
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            android,
        );
        runtime.enroll(package.clone(), false, None).unwrap();
        runtime
            .create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
            .unwrap();
        runtime
    }
}
