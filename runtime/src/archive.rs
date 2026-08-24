use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{
    MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _, fchown, symlink,
};
use std::path::{Component, Path, PathBuf};

use age::secrecy::SecretString;
use filetime::FileTime;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use zeroize::Zeroize as _;

const MAGIC: &[u8; 8] = b"UCSBKP01";
const FLAG_ENCRYPTED: u8 = 1;
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ACCOUNTS: usize = 256;
const MAX_ENTRIES: usize = 1_000_000;
const MAX_PATH_BYTES: usize = 4096;
const MAX_LOGICAL_BYTES: u64 = 1 << 40;
const EXCLUDED_TOP_LEVEL: [&str; 2] = ["cache", "code_cache"];

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("backup is invalid")]
    Invalid,
    #[error("backup password is required")]
    PasswordRequired,
    #[error("backup authentication failed")]
    AuthenticationFailed,
    #[error("insufficient storage")]
    InsufficientStorage,
    #[error("backup is incompatible")]
    Incompatible,
    #[error("archive I/O failed: {0}")]
    Io(String),
}

impl ArchiveError {
    pub fn wire_code(&self) -> &'static str {
        match self {
            Self::Invalid | Self::Io(_) => "backup_invalid",
            Self::PasswordRequired => "backup_password_required",
            Self::AuthenticationFailed => "backup_auth_failed",
            Self::InsufficientStorage => "insufficient_storage",
            Self::Incompatible => "backup_incompatible",
        }
    }
}

fn io_error(error: impl ToString) -> ArchiveError {
    let message = error.to_string();
    if message.contains("No space left on device") || message.contains("os error 28") {
        ArchiveError::InsufficientStorage
    } else {
        ArchiveError::Io(message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveAccountKind {
    Base,
    Slot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveScope {
    Account,
    AllAccounts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveSigningKind {
    Lineage,
    Multiple,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainState {
    Empty,
    Data,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestEntryKind {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestEntry {
    pub path: String,
    pub kind: ManifestEntryKind,
    pub size: u64,
    pub sha256: Option<String>,
    pub link_target: Option<String>,
    pub mode: u32,
    pub mtime_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainManifest {
    pub state: DomainState,
    pub logical_size: u64,
    pub entries: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveAccountManifest {
    pub archive_account_id: String,
    pub name: String,
    pub kind: ArchiveAccountKind,
    pub source_slot: String,
    pub ce: DomainManifest,
    pub de: DomainManifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveManifest {
    pub format_version: u8,
    pub package: String,
    pub signing_kind: ArchiveSigningKind,
    pub signing_sha256: Vec<String>,
    pub app_version: String,
    pub android_version: String,
    pub device: String,
    pub created_at_millis: u64,
    pub scope: ArchiveScope,
    pub active_account_id: Option<String>,
    pub launch_after_reboot: bool,
    pub logical_size: u64,
    pub accounts: Vec<ArchiveAccountManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveSource {
    pub archive_account_id: String,
    pub name: String,
    pub kind: ArchiveAccountKind,
    pub source_slot: String,
    pub ce_path: PathBuf,
    pub de_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupRequest {
    pub output_path: PathBuf,
    pub password: Option<String>,
    pub package: String,
    pub signing_kind: ArchiveSigningKind,
    pub signing_sha256: Vec<String>,
    pub app_version: String,
    pub android_version: String,
    pub device: String,
    pub created_at_millis: u64,
    pub scope: ArchiveScope,
    pub active_account_id: Option<String>,
    pub launch_after_reboot: bool,
    pub sources: Vec<ArchiveSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreDestination {
    pub archive_account_id: String,
    pub ce_path: PathBuf,
    pub de_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreRequest {
    pub input_path: PathBuf,
    pub password: Option<String>,
    pub destinations: Vec<RestoreDestination>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum HelperRequest {
    Backup(BackupRequest),
    Inspect {
        input_path: PathBuf,
        password: Option<String>,
    },
    Restore(RestoreRequest),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HelperResponse {
    Manifest { manifest: ArchiveManifest },
    Error { error: HelperErrorBody },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperErrorBody {
    pub code: String,
}

pub fn read_control_frame(reader: &mut impl Read) -> Result<HelperRequest, ArchiveError> {
    let mut length = [0_u8; 4];
    reader.read_exact(&mut length).map_err(io_error)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_MANIFEST_BYTES as usize {
        return Err(ArchiveError::Invalid);
    }
    let mut bytes = vec![0_u8; length];
    reader.read_exact(&mut bytes).map_err(io_error)?;
    let request = serde_json::from_slice(&bytes).map_err(|_error| ArchiveError::Invalid);
    bytes.zeroize();
    request
}

pub fn write_response(
    writer: &mut impl Write,
    response: &HelperResponse,
) -> Result<(), ArchiveError> {
    serde_json::to_writer(&mut *writer, response).map_err(io_error)?;
    writer.write_all(b"\n").map_err(io_error)
}

pub fn create_backup(request: BackupRequest) -> Result<ArchiveManifest, ArchiveError> {
    create_backup_with_owner(request, None)
}

pub fn create_backup_for_owner(
    request: BackupRequest,
    uid: u32,
    gid: u32,
) -> Result<ArchiveManifest, ArchiveError> {
    create_backup_with_owner(request, Some((uid, gid)))
}

fn create_backup_with_owner(
    mut request: BackupRequest,
    output_owner: Option<(u32, u32)>,
) -> Result<ArchiveManifest, ArchiveError> {
    validate_backup_request(&request)?;
    let manifest = build_manifest(&request)?;
    let password = request.password.take().map(SecretString::from);
    let temporary = request.output_path.with_extension("ucsbackup.partial");
    let _stale = fs::remove_file(&temporary);
    let result = (|| {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(io_error)?;
        output.write_all(MAGIC).map_err(io_error)?;
        output
            .write_all(&[if password.is_some() {
                FLAG_ENCRYPTED
            } else {
                0
            }])
            .map_err(io_error)?;
        let mut output = match password {
            Some(password) => {
                let encryptor = age::Encryptor::with_user_passphrase(password);
                let age_writer = encryptor.wrap_output(output).map_err(io_error)?;
                let age_writer = write_compressed_tar(age_writer, &manifest, &request.sources)?;
                age_writer.finish().map_err(io_error)?
            }
            None => write_compressed_tar(output, &manifest, &request.sources)?,
        };
        if let Some((uid, gid)) = output_owner {
            fchown(&output, Some(uid), Some(gid)).map_err(io_error)?;
            output
                .set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(io_error)?;
        }
        output.flush().map_err(io_error)?;
        output.sync_all().map_err(io_error)?;
        drop(output);
        fs::rename(&temporary, &request.output_path).map_err(io_error)?;
        let parent = request.output_path.parent().ok_or(ArchiveError::Invalid)?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(io_error)
    })();
    if result.is_err() {
        let _cleanup = fs::remove_file(&temporary);
    }
    result.map(|()| manifest)
}

pub fn inspect_backup(
    input_path: &Path,
    password: Option<String>,
) -> Result<ArchiveManifest, ArchiveError> {
    let reader = open_payload(input_path, password)?;
    let decoder = zstd::stream::read::Decoder::new(reader).map_err(io_error)?;
    let mut archive = tar::Archive::new(decoder);
    let manifest = {
        let mut entries = archive.entries().map_err(io_error)?;
        let Some(first) = entries.next() else {
            return Err(ArchiveError::Invalid);
        };
        let mut first = first.map_err(io_error)?;
        if first.path().map_err(io_error)?.as_ref() != Path::new("manifest.json")
            || first.size() > MAX_MANIFEST_BYTES
        {
            return Err(ArchiveError::Invalid);
        }
        let mut bytes = Vec::with_capacity(first.size() as usize);
        first.read_to_end(&mut bytes).map_err(io_error)?;
        let manifest: ArchiveManifest =
            serde_json::from_slice(&bytes).map_err(|_error| ArchiveError::Invalid)?;
        validate_manifest(&manifest)?;
        let expected = manifest_entry_index(&manifest);
        let mut seen = BTreeSet::new();
        let mut entry_count = 0_usize;
        let mut inspected_size = 0_u64;
        for entry in &mut entries {
            entry_count = entry_count.saturating_add(1);
            if entry_count > MAX_ENTRIES {
                return Err(ArchiveError::Invalid);
            }
            let mut entry = entry.map_err(io_error)?;
            let path = entry.path().map_err(io_error)?.into_owned();
            let (account_id, domain, relative) = parse_archive_data_path(&path)?;
            let key = (account_id.to_owned(), domain.to_owned(), relative);
            if !seen.insert(key.clone()) {
                return Err(ArchiveError::Invalid);
            }
            let expected_entry = expected.get(&key).ok_or(ArchiveError::Invalid)?;
            validate_archive_entry(
                &mut entry,
                expected_entry,
                &mut inspected_size,
                manifest.logical_size,
            )?;
        }
        if seen.len() != expected.len() || inspected_size != manifest.logical_size {
            return Err(ArchiveError::Invalid);
        }
        manifest
    };
    validate_archive_end(archive.into_inner())?;
    Ok(manifest)
}

pub fn restore_backup(mut request: RestoreRequest) -> Result<ArchiveManifest, ArchiveError> {
    let password = request.password.take();
    let result = restore_backup_inner(&request.input_path, password, &request.destinations);
    if result.is_err() {
        for destination in &request.destinations {
            let _ce = clear_directory(&destination.ce_path);
            let _de = clear_directory(&destination.de_path);
        }
    }
    result
}

fn restore_backup_inner(
    input_path: &Path,
    password: Option<String>,
    destinations: &[RestoreDestination],
) -> Result<ArchiveManifest, ArchiveError> {
    let reader = open_payload(input_path, password)?;
    let decoder = zstd::stream::read::Decoder::new(reader).map_err(io_error)?;
    let mut archive = tar::Archive::new(decoder);
    let (manifest, directories) = {
        let mut entries = archive.entries().map_err(io_error)?;
        let Some(first) = entries.next() else {
            return Err(ArchiveError::Invalid);
        };
        let mut first = first.map_err(io_error)?;
        if first.path().map_err(io_error)?.as_ref() != Path::new("manifest.json")
            || first.size() > MAX_MANIFEST_BYTES
        {
            return Err(ArchiveError::Invalid);
        }
        let mut manifest_bytes = Vec::with_capacity(first.size() as usize);
        first.read_to_end(&mut manifest_bytes).map_err(io_error)?;
        let manifest: ArchiveManifest =
            serde_json::from_slice(&manifest_bytes).map_err(|_error| ArchiveError::Invalid)?;
        validate_manifest(&manifest)?;
        let destination_map = destinations
            .iter()
            .map(|destination| (destination.archive_account_id.as_str(), destination))
            .collect::<BTreeMap<_, _>>();
        if destination_map.len() != destinations.len()
            || destinations.len() != manifest.accounts.len()
            || manifest
                .accounts
                .iter()
                .any(|account| !destination_map.contains_key(account.archive_account_id.as_str()))
        {
            return Err(ArchiveError::Incompatible);
        }
        for destination in destinations {
            require_empty_real_directory(&destination.ce_path)?;
            require_empty_real_directory(&destination.de_path)?;
        }
        let expected = manifest_entry_index(&manifest);
        let mut seen = BTreeSet::new();
        let mut entry_count = 0_usize;
        let mut extracted_size = 0_u64;
        let mut directories = Vec::new();
        for entry in &mut entries {
            entry_count = entry_count.saturating_add(1);
            if entry_count > MAX_ENTRIES {
                return Err(ArchiveError::Invalid);
            }
            let mut entry = entry.map_err(io_error)?;
            let path = entry.path().map_err(io_error)?.into_owned();
            let (account_id, domain, relative) = parse_archive_data_path(&path)?;
            let key = (account_id.to_owned(), domain.to_owned(), relative.clone());
            if !seen.insert(key.clone()) {
                return Err(ArchiveError::Invalid);
            }
            let expected_entry = expected.get(&key).ok_or(ArchiveError::Invalid)?;
            let destination = destination_map
                .get(account_id)
                .ok_or(ArchiveError::Incompatible)?;
            let domain_root = if domain == "ce" {
                &destination.ce_path
            } else {
                &destination.de_path
            };
            let output = safe_join(domain_root, &relative)?;
            extract_entry(
                &mut entry,
                domain_root,
                &output,
                expected_entry,
                &mut extracted_size,
                manifest.logical_size,
                &mut directories,
            )?;
        }
        if seen.len() != expected.len() || extracted_size != manifest.logical_size {
            return Err(ArchiveError::Invalid);
        }
        (manifest, directories)
    };
    validate_archive_end(archive.into_inner())?;
    restore_directory_metadata(directories)?;
    Ok(manifest)
}

fn validate_backup_request(request: &BackupRequest) -> Result<(), ArchiveError> {
    if request.package.is_empty()
        || request.sources.is_empty()
        || request.sources.len() > MAX_ACCOUNTS
        || request.output_path.as_os_str().is_empty()
    {
        return Err(ArchiveError::Invalid);
    }
    validate_signing_identity(request.signing_kind, &request.signing_sha256)?;
    let ids = request
        .sources
        .iter()
        .map(|source| source.archive_account_id.as_str())
        .collect::<BTreeSet<_>>();
    if ids.len() != request.sources.len()
        || request
            .active_account_id
            .as_ref()
            .is_some_and(|active| !ids.contains(active.as_str()))
    {
        return Err(ArchiveError::Invalid);
    }
    for source in &request.sources {
        validate_archive_id(&source.archive_account_id)?;
        require_real_directory(&source.ce_path)?;
        require_real_directory(&source.de_path)?;
    }
    Ok(())
}

fn build_manifest(request: &BackupRequest) -> Result<ArchiveManifest, ArchiveError> {
    let mut accounts = Vec::with_capacity(request.sources.len());
    let mut logical_size = 0_u64;
    for source in &request.sources {
        let ce = scan_domain(&source.ce_path)?;
        let de = scan_domain(&source.de_path)?;
        logical_size = logical_size
            .checked_add(ce.logical_size)
            .and_then(|size| size.checked_add(de.logical_size))
            .ok_or(ArchiveError::Invalid)?;
        if logical_size > MAX_LOGICAL_BYTES {
            return Err(ArchiveError::Invalid);
        }
        accounts.push(ArchiveAccountManifest {
            archive_account_id: source.archive_account_id.clone(),
            name: if source.kind == ArchiveAccountKind::Base {
                "系统原始空间".to_owned()
            } else {
                source.name.clone()
            },
            kind: source.kind,
            source_slot: source.source_slot.clone(),
            ce,
            de,
        });
    }
    let manifest = ArchiveManifest {
        format_version: 1,
        package: request.package.clone(),
        signing_kind: request.signing_kind,
        signing_sha256: request.signing_sha256.clone(),
        app_version: request.app_version.clone(),
        android_version: request.android_version.clone(),
        device: request.device.clone(),
        created_at_millis: request.created_at_millis,
        scope: request.scope,
        active_account_id: request.active_account_id.clone(),
        launch_after_reboot: request.launch_after_reboot,
        logical_size,
        accounts,
    };
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn scan_domain(root: &Path) -> Result<DomainManifest, ArchiveError> {
    require_real_directory(root)?;
    let mut entries = Vec::new();
    collect_entries(root, root, 0, &mut entries)?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    if entries.len() > MAX_ENTRIES {
        return Err(ArchiveError::Invalid);
    }
    let logical_size = entries.iter().try_fold(0_u64, |total, entry| {
        total.checked_add(entry.size).ok_or(ArchiveError::Invalid)
    })?;
    Ok(DomainManifest {
        state: if entries.is_empty() {
            DomainState::Empty
        } else {
            DomainState::Data
        },
        logical_size,
        entries,
    })
}

fn collect_entries(
    root: &Path,
    directory: &Path,
    depth: usize,
    entries: &mut Vec<ManifestEntry>,
) -> Result<(), ArchiveError> {
    let mut children = fs::read_dir(directory)
        .map_err(io_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_error)?;
    children.sort_by_key(|entry| entry.file_name());
    for entry in children {
        let name = entry
            .file_name()
            .to_str()
            .map(str::to_owned)
            .ok_or(ArchiveError::Invalid)?;
        if depth == 0 && EXCLUDED_TOP_LEVEL.contains(&name.as_str()) {
            continue;
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_error| ArchiveError::Invalid)?;
        validate_relative_path(relative)?;
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        let mode = metadata.mode() & 0o1777;
        let mtime_seconds = metadata.mtime().max(0) as u64;
        if metadata.file_type().is_dir() {
            entries.push(ManifestEntry {
                path: path_text(relative)?,
                kind: ManifestEntryKind::Directory,
                size: 0,
                sha256: None,
                link_target: None,
                mode,
                mtime_seconds,
            });
            collect_entries(root, &path, depth.saturating_add(1), entries)?;
        } else if metadata.file_type().is_file() {
            if metadata.nlink() != 1 {
                return Err(ArchiveError::Invalid);
            }
            entries.push(ManifestEntry {
                path: path_text(relative)?,
                kind: ManifestEntryKind::File,
                size: metadata.len(),
                sha256: Some(hash_file(&path)?),
                link_target: None,
                mode,
                mtime_seconds,
            });
        } else if metadata.file_type().is_symlink() {
            let target = fs::read_link(&path).map_err(io_error)?;
            if target.is_absolute() || !relative_link_stays_inside(root, &path, &target) {
                return Err(ArchiveError::Invalid);
            }
            entries.push(ManifestEntry {
                path: path_text(relative)?,
                kind: ManifestEntryKind::Symlink,
                size: 0,
                sha256: None,
                link_target: Some(path_text(&target)?),
                mode,
                mtime_seconds,
            });
        } else {
            return Err(ArchiveError::Invalid);
        }
        if entries.len() > MAX_ENTRIES {
            return Err(ArchiveError::Invalid);
        }
    }
    Ok(())
}

fn write_compressed_tar<W: Write>(
    writer: W,
    manifest: &ArchiveManifest,
    sources: &[ArchiveSource],
) -> Result<W, ArchiveError> {
    let encoder = zstd::stream::write::Encoder::new(writer, 3).map_err(io_error)?;
    let mut builder = tar::Builder::new(encoder);
    builder.follow_symlinks(false);
    let manifest_bytes = serde_json::to_vec(manifest).map_err(io_error)?;
    if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(ArchiveError::Invalid);
    }
    append_bytes(&mut builder, "manifest.json", &manifest_bytes, 0o600, 0)?;
    let source_map = sources
        .iter()
        .map(|source| (source.archive_account_id.as_str(), source))
        .collect::<BTreeMap<_, _>>();
    for account in &manifest.accounts {
        let source = source_map
            .get(account.archive_account_id.as_str())
            .ok_or(ArchiveError::Invalid)?;
        append_domain(
            &mut builder,
            &source.ce_path,
            &account.archive_account_id,
            "ce",
            &account.ce,
        )?;
        append_domain(
            &mut builder,
            &source.de_path,
            &account.archive_account_id,
            "de",
            &account.de,
        )?;
    }
    builder.finish().map_err(io_error)?;
    let encoder = builder.into_inner().map_err(io_error)?;
    encoder.finish().map_err(io_error)
}

fn append_domain<W: Write>(
    builder: &mut tar::Builder<W>,
    root: &Path,
    account_id: &str,
    domain: &str,
    manifest: &DomainManifest,
) -> Result<(), ArchiveError> {
    for entry in &manifest.entries {
        let source = safe_join(root, Path::new(&entry.path))?;
        let archive_path = PathBuf::from("accounts")
            .join(account_id)
            .join(domain)
            .join(&entry.path);
        let metadata = fs::symlink_metadata(&source).map_err(io_error)?;
        let actual_kind = if metadata.file_type().is_dir() {
            ManifestEntryKind::Directory
        } else if metadata.file_type().is_file() {
            ManifestEntryKind::File
        } else if metadata.file_type().is_symlink() {
            ManifestEntryKind::Symlink
        } else {
            return Err(ArchiveError::Invalid);
        };
        if actual_kind != entry.kind {
            return Err(ArchiveError::Invalid);
        }
        let mut header = tar::Header::new_gnu();
        header.set_uid(0);
        header.set_gid(0);
        header.set_mode(entry.mode);
        header.set_mtime(entry.mtime_seconds);
        match entry.kind {
            ManifestEntryKind::Directory => {
                header.set_entry_type(tar::EntryType::Directory);
                header.set_size(0);
                header.set_cksum();
                builder
                    .append_data(&mut header, archive_path, std::io::empty())
                    .map_err(io_error)?;
            }
            ManifestEntryKind::File => {
                if metadata.len() != entry.size || hash_file(&source)? != entry.sha256_value()? {
                    return Err(ArchiveError::Invalid);
                }
                header.set_entry_type(tar::EntryType::Regular);
                header.set_size(entry.size);
                header.set_cksum();
                let file = File::open(source).map_err(io_error)?;
                builder
                    .append_data(&mut header, archive_path, file)
                    .map_err(io_error)?;
            }
            ManifestEntryKind::Symlink => {
                let target = fs::read_link(&source).map_err(io_error)?;
                if path_text(&target)? != entry.link_target_value()? {
                    return Err(ArchiveError::Invalid);
                }
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_size(0);
                header.set_link_name(target).map_err(io_error)?;
                header.set_cksum();
                builder
                    .append_data(&mut header, archive_path, std::io::empty())
                    .map_err(io_error)?;
            }
        }
    }
    Ok(())
}

fn append_bytes<W: Write>(
    builder: &mut tar::Builder<W>,
    path: &str,
    bytes: &[u8],
    mode: u32,
    mtime: u64,
) -> Result<(), ArchiveError> {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mode(mode);
    header.set_mtime(mtime);
    header.set_size(bytes.len() as u64);
    header.set_cksum();
    builder
        .append_data(&mut header, path, bytes)
        .map_err(io_error)
}

fn open_payload(
    input_path: &Path,
    password: Option<String>,
) -> Result<Box<dyn Read>, ArchiveError> {
    let mut input = BufReader::new(File::open(input_path).map_err(io_error)?);
    let mut magic = [0_u8; 8];
    input.read_exact(&mut magic).map_err(io_error)?;
    if &magic != MAGIC {
        return Err(ArchiveError::Invalid);
    }
    let mut flags = [0_u8; 1];
    input.read_exact(&mut flags).map_err(io_error)?;
    if flags[0] & !FLAG_ENCRYPTED != 0 {
        return Err(ArchiveError::Incompatible);
    }
    if flags[0] & FLAG_ENCRYPTED == 0 {
        return Ok(Box::new(input));
    }
    let password = password.ok_or(ArchiveError::PasswordRequired)?;
    let identity = age::scrypt::Identity::new(SecretString::from(password));
    let decryptor = age::Decryptor::new(input).map_err(|_error| ArchiveError::Invalid)?;
    let reader = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(|_error| ArchiveError::AuthenticationFailed)?;
    Ok(Box::new(reader))
}

fn validate_manifest(manifest: &ArchiveManifest) -> Result<(), ArchiveError> {
    if manifest.format_version != 1
        || manifest.package.is_empty()
        || manifest.accounts.is_empty()
        || manifest.accounts.len() > MAX_ACCOUNTS
        || manifest.logical_size > MAX_LOGICAL_BYTES
    {
        return Err(ArchiveError::Incompatible);
    }
    validate_signing_identity(manifest.signing_kind, &manifest.signing_sha256)?;
    let ids = manifest
        .accounts
        .iter()
        .map(|account| account.archive_account_id.as_str())
        .collect::<BTreeSet<_>>();
    if ids.len() != manifest.accounts.len()
        || manifest
            .active_account_id
            .as_ref()
            .is_some_and(|active| !ids.contains(active.as_str()))
    {
        return Err(ArchiveError::Invalid);
    }
    let base_count = manifest
        .accounts
        .iter()
        .filter(|account| account.kind == ArchiveAccountKind::Base)
        .count();
    let scope_is_valid = match manifest.scope {
        ArchiveScope::Account => {
            manifest.accounts.len() == 1
                && manifest.active_account_id.as_deref()
                    == Some(manifest.accounts[0].archive_account_id.as_str())
        }
        ArchiveScope::AllAccounts => base_count == 1 && manifest.active_account_id.is_some(),
    };
    if base_count > 1
        || manifest.accounts.iter().any(|account| {
            (account.kind == ArchiveAccountKind::Base) != (account.source_slot == "base")
        })
        || !scope_is_valid
    {
        return Err(ArchiveError::Invalid);
    }
    let mut entry_count = 0_usize;
    let mut logical_size = 0_u64;
    for account in &manifest.accounts {
        validate_archive_id(&account.archive_account_id)?;
        for domain in [&account.ce, &account.de] {
            entry_count = entry_count
                .checked_add(domain.entries.len())
                .ok_or(ArchiveError::Invalid)?;
            if entry_count > MAX_ENTRIES {
                return Err(ArchiveError::Invalid);
            }
            let paths = domain
                .entries
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<BTreeSet<_>>();
            if paths.len() != domain.entries.len() {
                return Err(ArchiveError::Invalid);
            }
            let domain_size = domain.entries.iter().try_fold(0_u64, |total, entry| {
                validate_relative_path(Path::new(&entry.path))?;
                validate_manifest_entry(entry)?;
                total.checked_add(entry.size).ok_or(ArchiveError::Invalid)
            })?;
            if domain_size != domain.logical_size
                || (domain.state == DomainState::Empty) != domain.entries.is_empty()
            {
                return Err(ArchiveError::Invalid);
            }
            logical_size = logical_size
                .checked_add(domain_size)
                .ok_or(ArchiveError::Invalid)?;
        }
    }
    (logical_size == manifest.logical_size)
        .then_some(())
        .ok_or(ArchiveError::Invalid)
}

fn validate_signing_identity(
    kind: ArchiveSigningKind,
    digests: &[String],
) -> Result<(), ArchiveError> {
    if digests.is_empty()
        || digests.len() > 16
        || digests.iter().any(|digest| {
            digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        })
        || digests.iter().collect::<BTreeSet<_>>().len() != digests.len()
        || (kind == ArchiveSigningKind::Multiple
            && (digests.len() < 2
                || !digests
                    .windows(2)
                    .all(|pair| pair[0].as_str() < pair[1].as_str())))
    {
        return Err(ArchiveError::Invalid);
    }
    Ok(())
}

fn validate_manifest_entry(entry: &ManifestEntry) -> Result<(), ArchiveError> {
    if entry.mtime_seconds > i64::MAX as u64 {
        return Err(ArchiveError::Invalid);
    }
    match entry.kind {
        ManifestEntryKind::File => {
            let digest = entry.sha256.as_deref().ok_or(ArchiveError::Invalid)?;
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
                || entry.link_target.is_some()
            {
                return Err(ArchiveError::Invalid);
            }
        }
        ManifestEntryKind::Directory => {
            if entry.size != 0 || entry.sha256.is_some() || entry.link_target.is_some() {
                return Err(ArchiveError::Invalid);
            }
        }
        ManifestEntryKind::Symlink => {
            let target = entry.link_target.as_deref().ok_or(ArchiveError::Invalid)?;
            if entry.size != 0
                || entry.sha256.is_some()
                || Path::new(target).is_absolute()
                || !relative_link_target_is_safe(Path::new(&entry.path), Path::new(target))
            {
                return Err(ArchiveError::Invalid);
            }
        }
    }
    if entry.mode & !0o1777 != 0 {
        return Err(ArchiveError::Invalid);
    }
    Ok(())
}

fn manifest_entry_index(
    manifest: &ArchiveManifest,
) -> BTreeMap<(String, String, PathBuf), &ManifestEntry> {
    let mut index = BTreeMap::new();
    for account in &manifest.accounts {
        for (domain, content) in [("ce", &account.ce), ("de", &account.de)] {
            for entry in &content.entries {
                index.insert(
                    (
                        account.archive_account_id.clone(),
                        domain.to_owned(),
                        PathBuf::from(&entry.path),
                    ),
                    entry,
                );
            }
        }
    }
    index
}

fn parse_archive_data_path(path: &Path) -> Result<(&str, &str, PathBuf), ArchiveError> {
    validate_relative_path(path)?;
    let parts = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    if parts.len() < 4 || parts[0] != "accounts" || !matches!(parts[2], "ce" | "de") {
        return Err(ArchiveError::Invalid);
    }
    validate_archive_id(parts[1])?;
    Ok((parts[1], parts[2], parts[3..].iter().collect::<PathBuf>()))
}

fn extract_entry<R: Read>(
    entry: &mut tar::Entry<'_, R>,
    domain_root: &Path,
    output: &Path,
    expected: &ManifestEntry,
    extracted_size: &mut u64,
    declared_total: u64,
    directories: &mut Vec<(PathBuf, u32, u64)>,
) -> Result<(), ArchiveError> {
    validate_archive_header(entry, expected)?;
    ensure_safe_parent(domain_root, output)?;
    match expected.kind {
        ManifestEntryKind::Directory => {
            match fs::create_dir(output) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    require_real_directory(output)?;
                }
                Err(error) => return Err(io_error(error)),
            }
            directories.push((output.to_path_buf(), expected.mode, expected.mtime_seconds));
        }
        ManifestEntryKind::File => {
            *extracted_size = extracted_size
                .checked_add(expected.size)
                .filter(|size| *size <= declared_total)
                .ok_or(ArchiveError::Invalid)?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(output)
                .map_err(io_error)?;
            let mut hasher = Sha256::new();
            let mut remaining = expected.size;
            let mut buffer = [0_u8; 64 * 1024];
            while remaining != 0 {
                let requested = usize::try_from(remaining.min(buffer.len() as u64))
                    .map_err(|_error| ArchiveError::Invalid)?;
                let read = entry.read(&mut buffer[..requested]).map_err(io_error)?;
                if read == 0 {
                    return Err(ArchiveError::Invalid);
                }
                file.write_all(&buffer[..read]).map_err(io_error)?;
                hasher.update(&buffer[..read]);
                remaining -= read as u64;
            }
            file.sync_all().map_err(io_error)?;
            fs::set_permissions(output, fs::Permissions::from_mode(expected.mode))
                .map_err(io_error)?;
            let digest = format!("{:x}", hasher.finalize());
            if digest != expected.sha256_value()? {
                return Err(ArchiveError::Invalid);
            }
        }
        ManifestEntryKind::Symlink => {
            let target = entry
                .link_name()
                .map_err(io_error)?
                .ok_or(ArchiveError::Invalid)?
                .into_owned();
            if path_text(&target)? != expected.link_target_value()?
                || target.is_absolute()
                || !relative_link_stays_inside(domain_root, output, &target)
            {
                return Err(ArchiveError::Invalid);
            }
            symlink(target, output).map_err(io_error)?;
            let time = FileTime::from_unix_time(expected.mtime_seconds as i64, 0);
            filetime::set_symlink_file_times(output, time, time).map_err(io_error)?;
        }
    }
    if expected.kind == ManifestEntryKind::File {
        filetime::set_file_mtime(
            output,
            FileTime::from_unix_time(expected.mtime_seconds as i64, 0),
        )
        .map_err(io_error)?;
    }
    Ok(())
}

fn validate_archive_entry<R: Read>(
    entry: &mut tar::Entry<'_, R>,
    expected: &ManifestEntry,
    inspected_size: &mut u64,
    declared_total: u64,
) -> Result<(), ArchiveError> {
    validate_archive_header(entry, expected)?;
    match expected.kind {
        ManifestEntryKind::File => {
            *inspected_size = inspected_size
                .checked_add(expected.size)
                .filter(|size| *size <= declared_total)
                .ok_or(ArchiveError::Invalid)?;
            let mut hasher = Sha256::new();
            let mut remaining = expected.size;
            let mut buffer = [0_u8; 64 * 1024];
            while remaining != 0 {
                let requested = usize::try_from(remaining.min(buffer.len() as u64))
                    .map_err(|_error| ArchiveError::Invalid)?;
                let read = entry.read(&mut buffer[..requested]).map_err(io_error)?;
                if read == 0 {
                    return Err(ArchiveError::Invalid);
                }
                hasher.update(&buffer[..read]);
                remaining -= read as u64;
            }
            if format!("{:x}", hasher.finalize()) != expected.sha256_value()? {
                return Err(ArchiveError::Invalid);
            }
        }
        ManifestEntryKind::Directory => {}
        ManifestEntryKind::Symlink => {
            let target = entry
                .link_name()
                .map_err(io_error)?
                .ok_or(ArchiveError::Invalid)?
                .into_owned();
            if path_text(&target)? != expected.link_target_value()? {
                return Err(ArchiveError::Invalid);
            }
        }
    }
    Ok(())
}

fn validate_archive_header<R: Read>(
    entry: &tar::Entry<'_, R>,
    expected: &ManifestEntry,
) -> Result<(), ArchiveError> {
    let entry_type = entry.header().entry_type();
    let actual_kind = if entry_type.is_file() {
        ManifestEntryKind::File
    } else if entry_type.is_dir() {
        ManifestEntryKind::Directory
    } else if entry_type.is_symlink() {
        ManifestEntryKind::Symlink
    } else {
        return Err(ArchiveError::Invalid);
    };
    if actual_kind != expected.kind
        || entry.size() != expected.size
        || entry.header().mode().map_err(io_error)? & 0o1777 != expected.mode
        || entry.header().mtime().map_err(io_error)? != expected.mtime_seconds
    {
        return Err(ArchiveError::Invalid);
    }
    Ok(())
}

fn validate_archive_end<R: BufRead>(
    mut decoder: zstd::stream::read::Decoder<'_, R>,
) -> Result<(), ArchiveError> {
    let mut trailing = [0_u8; 1024];
    let mut trailing_bytes = 0_usize;
    loop {
        let read = decoder.read(&mut trailing).map_err(io_error)?;
        if read == 0 {
            break;
        }
        trailing_bytes = trailing_bytes
            .checked_add(read)
            .filter(|count| *count <= 1024)
            .ok_or(ArchiveError::Invalid)?;
        if trailing[..read].iter().any(|byte| *byte != 0) {
            return Err(ArchiveError::Invalid);
        }
    }
    let mut payload = decoder.finish();
    let mut extra = [0_u8; 1];
    if payload.read(&mut extra).map_err(io_error)? != 0 {
        return Err(ArchiveError::Invalid);
    }
    Ok(())
}

fn restore_directory_metadata(
    mut directories: Vec<(PathBuf, u32, u64)>,
) -> Result<(), ArchiveError> {
    directories.sort_by_key(|(path, _, _)| std::cmp::Reverse(path.components().count()));
    for (path, mode, mtime_seconds) in directories {
        require_real_directory(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).map_err(io_error)?;
        filetime::set_file_mtime(&path, FileTime::from_unix_time(mtime_seconds as i64, 0))
            .map_err(io_error)?;
    }
    Ok(())
}

impl ManifestEntry {
    fn sha256_value(&self) -> Result<&str, ArchiveError> {
        self.sha256.as_deref().ok_or(ArchiveError::Invalid)
    }

    fn link_target_value(&self) -> Result<&str, ArchiveError> {
        self.link_target.as_deref().ok_or(ArchiveError::Invalid)
    }
}

fn hash_file(path: &Path) -> Result<String, ArchiveError> {
    let mut file = File::open(path).map_err(io_error)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_archive_id(value: &str) -> Result<(), ArchiveError> {
    (!value.is_empty()
        && value.len() <= 64
        && value != "."
        && value != ".."
        && !value.starts_with('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')))
    .then_some(())
    .ok_or(ArchiveError::Invalid)
}

fn validate_relative_path(path: &Path) -> Result<(), ArchiveError> {
    if path.as_os_str().is_empty()
        || path.as_os_str().as_encoded_bytes().len() > MAX_PATH_BYTES
        || path.is_absolute()
        || path.components().any(|component| {
            !matches!(component, Component::Normal(_)) || component.as_os_str().to_str().is_none()
        })
    {
        return Err(ArchiveError::Invalid);
    }
    Ok(())
}

fn safe_join(root: &Path, relative: &Path) -> Result<PathBuf, ArchiveError> {
    validate_relative_path(relative)?;
    Ok(root.join(relative))
}

fn ensure_safe_parent(root: &Path, output: &Path) -> Result<(), ArchiveError> {
    let parent = output.parent().ok_or(ArchiveError::Invalid)?;
    let relative = parent
        .strip_prefix(root)
        .map_err(|_error| ArchiveError::Invalid)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(ArchiveError::Invalid);
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
                    return Err(ArchiveError::Invalid);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(io_error)?;
            }
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(())
}

fn require_real_directory(path: &Path) -> Result<(), ArchiveError> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    (metadata.file_type().is_dir() && !metadata.file_type().is_symlink())
        .then_some(())
        .ok_or(ArchiveError::Invalid)
}

fn require_empty_real_directory(path: &Path) -> Result<(), ArchiveError> {
    require_real_directory(path)?;
    fs::read_dir(path)
        .map_err(io_error)?
        .next()
        .is_none()
        .then_some(())
        .ok_or(ArchiveError::Invalid)
}

fn clear_directory(path: &Path) -> Result<(), ArchiveError> {
    require_real_directory(path)?;
    for entry in fs::read_dir(path).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
        if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
            fs::remove_dir_all(entry.path()).map_err(io_error)?;
        } else {
            fs::remove_file(entry.path()).map_err(io_error)?;
        }
    }
    Ok(())
}

fn relative_link_stays_inside(root: &Path, link: &Path, target: &Path) -> bool {
    let Some(parent) = link.parent() else {
        return false;
    };
    let Ok(relative_parent) = parent.strip_prefix(root) else {
        return false;
    };
    relative_link_depth_is_safe(relative_parent.components().chain(target.components()))
}

fn relative_link_target_is_safe(link_path: &Path, target: &Path) -> bool {
    let parent = link_path.parent().unwrap_or_else(|| Path::new(""));
    relative_link_depth_is_safe(parent.components().chain(target.components()))
}

fn relative_link_depth_is_safe<'a>(components: impl Iterator<Item = Component<'a>>) -> bool {
    let mut depth = 0_usize;
    for component in components {
        match component {
            Component::Normal(_) => depth = depth.saturating_add(1),
            Component::CurDir => {}
            Component::ParentDir => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
            }
            Component::Prefix(_) | Component::RootDir => return false,
        }
    }
    true
}

fn path_text(path: &Path) -> Result<String, ArchiveError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or(ArchiveError::Invalid)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn request(root: &Path, encrypted: bool) -> BackupRequest {
        let ce = root.join("source/ce");
        let de = root.join("source/de");
        fs::create_dir_all(ce.join("files")).unwrap();
        fs::create_dir_all(ce.join("cache")).unwrap();
        fs::create_dir_all(de.join("shared_prefs")).unwrap();
        fs::write(ce.join("files/account.json"), b"account").unwrap();
        fs::write(ce.join("cache/discard.bin"), b"cache").unwrap();
        fs::write(de.join("shared_prefs/session.xml"), b"session").unwrap();
        symlink("account.json", ce.join("files/account-link")).unwrap();
        fs::set_permissions(ce.join("files"), fs::Permissions::from_mode(0o500)).unwrap();
        filetime::set_file_mtime(ce.join("files"), FileTime::from_unix_time(42, 0)).unwrap();
        let link_time = FileTime::from_unix_time(43, 0);
        filetime::set_symlink_file_times(ce.join("files/account-link"), link_time, link_time)
            .unwrap();
        BackupRequest {
            output_path: root.join("account.ucsbackup"),
            password: encrypted.then(|| "correct horse battery staple".to_owned()),
            package: "com.example.app".to_owned(),
            signing_kind: ArchiveSigningKind::Lineage,
            signing_sha256: vec!["a".repeat(64)],
            app_version: "1.0".to_owned(),
            android_version: "17".to_owned(),
            device: "test".to_owned(),
            created_at_millis: 1,
            scope: ArchiveScope::Account,
            active_account_id: Some("account-0".to_owned()),
            launch_after_reboot: false,
            sources: vec![ArchiveSource {
                archive_account_id: "account-0".to_owned(),
                name: "系统原始空间".to_owned(),
                kind: ArchiveAccountKind::Base,
                source_slot: "base".to_owned(),
                ce_path: ce,
                de_path: de,
            }],
        }
    }

    #[test]
    fn encrypted_and_plain_archives_round_trip_without_top_level_caches() {
        for encrypted in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let request = request(root.path(), encrypted);
            let password = request.password.clone();
            let output = request.output_path.clone();
            let manifest = create_backup(request).unwrap();
            assert_eq!(inspect_backup(&output, password.clone()).unwrap(), manifest);
            assert!(
                !manifest.accounts[0]
                    .ce
                    .entries
                    .iter()
                    .any(|entry| entry.path.starts_with("cache"))
            );
            let ce = root.path().join("restore/ce");
            let de = root.path().join("restore/de");
            fs::create_dir_all(&ce).unwrap();
            fs::create_dir_all(&de).unwrap();
            restore_backup(RestoreRequest {
                input_path: output,
                password,
                destinations: vec![RestoreDestination {
                    archive_account_id: "account-0".to_owned(),
                    ce_path: ce.clone(),
                    de_path: de.clone(),
                }],
            })
            .unwrap();
            assert_eq!(fs::read(ce.join("files/account.json")).unwrap(), b"account");
            let files = fs::metadata(ce.join("files")).unwrap();
            assert_eq!(files.mode() & 0o7777, 0o500);
            assert_eq!(files.mtime(), 42);
            assert_eq!(
                fs::read_link(ce.join("files/account-link")).unwrap(),
                Path::new("account.json"),
            );
            assert_eq!(
                fs::symlink_metadata(ce.join("files/account-link"))
                    .unwrap()
                    .mtime(),
                43,
            );
            assert_eq!(
                fs::read(de.join("shared_prefs/session.xml")).unwrap(),
                b"session"
            );
            assert!(!ce.join("cache").exists());
        }
    }

    #[test]
    fn sparse_regular_file_round_trips_by_logical_content() {
        let root = tempfile::tempdir().unwrap();
        let request = request(root.path(), true);
        let sparse = request.sources[0].ce_path.join("Web Data");
        let mut file = File::create(&sparse).unwrap();
        file.write_all(b"SQLite format 3\0").unwrap();
        file.set_len(16 * 1024 * 1024).unwrap();
        file.sync_all().unwrap();
        let source_bytes = fs::read(&sparse).unwrap();
        let source_metadata = fs::metadata(&sparse).unwrap();
        assert!(source_metadata.blocks().saturating_mul(512) < source_metadata.len());

        let password = request.password.clone();
        let output = request.output_path.clone();
        let manifest = create_backup(request).unwrap();
        assert_eq!(inspect_backup(&output, password.clone()).unwrap(), manifest);
        let archived = manifest.accounts[0]
            .ce
            .entries
            .iter()
            .find(|entry| entry.path == "Web Data")
            .unwrap();
        assert_eq!(archived.kind, ManifestEntryKind::File);
        assert_eq!(archived.size, source_metadata.len());

        let ce = root.path().join("restore-sparse/ce");
        let de = root.path().join("restore-sparse/de");
        fs::create_dir_all(&ce).unwrap();
        fs::create_dir_all(&de).unwrap();
        restore_backup(RestoreRequest {
            input_path: output,
            password,
            destinations: vec![RestoreDestination {
                archive_account_id: "account-0".to_owned(),
                ce_path: ce.clone(),
                de_path: de,
            }],
        })
        .unwrap();
        assert_eq!(fs::read(ce.join("Web Data")).unwrap(), source_bytes);
    }

    #[test]
    fn requested_output_owner_and_private_mode_are_applied_before_publish() {
        let root = tempfile::tempdir().unwrap();
        let root_metadata = fs::metadata(root.path()).unwrap();
        let request = request(root.path(), true);
        let output = request.output_path.clone();

        create_backup_for_owner(request, root_metadata.uid(), root_metadata.gid()).unwrap();

        let output_metadata = fs::metadata(output).unwrap();
        assert_eq!(output_metadata.uid(), root_metadata.uid());
        assert_eq!(output_metadata.gid(), root_metadata.gid());
        assert_eq!(output_metadata.mode() & 0o777, 0o600);
    }

    #[test]
    fn wrong_password_and_truncated_archive_fail_without_leaving_staged_data() {
        let root = tempfile::tempdir().unwrap();
        let request = request(root.path(), true);
        let output = request.output_path.clone();
        create_backup(request).unwrap();
        assert!(matches!(
            inspect_backup(&output, Some("wrong".to_owned())),
            Err(ArchiveError::AuthenticationFailed)
        ));
        let bytes = fs::read(&output).unwrap();
        fs::write(&output, &bytes[..bytes.len() / 2]).unwrap();
        assert!(inspect_backup(&output, Some("correct horse battery staple".to_owned()),).is_err());
        let ce = root.path().join("restore/ce");
        let de = root.path().join("restore/de");
        fs::create_dir_all(&ce).unwrap();
        fs::create_dir_all(&de).unwrap();
        assert!(
            restore_backup(RestoreRequest {
                input_path: output,
                password: Some("correct horse battery staple".to_owned()),
                destinations: vec![RestoreDestination {
                    archive_account_id: "account-0".to_owned(),
                    ce_path: ce.clone(),
                    de_path: de.clone(),
                }],
            })
            .is_err()
        );
        assert!(fs::read_dir(ce).unwrap().next().is_none());
        assert!(fs::read_dir(de).unwrap().next().is_none());
    }

    #[test]
    fn source_symlink_that_escapes_the_account_root_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let request = request(root.path(), false);
        symlink("../../outside", request.sources[0].ce_path.join("escape")).unwrap();

        assert!(matches!(create_backup(request), Err(ArchiveError::Invalid)));
    }

    #[test]
    fn archive_scope_requires_a_complete_active_account_and_base_contract() {
        let root = tempfile::tempdir().unwrap();
        let mut manifest = build_manifest(&request(root.path(), false)).unwrap();
        manifest.active_account_id = None;
        assert!(validate_manifest(&manifest).is_err());

        manifest.active_account_id = Some("account-0".to_owned());
        manifest.scope = ArchiveScope::AllAccounts;
        assert!(validate_manifest(&manifest).is_ok());

        manifest.accounts[0].kind = ArchiveAccountKind::Slot;
        manifest.accounts[0].source_slot = "slot-1".to_owned();
        assert!(validate_manifest(&manifest).is_err());
    }
}
