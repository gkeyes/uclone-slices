#![doc = "Restricted command-line client for the Slots Preview control plane."]

use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};

use crate::domain::{PackageName, SlotId};
use crate::layout::RuntimeLayout;
use crate::protocol::{
    Command, ErrorCode, ProtocolError, Request, RequestId, Response, UnixClient,
};

mod direct;
mod execute;
mod fallback;
mod timeout;

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

/// Operations accepted by the Preview control plane.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum CliCommand {
    /// Probe runtime and device capabilities.
    Probe,
    /// Enroll the allowlisted package's immutable base.
    Enroll {
        /// Android package name.
        package: String,
    },
    /// Read the allowlisted package's current status.
    Status {
        /// Android package name.
        package: String,
    },
    /// Switch the allowlisted package to a logical slot.
    Switch {
        /// Android package name.
        package: String,
        /// Logical slot identifier.
        slot: String,
    },
    /// Reconcile an interrupted durable transaction.
    Reconcile,
    /// Return the allowlisted package to its immutable base.
    Rescue {
        /// Android package name.
        package: String,
        /// Required explicit confirmation of the base-only rescue operation.
        #[arg(long = "to-base", required = true)]
        to_base: bool,
    },
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

impl CliCommand {
    /// Converts the parsed command into one strict, allowlisted protocol request.
    pub fn request(&self) -> Result<Request, CliError> {
        let request_id = next_request_id()?;
        let command = match self {
            Self::Probe => Command::Probe,
            Self::Enroll { package } => Command::EnrollPackage {
                package: parse_package(package)?,
            },
            Self::Status { package } => Command::StatusPackage {
                package: parse_package(package)?,
            },
            Self::Switch { package, slot } => Command::Switch {
                package: parse_package(package)?,
                slot: parse_slot(slot)?,
            },
            Self::Reconcile => Command::Reconcile,
            Self::Rescue { package, to_base } => {
                if !to_base {
                    return Err(CliError::InvalidArgument(
                        "rescue requires --to-base".to_owned(),
                    ));
                }
                Command::RescueToBase {
                    package: parse_package(package)?,
                }
            }
        };
        Request::new(request_id, command).map_err(CliError::Protocol)
    }
}

/// Connects the fixed transport and executes a parsed command on stdio.
pub fn run(cli: &Cli) -> Result<(), CliError> {
    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut output = stdout.lock();
    let mut diagnostics = stderr.lock();
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

fn next_request_id() -> Result<RequestId, CliError> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CliError::Clock)?
        .as_nanos();
    let sequence = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let raw = format!("slotctl-{}-{timestamp}-{sequence}", std::process::id());
    RequestId::new(&raw).map_err(CliError::Protocol)
}

fn parse_package(raw: &str) -> Result<PackageName, CliError> {
    PackageName::parse(raw).map_err(|error| CliError::InvalidArgument(error.to_string()))
}

fn parse_slot(raw: &str) -> Result<SlotId, CliError> {
    SlotId::parse(raw).map_err(|error| CliError::InvalidArgument(error.to_string()))
}

fn fail<E: Write>(error: CliError, diagnostics: &mut E) -> Result<(), CliError> {
    let _ = writeln!(diagnostics, "slotctl: {error}");
    Err(error)
}
