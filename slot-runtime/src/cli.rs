#![doc = "Restricted command-line client for the Slots Preview control plane."]

use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;

use crate::layout::RuntimeLayout;
use crate::protocol::{ErrorCode, ProtocolError, Request, RequestId, Response, UnixClient};

mod command;
mod direct;
mod execute;
mod fallback;
mod rpc;
mod timeout;

pub use command::{CliCommand, SeedArgument};
pub use execute::{execute, execute_request};

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// The complete `slotctl` command line.
#[derive(Debug, Parser)]
#[command(
    name = "slotctl",
    version,
    about = "UClone Slots Preview control-plane client",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// The one supported control-plane operation.
    #[command(subcommand)]
    pub command: CliCommand,
}

/// Errors returned by request construction, transport, output, or the daemon.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// A command argument did not pass the typed boundary parser.
    #[error("invalid command argument: {0}")]
    InvalidArgument(String),
    /// The protocol rejected a locally built request or transport exchange.
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    /// The daemon returned a typed error response.
    #[error("daemon rejected request: {0:?}")]
    Daemon(ErrorCode),
    /// Writing the response or diagnostic failed.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    /// The system clock could not produce a request timestamp.
    #[error("system clock is before the Unix epoch")]
    Clock,
}

/// A request/response transport boundary that can be replaced in tests.
pub trait Transport {
    /// Sends one validated request and returns its matching response.
    fn request(&mut self, request: &Request) -> Result<Response, ProtocolError>;
}

impl Transport for UnixClient {
    fn request(&mut self, request: &Request) -> Result<Response, ProtocolError> {
        Self::request(self, request)
    }
}

/// Fixed-socket transport used by the `slotctl` binary.
#[derive(Debug)]
pub struct UnixTransport {
    client: UnixClient,
}

impl UnixTransport {
    /// Connects only to the compiled Preview runtime socket.
    pub fn connect() -> Result<Self, ProtocolError> {
        UnixClient::connect(RuntimeLayout::socket()).map(|client| Self { client })
    }

    /// Wraps an injected protocol client for host-side tests.
    pub const fn from_client(client: UnixClient) -> Self {
        Self { client }
    }

    /// Wraps an injected Unix stream for host-side tests.
    pub const fn from_stream(stream: UnixStream) -> Self {
        Self::from_client(UnixClient::from_stream(stream))
    }

    fn set_timeout(&mut self, timeout: std::time::Duration) -> Result<(), ProtocolError> {
        self.client.set_timeout(timeout)
    }
}

impl Transport for UnixTransport {
    fn request(&mut self, request: &Request) -> Result<Response, ProtocolError> {
        self.client.request(request)
    }
}

/// Connects the fixed transport and executes a parsed command on stdio.
pub fn run(cli: &Cli) -> Result<(), CliError> {
    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut output = stdout.lock();
    let mut diagnostics = stderr.lock();
    if matches!(cli.command, CliCommand::Rpc) {
        return rpc::run(io::stdin().lock(), &mut output, &mut diagnostics);
    }
    let request = match cli.command.request() {
        Ok(request) => request,
        Err(error) => return fail(error, &mut diagnostics),
    };
    let mut transport = match UnixTransport::connect() {
        Ok(transport) => transport,
        Err(_error) if matches!(cli.command, CliCommand::Rescue { .. }) => {
            return direct::run(&request, &mut output, &mut diagnostics);
        }
        Err(error) => return fail(CliError::Protocol(error), &mut diagnostics),
    };
    if let Err(error) = transport.set_timeout(timeout::for_command(&cli.command)) {
        return fail(CliError::Protocol(error), &mut diagnostics);
    }
    let mut transport = fallback::RescueFallback::new(&mut transport, direct::direct_response);
    execute_request(&request, &mut transport, &mut output, &mut diagnostics)
}

pub(super) fn next_request_id() -> Result<RequestId, CliError> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CliError::Clock)?
        .as_nanos();
    let sequence = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let raw = format!("slotctl-{}-{timestamp}-{sequence}", std::process::id());
    RequestId::new(&raw).map_err(CliError::Protocol)
}

fn fail<E: Write>(error: CliError, diagnostics: &mut E) -> Result<(), CliError> {
    let _ = writeln!(diagnostics, "slotctl: {error}");
    Err(error)
}
