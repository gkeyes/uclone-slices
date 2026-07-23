use crate::domain::ManagedPackage;

/// Result of asking Android to open the committed package front door.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchDisposition {
    /// Android accepted the fixed launcher activity request.
    Launched,
    /// The package has no enabled INFO or LAUNCHER activity for user zero.
    EntryNotFound,
    /// `PackageManager` no longer reports the identity enrolled by the runtime.
    IdentityChanged,
    /// Package metadata, Base anchors, or install-session state changed.
    PackageStateChanged,
    /// The fixed launch bridge failed after the data view was already committed.
    Failed,
}

/// Platform boundary used only after a slot commit has been revalidated.
pub trait AppLaunchBackend: core::fmt::Debug {
    /// Opens the package's system-resolved launcher activity without caller intents.
    fn launch_package(&mut self, package: &ManagedPackage) -> LaunchDisposition;
}

impl<T> AppLaunchBackend for &mut T
where
    T: AppLaunchBackend + ?Sized,
{
    fn launch_package(&mut self, package: &ManagedPackage) -> LaunchDisposition {
        (**self).launch_package(package)
    }
}
