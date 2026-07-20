use super::{
    ALLOWED_PACKAGE, ALLOWED_USER_ID, BridgeCommand, BridgeError, BridgeErrorCode, BridgePayload,
    BridgeResponse, BridgeRunnerError, DeviceSnapshot, MAX_OUTPUT_BYTES, PackageEnabledState,
    PackageSnapshot,
};

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
        match self.execute(BridgeCommand::DeviceStatus)? {
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
        require_package(package)?;
        require_user(user_id)?;
        match self.execute(BridgeCommand::PackageStatus)? {
            BridgePayload::Package(snapshot) => validate_package_snapshot(snapshot),
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
        require_package(package)?;
        require_user(user_id)?;
        match self.execute(BridgeCommand::GateStatus)? {
            BridgePayload::Gate(snapshot) => Ok(snapshot.into_gate_snapshot()),
            _ => Err(BridgeError::new(
                BridgeErrorCode::InvalidResponse,
                "gate command returned the wrong payload type",
            )),
        }
    }

    /// Sets the fixed package's enabled-state enum for user 0.
    pub fn set_enabled(
        &mut self,
        package: &str,
        user_id: u32,
        state: PackageEnabledState,
    ) -> Result<(), BridgeError> {
        require_package(package)?;
        require_user(user_id)?;
        let payload = self.execute(BridgeCommand::SetEnabled(state))?;
        require_ack(&payload)
    }

    /// Sets the fixed package's suspended state for user 0.
    pub fn set_suspended(
        &mut self,
        package: &str,
        user_id: u32,
        suspended: bool,
    ) -> Result<(), BridgeError> {
        require_package(package)?;
        require_user(user_id)?;
        let payload = self.execute(BridgeCommand::SetSuspended(suspended))?;
        require_ack(&payload)
    }

    /// Returns the wrapped runner after the client is no longer needed.
    pub fn into_inner(self) -> R {
        self.runner
    }

    fn execute(&mut self, command: BridgeCommand) -> Result<BridgePayload, BridgeError> {
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

fn require_package(package: &str) -> Result<(), BridgeError> {
    if package == ALLOWED_PACKAGE {
        Ok(())
    } else {
        Err(BridgeError::new(
            BridgeErrorCode::PackageNotAllowed,
            "package is outside the fixed allowlist",
        ))
    }
}

fn require_user(user_id: u32) -> Result<(), BridgeError> {
    if user_id == ALLOWED_USER_ID {
        Ok(())
    } else {
        Err(BridgeError::new(
            BridgeErrorCode::UserNotAllowed,
            "only Android user 0 is supported",
        ))
    }
}

fn validate_package_snapshot(snapshot: PackageSnapshot) -> Result<PackageSnapshot, BridgeError> {
    require_package(snapshot.package_name())?;
    require_user(snapshot.user_id())?;
    Ok(snapshot)
}

fn require_ack(payload: &BridgePayload) -> Result<(), BridgeError> {
    if matches!(payload, BridgePayload::Ack(_)) {
        Ok(())
    } else {
        Err(BridgeError::new(
            BridgeErrorCode::InvalidResponse,
            "mutation returned the wrong payload type",
        ))
    }
}

const fn payload_name(payload: &BridgePayload) -> &'static str {
    match payload {
        BridgePayload::Device(_) => "device",
        BridgePayload::Package(_) => "package",
        BridgePayload::Gate(_) => "gate",
        BridgePayload::Ack(_) => "ack",
    }
}

fn map_runner_error(error: &BridgeRunnerError) -> BridgeError {
    match error {
        BridgeRunnerError::Io(_) => BridgeError::new(
            BridgeErrorCode::RunnerUnavailable,
            "fixed app_process could not be executed",
        ),
        BridgeRunnerError::NonZeroExit { .. } => BridgeError::new(
            BridgeErrorCode::CommandFailed,
            "fixed app_process returned a non-zero status",
        ),
        BridgeRunnerError::TimedOut => BridgeError::new(
            BridgeErrorCode::TimedOut,
            "fixed app_process exceeded the five-second deadline",
        ),
        BridgeRunnerError::OutputTooLarge { size } => BridgeError::new(
            BridgeErrorCode::ResponseTooLarge,
            format!("fixed app_process response is {size} bytes"),
        ),
    }
}
