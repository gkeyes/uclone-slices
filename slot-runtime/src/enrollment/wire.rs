use serde::de::Deserializer;
use serde::{Deserialize, Serialize};

use crate::domain::{
    AppIdentity, DataInodes, ManagedPackage, PackageKey, PackageName, SlotId, SlotView, UserId,
};
use crate::integrity::digest_json;
use crate::lifecycle::LifecycleState;

use super::{EnrollmentError, SCHEMA_VERSION, validate_base};

#[derive(Debug, Serialize)]
pub(super) struct EnrollmentRecord {
    pub(super) schema_version: u32,
    pub(super) managed: ManagedPackage,
    pub(super) sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnrollmentWire {
    schema_version: u32,
    managed: ManagedWire,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagedWire {
    package_name: String,
    user_id: u32,
    identity: IdentityWire,
    base_inodes: InodesWire,
    active_slot: String,
    active_inodes: InodesWire,
    lifecycle_state: LifecycleState,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityWire {
    uid: u32,
    signature_sha256: String,
    version_code: u64,
    code_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InodesWire {
    ce: u64,
    de: u64,
}

impl<'de> Deserialize<'de> for EnrollmentRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = EnrollmentWire::deserialize(deserializer)?;
        Self::try_from(wire).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<EnrollmentWire> for EnrollmentRecord {
    type Error = String;

    fn try_from(value: EnrollmentWire) -> Result<Self, Self::Error> {
        Ok(Self {
            schema_version: value.schema_version,
            managed: value.managed.try_into()?,
            sha256: value.sha256,
        })
    }
}

impl TryFrom<ManagedWire> for ManagedPackage {
    type Error = String;

    fn try_from(value: ManagedWire) -> Result<Self, Self::Error> {
        let package_name =
            PackageName::parse(&value.package_name).map_err(|error| error.to_string())?;
        let user_id = UserId::try_from(value.user_id).map_err(|error| error.to_string())?;
        let identity = AppIdentity::new(
            value.identity.uid,
            &value.identity.signature_sha256,
            value.identity.version_code,
            &value.identity.code_path,
        )
        .map_err(|error| error.to_string())?;
        let base = DataInodes::new(value.base_inodes.ce, value.base_inodes.de)
            .map_err(|error| error.to_string())?;
        let active = DataInodes::new(value.active_inodes.ce, value.active_inodes.de)
            .map_err(|error| error.to_string())?;
        let slot = SlotId::parse(&value.active_slot).map_err(|error| error.to_string())?;
        Self::new(
            PackageKey::new(package_name, user_id),
            identity,
            base,
            SlotView::new(slot, active),
            value.lifecycle_state,
        )
        .map_err(|error| error.to_string())
    }
}

impl EnrollmentRecord {
    pub(super) fn verify(&self) -> Result<(), EnrollmentError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(EnrollmentError::Corrupt(
                "unsupported schema version".to_owned(),
            ));
        }
        validate_base(&self.managed).map_err(EnrollmentError::Corrupt)?;
        if self.digest()? != self.sha256 {
            return Err(EnrollmentError::Corrupt("digest mismatch".to_owned()));
        }
        Ok(())
    }

    pub(super) fn digest(&self) -> Result<String, EnrollmentError> {
        Ok(digest_json(&UnsignedEnrollment {
            schema_version: self.schema_version,
            managed: &self.managed,
        })?)
    }
}

#[derive(Serialize)]
struct UnsignedEnrollment<'a> {
    schema_version: u32,
    managed: &'a ManagedPackage,
}
