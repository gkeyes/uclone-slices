use serde::{Deserialize, Serialize};

use crate::model::{
    AccountIoLease, AccountIoScope, AccountIoStatus, AccountIoToken, ArchiveAccountId,
    ArchivedState, Capabilities, DisplayName, PackageName, PackageSnapshot, RestoreBatchResult,
    RestoreItemResult, RestoreMapping, SeedMode, SigningIdentity, SlotId, TransferId,
};
use crate::ports::{AndroidOps, PackageStore, SlotStorage};
use crate::usecases::{Runtime, RuntimeError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    // Field-less ops are empty struct variants rather than unit variants: serde ignores
    // extra fields on internally tagged unit variants even with `deny_unknown_fields`.
    Probe {},
    ListPackages {},
    GetPackage {
        package: PackageName,
    },
    Enroll {
        package: PackageName,
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
    BeginBackupIo {
        package: PackageName,
        scope: AccountIoScope,
    },
    FinishBackupIo {
        io_token: AccountIoToken,
    },
    BeginRestoreIo {
        package: PackageName,
        transfer_id: TransferId,
        mappings: Vec<RestoreMapping>,
        archived_state: ArchivedState,
    },
    CommitRestoreAccount {
        io_token: AccountIoToken,
        archive_account_id: ArchiveAccountId,
    },
    FinishRestoreIo {
        io_token: AccountIoToken,
    },
    AbortAccountIo {
        io_token: AccountIoToken,
    },
    ListAccountIoStatus {},
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SuccessPayload {
    Capabilities(Capabilities),
    PackageList { packages: Vec<PackageSnapshot> },
    Package { package: PackageSnapshot },
    AccountIoLease(AccountIoLease),
    AccountIoList { account_io: Vec<AccountIoStatus> },
    RestoreItem(RestoreItemResult),
    RestoreBatch(RestoreBatchResult),
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
    IoBusy,
    InsufficientStorage,
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

/// Decodes one request line.
///
/// The wire format is flat: `client_build_id` sits next to `op` and the command fields
/// (shipped Manager builds send it that way, and every op except `probe` must carry it).
/// It is taken out first, checked, and the rest is left to serde, whose
/// `deny_unknown_fields` on [`Command`] rejects any field an op does not consume.
fn decode_command(line: &str, expected_build_id: &str) -> Result<Command, DecodeError> {
    let mut value = serde_json::from_str::<serde_json::Value>(line)
        .map_err(|_error| DecodeError::InvalidRequest)?;
    let object = value.as_object_mut().ok_or(DecodeError::InvalidRequest)?;
    let is_probe = match object.get("op").and_then(serde_json::Value::as_str) {
        Some(operation) => operation == "probe",
        None => return Err(DecodeError::InvalidRequest),
    };
    let client_build_id = object.remove("client_build_id");
    match (is_probe, client_build_id) {
        // `probe` predates the build handshake and keeps its legacy shape.
        (true, None) => {}
        (true, Some(_)) => return Err(DecodeError::InvalidRequest),
        (false, None) => return Err(DecodeError::MissingClientBuildId),
        (false, Some(client_build_id)) => match client_build_id.as_str() {
            Some(client_build_id) if client_build_id == expected_build_id => {}
            Some(_) => return Err(DecodeError::ClientBuildMismatch),
            None => return Err(DecodeError::MissingClientBuildId),
        },
    }
    serde_json::from_value(value).map_err(|_error| DecodeError::InvalidRequest)
}

fn dispatch<P, S, A>(runtime: &mut Runtime<P, S, A>, command: Command) -> Response
where
    P: PackageStore,
    S: SlotStorage,
    A: AndroidOps,
{
    let result = match command {
        Command::Probe {} => runtime.probe().map(SuccessPayload::Capabilities),
        Command::ListPackages {} => runtime
            .list_packages()
            .map(|packages| SuccessPayload::PackageList { packages }),
        Command::GetPackage { package } => runtime
            .get_package(&package)
            .map(|package| SuccessPayload::Package { package }),
        Command::Enroll { package, signing } => runtime
            .enroll(package, signing)
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
        Command::BeginBackupIo { package, scope } => runtime
            .begin_backup_io(&package, scope)
            .map(SuccessPayload::AccountIoLease),
        Command::FinishBackupIo { io_token } => runtime
            .finish_backup_io(&io_token)
            .map(|()| SuccessPayload::Empty {}),
        Command::BeginRestoreIo {
            package,
            transfer_id,
            mappings,
            archived_state,
        } => runtime
            .begin_restore_io(&package, transfer_id, mappings, archived_state)
            .map(SuccessPayload::AccountIoLease),
        Command::CommitRestoreAccount {
            io_token,
            archive_account_id,
        } => runtime
            .commit_restore_account(&io_token, &archive_account_id)
            .map(SuccessPayload::RestoreItem),
        Command::FinishRestoreIo { io_token } => runtime
            .finish_restore_io(&io_token)
            .map(SuccessPayload::RestoreBatch),
        Command::AbortAccountIo { io_token } => runtime
            .abort_account_io(&io_token)
            .map(|()| SuccessPayload::Empty {}),
        Command::ListAccountIoStatus {} => runtime
            .list_account_io_status()
            .map(|account_io| SuccessPayload::AccountIoList { account_io }),
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
        RuntimeError::IoBusy => ErrorCode::IoBusy,
        RuntimeError::InsufficientStorage => ErrorCode::InsufficientStorage,
    }
}

#[cfg(test)]
mod tests;
