use serde::{Deserialize, Serialize};

use crate::domain::{AppIdentity, DataInodes, PackageKey, PackageName, SlotId, UserId};
use crate::integrity::digest_json;

use super::{CatalogEntry, CatalogError, PathSecurityProof, SecurityProfileProof};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    package_key: PackageWire,
    slot_id: String,
    data_inodes: InodesWire,
    enrolled_identity: IdentityWire,
    created_version_code: u64,
    security_profile: SecurityWire,
    sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageWire {
    package_name: String,
    user_id: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InodesWire {
    ce: u64,
    de: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityWire {
    uid: u32,
    signature_sha256: String,
    version_code: u64,
    code_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SecurityWire {
    ce: PathSecurityWire,
    de: PathSecurityWire,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PathSecurityWire {
    uid: u32,
    gid: u32,
    mode: u32,
    selinux_context: String,
    fscrypt_policy_sha256: String,
}

#[derive(Serialize)]
struct UnsignedManifest<'a> {
    schema_version: u32,
    package_key: &'a PackageWire,
    slot_id: &'a str,
    data_inodes: &'a InodesWire,
    enrolled_identity: &'a IdentityWire,
    created_version_code: u64,
    security_profile: &'a SecurityWire,
}

pub(super) fn encode(entry: &CatalogEntry) -> Result<Vec<u8>, CatalogError> {
    let mut manifest = Manifest::from_entry(entry);
    manifest.sha256 = manifest.digest()?;
    Ok(serde_json::to_vec(&manifest)?)
}

pub(super) fn decode(bytes: &[u8]) -> Result<CatalogEntry, CatalogError> {
    let manifest = serde_json::from_slice::<Manifest>(bytes)
        .map_err(|source| CatalogError::Corrupt(format!("invalid manifest JSON: {source}")))?;
    if manifest.schema_version != SCHEMA_VERSION {
        return Err(CatalogError::Corrupt(
            "unsupported schema version".to_owned(),
        ));
    }
    let expected = manifest
        .digest()
        .map_err(|source| CatalogError::Corrupt(format!("cannot hash manifest: {source}")))?;
    if expected != manifest.sha256 {
        return Err(CatalogError::Corrupt("digest mismatch".to_owned()));
    }
    manifest.into_entry()
}

impl Manifest {
    fn from_entry(entry: &CatalogEntry) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            package_key: PackageWire::from(entry.package_key()),
            slot_id: entry.slot_id().as_str().to_owned(),
            data_inodes: InodesWire::from(entry.inodes()),
            enrolled_identity: IdentityWire::from(entry.enrolled_identity()),
            created_version_code: entry.created_version_code(),
            security_profile: SecurityWire::from(entry.security_profile()),
            sha256: String::new(),
        }
    }

    fn digest(&self) -> Result<String, serde_json::Error> {
        digest_json(&UnsignedManifest {
            schema_version: self.schema_version,
            package_key: &self.package_key,
            slot_id: &self.slot_id,
            data_inodes: &self.data_inodes,
            enrolled_identity: &self.enrolled_identity,
            created_version_code: self.created_version_code,
            security_profile: &self.security_profile,
        })
    }

    fn into_entry(self) -> Result<CatalogEntry, CatalogError> {
        let package_name = PackageName::parse(&self.package_key.package_name)
            .map_err(|source| corrupt_domain("package name", source))?;
        let user_id = UserId::try_from(self.package_key.user_id)
            .map_err(|source| corrupt_domain("user id", source))?;
        let slot_id =
            SlotId::parse(&self.slot_id).map_err(|source| corrupt_domain("slot id", source))?;
        let inodes = DataInodes::new(self.data_inodes.ce, self.data_inodes.de)
            .map_err(|source| corrupt_domain("data inodes", source))?;
        let identity = AppIdentity::new(
            self.enrolled_identity.uid,
            &self.enrolled_identity.signature_sha256,
            self.enrolled_identity.version_code,
            &self.enrolled_identity.code_path,
        )
        .map_err(|source| corrupt_domain("app identity", source))?;
        let profile = self.security_profile.into_model()?;
        CatalogEntry::new(
            PackageKey::new(package_name, user_id),
            slot_id,
            inodes,
            identity,
            self.created_version_code,
            profile,
        )
        .map_err(|source| CatalogError::Corrupt(source.to_string()))
    }
}

impl From<&PackageKey> for PackageWire {
    fn from(value: &PackageKey) -> Self {
        Self {
            package_name: value.package_name().as_str().to_owned(),
            user_id: value.user_id().get(),
        }
    }
}

impl From<DataInodes> for InodesWire {
    fn from(value: DataInodes) -> Self {
        Self {
            ce: value.ce().get(),
            de: value.de().get(),
        }
    }
}

impl From<&AppIdentity> for IdentityWire {
    fn from(value: &AppIdentity) -> Self {
        Self {
            uid: value.uid(),
            signature_sha256: value.signature_sha256().to_owned(),
            version_code: value.version_code(),
            code_path: value.code_path().to_owned(),
        }
    }
}

impl From<&SecurityProfileProof> for SecurityWire {
    fn from(value: &SecurityProfileProof) -> Self {
        Self {
            ce: PathSecurityWire::from(value.ce()),
            de: PathSecurityWire::from(value.de()),
        }
    }
}

impl From<&PathSecurityProof> for PathSecurityWire {
    fn from(value: &PathSecurityProof) -> Self {
        Self {
            uid: value.uid(),
            gid: value.gid(),
            mode: value.mode(),
            selinux_context: value.selinux_context().to_owned(),
            fscrypt_policy_sha256: value.fscrypt_policy_sha256().to_owned(),
        }
    }
}

impl SecurityWire {
    fn into_model(self) -> Result<SecurityProfileProof, CatalogError> {
        Ok(SecurityProfileProof::new(
            self.ce.into_model()?,
            self.de.into_model()?,
        ))
    }
}

impl PathSecurityWire {
    fn into_model(self) -> Result<PathSecurityProof, CatalogError> {
        PathSecurityProof::new(
            self.uid,
            self.gid,
            self.mode,
            &self.selinux_context,
            &self.fscrypt_policy_sha256,
        )
        .map_err(|source| CatalogError::Corrupt(source.to_string()))
    }
}

fn corrupt_domain(label: &str, source: impl std::fmt::Display) -> CatalogError {
    CatalogError::Corrupt(format!("invalid {label}: {source}"))
}
