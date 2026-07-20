use std::io::Write;

use crate::android::{AndroidBackend, SystemCommandRunner, SystemPackageProbe};
use crate::daemon::{RequestHandler, RuntimeLock, RuntimeLockError};
use crate::layout::RuntimeLayout;
use crate::production::SystemMetadataSource;
use crate::protocol::{Command, ErrorCode, Request, Response};
use crate::rescue::OfflineRescuePlatform;

use super::CliError;

pub(super) fn run<O, E>(
    request: &Request,
    output: &mut O,
    diagnostics: &mut E,
) -> Result<(), CliError>
where
    O: Write,
    E: Write,
{
    super::execute::write_response(request, &direct_response(request), output, diagnostics)
}

pub(super) fn direct_response(request: &Request) -> Response {
    if !matches!(request.command(), Command::RescueToBase { .. }) {
        return Response::error(request.request_id().clone(), ErrorCode::InvalidRequest);
    }
    let lock = match RuntimeLock::acquire(RuntimeLayout::lock(), "slotctl") {
        Ok(lock) => lock,
        Err(error) => {
            return Response::error(request.request_id().clone(), map_lock_error(&error));
        }
    };
    let runtime = AndroidBackend::new(SystemCommandRunner::new(), SystemPackageProbe::new());
    let platform = OfflineRescuePlatform::open_fixed(runtime, SystemMetadataSource::new());
    let mut service = crate::service::PreviewService::new(platform);
    let response = service.handle(request);
    drop(lock);
    response
}

const fn map_lock_error(error: &RuntimeLockError) -> ErrorCode {
    match error {
        RuntimeLockError::Busy => ErrorCode::Busy,
        RuntimeLockError::Invalid | RuntimeLockError::Io(_) => ErrorCode::RecoveryRequired,
    }
}
