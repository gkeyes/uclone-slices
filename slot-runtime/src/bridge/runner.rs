use std::env;
use std::io;
use std::process::Command;

use super::{APP_PROCESS_PATH, BRIDGE_CLASSPATH, BridgeCommand, MAX_OUTPUT_BYTES};

mod session;
use session::AppProcessSession;

const ANDROID_ART_ROOT: &str = "/apex/com.android.art";
const ANDROID_I18N_ROOT: &str = "/apex/com.android.i18n";
const ANDROID_TZDATA_ROOT: &str = "/apex/com.android.tzdata";
const BOOTCLASSPATH: &str = "BOOTCLASSPATH";
const DEX2OATBOOTCLASSPATH: &str = "DEX2OATBOOTCLASSPATH";
const MAX_BOOT_CLASSPATH_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedBootEnvironment {
    bootclasspath: String,
    dex2oatbootclasspath: String,
}

impl ValidatedBootEnvironment {
    fn from_daemon_environment() -> Result<Self, io::Error> {
        Self::from_values(
            Some(required_environment_value(BOOTCLASSPATH)?),
            Some(required_environment_value(DEX2OATBOOTCLASSPATH)?),
        )
    }

    fn from_values(
        bootclasspath: Option<String>,
        dex2oatbootclasspath: Option<String>,
    ) -> Result<Self, io::Error> {
        let bootclasspath = validate_classpath(BOOTCLASSPATH, bootclasspath)?;
        let dex2oatbootclasspath = validate_classpath(DEX2OATBOOTCLASSPATH, dex2oatbootclasspath)?;
        Ok(Self {
            bootclasspath,
            dex2oatbootclasspath,
        })
    }

    fn environment_entries(&self) -> [(&'static str, &str); 8] {
        [
            ("CLASSPATH", BRIDGE_CLASSPATH),
            ("ANDROID_ROOT", "/system"),
            ("ANDROID_DATA", "/data"),
            ("ANDROID_ART_ROOT", ANDROID_ART_ROOT),
            ("ANDROID_I18N_ROOT", ANDROID_I18N_ROOT),
            ("ANDROID_TZDATA_ROOT", ANDROID_TZDATA_ROOT),
            (BOOTCLASSPATH, &self.bootclasspath),
            (DEX2OATBOOTCLASSPATH, &self.dex2oatbootclasspath),
        ]
    }

    fn configure(&self, command: &mut Command) {
        command.env_clear().envs(self.environment_entries());
    }
}

fn required_environment_value(name: &str) -> Result<String, io::Error> {
    match env::var(name) {
        Ok(value) => Ok(value),
        Err(env::VarError::NotPresent | env::VarError::NotUnicode(_)) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("missing or invalid {name}"),
        )),
    }
}

fn validate_classpath(name: &str, value: Option<String>) -> Result<String, io::Error> {
    let value = value
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("missing {name}")))?;
    if value.is_empty() || value.len() > MAX_BOOT_CLASSPATH_BYTES {
        return Err(invalid_classpath(name));
    }
    for path in value.split(':') {
        if !is_trusted_path(path) {
            return Err(invalid_classpath(name));
        }
    }
    Ok(value)
}

fn is_trusted_path(path: &str) -> bool {
    if path.is_empty()
        || !path.starts_with('/')
        || path.contains("..")
        || path
            .split('/')
            .skip(1)
            .any(|component| component.is_empty() || component == ".")
        || path.chars().any(|character| {
            !character.is_ascii()
                || character.is_control()
                || character.is_ascii_whitespace()
                || character == '\\'
        })
    {
        return false;
    }
    if path.starts_with("/system/framework/") || path.starts_with("/system_ext/framework/") {
        return path
            .rsplit('/')
            .next()
            .is_some_and(|entry| !entry.is_empty());
    }
    let Some(apex_path) = path.strip_prefix("/apex/") else {
        return false;
    };
    let Some(javalib) = apex_path.find("/javalib/") else {
        return false;
    };
    javalib > 0 && javalib + "/javalib/".len() < apex_path.len()
}

fn invalid_classpath(name: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("invalid {name}: expected trusted framework or APEX paths"),
    )
}

#[doc = "Failures at the fixed app_process execution boundary."]
#[derive(Debug, thiserror::Error)]
pub enum BridgeRunnerError {
    #[doc = "The fixed process could not be spawned, read, or waited on."]
    #[error("fixed app_process unavailable")]
    Io(#[source] io::Error),
    #[doc = "The fixed process returned a non-success exit status."]
    #[error("fixed app_process exited unsuccessfully: {status:?}")]
    NonZeroExit {
        #[doc = "Child exit code, when one was reported by the operating system."]
        status: Option<i32>,
    },
    #[doc = "The child exceeded the bridge response budget."]
    #[error("fixed app_process response exceeded 16 KiB: {size} bytes")]
    OutputTooLarge {
        #[doc = "Number of bytes observed before the bound was enforced."]
        size: usize,
    },
    #[doc = "The fixed child did not finish within the five-second deadline."]
    #[error("fixed app_process timed out after five seconds")]
    TimedOut,
}

#[doc = "Injected typed command runner used by BridgeClient and tests."]
pub trait BridgeCommandRunner: core::fmt::Debug {
    #[doc = "Runs one fixed command and returns its bounded stdout bytes."]
    fn run(&mut self, command: &BridgeCommand) -> Result<Vec<u8>, BridgeRunnerError>;
}

#[doc = "Production runner with a fixed executable and strictly validated daemon classpaths."]
#[derive(Debug, Default)]
pub struct AppProcessRunner {
    session: Option<AppProcessSession>,
}

impl AppProcessRunner {
    #[doc = "Constructs a runner bound to the preview bridge APK."]
    pub const fn new() -> Self {
        Self { session: None }
    }
}

impl BridgeCommandRunner for AppProcessRunner {
    fn run(&mut self, command: &BridgeCommand) -> Result<Vec<u8>, BridgeRunnerError> {
        if self.session.is_none() {
            let environment = ValidatedBootEnvironment::from_daemon_environment()
                .map_err(BridgeRunnerError::Io)?;
            let mut process = Command::new(APP_PROCESS_PATH);
            environment.configure(&mut process);
            self.session = Some(AppProcessSession::spawn(process)?);
        }
        let result = self
            .session
            .as_mut()
            .ok_or_else(|| BridgeRunnerError::Io(io::Error::other("bridge session missing")))?
            .exchange(command);
        if result.is_err() {
            self.session = None;
        }
        result
    }
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
