use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use super::{BridgeError, PackageEnabledState, invalid_response};

#[doc = "PackageManager facts returned by the fixed bridge."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageSnapshot {
    package_name: String,
    user_id: u32,
    uid: u32,
    signature_sha256: String,
    version_code: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    version_name: Option<String>,
    code_path: String,
    ce_data_path: String,
    de_data_path: String,
    package_manager_ce_inode: u64,
    package_manager_de_inode: u64,
    enabled_state: PackageEnabledState,
    suspended: bool,
    pending_install: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackageSnapshotWire {
    package_name: String,
    user_id: u32,
    uid: u32,
    signature_sha256: String,
    version_code: u64,
    #[serde(default)]
    version_name: Option<String>,
    code_path: String,
    ce_data_path: String,
    de_data_path: String,
    package_manager_ce_inode: u64,
    package_manager_de_inode: u64,
    enabled_state: PackageEnabledState,
    suspended: bool,
    pending_install: bool,
}

impl<'de> Deserialize<'de> for PackageSnapshot {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_wire(PackageSnapshotWire::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

impl PackageSnapshot {
    #[doc = "Constructs and validates package facts from the bridge boundary."]
    #[allow(clippy::too_many_arguments)]
    #[allow(
        clippy::similar_names,
        reason = "CE and DE inode arguments are an intentional paired storage fact"
    )]
    pub fn new(
        package_name: &str,
        user_id: u32,
        uid: u32,
        signature_sha256: &str,
        version_code: u64,
        version_name: Option<&str>,
        code_path: &str,
        ce_data_path: &str,
        de_data_path: &str,
        package_manager_ce_inode: u64,
        package_manager_de_inode: u64,
        enabled_state: PackageEnabledState,
        suspended: bool,
        pending_install: bool,
    ) -> Result<Self, BridgeError> {
        Self::from_wire(PackageSnapshotWire {
            package_name: package_name.to_owned(),
            user_id,
            uid,
            signature_sha256: signature_sha256.to_owned(),
            version_code,
            version_name: version_name.map(str::to_owned),
            code_path: code_path.to_owned(),
            ce_data_path: ce_data_path.to_owned(),
            de_data_path: de_data_path.to_owned(),
            package_manager_ce_inode,
            package_manager_de_inode,
            enabled_state,
            suspended,
            pending_install,
        })
    }

    fn from_wire(wire: PackageSnapshotWire) -> Result<Self, BridgeError> {
        super::validation::validate_package_name(&wire.package_name)?;
        if wire.uid < 10_000 {
            return Err(invalid_response("package uid is outside app range"));
        }
        if !super::validation::is_sha256(&wire.signature_sha256) {
            return Err(invalid_response(
                "signatureSha256 must be 64 hex characters",
            ));
        }
        if wire.version_code == 0 {
            return Err(invalid_response("versionCode must be non-zero"));
        }
        if let Some(version_name) = wire.version_name.as_deref() {
            super::validation::validate_text(version_name, 128, true, "versionName")?;
        }
        super::validation::validate_code_path(&wire.code_path)?;
        super::validation::validate_data_path(
            &wire.ce_data_path,
            "/data/user/0/",
            &wire.package_name,
        )?;
        super::validation::validate_data_path(
            &wire.de_data_path,
            "/data/user_de/0/",
            &wire.package_name,
        )?;
        if wire.package_manager_ce_inode == 0 || wire.package_manager_de_inode == 0 {
            return Err(invalid_response(
                "PackageManager CE/DE inodes must be non-zero",
            ));
        }
        Ok(Self {
            package_name: wire.package_name,
            user_id: wire.user_id,
            uid: wire.uid,
            signature_sha256: wire.signature_sha256.to_ascii_lowercase(),
            version_code: wire.version_code,
            version_name: wire.version_name,
            code_path: wire.code_path,
            ce_data_path: wire.ce_data_path,
            de_data_path: wire.de_data_path,
            package_manager_ce_inode: wire.package_manager_ce_inode,
            package_manager_de_inode: wire.package_manager_de_inode,
            enabled_state: wire.enabled_state,
            suspended: wire.suspended,
            pending_install: wire.pending_install,
        })
    }

    #[doc = "Returns the PackageManager package name."]
    pub fn package_name(&self) -> &str {
        &self.package_name
    }

    #[doc = "Returns the PackageManager user id."]
    pub const fn user_id(&self) -> u32 {
        self.user_id
    }

    #[doc = "Returns the app UID."]
    pub const fn uid(&self) -> u32 {
        self.uid
    }

    #[doc = "Returns the lower-case signing certificate SHA-256 digest."]
    pub fn signature_sha256(&self) -> &str {
        &self.signature_sha256
    }

    #[doc = "Returns the long version code."]
    pub const fn version_code(&self) -> u64 {
        self.version_code
    }

    #[doc = "Returns the optional version name."]
    pub fn version_name(&self) -> Option<&str> {
        self.version_name.as_deref()
    }

    #[doc = "Returns the validated absolute APK code path."]
    pub fn code_path(&self) -> &str {
        &self.code_path
    }

    #[doc = "Returns the exact canonical credential-encrypted data path."]
    pub fn ce_data_path(&self) -> &str {
        &self.ce_data_path
    }

    #[doc = "Returns the exact canonical device-encrypted data path."]
    pub fn de_data_path(&self) -> &str {
        &self.de_data_path
    }

    #[doc = "Returns PackageManager's persisted credential-encrypted data inode."]
    pub const fn package_manager_ce_inode(&self) -> u64 {
        self.package_manager_ce_inode
    }

    #[doc = "Returns PackageManager's persisted device-encrypted data inode."]
    pub const fn package_manager_de_inode(&self) -> u64 {
        self.package_manager_de_inode
    }

    #[doc = "Returns the exact enabled-state enum."]
    pub const fn enabled_state(&self) -> PackageEnabledState {
        self.enabled_state
    }

    #[doc = "Returns whether PackageManager suspended the package."]
    pub const fn suspended(&self) -> bool {
        self.suspended
    }

    #[doc = "Returns whether a pending install session was observed."]
    pub const fn pending_install(&self) -> bool {
        self.pending_install
    }
}
