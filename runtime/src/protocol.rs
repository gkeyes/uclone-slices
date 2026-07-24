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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SuccessPayload {
    Capabilities(Capabilities),
    PackageList { packages: Vec<PackageSnapshot> },
    Package { package: PackageSnapshot },
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
        "get_package" | "enroll" => &["op", "package"][..],
        "create_slot" => &["op", "package", "name", "seed"][..],
        "activate_slot" => &["op", "package", "slot"][..],
        _ => return Err(()),
    };
    if !object.keys().all(|key| allowed.contains(&key.as_str())) {
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
        Command::Enroll { package } => runtime
            .enroll(package)
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

    #[test]
    fn shared_fixtures_cover_the_six_command_flow() {
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
    fn unknown_field_is_an_invalid_request() {
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            MemoryAndroidOps::default(),
        );

        let response = handle_line(&mut runtime, r#"{"op":"probe","future":true}"#);

        assert_eq!(response, "{\"error\":{\"code\":\"invalid_request\"}}\n");
    }

    fn fixture(name: String) -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../protocol/fixtures")
            .join(name);
        fs::read_to_string(path).unwrap()
    }
}
