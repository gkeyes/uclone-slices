#[doc = "Validation failures for persisted and command-boundary domain values."]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[doc = "The package name is not a safe Android package identifier."]
    #[error("invalid package name: {0}")]
    InvalidPackageName(String),
    #[doc = "The slot id is not safe for Registry-derived paths."]
    #[error("invalid slot id: {0}")]
    InvalidSlotId(String),
    #[doc = "The transaction id is not safe for journal directory names."]
    #[error("invalid transaction id: {0}")]
    InvalidTransactionId(String),
    #[doc = "The transaction commit nonce is malformed."]
    #[error("invalid transaction commit nonce")]
    InvalidCommitNonce,
    #[doc = "The Linux boot identifier is malformed."]
    #[error("invalid boot identifier")]
    InvalidBootId,
    #[doc = "The managed-update token is malformed."]
    #[error("invalid managed-update token")]
    InvalidUpdateToken,
    #[doc = "The signing-certificate digest is not a SHA-256 hex string."]
    #[error("invalid signature digest")]
    InvalidSignatureDigest,
    #[doc = "The UID is outside the ordinary application range."]
    #[error("invalid application uid: {0}")]
    InvalidAppUid(u32),
    #[doc = "The Preview currently supports only Android user 0."]
    #[error("unsupported Android user id: {0}")]
    UnsupportedUserId(u32),
    #[doc = "The installed APK version code is zero."]
    #[error("invalid version code: {0}")]
    InvalidVersionCode(u64),
    #[doc = "The APK path is outside the canonical /data/app tree."]
    #[error("invalid APK code path: {0}")]
    InvalidCodePath(String),
    #[doc = "A zero inode was observed at a persistence boundary."]
    #[error("inode must be non-zero")]
    InvalidInode,
    #[doc = "The Android-owned base slot does not use its enrolled inode anchor."]
    #[error("base slot must use base inodes")]
    BaseSlotInodeMismatch,
    #[doc = "A managed non-base slot reuses the Android-owned base inode pair."]
    #[error("non-base slot must not reuse both base inodes")]
    DuplicateSlotInodes,
}
