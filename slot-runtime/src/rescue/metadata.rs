use crate::domain::{BootId, CommitNonce};

use super::{RescueError, RescueId};

#[doc = "Fresh identifiers captured before a new rescue epoch is prepared."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RescueMetadata {
    rescue_id: RescueId,
    commit_nonce: CommitNonce,
    boot_id: BootId,
}

impl RescueMetadata {
    #[doc = "Groups validated rescue identifiers from a fixed system source."]
    pub const fn new(rescue_id: RescueId, commit_nonce: CommitNonce, boot_id: BootId) -> Self {
        Self {
            rescue_id,
            commit_nonce,
            boot_id,
        }
    }

    #[doc = "Returns the create-once rescue identifier."]
    pub const fn rescue_id(&self) -> &RescueId {
        &self.rescue_id
    }

    #[doc = "Returns the native-base commit nonce."]
    pub const fn commit_nonce(&self) -> &CommitNonce {
        &self.commit_nonce
    }

    #[doc = "Returns the boot in which the rescue was first prepared."]
    pub const fn boot_id(&self) -> &BootId {
        &self.boot_id
    }
}

#[doc = "Fixed-source boundary for fresh rescue identifiers."]
pub trait RescueMetadataSource: core::fmt::Debug {
    #[doc = "Returns one fresh bounded metadata set without caller input."]
    fn next_rescue(&mut self) -> Result<RescueMetadata, RescueError>;
}

impl<T> RescueMetadataSource for &mut T
where
    T: RescueMetadataSource + ?Sized,
{
    fn next_rescue(&mut self) -> Result<RescueMetadata, RescueError> {
        (**self).next_rescue()
    }
}
