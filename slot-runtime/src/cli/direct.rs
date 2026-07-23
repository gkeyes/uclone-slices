use std::io::Write;

use crate::android::{AndroidBackend, SystemCommandRunner, SystemPackageProbe};
use crate::daemon::{RequestHandler, RuntimeLock, RuntimeLockError};
use crate::domain::{PackageKey, UserId};
use crate::layout::RuntimeLayout;
use crate::production::SystemMetadataSource;
use crate::protocol::{Command, ErrorCode, Request, Response};
use crate::rescue::OfflineRescuePlatform;

use super::CliError;

#[cfg(test)]
#[path = "direct/tests.rs"]
mod tests;

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
    let mut platform = OfflineRescuePlatform::open_fixed(runtime, SystemMetadataSource::new());
    if let Err(code) = authorize_direct_target(&mut platform, request) {
        return Response::error(request.request_id().clone(), code);
    }
    let mut service = crate::service::PreviewService::new_recovery_only(platform);
    let response = service.handle(request);
    drop(lock);
    response
}

fn authorize_direct_target<B, T, F>(
    platform: &mut OfflineRescuePlatform<B, T, F>,
    request: &Request,
) -> Result<(), ErrorCode> {
    let Command::RescueToBase { package } = request.command() else {
        return Err(ErrorCode::InvalidRequest);
    };
    let key = PackageKey::new(package.clone(), UserId::PRIMARY);
    match platform.authorize_existing_target(&key) {
        Ok(true) => Ok(()),
        Ok(false) => Err(ErrorCode::NotFound),
        Err(_) => Err(ErrorCode::RecoveryRequired),
    }
}

const fn map_lock_error(error: &RuntimeLockError) -> ErrorCode {
    match error {
        RuntimeLockError::Busy => ErrorCode::Busy,
        RuntimeLockError::Invalid | RuntimeLockError::Io(_) => ErrorCode::RecoveryRequired,
    }
}
