use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::materializer::DataDomain;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixedExecutable {
    Copy,
    Chown,
    Chmod,
    Chcon,
    Getfattr,
    FsProbe,
}

impl FixedExecutable {
    const fn program(self) -> &'static str {
        match self {
            Self::Copy => "/system/bin/cp",
            Self::Chown => "/system/bin/chown",
            Self::Chmod => "/system/bin/chmod",
            Self::Chcon => "/system/bin/chcon",
            Self::Getfattr => "/system/bin/getfattr",
            Self::FsProbe => crate::target::FSPROBE_PATH,
        }
    }
}

#[doc = "One fixed Android materialization operation."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterializerCommandKind {
    #[doc = "Copies one canonical data domain into its staging directory."]
    Copy(DataDomain),
    #[doc = "Applies the exact owner to one staging root."]
    Chown(DataDomain),
    #[doc = "Applies the exact mode to one staging root."]
    Chmod(DataDomain),
    #[doc = "Applies one `SELinux` context recursively."]
    Chcon(DataDomain),
    #[doc = "Reads one root `SELinux` context."]
    GetSelinux(DataDomain),
    #[doc = "Reads one root fscrypt-policy digest."]
    FscryptPolicy(DataDomain),
    #[doc = "Reads available bytes and filesystem identity for one base domain."]
    FreeSpace(DataDomain),
}

#[doc = "Opaque command built only from adapter-derived paths and fixed executables."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializerCommand {
    kind: MaterializerCommandKind,
    executable: FixedExecutable,
    arguments: Vec<OsString>,
}

impl MaterializerCommand {
    const FAST_EXECUTION_TIMEOUT: Duration = Duration::from_secs(5);
    const TREE_EXECUTION_TIMEOUT: Duration = Duration::from_mins(30);

    pub(super) fn copy(domain: DataDomain, source: &Path, target: &Path) -> Self {
        let mut source_contents = PathBuf::from(source);
        source_contents.push(".");
        Self::new(
            MaterializerCommandKind::Copy(domain),
            FixedExecutable::Copy,
            [
                OsString::from("-a"),
                OsString::from("--"),
                source_contents.into_os_string(),
                target.as_os_str().to_owned(),
            ],
        )
    }

    pub(super) fn chown(domain: DataDomain, uid: u32, gid: u32, target: &Path) -> Self {
        Self::new(
            MaterializerCommandKind::Chown(domain),
            FixedExecutable::Chown,
            [
                OsString::from("--"),
                OsString::from(format!("{uid}:{gid}")),
                target.as_os_str().to_owned(),
            ],
        )
    }

    pub(super) fn chmod(domain: DataDomain, mode: u32, target: &Path) -> Self {
        Self::new(
            MaterializerCommandKind::Chmod(domain),
            FixedExecutable::Chmod,
            [
                OsString::from("--"),
                OsString::from(format!("{mode:04o}")),
                target.as_os_str().to_owned(),
            ],
        )
    }

    pub(super) fn chcon(domain: DataDomain, context: &str, target: &Path) -> Self {
        Self::new(
            MaterializerCommandKind::Chcon(domain),
            FixedExecutable::Chcon,
            [
                OsString::from("-R"),
                OsString::from("--"),
                OsString::from(context),
                target.as_os_str().to_owned(),
            ],
        )
    }

    pub(super) fn get_selinux(domain: DataDomain, target: &Path) -> Self {
        Self::new(
            MaterializerCommandKind::GetSelinux(domain),
            FixedExecutable::Getfattr,
            [
                OsString::from("--only-values"),
                OsString::from("-n"),
                OsString::from("security.selinux"),
                target.as_os_str().to_owned(),
            ],
        )
    }

    pub(super) fn fscrypt_policy(domain: DataDomain, target: &Path) -> Self {
        Self::new(
            MaterializerCommandKind::FscryptPolicy(domain),
            FixedExecutable::FsProbe,
            [OsString::from("policy"), target.as_os_str().to_owned()],
        )
    }

    pub(super) fn free_space(domain: DataDomain, target: &Path) -> Self {
        Self::new(
            MaterializerCommandKind::FreeSpace(domain),
            FixedExecutable::FsProbe,
            [OsString::from("space"), target.as_os_str().to_owned()],
        )
    }

    fn new<const N: usize>(
        kind: MaterializerCommandKind,
        executable: FixedExecutable,
        arguments: [OsString; N],
    ) -> Self {
        Self {
            kind,
            executable,
            arguments: Vec::from(arguments),
        }
    }

    #[doc = "Returns the typed operation classification."]
    pub const fn kind(&self) -> MaterializerCommandKind {
        self.kind
    }

    #[doc = "Returns a finite deadline sized for this command's bounded workload."]
    pub const fn execution_timeout(&self) -> Duration {
        match self.kind {
            MaterializerCommandKind::Copy(_) | MaterializerCommandKind::Chcon(_) => {
                Self::TREE_EXECUTION_TIMEOUT
            }
            MaterializerCommandKind::Chown(_)
            | MaterializerCommandKind::Chmod(_)
            | MaterializerCommandKind::GetSelinux(_)
            | MaterializerCommandKind::FscryptPolicy(_)
            | MaterializerCommandKind::FreeSpace(_) => Self::FAST_EXECUTION_TIMEOUT,
        }
    }

    #[doc = "Returns the fixed absolute executable path."]
    pub fn executable(&self) -> &'static Path {
        Path::new(self.executable.program())
    }

    pub(super) const fn program(&self) -> &'static str {
        self.executable.program()
    }

    #[doc = "Returns the argv vector without an executable or shell string."]
    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }
}
