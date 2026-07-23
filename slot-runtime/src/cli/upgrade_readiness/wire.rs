use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::domain::{PackageName, SlotId};
use crate::lifecycle::LifecycleState;
use crate::protocol::MAX_FRAME_SIZE;

use super::UpgradeReadinessError;

const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Serialize)]
struct CompatRequest<'a> {
    schema_version: u32,
    request_id: &'static str,
    command: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    build_id: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompatResponse {
    pub(super) schema_version: u32,
    pub(super) request_id: String,
    pub(super) status: String,
    #[serde(default)]
    pub(super) error_code: Option<String>,
    #[serde(default)]
    pub(super) payload: Option<CompatPayload>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub(super) enum CompatPayload {
    ProbeReport(CompatProbe),
    ManagedApps(CompatApps),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "the compatibility payload mirrors the deployed probe schema"
)]
pub(super) struct CompatProbe {
    pub(super) ready: bool,
    pub(super) user_unlocked: bool,
    pub(super) ce_de_supported: bool,
    #[serde(default)]
    pub(super) recovery_only: bool,
    #[serde(default)]
    pub(super) runtime_version: Option<String>,
    #[serde(default)]
    pub(super) build_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompatApps {
    pub(super) apps: Vec<CompatApp>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompatApp {
    pub(super) package: PackageName,
    pub(super) active_slot: SlotId,
    pub(super) lifecycle: LifecycleState,
}

pub(super) fn request(
    schema_version: u32,
    request_id: &'static str,
    command: &'static str,
    build_id: Option<&str>,
) -> Result<Vec<u8>, UpgradeReadinessError> {
    let mut frame = serde_json::to_vec(&CompatRequest {
        schema_version,
        request_id,
        command,
        build_id,
    })
    .map_err(|_| UpgradeReadinessError::InvalidFrame)?;
    frame.push(b'\n');
    Ok(frame)
}

pub(super) fn exchange<C>(
    connect: &mut C,
    request: &[u8],
) -> Result<CompatResponse, UpgradeReadinessError>
where
    C: FnMut() -> io::Result<UnixStream>,
{
    let mut stream = connect().map_err(UpgradeReadinessError::Transport)?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(TIMEOUT)))
        .map_err(UpgradeReadinessError::Transport)?;
    stream
        .write_all(request)
        .map_err(UpgradeReadinessError::Transport)?;
    let frame = read_frame(&mut stream)?;
    serde_json::from_slice(
        frame
            .strip_suffix(b"\n")
            .ok_or(UpgradeReadinessError::InvalidFrame)?,
    )
    .map_err(|_| UpgradeReadinessError::InvalidFrame)
}

fn read_frame(stream: &mut UnixStream) -> Result<Vec<u8>, UpgradeReadinessError> {
    let mut frame = Vec::with_capacity(256);
    let mut byte = [0_u8; 1];
    loop {
        match stream
            .read(&mut byte)
            .map_err(UpgradeReadinessError::Transport)?
        {
            1 => {
                frame.push(byte[0]);
                if frame.len() > MAX_FRAME_SIZE {
                    return Err(UpgradeReadinessError::InvalidFrame);
                }
                if byte[0] == b'\n' {
                    return Ok(frame);
                }
            }
            0 | 2.. => return Err(UpgradeReadinessError::InvalidFrame),
        }
    }
}
