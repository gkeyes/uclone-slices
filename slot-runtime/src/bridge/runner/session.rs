use std::io::{self, BufRead as _, BufReader, Write as _};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::{BridgeCommand, BridgeRunnerError, MAX_OUTPUT_BYTES};
use crate::bridge::{BRIDGE_MAIN_CLASS, BridgePayload, BridgeResponse};
use crate::protocol::RUNTIME_BUILD_ID;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
enum SessionMessage {
    Output(Vec<u8>),
    OutputTooLarge(usize),
    Io(io::Error),
}

#[derive(Debug)]
pub(super) struct AppProcessSession {
    child: Child,
    stdin: ChildStdin,
    responses: Receiver<SessionMessage>,
    reader: Option<JoinHandle<()>>,
}

impl AppProcessSession {
    pub(super) fn spawn(mut command: Command) -> Result<Self, BridgeRunnerError> {
        let mut child = command
            .args([
                "/system/bin",
                BRIDGE_MAIN_CLASS,
                "serve-v2",
                RUNTIME_BUILD_ID,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(BridgeRunnerError::Io)?;
        let stdin = take_stdin(&mut child)?;
        let stdout = take_stdout(&mut child)?;
        let (sender, responses) = mpsc::channel();
        let reader = spawn_reader(stdout, sender).map_err(|error| {
            stop_child(&mut child);
            BridgeRunnerError::Io(error)
        })?;
        if let Err(error) = verify_startup_handshake(&responses) {
            stop_child(&mut child);
            drop(responses);
            let _ = reader.join();
            return Err(error);
        }
        Ok(Self {
            child,
            stdin,
            responses,
            reader: Some(reader),
        })
    }

    pub(super) fn exchange(
        &mut self,
        command: &BridgeCommand,
    ) -> Result<Vec<u8>, BridgeRunnerError> {
        if let Some(status) = self.child.try_wait().map_err(BridgeRunnerError::Io)? {
            return Err(BridgeRunnerError::NonZeroExit {
                status: status.code(),
            });
        }
        self.stdin
            .write_all(&command.session_request())
            .and_then(|()| self.stdin.flush())
            .map_err(BridgeRunnerError::Io)?;
        match self.responses.recv_timeout(RESPONSE_TIMEOUT) {
            Ok(SessionMessage::Output(output)) => Ok(output),
            Ok(SessionMessage::OutputTooLarge(size)) => {
                Err(BridgeRunnerError::OutputTooLarge { size })
            }
            Ok(SessionMessage::Io(error)) => Err(BridgeRunnerError::Io(error)),
            Err(RecvTimeoutError::Timeout) => Err(BridgeRunnerError::TimedOut),
            Err(RecvTimeoutError::Disconnected) => Err(self.disconnected_error()),
        }
    }

    fn disconnected_error(&mut self) -> BridgeRunnerError {
        match self.child.try_wait() {
            Ok(Some(status)) => BridgeRunnerError::NonZeroExit {
                status: status.code(),
            },
            Ok(None) => BridgeRunnerError::Io(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "fixed app_process response channel disconnected",
            )),
            Err(error) => BridgeRunnerError::Io(error),
        }
    }
}

fn verify_startup_handshake(responses: &Receiver<SessionMessage>) -> Result<(), BridgeRunnerError> {
    let bytes = match responses.recv_timeout(RESPONSE_TIMEOUT) {
        Ok(SessionMessage::Output(output)) => output,
        Ok(SessionMessage::OutputTooLarge(size)) => {
            return Err(BridgeRunnerError::OutputTooLarge { size });
        }
        Ok(SessionMessage::Io(error)) => return Err(BridgeRunnerError::Io(error)),
        Err(RecvTimeoutError::Timeout) => return Err(BridgeRunnerError::TimedOut),
        Err(RecvTimeoutError::Disconnected) => {
            return Err(BridgeRunnerError::InvalidHandshake);
        }
    };
    validate_startup_handshake(&bytes)
}

pub(super) fn validate_startup_handshake(bytes: &[u8]) -> Result<(), BridgeRunnerError> {
    let response =
        BridgeResponse::from_bytes(bytes).map_err(|_| BridgeRunnerError::InvalidHandshake)?;
    response
        .require_paired_v2(RUNTIME_BUILD_ID)
        .map_err(|error| match error.code() {
            crate::bridge::BridgeErrorCode::BuildMismatch => BridgeRunnerError::BuildMismatch,
            _ => BridgeRunnerError::InvalidHandshake,
        })?;
    if response.request_id() != "handshake"
        || !response.is_ok()
        || !matches!(response.payload(), Some(BridgePayload::Ack(_)))
    {
        return Err(BridgeRunnerError::InvalidHandshake);
    }
    Ok(())
}

impl Drop for AppProcessSession {
    fn drop(&mut self) {
        stop_child(&mut self.child);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn take_stdin(child: &mut Child) -> Result<ChildStdin, BridgeRunnerError> {
    child.stdin.take().ok_or_else(|| {
        stop_child(child);
        BridgeRunnerError::Io(io::Error::other("fixed app_process stdin unavailable"))
    })
}

fn take_stdout(child: &mut Child) -> Result<ChildStdout, BridgeRunnerError> {
    child.stdout.take().ok_or_else(|| {
        stop_child(child);
        BridgeRunnerError::Io(io::Error::other("fixed app_process stdout unavailable"))
    })
}

fn spawn_reader(stdout: ChildStdout, sender: Sender<SessionMessage>) -> io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name(String::from("uclone-bridge-stdout"))
        .spawn(move || read_responses(stdout, &sender))
}

fn read_responses(stdout: ChildStdout, sender: &Sender<SessionMessage>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut response = Vec::new();
        match reader.read_until(b'\n', &mut response) {
            Ok(0) => break,
            Ok(_) if response.len() > MAX_OUTPUT_BYTES => {
                let _ = sender.send(SessionMessage::OutputTooLarge(response.len()));
                break;
            }
            Ok(_) => {
                if sender.send(SessionMessage::Output(response)).is_err() {
                    break;
                }
            }
            Err(error) => {
                let _ = sender.send(SessionMessage::Io(error));
                break;
            }
        }
    }
}

fn stop_child(child: &mut Child) {
    if matches!(child.try_wait(), Ok(None)) {
        let _ = child.kill();
    }
    let _ = child.wait();
}
