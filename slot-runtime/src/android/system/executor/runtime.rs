use std::io::{self, Read as _};
use std::process::{Child, ChildStdout};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::{ProcessFailure, ProcessInvocation, ProcessOutput};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(super) fn execute_child(
    child: &mut Child,
    stdout: ChildStdout,
    invocation: &ProcessInvocation,
) -> Result<ProcessOutput, ProcessFailure> {
    let Some(deadline) = Instant::now().checked_add(invocation.timeout()) else {
        stop_and_reap(child).map_err(ProcessFailure::Io)?;
        return Err(ProcessFailure::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "process timeout exceeds the monotonic clock range",
        )));
    };
    let (sender, receiver) = mpsc::sync_channel(1);
    let output_limit = invocation.output_limit();
    let reader = thread::Builder::new()
        .name(String::from("uclone-process-stdout"))
        .spawn(move || {
            let result = read_bounded(stdout, output_limit);
            let _send_result = sender.send(result);
        });
    let reader = match reader {
        Ok(reader) => reader,
        Err(error) => {
            stop_and_reap(child).map_err(ProcessFailure::Io)?;
            return Err(ProcessFailure::Io(error));
        }
    };
    poll_child(child, reader, &receiver, invocation, deadline)
}

fn poll_child(
    child: &mut Child,
    reader: JoinHandle<()>,
    receiver: &Receiver<io::Result<Vec<u8>>>,
    invocation: &ProcessInvocation,
    deadline: Instant,
) -> Result<ProcessOutput, ProcessFailure> {
    let output_limit = invocation.output_limit();
    let mut captured = None;
    loop {
        if captured.is_none() {
            match receiver.try_recv() {
                Ok(Ok(bytes)) if bytes.len() > output_limit => {
                    return fail_after_stop(
                        child,
                        reader,
                        ProcessFailure::OutputTooLarge { size: bytes.len() },
                    );
                }
                Ok(Ok(bytes)) => captured = Some(bytes),
                Ok(Err(error)) => {
                    return fail_after_stop(child, reader, ProcessFailure::Io(error));
                }
                Err(TryRecvError::Disconnected) => {
                    return fail_after_stop(child, reader, reader_failure());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let read_result = captured.map_or_else(|| receive_output(receiver), Ok);
                join_reader(reader)?;
                let bytes = read_result?;
                if bytes.len() > output_limit {
                    return Err(ProcessFailure::OutputTooLarge { size: bytes.len() });
                }
                return Ok(ProcessOutput::new(status.code(), bytes));
            }
            Ok(None) => {}
            Err(error) => return fail_after_stop(child, reader, ProcessFailure::Io(error)),
        }
        let now = Instant::now();
        if now >= deadline {
            return fail_after_stop(child, reader, ProcessFailure::TimedOut);
        }
        thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(now)));
    }
}

fn read_bounded(stdout: ChildStdout, output_limit: usize) -> io::Result<Vec<u8>> {
    let observed_limit = output_limit.saturating_add(1);
    let byte_limit = u64::try_from(observed_limit).map_or(u64::MAX, |limit| limit);
    let mut bytes = Vec::new();
    stdout.take(byte_limit).read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn receive_output(receiver: &Receiver<io::Result<Vec<u8>>>) -> Result<Vec<u8>, ProcessFailure> {
    receiver
        .recv()
        .map_err(|_| reader_failure())?
        .map_err(ProcessFailure::Io)
}

fn fail_after_stop(
    child: &mut Child,
    reader: JoinHandle<()>,
    failure: ProcessFailure,
) -> Result<ProcessOutput, ProcessFailure> {
    stop_and_reap(child).map_err(ProcessFailure::Io)?;
    join_reader(reader)?;
    Err(failure)
}

pub(super) fn stop_and_reap(child: &mut Child) -> io::Result<()> {
    if child.try_wait()?.is_none() {
        child.kill()?;
        let _status = child.wait()?;
    }
    Ok(())
}

fn join_reader(reader: JoinHandle<()>) -> Result<(), ProcessFailure> {
    reader.join().map_err(|_| reader_failure())
}

fn reader_failure() -> ProcessFailure {
    ProcessFailure::Io(io::Error::other(
        "stdout reader thread stopped unexpectedly",
    ))
}
