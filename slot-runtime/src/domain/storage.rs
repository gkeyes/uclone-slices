use serde::{Deserialize, Serialize};

use super::{DomainError, PackageName, SlotId};

#[doc = "Android physical user identifier supported by this Preview."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct UserId(u32);

impl UserId {
    #[doc = "The only user supported by the first Preview runtime."]
    pub const PRIMARY: Self = Self(0);

    #[doc = "Returns the numeric Android user id."]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for UserId {
    type Error = DomainError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value == Self::PRIMARY.0 {
            Ok(Self::PRIMARY)
        } else {
            Err(DomainError::UnsupportedUserId(value))
        }
    }
}

impl From<UserId> for u32 {
    fn from(value: UserId) -> Self {
        value.0
    }
}

#[doc = "Validated package and Android-user identity for one managed runtime stream."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageKey {
    pub(super) package_name: PackageName,
    pub(super) user_id: UserId,
}

impl PackageKey {
    #[doc = "Combines a validated package name with the supported Android user."]
    pub const fn new(package_name: PackageName, user_id: UserId) -> Self {
        Self {
            package_name,
            user_id,
        }
    }

    #[doc = "Returns the package name."]
    pub const fn package_name(&self) -> &PackageName {
        &self.package_name
    }

    #[doc = "Returns the Android user id."]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }
}

#[doc = "Non-zero filesystem inode."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct Inode(u64);

impl Inode {
    #[doc = "Constructs a non-zero inode."]
    pub const fn new(value: u64) -> Result<Self, DomainError> {
        if value == 0 {
            Err(DomainError::InvalidInode)
        } else {
            Ok(Self(value))
        }
    }

    #[doc = "Returns the raw inode value."]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for Inode {
    type Error = DomainError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Inode> for u64 {
    fn from(value: Inode) -> Self {
        value.0
    }
}

#[doc = "A matched CE and DE inode pair."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataInodes {
    ce: Inode,
    de: Inode,
}

impl DataInodes {
    #[doc = "Constructs a validated CE/DE pair."]
    pub fn new(ce: u64, de: u64) -> Result<Self, DomainError> {
        Ok(Self {
            ce: Inode::new(ce)?,
            de: Inode::new(de)?,
        })
    }

    #[doc = "Returns the CE inode."]
    pub const fn ce(self) -> Inode {
        self.ce
    }

    #[doc = "Returns the DE inode."]
    pub const fn de(self) -> Inode {
        self.de
    }
}

#[doc = "One named slot and the CE/DE inode pair visible when it is active."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotView {
    pub(super) slot_id: SlotId,
    pub(super) inodes: DataInodes,
}

impl SlotView {
    #[doc = "Combines a validated slot identifier and non-zero inode pair."]
    pub const fn new(slot_id: SlotId, inodes: DataInodes) -> Self {
        Self { slot_id, inodes }
    }

    #[doc = "Returns the slot identifier."]
    pub const fn slot_id(&self) -> &SlotId {
        &self.slot_id
    }

    #[doc = "Returns the slot's CE/DE inode pair."]
    pub const fn inodes(&self) -> DataInodes {
        self.inodes
    }
}
