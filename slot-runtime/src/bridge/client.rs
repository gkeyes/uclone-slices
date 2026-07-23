use super::client_validation::{
    map_runner_error, payload_name, require_ack, require_package, require_user,
    validate_package_snapshot,
};
use super::{
    ALLOWED_USER_ID, BridgeCommand, BridgeError, BridgeErrorCode, BridgePayload, BridgeResponse,
    DeviceSnapshot, MAX_OUTPUT_BYTES, PackageEnabledState, PackageSnapshot,
};
use crate::domain::{AppIdentity, DataInodes};

/// Typed client that validates every request and response around an injected runner.
#[derive(Debug)]
pub struct BridgeClient<R> {
    runner: R,
}

impl<R: super::BridgeCommandRunner> BridgeClient<R> {
    /// Wraps a production or fake typed runner.
    pub const fn new(runner: R) -> Self {
        Self { runner }
    }

    /// Reads the fixed user-0 device unlock state.
    pub fn query_device(&mut self, user_id: u32) -> Result<DeviceSnapshot, BridgeError> {
        require_user(user_id)?;
        match self.execute(&BridgeCommand::DeviceStatus)? {
            BridgePayload::Device(snapshot) if snapshot.user_id() == ALLOWED_USER_ID => {
                Ok(snapshot)
            }
            BridgePayload::Device(_) => Err(BridgeError::new(
                BridgeErrorCode::UserNotAllowed,
                "device response userId is not user 0",
            )),
            _ => Err(BridgeError::new(
                BridgeErrorCode::InvalidResponse,
                "device command returned the wrong payload type",
            )),
        }
    }

    #[doc = "Alias for query_device using the bridge operation's status naming."]
    pub fn device_status(&mut self, user_id: u32) -> Result<DeviceSnapshot, BridgeError> {
        self.query_device(user_id)
    }

    /// Reads the fixed package's user-0 PackageManager state.
    pub fn query_package(
        &mut self,
        package: &str,
        user_id: u32,
    ) -> Result<PackageSnapshot, BridgeError> {
        let package = require_package(package)?;
        require_user(user_id)?;
        match self.execute(&BridgeCommand::PackageStatus(package.clone()))? {
            BridgePayload::Package(snapshot) => validate_package_snapshot(snapshot, &package),
            _ => Err(BridgeError::new(
                BridgeErrorCode::InvalidResponse,
                "package command returned the wrong payload type",
            )),
        }
    }

    #[doc = "Alias for query_package using the bridge operation's status naming."]
    pub fn package_status(
        &mut self,
        package: &str,
        user_id: u32,
    ) -> Result<PackageSnapshot, BridgeError> {
        self.query_package(package, user_id)
    }

    /// Reads only the fixed package's enabled and suspended state.
    pub fn query_gate(
        &mut self,
        package: &str,
        user_id: u32,
    ) -> Result<crate::domain::GateSnapshot, BridgeError> {
        let package = require_package(package)?;
        require_user(user_id)?;
        match self.execute(&BridgeCommand::GateStatus(package))? {
            BridgePayload::Gate(snapshot) => Ok(snapshot.into_gate_snapshot()),
            _ => Err(BridgeError::new(
                BridgeErrorCode::InvalidResponse,
                "gate command returned the wrong payload type",
            )),
        }
    }

    /// Acquires the user-zero gate by setting only `DisabledUser`.
    pub fn set_enabled(
        &mut self,
        package: &str,
        user_id: u32,
        state: PackageEnabledState,
    ) -> Result<(), BridgeError> {
        if state != PackageEnabledState::DisabledUser {
            return Err(BridgeError::new(
                BridgeErrorCode::InvalidRequest,
                "unchecked enabled mutation may only acquire DisabledUser",
            ));
        }
        let package = require_package(package)?;
        require_user(user_id)?;
        let payload = self.execute(&BridgeCommand::SetEnabled(package, state))?;
        require_ack(&payload)
    }

    /// Restores enabled state only when the complete enrolled contract still matches.
    pub fn restore_enabled(
        &mut self,
        package: &str,
        user_id: u32,
        state: PackageEnabledState,
        expected_identity: &AppIdentity,
        expected_base_inodes: DataInodes,
    ) -> Result<(), BridgeError> {
        let package = require_package(package)?;
        require_user(user_id)?;
        let payload = self.execute(&BridgeCommand::RestoreEnabled {
            package,
            state,
            expected_identity: expected_identity.clone(),
            expected_base_inodes,
        })?;
        require_ack(&payload)
    }

    /// Restores suspension only when the complete enrolled contract still matches.
    pub fn restore_suspended(
        &mut self,
        package: &str,
        user_id: u32,
        suspended: bool,
        expected_identity: &AppIdentity,
        expected_base_inodes: DataInodes,
    ) -> Result<(), BridgeError> {
        let package = require_package(package)?;
        require_user(user_id)?;
        let payload = self.execute(&BridgeCommand::RestoreSuspended {
            package,
            suspended,
            expected_identity: expected_identity.clone(),
            expected_base_inodes,
        })?;
        require_ack(&payload)
    }

    /// Opens the validated package through its system-resolved user-zero launcher entry.
    pub fn launch_package(
        &mut self,
        package: &str,
        user_id: u32,
        expected_identity: &AppIdentity,
        expected_base_inodes: DataInodes,
    ) -> Result<(), BridgeError> {
        let package = require_package(package)?;
        require_user(user_id)?;
        let payload = self.execute(&BridgeCommand::LaunchPackage {
            package,
            expected_identity: expected_identity.clone(),
            expected_base_inodes,
        })?;
        require_ack(&payload)
    }

    /// Returns the wrapped runner after the client is no longer needed.
    pub fn into_inner(self) -> R {
        self.runner
    }

    fn execute(&mut self, command: &BridgeCommand) -> Result<BridgePayload, BridgeError> {
        let bytes = self
            .runner
            .run(command)
            .map_err(|error| map_runner_error(&error))?;
        if bytes.len() > MAX_OUTPUT_BYTES {
            return Err(BridgeError::new(
                BridgeErrorCode::ResponseTooLarge,
                format!("bridge response is {} bytes", bytes.len()),
            ));
        }
        let response = BridgeResponse::from_bytes(&bytes)?;
        response.require_compatible(
            crate::protocol::RUNTIME_BUILD_ID,
            command.allows_legacy_v1(),
        )?;
        if response.request_id() != command.request_id() {
            return Err(BridgeError::new(
                BridgeErrorCode::RequestMismatch,
                format!("expected requestId {}", command.request_id()),
            ));
        }
        if !response.is_ok() {
            let code = response.error_code().ok_or_else(|| {
                BridgeError::new(BridgeErrorCode::InvalidResponse, "missing errorCode")
            })?;
            return Err(BridgeError::new(code, "bridge command rejected"));
        }
        let payload = response
            .payload()
            .cloned()
            .ok_or_else(|| BridgeError::new(BridgeErrorCode::InvalidResponse, "missing payload"))?;
        if payload_name(&payload) != command.payload_name() {
            return Err(BridgeError::new(
                BridgeErrorCode::InvalidResponse,
                "response payload type does not match command",
            ));
        }
        Ok(payload)
    }
}
