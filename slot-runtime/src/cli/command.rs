use clap::{Subcommand, ValueEnum};

use crate::domain::{PackageName, SlotId};
use crate::protocol::{Command, Request};
use crate::slot_metadata::{SlotDisplayName, SlotSeedMode};

use super::{CliError, next_request_id};

/// Initial content used when creating one extension slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SeedArgument {
    /// Create empty CE and DE roots.
    Blank,
    /// Copy the immutable Base roots once.
    CloneBase,
}

impl From<SeedArgument> for SlotSeedMode {
    fn from(value: SeedArgument) -> Self {
        match value {
            SeedArgument::Blank => Self::Blank,
            SeedArgument::CloneBase => Self::CloneBase,
        }
    }
}

/// Operations accepted by the Preview control plane.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum CliCommand {
    /// Read exactly one strict protocol frame from standard input.
    Rpc,
    /// Probe runtime and device capabilities.
    Probe,
    /// Inspect one installed user-zero package before enrollment.
    Inspect {
        /// Installed package name selected by the APK.
        package: String,
    },
    /// List all durable managed applications.
    Apps,
    /// Enroll one installed package's immutable Base.
    Enroll {
        /// Installed package name selected by the APK.
        package: String,
    },
    /// Read one managed package's current status.
    Status {
        /// Managed package name.
        package: String,
    },
    /// Create and activate a runtime-named extension slot.
    Create {
        /// Managed package name.
        package: String,
        /// Display-only slot label.
        #[arg(long)]
        name: String,
        /// Initial CE and DE content mode.
        #[arg(long, value_enum)]
        seed: SeedArgument,
    },
    /// List Base and extension slots for one package.
    Slots {
        /// Managed package name.
        package: String,
    },
    /// Switch one managed package to a logical slot.
    Switch {
        /// Managed package name.
        package: String,
        /// Runtime-issued slot identifier.
        slot: String,
    },
    /// Change a slot's display-only label.
    Rename {
        /// Managed package name.
        package: String,
        /// Runtime-issued extension slot identifier.
        slot: String,
        /// Replacement display-only label.
        #[arg(long)]
        name: String,
    },
    /// Delete one inactive extension slot.
    Delete {
        /// Managed package name.
        package: String,
        /// Runtime-issued inactive extension slot identifier.
        slot: String,
    },
    /// Reconcile one package, or every package when omitted.
    Reconcile {
        /// Optional managed package; omission selects all packages.
        package: Option<String>,
    },
    /// Return a package to Base and retire its management state.
    Retire {
        /// Managed package name.
        package: String,
    },
    /// Independently return one package to immutable Base.
    Rescue {
        /// Known package name whose native Base must be restored.
        package: String,
        /// Explicit confirmation for the terminating Base rescue.
        #[arg(long = "to-base", required = true)]
        to_base: bool,
    },
}

impl CliCommand {
    /// Converts validated CLI fields into one strict protocol request.
    pub fn request(&self) -> Result<Request, CliError> {
        let command = match self {
            Self::Rpc => {
                return Err(CliError::InvalidArgument(
                    "rpc requests must be read from standard input".to_owned(),
                ));
            }
            Self::Probe => Command::Probe,
            Self::Inspect { package } => Command::InspectPackage {
                package: parse_package(package)?,
            },
            Self::Apps => Command::ListManagedApps,
            Self::Enroll { package } => Command::EnrollPackage {
                package: parse_package(package)?,
            },
            Self::Status { package } => Command::StatusPackage {
                package: parse_package(package)?,
            },
            Self::Create {
                package,
                name,
                seed,
            } => Command::CreateSlot {
                package: parse_package(package)?,
                display_name: parse_name(name)?,
                seed_mode: (*seed).into(),
            },
            Self::Slots { package } => Command::ListSlots {
                package: parse_package(package)?,
            },
            Self::Switch { package, slot } => Command::Switch {
                package: parse_package(package)?,
                slot: parse_slot(slot)?,
            },
            Self::Rename {
                package,
                slot,
                name,
            } => Command::RenameSlot {
                package: parse_package(package)?,
                slot: parse_slot(slot)?,
                display_name: parse_name(name)?,
            },
            Self::Delete { package, slot } => Command::DeleteSlot {
                package: parse_package(package)?,
                slot: parse_slot(slot)?,
            },
            Self::Reconcile {
                package: Some(package),
            } => Command::ReconcilePackage {
                package: parse_package(package)?,
            },
            Self::Reconcile { package: None } => Command::Reconcile,
            Self::Retire { package } => Command::RetirePackage {
                package: parse_package(package)?,
            },
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
        Request::new(next_request_id()?, command).map_err(CliError::Protocol)
    }
}

fn parse_package(raw: &str) -> Result<PackageName, CliError> {
    PackageName::parse(raw).map_err(|error| CliError::InvalidArgument(error.to_string()))
}

fn parse_slot(raw: &str) -> Result<SlotId, CliError> {
    SlotId::parse(raw).map_err(|error| CliError::InvalidArgument(error.to_string()))
}

fn parse_name(raw: &str) -> Result<SlotDisplayName, CliError> {
    SlotDisplayName::parse(raw).map_err(|error| CliError::InvalidArgument(error.to_string()))
}
