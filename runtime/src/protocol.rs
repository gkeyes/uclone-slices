use serde::{Deserialize, Serialize};

use crate::model::{Capabilities, DisplayName, PackageName, PackageSnapshot, SeedMode, SlotId};
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
    },
    Unenroll {
        package: PackageName,
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

pub fn handle_line<P, S, A>(runtime: &mut Runtime<P, S, A>, line: &str) -> String
where
    P: PackageStore,
    S: SlotStorage,
    A: AndroidOps,
{
    let response = match decode_command(line) {
        Ok(command) => dispatch(runtime, command),
        Err(_error) => Response::Error {
            error: ErrorBody {
                code: ErrorCode::InvalidRequest,
            },
        },
    };
    match serde_json::to_string(&response) {
        Ok(mut encoded) => {
            encoded.push('\n');
            encoded
        }
        Err(_error) => "{\"error\":{\"code\":\"operation_failed\"}}\n".to_owned(),
    }
}

fn decode_command(line: &str) -> Result<Command, ()> {
    let value = serde_json::from_str::<serde_json::Value>(line).map_err(|_error| ())?;
    let object = value.as_object().ok_or(())?;
    let operation = object
        .get("op")
        .and_then(serde_json::Value::as_str)
        .ok_or(())?;
    let allowed = match operation {
        "probe" | "list_packages" => &["op"][..],
        "get_package" => &["op", "package"][..],
        "enroll" => &["op", "package", "reset"][..],
        "unenroll" => &["op", "package"][..],
        "create_slot" => &["op", "package", "name", "seed"][..],
        "activate_slot" => &["op", "package", "slot"][..],
        "rename_slot" => &["op", "package", "slot", "name"][..],
        "delete_slot" => &["op", "package", "slot"][..],
        _ => return Err(()),
    };
    if !object.keys().all(|key| allowed.contains(&key.as_str())) {
        return Err(());
    }
    if operation == "enroll"
        && object
            .get("reset")
            .is_some_and(|reset| reset != &serde_json::Value::Bool(true))
    {
        return Err(());
    }
    serde_json::from_value(value).map_err(|_error| ())
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
        Command::Enroll { package, reset } => runtime
            .enroll(package, reset)
            .map(|package| SuccessPayload::Package { package }),
        Command::Unenroll { package } => runtime
            .unenroll(&package)
            .map(|()| SuccessPayload::Empty {}),
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
    use crate::model::ObservedView;

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
        ] {
            let request = fixture(format!("{name}.request.json"));
            let expected = fixture(format!("{name}.response.json"));
            let actual = handle_line(&mut runtime, request.trim());
            assert_eq!(actual.trim(), expected.trim(), "fixture {name}");
        }
    }

    #[test]
    fn shared_rename_fixture_covers_the_metadata_operation() {
        let mut runtime = runtime_with_slot();
        let request = fixture("rename_slot.request.json".to_owned());
        let expected = fixture("rename_slot.response.json".to_owned());

        let actual = handle_line(&mut runtime, request.trim());

        assert_eq!(actual.trim(), expected.trim());
    }

    #[test]
    fn shared_delete_fixture_covers_the_storage_operation() {
        let mut runtime = runtime_with_slot();
        let request = fixture("delete_slot.request.json".to_owned());
        let expected = fixture("delete_slot.response.json".to_owned());

        let actual = handle_line(&mut runtime, request.trim());

        assert_eq!(actual.trim(), expected.trim());
    }

    #[test]
    fn shared_unenroll_fixture_covers_the_registration_removal() {
        let mut runtime = runtime_with_slot();
        let request = fixture("unenroll.request.json".to_owned());
        let expected = fixture("unenroll.response.json".to_owned());

        let actual = handle_line(&mut runtime, request.trim());
        let repeated = handle_line(&mut runtime, request.trim());

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

        let response = handle_line(&mut runtime, r#"{"op":"probe","future":true}"#);

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
            r#"{"op":"unenroll","package":"com.example.app","slot":"base"}"#,
        );

        assert_eq!(response, "{\"error\":{\"code\":\"invalid_request\"}}\n");
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
            r#"{"op":"enroll","package":"com.example.app","reset":false}"#,
        );

        assert_eq!(response, "{\"error\":{\"code\":\"invalid_request\"}}\n");
    }

    #[test]
    fn explicit_reset_enroll_recovers_an_identity_conflict() {
        let package = PackageName::new("com.example.app").unwrap();
        let mut android = MemoryAndroidOps::default();
        android.install(package.clone());
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            android,
        );
        runtime.enroll(package.clone(), false).unwrap();
        let created = runtime
            .create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
            .unwrap();
        let target = created.slots[1].id.clone();
        let (packages, slots, mut android) = runtime.into_parts();
        android.change_identity(&package);
        let mut runtime = Runtime::new(packages, slots, android);
        assert_eq!(
            runtime.get_package(&package),
            Err(RuntimeError::StateConflict)
        );

        let request = fixture("enroll_reset.request.json".to_owned());
        let expected = fixture("enroll_reset.response.json".to_owned());
        let actual = handle_line(&mut runtime, request.trim());

        assert_eq!(actual.trim(), expected.trim());
        let (_packages, slots, android) = runtime.into_parts();
        assert_eq!(slots.domain_state(&package, &target), None);
        assert_eq!(android.view(&package), Some(&ObservedView::Base));
        assert!(!android.is_running(&package));
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
        runtime.enroll(package.clone(), false).unwrap();
        runtime
            .create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
            .unwrap();
        runtime
    }
}
