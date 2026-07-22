#![doc = "Host-testable Unix socket server for the root `ucloned` runtime."]

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::layout::RuntimeLayout;
use crate::protocol::{ErrorCode, ProtocolError, Request, Response, decode_request};

mod connection;
mod lock;
mod socket;

use connection::{invalid_request_response, read_frame, write_response};
pub use lock::{RuntimeLock, RuntimeLockError};
use socket::prepare_socket_path;

/// Default per-connection read and write timeout.
pub const DEFAULT_IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Errors raised while creating or serving the daemon socket.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    /// The socket path has no usable parent directory.
    #[error("socket path has no parent directory: {0}")]
    InvalidSocketPath(PathBuf),
    /// A socket parent or socket path is a symbolic link.
    #[error("socket path component must not be a symlink: {0}")]
    SymlinkPath(PathBuf),
    /// The socket parent exists but is not a directory.
    #[error("socket parent is not a directory: {0}")]
    ParentNotDirectory(PathBuf),
    /// An existing path is not a Unix socket and cannot be replaced.
    #[error("existing socket path is not a Unix socket: {0}")]
    NonSocketPath(PathBuf),
    /// An existing Unix socket is still accepting connections.
    #[error("Unix socket is already in use: {0}")]
    SocketInUse(PathBuf),
    /// A filesystem or stream operation failed.
    #[error("daemon I/O: {0}")]
    Io(#[from] io::Error),
    /// Encoding a response failed at the protocol boundary.
    #[error("daemon protocol: {0}")]
    Protocol(#[from] ProtocolError),
    /// A zero-length timeout cannot be applied to a Unix stream.
    #[error("daemon I/O timeout must be non-zero")]
    InvalidTimeout,
    #[doc = "The fixed daemon/rescue runtime lock could not be acquired."]
    #[error("runtime lock unavailable")]
    RuntimeLock,
}

/// Dispatch boundary owned by the runtime implementation.
pub trait RequestHandler {
    /// Handles one already-decoded request and returns its complete response.
    fn handle(&mut self, request: &Request) -> Response;
}

impl<F> RequestHandler for F
where
    F: FnMut(&Request) -> Response,
{
    fn handle(&mut self, request: &Request) -> Response {
        self(request)
    }
}

/// A non-blocking mutation gate shared by cloned request handlers.
#[derive(Clone, Debug, Default)]
pub struct MutationGuard(Arc<Mutex<()>>);

impl MutationGuard {
    /// Creates an independent mutation gate.
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempts to reserve the gate, returning `Busy` instead of waiting.
    pub fn try_lock(&self) -> Result<MutationPermit<'_>, ErrorCode> {
        self.0
            .try_lock()
            .map(|guard| MutationPermit { _guard: guard })
            .map_err(|_| ErrorCode::Busy)
    }

    /// Alias for [`Self::try_lock`] suitable for mutation handlers.
    pub fn try_acquire(&self) -> Result<MutationPermit<'_>, ErrorCode> {
        self.try_lock()
    }
}

/// A held mutation reservation. Dropping it releases the global gate.
#[derive(Debug)]
pub struct MutationPermit<'a> {
    _guard: MutexGuard<'a, ()>,
}

/// Synchronous one-request/one-response Unix socket server.
#[derive(Debug)]
pub struct DaemonServer<H> {
    listener: UnixListener,
    socket_path: PathBuf,
    handler: H,
    timeout: Duration,
    _runtime_lock: RuntimeLock,
}

impl<H: RequestHandler> DaemonServer<H> {
    /// Binds the production socket derived from [`RuntimeLayout::socket`].
    pub fn bind(handler: H) -> Result<Self, DaemonError> {
        let socket_path = RuntimeLayout::socket().to_path_buf();
        prepare_socket_path(&socket_path, true)?;
        let runtime_lock = RuntimeLock::acquire(RuntimeLayout::lock(), "ucloned")
            .map_err(|_| DaemonError::RuntimeLock)?;
        prepare_socket_path(&socket_path, true)?;
        Self::bind_prepared(socket_path, handler, runtime_lock)
    }

    /// Binds the production socket while retaining an already-acquired startup lock.
    pub fn bind_with_lock(handler: H, runtime_lock: RuntimeLock) -> Result<Self, DaemonError> {
        let socket_path = RuntimeLayout::socket().to_path_buf();
        prepare_socket_path(&socket_path, true)?;
        Self::bind_prepared(socket_path, handler, runtime_lock)
    }

    /// Binds an injected socket path for host-side tests.
    pub fn bind_at(path: impl AsRef<Path>, handler: H) -> Result<Self, DaemonError> {
        let socket_path = path.as_ref().to_path_buf();
        prepare_socket_path(&socket_path, false)?;
        let lock_path = socket_path.with_extension("lock");
        let runtime_lock =
            RuntimeLock::acquire(&lock_path, "ucloned").map_err(|_| DaemonError::RuntimeLock)?;
        Self::bind_prepared(socket_path, handler, runtime_lock)
    }

    fn bind_prepared(
        socket_path: PathBuf,
        handler: H,
        runtime_lock: RuntimeLock,
    ) -> Result<Self, DaemonError> {
        let listener = UnixListener::bind(&socket_path)?;
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))?;
        Ok(Self {
            listener,
            socket_path,
            handler,
            timeout: DEFAULT_IO_TIMEOUT,
            _runtime_lock: runtime_lock,
        })
    }

    /// Sets a non-zero timeout for each accepted stream.
    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self, DaemonError> {
        validate_timeout(timeout)?;
        self.timeout = timeout;
        Ok(self)
    }

    /// Updates the timeout on an existing server.
    pub fn set_timeout(&mut self, timeout: Duration) -> Result<(), DaemonError> {
        validate_timeout(timeout)?;
        self.timeout = timeout;
        Ok(())
    }

    /// Returns the path this server bound.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Accepts and serves connections until an accept or stream error occurs.
    pub fn run(&mut self) -> Result<(), DaemonError> {
        loop {
            let (stream, _) = self.listener.accept()?;
            match self.serve_stream(stream) {
                Ok(()) => {}
                Err(error) if is_connection_error(&error) => {}
                Err(error) => return Err(error),
            }
        }
    }

    /// Accepts and serves exactly one connection.
    pub fn run_once(&mut self) -> Result<(), DaemonError> {
        let (stream, _) = self.listener.accept()?;
        self.serve_stream(stream)
    }

    /// Serves one already-connected stream and then closes it by dropping it.
    pub fn serve_stream(&mut self, mut stream: UnixStream) -> Result<(), DaemonError> {
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;
        let Some(frame) = read_frame(&mut stream)? else {
            return Ok(());
        };
        match decode_request(&frame) {
            Ok(request) => {
                let response = self.handler.handle(&request);
                let response = if response.request_id() == request.request_id() {
                    response
                } else {
                    Response::error(request.request_id().clone(), ErrorCode::Internal)
                };
                write_response(&mut stream, &response)?;
            }
            Err(error) => {
                let code = if matches!(error, ProtocolError::RuntimePairMismatch) {
                    ErrorCode::RuntimePairMismatch
                } else {
                    ErrorCode::InvalidRequest
                };
                if let Some(response) = invalid_request_response(&frame, code) {
                    write_response(&mut stream, &response)?;
                }
            }
        }
        Ok(())
    }
}

const fn validate_timeout(timeout: Duration) -> Result<(), DaemonError> {
    if timeout.is_zero() {
        Err(DaemonError::InvalidTimeout)
    } else {
        Ok(())
    }
}

fn is_connection_error(error: &DaemonError) -> bool {
    matches!(
        error,
        DaemonError::Io(io_error)
            if matches!(
                io_error.kind(),
                io::ErrorKind::BrokenPipe
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::NotConnected
                    | io::ErrorKind::TimedOut
                    | io::ErrorKind::UnexpectedEof
                    | io::ErrorKind::WouldBlock
            )
    )
}
