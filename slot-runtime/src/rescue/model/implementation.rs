use super::{
    ALLOWED_PACKAGE, AppIdentity, BootId, CommitNonce, DataInodes, GateSnapshot, PackageKey,
    RescueDisposition, RescueError, RescueId, RescueSpec, SCHEMA_VERSION, UserId,
};

impl RescueSpec {
    #[doc = "Constructs a fixed-package, user-zero native-base retirement specification."]
    #[allow(
        clippy::too_many_arguments,
        reason = "every independent rescue root-of-trust field must be explicit"
    )]
    pub fn new(
        rescue_id: RescueId,
        package_key: PackageKey,
        enrolled_identity: AppIdentity,
        base_inodes: DataInodes,
        gate_snapshot: GateSnapshot,
        enrollment_sha256: &str,
        base_manifest_sha256: &str,
        boot_id: &str,
        commit_nonce: CommitNonce,
    ) -> Result<Self, RescueError> {
        let spec = Self {
            schema_version: SCHEMA_VERSION,
            rescue_id,
            package_key,
            enrolled_identity,
            base_inodes,
            gate_snapshot,
            enrollment_sha256: enrollment_sha256.to_owned(),
            base_manifest_sha256: base_manifest_sha256.to_owned(),
            boot_id: BootId::parse(boot_id)?,
            commit_nonce,
            disposition: RescueDisposition::RetireToBase,
        };
        spec.validate()?;
        Ok(spec)
    }

    #[doc = "Returns the rescue identifier."]
    pub const fn rescue_id(&self) -> &RescueId {
        &self.rescue_id
    }

    #[doc = "Returns the fixed package/user key."]
    pub const fn package_key(&self) -> &PackageKey {
        &self.package_key
    }

    #[doc = "Returns the complete enrolled package identity."]
    pub const fn enrolled_identity(&self) -> &AppIdentity {
        &self.enrolled_identity
    }

    #[doc = "Returns the immutable native CE/DE inode anchor."]
    pub const fn base_inodes(&self) -> DataInodes {
        self.base_inodes
    }

    #[doc = "Returns the exact pre-gate package state."]
    pub const fn gate_snapshot(&self) -> GateSnapshot {
        self.gate_snapshot
    }

    #[doc = "Returns the pinned immutable enrollment digest."]
    pub fn enrollment_sha256(&self) -> &str {
        &self.enrollment_sha256
    }

    #[doc = "Returns the pinned exact base-manifest digest."]
    pub fn base_manifest_sha256(&self) -> &str {
        &self.base_manifest_sha256
    }

    #[doc = "Returns the boot identifier captured before rescue side effects."]
    pub fn boot_id(&self) -> &str {
        self.boot_id.as_str()
    }

    #[doc = "Returns the nonce that must appear at the native-base commit point."]
    pub const fn commit_nonce(&self) -> &CommitNonce {
        &self.commit_nonce
    }

    #[doc = "Returns the fixed rescue disposition."]
    pub const fn disposition(&self) -> RescueDisposition {
        self.disposition
    }

    pub(in crate::rescue) fn validate(&self) -> Result<(), RescueError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(RescueError::InvalidSpec(
                "unsupported specification schema".to_owned(),
            ));
        }
        if self.package_key.user_id() != UserId::PRIMARY
            || self.package_key.package_name().as_str() != ALLOWED_PACKAGE
        {
            return Err(RescueError::UnsupportedPackage);
        }
        if !valid_digest(&self.enrollment_sha256) || !valid_digest(&self.base_manifest_sha256) {
            return Err(RescueError::InvalidSpec(
                "pinned digests must be lowercase SHA-256".to_owned(),
            ));
        }
        Ok(())
    }
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
