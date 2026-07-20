use crate::daemon::RequestHandler;
use crate::domain::{PackageName, SlotId};
use crate::protocol::{Command, Request, Response, ResponsePayload};

use super::{PreviewService, ServiceError, ServicePlatform};

enum Mutation<'a> {
    Enroll(&'a PackageName),
    Switch(&'a PackageName, &'a SlotId),
    Reconcile,
    Rescue(&'a PackageName),
}

impl<P: ServicePlatform> RequestHandler for PreviewService<P> {
    fn handle(&mut self, request: &Request) -> Response {
        let result = match request.command() {
            Command::Probe => self.probe_command(),
            Command::StatusPackage { package } => self.status_command(package),
            Command::EnrollPackage { package } => self.mutate(&Mutation::Enroll(package)),
            Command::Switch { package, slot } => self.mutate(&Mutation::Switch(package, slot)),
            Command::Reconcile => self.mutate(&Mutation::Reconcile),
            Command::RescueToBase { package } => self.mutate(&Mutation::Rescue(package)),
        };
        match result {
            Ok(payload) => {
                Response::ok(request.request_id().clone(), payload).unwrap_or_else(|_| {
                    Response::error(request.request_id().clone(), ServiceError::Internal.code())
                })
            }
            Err(error) => Response::error(request.request_id().clone(), error.code()),
        }
    }
}

impl<P: ServicePlatform> PreviewService<P> {
    fn mutate(&mut self, operation: &Mutation<'_>) -> Result<ResponsePayload, ServiceError> {
        let guard = self.mutations.clone();
        let _permit = guard.try_acquire().map_err(|_| ServiceError::Busy)?;
        match operation {
            Mutation::Enroll(package) => self.enroll_command(package),
            Mutation::Switch(package, slot) => self.switch_command(package, slot),
            Mutation::Reconcile => self.reconcile_command(),
            Mutation::Rescue(package) => self.rescue_command(package),
        }
    }
}
