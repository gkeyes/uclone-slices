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
const MAX_CONTROL_FRAME_BYTES: u64 = 4 * 1024 * 1024;
// A manifest line is about 260 bytes for a typical app data path (measured: 4 MiB held about
// 16,000 files). 64 MiB keeps roughly 250,000 entries in one in-memory manifest.
const MAX_MANIFEST_BYTES: u64 = 64 * 1024 * 1024;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveDomain {
    Ce,
    De,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveBackupBehavior {
    Exclude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveRestoreBehavior {
    Preserve,
    Discard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupProfileRule {
    pub domain: ArchiveDomain,
    pub path: String,
    pub backup: ArchiveBackupBehavior,
    pub restore: ArchiveRestoreBehavior,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupProfile {
    pub id: String,
    pub revision: u32,
    pub rules_digest: String,
    pub rules: Vec<BackupProfileRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveProfileManifest {
    pub id: String,
    pub revision: u32,
    pub rules_digest: String,
    pub excluded_entries: u64,
    pub excluded_logical_size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveBackupType {
    ProfiledAccount,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version_code: Option<u64>,
    pub android_version: String,
    pub device: String,
    pub created_at_millis: u64,
    pub scope: ArchiveScope,
    pub active_account_id: Option<String>,
    pub launch_after_reboot: bool,
    pub logical_size: u64,
    pub accounts: Vec<ArchiveAccountManifest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_type: Option<ArchiveBackupType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<ArchiveProfileManifest>,
}

/// What the Manager needs from a manifest: everything except the per-entry list, plus the
/// digest of the full manifest so a restore can prove it read the same archive it previewed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainSummary {
    pub state: DomainState,
    pub logical_size: u64,
    pub entry_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveAccountSummary {
    pub archive_account_id: String,
    pub name: String,
    pub kind: ArchiveAccountKind,
    pub source_slot: String,
    pub ce: DomainSummary,
    pub de: DomainSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveSummary {
    pub format_version: u8,
    pub package: String,
    pub signing_kind: ArchiveSigningKind,
    pub signing_sha256: Vec<String>,
    pub app_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version_code: Option<u64>,
    pub android_version: String,
    pub device: String,
    pub created_at_millis: u64,
    pub scope: ArchiveScope,
    pub active_account_id: Option<String>,
    pub launch_after_reboot: bool,
    pub logical_size: u64,
    pub accounts: Vec<ArchiveAccountSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_type: Option<ArchiveBackupType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<ArchiveProfileManifest>,
    pub manifest_sha256: String,
}

impl ArchiveManifest {
    pub fn summary(&self) -> Result<ArchiveSummary, ArchiveError> {
        let bytes = serde_json::to_vec(self).map_err(io_error)?;
        let domain = |manifest: &DomainManifest| DomainSummary {
            state: manifest.state,
            logical_size: manifest.logical_size,
            entry_count: manifest.entries.len() as u64,
        };
        Ok(ArchiveSummary {
            format_version: self.format_version,
            package: self.package.clone(),
            signing_kind: self.signing_kind,
            signing_sha256: self.signing_sha256.clone(),
            app_version: self.app_version.clone(),
            app_version_code: self.app_version_code,
            android_version: self.android_version.clone(),
            device: self.device.clone(),
            created_at_millis: self.created_at_millis,
            scope: self.scope,
            active_account_id: self.active_account_id.clone(),
            launch_after_reboot: self.launch_after_reboot,
            logical_size: self.logical_size,
            accounts: self
                .accounts
                .iter()
                .map(|account| ArchiveAccountSummary {
                    archive_account_id: account.archive_account_id.clone(),
                    name: account.name.clone(),
                    kind: account.kind,
                    source_slot: account.source_slot.clone(),
                    ce: domain(&account.ce),
                    de: domain(&account.de),
                })
                .collect(),
            backup_type: self.backup_type,
            profile: self.profile.clone(),
            manifest_sha256: format!("{:x}", Sha256::digest(&bytes)),
        })
    }
}

/// Reports bytes processed against a known total, once per whole percent.
pub struct Progress<'a> {
    done: u64,
    total: u64,
    last_percent: Option<u64>,
    sink: &'a mut dyn FnMut(u64, u64),
}

impl<'a> Progress<'a> {
    pub fn new(sink: &'a mut dyn FnMut(u64, u64)) -> Self {
        Self {
            done: 0,
            total: 0,
            last_percent: None,
            sink,
        }
    }

    fn set_total(&mut self, total: u64) {
        self.total = total;
        self.done = 0;
        self.last_percent = None;
        self.advance(0);
    }

    fn advance(&mut self, bytes: u64) {
        self.done = self.done.saturating_add(bytes).min(self.total);
        let percent = self
            .done
            .saturating_mul(100)
            .checked_div(self.total)
            .unwrap_or(100);
        if self.last_percent != Some(percent) {
            self.last_percent = Some(percent);
            (self.sink)(self.done, self.total);
        }
    }
}

fn ignore_progress(_done: u64, _total: u64) {}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version_code: Option<u64>,
    pub android_version: String,
    pub device: String,
    pub created_at_millis: u64,
    pub scope: ArchiveScope,
    pub active_account_id: Option<String>,
    pub launch_after_reboot: bool,
    pub sources: Vec<ArchiveSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<BackupProfile>,
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
    Backup(Box<BackupRequest>),
    Inspect {
        input_path: PathBuf,
        password: Option<String>,
    },
    Restore(RestoreRequest),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HelperResponse {
    Manifest { manifest: Box<ArchiveSummary> },
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
    if length == 0 || length > MAX_CONTROL_FRAME_BYTES as usize {
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
    create_backup_with_progress(request, None, &mut Progress::new(&mut ignore_progress))
}

pub fn create_backup_for_owner(
    request: BackupRequest,
    uid: u32,
    gid: u32,
) -> Result<ArchiveManifest, ArchiveError> {
    create_backup_with_progress(
        request,
        Some((uid, gid)),
        &mut Progress::new(&mut ignore_progress),
    )
}

/// Creates the archive, reporting progress over both passes: hashing for the manifest, then
/// streaming every file into the archive while hashing it again against that manifest.
pub fn create_backup_with_progress(
    mut request: BackupRequest,
    output_owner: Option<(u32, u32)>,
    progress: &mut Progress<'_>,
) -> Result<ArchiveManifest, ArchiveError> {
    validate_backup_request(&request)?;
    let manifest = build_manifest(&request, progress)?;
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
                let age_writer =
                    write_compressed_tar(age_writer, &manifest, &request.sources, progress)?;
                age_writer.finish().map_err(io_error)?
            }
            None => write_compressed_tar(output, &manifest, &request.sources, progress)?,
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

/// Reads only the header and the manifest, which is all a preview shows.
///
/// A wrong password or a damaged header fails here. Every entry is still verified during
/// `restore_backup`, which writes only into Runtime staging and clears it on any failure, so a
/// damaged body is rejected before any account is committed.
pub fn inspect_backup(
    input_path: &Path,
    password: Option<String>,
) -> Result<ArchiveManifest, ArchiveError> {
    let reader = open_payload(input_path, password)?;
    let decoder = zstd::stream::read::Decoder::new(reader).map_err(io_error)?;
    let mut archive = tar::Archive::new(decoder);
    let mut entries = archive.entries().map_err(io_error)?;
    let first = entries.next().ok_or(ArchiveError::Invalid)?;
    read_manifest_entry(first.map_err(io_error)?)
}

fn read_manifest_entry<R: Read>(
    mut first: tar::Entry<'_, R>,
) -> Result<ArchiveManifest, ArchiveError> {
    if first.path().map_err(io_error)?.as_ref() != Path::new("manifest.json")
        || first.size() > MAX_MANIFEST_BYTES
    {
        return Err(ArchiveError::Invalid);
    }
    let capacity = usize::try_from(first.size()).map_err(|_error| ArchiveError::Invalid)?;
    let mut bytes = Vec::with_capacity(capacity);
    first.read_to_end(&mut bytes).map_err(io_error)?;
    let manifest: ArchiveManifest =
        serde_json::from_slice(&bytes).map_err(|_error| ArchiveError::Invalid)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub fn restore_backup(request: RestoreRequest) -> Result<ArchiveManifest, ArchiveError> {
    restore_backup_with_progress(request, &mut Progress::new(&mut ignore_progress))
}

pub fn restore_backup_with_progress(
    mut request: RestoreRequest,
    progress: &mut Progress<'_>,
) -> Result<ArchiveManifest, ArchiveError> {
    let password = request.password.take();
    let result = restore_backup_inner(
        &request.input_path,
        password,
        &request.destinations,
        progress,
    );
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
    progress: &mut Progress<'_>,
) -> Result<ArchiveManifest, ArchiveError> {
    let reader = open_payload(input_path, password)?;
    let decoder = zstd::stream::read::Decoder::new(reader).map_err(io_error)?;
    let mut archive = tar::Archive::new(decoder);
    let (manifest, directories) = {
        let mut entries = archive.entries().map_err(io_error)?;
        let first = entries.next().ok_or(ArchiveError::Invalid)?;
        let manifest = read_manifest_entry(first.map_err(io_error)?)?;
        progress.set_total(manifest.logical_size);
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
        let mut tally = ExtractTally {
            extracted: 0,
            declared_total: manifest.logical_size,
            progress,
        };
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
                &mut tally,
                &mut directories,
            )?;
        }
        if seen.len() != expected.len() || tally.extracted != manifest.logical_size {
            return Err(ArchiveError::Invalid);
        }
        (manifest, directories)
    };
    validate_archive_end(archive.into_inner())?;
    restore_directory_metadata(directories)?;
    // One syncfs per staging root replaces an fsync per extracted file; success is only
    // reported after everything written above is durable.
    for destination in destinations {
        sync_filesystem(&destination.ce_path)?;
        sync_filesystem(&destination.de_path)?;
    }
    Ok(manifest)
}

fn sync_filesystem(path: &Path) -> Result<(), ArchiveError> {
    let directory = File::open(path).map_err(io_error)?;
    rustix::fs::syncfs(&directory).map_err(|error| io_error(std::io::Error::from(error)))
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
    match (&request.profile, request.app_version_code) {
        (Some(profile), Some(_)) => validate_backup_profile(profile)?,
        (None, None) => {}
        _ => return Err(ArchiveError::Invalid),
    }
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

fn build_manifest(
    request: &BackupRequest,
    progress: &mut Progress<'_>,
) -> Result<ArchiveManifest, ArchiveError> {
    let mut accounts = Vec::with_capacity(request.sources.len());
    let mut logical_size = 0_u64;
    let mut excluded = ExcludedStats::default();
    let rules = request
        .profile
        .as_ref()
        .map(|profile| profile.rules.as_slice())
        .unwrap_or_default();
    for source in &request.sources {
        let ce = scan_domain(
            &source.ce_path,
            ArchiveDomain::Ce,
            rules,
            request.profile.is_some(),
        )?;
        let de = scan_domain(
            &source.de_path,
            ArchiveDomain::De,
            rules,
            request.profile.is_some(),
        )?;
        excluded.add(&ce.excluded)?;
        excluded.add(&de.excluded)?;
        logical_size = logical_size
            .checked_add(ce.manifest.logical_size)
            .and_then(|size| size.checked_add(de.manifest.logical_size))
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
            ce: ce.manifest,
            de: de.manifest,
        });
    }
    // The walk above only stats. Hashing is a separate pass so progress knows its total:
    // this pass reads every byte once, and writing the archive reads it once more.
    progress.set_total(logical_size.saturating_mul(2));
    for (account, source) in accounts.iter_mut().zip(&request.sources) {
        hash_domain_files(&source.ce_path, &mut account.ce, progress)?;
        hash_domain_files(&source.de_path, &mut account.de, progress)?;
    }
    let profile = request
        .profile
        .as_ref()
        .map(|profile| ArchiveProfileManifest {
            id: profile.id.clone(),
            revision: profile.revision,
            rules_digest: profile.rules_digest.clone(),
            excluded_entries: excluded.entries,
            excluded_logical_size: excluded.logical_size,
        });
    let manifest = ArchiveManifest {
        format_version: if profile.is_some() { 2 } else { 1 },
        package: request.package.clone(),
        signing_kind: request.signing_kind,
        signing_sha256: request.signing_sha256.clone(),
        app_version: request.app_version.clone(),
        app_version_code: profile.as_ref().and(request.app_version_code),
        android_version: request.android_version.clone(),
        device: request.device.clone(),
        created_at_millis: request.created_at_millis,
        scope: request.scope,
        active_account_id: request.active_account_id.clone(),
        launch_after_reboot: request.launch_after_reboot,
        logical_size,
        accounts,
        backup_type: profile.as_ref().map(|_| ArchiveBackupType::ProfiledAccount),
        profile,
    };
    validate_manifest(&manifest)?;
    Ok(manifest)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ExcludedStats {
    entries: u64,
    logical_size: u64,
}

impl ExcludedStats {
    fn add(&mut self, other: &Self) -> Result<(), ArchiveError> {
        self.entries = self
            .entries
            .checked_add(other.entries)
            .ok_or(ArchiveError::Invalid)?;
        self.logical_size = self
            .logical_size
            .checked_add(other.logical_size)
            .filter(|size| *size <= MAX_LOGICAL_BYTES)
            .ok_or(ArchiveError::Invalid)?;
        Ok(())
    }
}

fn hash_domain_files(
    root: &Path,
    domain: &mut DomainManifest,
    progress: &mut Progress<'_>,
) -> Result<(), ArchiveError> {
    for entry in &mut domain.entries {
        if entry.kind != ManifestEntryKind::File {
            continue;
        }
        let file = File::open(safe_join(root, Path::new(&entry.path))?).map_err(io_error)?;
        let mut reader = HashingReader::new(file.take(entry.size), progress);
        std::io::copy(&mut reader, &mut std::io::sink()).map_err(io_error)?;
        let (digest, read) = reader.finish();
        // A file that changed size after the walk would not match its manifest line.
        if read != entry.size {
            return Err(ArchiveError::Invalid);
        }
        entry.sha256 = Some(digest);
    }
    Ok(())
}

/// Hashes and counts what passes through, reporting each chunk as progress.
struct HashingReader<'p, 's, R> {
    inner: R,
    hasher: Sha256,
    read: u64,
    progress: &'p mut Progress<'s>,
}

impl<'p, 's, R: Read> HashingReader<'p, 's, R> {
    fn new(inner: R, progress: &'p mut Progress<'s>) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
            read: 0,
            progress,
        }
    }

    fn finish(self) -> (String, u64) {
        (format!("{:x}", self.hasher.finalize()), self.read)
    }
}

impl<R: Read> Read for HashingReader<'_, '_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.hasher.update(&buffer[..read]);
        self.read = self.read.saturating_add(read as u64);
        self.progress.advance(read as u64);
        Ok(read)
    }
}

struct DomainScan {
    manifest: DomainManifest,
    excluded: ExcludedStats,
}

struct ScanContext<'a> {
    root: &'a Path,
    domain: ArchiveDomain,
    rules: &'a [BackupProfileRule],
    record_exclusions: bool,
}

fn scan_domain(
    root: &Path,
    domain: ArchiveDomain,
    rules: &[BackupProfileRule],
    record_exclusions: bool,
) -> Result<DomainScan, ArchiveError> {
    require_real_directory(root)?;
    let mut entries = Vec::new();
    let mut excluded = ExcludedStats::default();
    let context = ScanContext {
        root,
        domain,
        rules,
        record_exclusions,
    };
    collect_entries(&context, root, 0, &mut entries, &mut excluded)?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    if entries.len() > MAX_ENTRIES {
        return Err(ArchiveError::Invalid);
    }
    let logical_size = entries.iter().try_fold(0_u64, |total, entry| {
        total.checked_add(entry.size).ok_or(ArchiveError::Invalid)
    })?;
    Ok(DomainScan {
        manifest: DomainManifest {
            state: if entries.is_empty() {
                DomainState::Empty
            } else {
                DomainState::Data
            },
            logical_size,
            entries,
        },
        excluded,
    })
}

fn collect_entries(
    context: &ScanContext<'_>,
    directory: &Path,
    depth: usize,
    entries: &mut Vec<ManifestEntry>,
    excluded: &mut ExcludedStats,
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
        let path = entry.path();
        let relative = path
            .strip_prefix(context.root)
            .map_err(|_error| ArchiveError::Invalid)?;
        validate_relative_path(relative)?;
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        let global_exclusion = depth == 0 && EXCLUDED_TOP_LEVEL.contains(&name.as_str());
        let profile_exclusion = context.rules.iter().any(|rule| {
            rule.domain == context.domain && profile_path_matches(&rule.path, relative)
        });
        if global_exclusion || profile_exclusion {
            if profile_exclusion
                && (!metadata.file_type().is_dir() || metadata.file_type().is_symlink())
            {
                return Err(ArchiveError::Invalid);
            }
            if context.record_exclusions {
                collect_excluded_stats(context.root, &path, &metadata, excluded)?;
            }
            continue;
        }
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
            collect_entries(context, &path, depth.saturating_add(1), entries, excluded)?;
        } else if metadata.file_type().is_file() {
            if metadata.nlink() != 1 {
                return Err(ArchiveError::Invalid);
            }
            entries.push(ManifestEntry {
                path: path_text(relative)?,
                kind: ManifestEntryKind::File,
                size: metadata.len(),
                sha256: None,
                link_target: None,
                mode,
                mtime_seconds,
            });
        } else if metadata.file_type().is_symlink() {
            let target = fs::read_link(&path).map_err(io_error)?;
            if target.is_absolute() || !relative_link_stays_inside(context.root, &path, &target) {
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

fn collect_excluded_stats(
    root: &Path,
    path: &Path,
    metadata: &fs::Metadata,
    stats: &mut ExcludedStats,
) -> Result<(), ArchiveError> {
    stats.entries = stats.entries.checked_add(1).ok_or(ArchiveError::Invalid)?;
    if stats.entries > MAX_ENTRIES as u64 {
        return Err(ArchiveError::Invalid);
    }
    if metadata.file_type().is_file() {
        stats.logical_size = stats
            .logical_size
            .checked_add(metadata.len())
            .filter(|size| *size <= MAX_LOGICAL_BYTES)
            .ok_or(ArchiveError::Invalid)?;
    } else if metadata.file_type().is_dir() {
        for entry in fs::read_dir(path).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let child = entry.path();
            let child_metadata = fs::symlink_metadata(&child).map_err(io_error)?;
            collect_excluded_stats(root, &child, &child_metadata, stats)?;
        }
    } else if metadata.file_type().is_symlink() {
        let target = fs::read_link(path).map_err(io_error)?;
        if target.is_absolute() || !relative_link_stays_inside(root, path, &target) {
            return Err(ArchiveError::Invalid);
        }
    } else {
        return Err(ArchiveError::Invalid);
    }
    Ok(())
}

fn write_compressed_tar<W: Write>(
    writer: W,
    manifest: &ArchiveManifest,
    sources: &[ArchiveSource],
    progress: &mut Progress<'_>,
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
            progress,
        )?;
        append_domain(
            &mut builder,
            &source.de_path,
            &account.archive_account_id,
            "de",
            &account.de,
            progress,
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
    progress: &mut Progress<'_>,
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
                if metadata.len() != entry.size {
                    return Err(ArchiveError::Invalid);
                }
                header.set_entry_type(tar::EntryType::Regular);
                header.set_size(entry.size);
                header.set_cksum();
                // Hash while streaming instead of re-reading the file before appending it.
                // A mismatch fails the whole archive, which is still an unpublished temp file.
                let file = File::open(source).map_err(io_error)?;
                let mut reader = HashingReader::new(file.take(entry.size), progress);
                builder
                    .append_data(&mut header, archive_path, &mut reader)
                    .map_err(io_error)?;
                let (digest, read) = reader.finish();
                if read != entry.size || digest != entry.sha256_value()? {
                    return Err(ArchiveError::Invalid);
                }
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
    if !matches!(manifest.format_version, 1 | 2)
        || manifest.package.is_empty()
        || manifest.accounts.is_empty()
        || manifest.accounts.len() > MAX_ACCOUNTS
        || manifest.logical_size > MAX_LOGICAL_BYTES
    {
        return Err(ArchiveError::Incompatible);
    }
    match manifest.format_version {
        1 if manifest.profile.is_some()
            || manifest.app_version_code.is_some()
            || manifest.backup_type.is_some() =>
        {
            return Err(ArchiveError::Invalid);
        }
        2 => {
            let profile = manifest.profile.as_ref().ok_or(ArchiveError::Invalid)?;
            if manifest.app_version_code.is_none()
                || manifest.backup_type != Some(ArchiveBackupType::ProfiledAccount)
            {
                return Err(ArchiveError::Invalid);
            }
            validate_profile_identity(&profile.id, profile.revision, &profile.rules_digest)?;
            if profile.excluded_logical_size > MAX_LOGICAL_BYTES
                || profile.excluded_entries > MAX_ENTRIES as u64
            {
                return Err(ArchiveError::Invalid);
            }
        }
        _ => {}
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

fn validate_backup_profile(profile: &BackupProfile) -> Result<(), ArchiveError> {
    validate_profile_identity(&profile.id, profile.revision, &profile.rules_digest)?;
    if profile.rules.is_empty() || profile.rules.len() > 64 {
        return Err(ArchiveError::Invalid);
    }
    for rule in &profile.rules {
        validate_profile_path(&rule.path)?;
    }
    for (index, left) in profile.rules.iter().enumerate() {
        for right in profile.rules.iter().skip(index + 1) {
            if left.domain == right.domain && profile_patterns_overlap(&left.path, &right.path) {
                return Err(ArchiveError::Invalid);
            }
        }
    }
    Ok(())
}

fn validate_profile_identity(
    id: &str,
    revision: u32,
    rules_digest: &str,
) -> Result<(), ArchiveError> {
    validate_archive_id(id)?;
    if revision == 0
        || rules_digest.len() != 64
        || !rules_digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(ArchiveError::Invalid);
    }
    Ok(())
}

fn validate_profile_path(value: &str) -> Result<(), ArchiveError> {
    if value.is_empty() || value.len() > MAX_PATH_BYTES || value.starts_with('/') {
        return Err(ArchiveError::Invalid);
    }
    let segments = value.split('/').collect::<Vec<_>>();
    if segments.is_empty()
        || segments.iter().any(|segment| {
            segment.is_empty()
                || matches!(*segment, "." | ".." | "**")
                || (*segment != "*"
                    && !segment.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                    }))
        })
    {
        return Err(ArchiveError::Invalid);
    }
    Ok(())
}

fn profile_path_matches(pattern: &str, path: &Path) -> bool {
    let pattern = pattern.split('/').collect::<Vec<_>>();
    let path = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    pattern.len() == path.len()
        && pattern
            .iter()
            .zip(path)
            .all(|(expected, actual)| *expected == "*" || *expected == actual)
}

fn profile_patterns_overlap(left: &str, right: &str) -> bool {
    let left = left.split('/').collect::<Vec<_>>();
    let right = right.split('/').collect::<Vec<_>>();
    let shared = left.len().min(right.len());
    left.iter()
        .take(shared)
        .zip(right.iter().take(shared))
        .all(|(left, right)| left == right || *left == "*" || *right == "*")
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

struct ExtractTally<'a, 'b> {
    extracted: u64,
    declared_total: u64,
    progress: &'a mut Progress<'b>,
}

fn extract_entry<R: Read>(
    entry: &mut tar::Entry<'_, R>,
    domain_root: &Path,
    output: &Path,
    expected: &ManifestEntry,
    tally: &mut ExtractTally<'_, '_>,
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
            tally.extracted = tally
                .extracted
                .checked_add(expected.size)
                .filter(|size| *size <= tally.declared_total)
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
                tally.progress.advance(read as u64);
            }
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
            app_version_code: None,
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
            profile: None,
        }
    }

    fn profiled_request(root: &Path) -> BackupRequest {
        let mut request = request(root, false);
        let ce = &request.sources[0].ce_path;
        fs::set_permissions(ce.join("files"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir_all(
            ce.join("files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer/version/patch"),
        )
        .unwrap();
        fs::write(
            ce.join("files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer/version/patch/a.pak"),
            vec![7_u8; 8192],
        )
        .unwrap();
        fs::create_dir_all(ce.join("files/UE4Game/DeltaForce/DeltaForce/Saved/Dolphin/1.2.3/Paks"))
            .unwrap();
        fs::write(
            ce.join("files/UE4Game/DeltaForce/DeltaForce/Saved/Dolphin/1.2.3/Paks/a.pak"),
            vec![8_u8; 4096],
        )
        .unwrap();
        fs::write(
            ce.join("files/UE4Game/DeltaForce/DeltaForce/Saved/Dolphin/config.dat"),
            b"account-related",
        )
        .unwrap();
        fs::write(ce.join("files/MSDK.mmap3"), b"login-state").unwrap();
        request.app_version_code = Some(2019);
        request.profile = Some(BackupProfile {
            id: "delta-force-cn-account-v1".to_owned(),
            revision: 1,
            rules_digest: "b".repeat(64),
            rules: vec![
                BackupProfileRule {
                    domain: ArchiveDomain::Ce,
                    path: "files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer".to_owned(),
                    backup: ArchiveBackupBehavior::Exclude,
                    restore: ArchiveRestoreBehavior::Preserve,
                },
                BackupProfileRule {
                    domain: ArchiveDomain::Ce,
                    path: "files/UE4Game/DeltaForce/DeltaForce/Saved/Dolphin/*/Paks".to_owned(),
                    backup: ArchiveBackupBehavior::Exclude,
                    restore: ArchiveRestoreBehavior::Preserve,
                },
            ],
        });
        request
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
    fn profiled_archive_prunes_downloadable_resources_without_dropping_account_state() {
        let root = tempfile::tempdir().unwrap();
        let request = profiled_request(root.path());
        let output = request.output_path.clone();

        let manifest = create_backup(request).unwrap();

        assert_eq!(manifest.format_version, 2);
        assert_eq!(
            manifest.backup_type,
            Some(ArchiveBackupType::ProfiledAccount)
        );
        assert_eq!(manifest.app_version_code, Some(2019));
        let profile = manifest.profile.as_ref().unwrap();
        assert_eq!(profile.id, "delta-force-cn-account-v1");
        assert_eq!(profile.excluded_logical_size, 8192 + 4096 + 5);
        let paths = manifest.accounts[0]
            .ce
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>();
        assert!(paths.contains(&"files/MSDK.mmap3"));
        assert!(paths.contains(&"files/UE4Game/DeltaForce/DeltaForce/Saved/Dolphin/config.dat"));
        assert!(!paths.iter().any(|path| path.contains("Puffer")));
        assert!(!paths.iter().any(|path| path.contains("/Paks")));
        assert_eq!(inspect_backup(&output, None).unwrap(), manifest);

        let restored_ce = root.path().join("restore-profiled/ce");
        let restored_de = root.path().join("restore-profiled/de");
        fs::create_dir_all(&restored_ce).unwrap();
        fs::create_dir_all(&restored_de).unwrap();
        assert_eq!(
            restore_backup(RestoreRequest {
                input_path: output,
                password: None,
                destinations: vec![RestoreDestination {
                    archive_account_id: "account-0".to_owned(),
                    ce_path: restored_ce.clone(),
                    de_path: restored_de,
                }],
            })
            .unwrap(),
            manifest,
        );
        assert_eq!(
            fs::read(restored_ce.join("files/MSDK.mmap3")).unwrap(),
            b"login-state",
        );
        assert!(
            !restored_ce
                .join("files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer")
                .exists()
        );
    }

    #[test]
    fn profile_paths_are_validated_before_scanning() {
        let root = tempfile::tempdir().unwrap();
        let mut request = profiled_request(root.path());
        request.profile.as_mut().unwrap().rules[0].path = "../Puffer".to_owned();
        assert!(matches!(create_backup(request), Err(ArchiveError::Invalid)));

        let root = tempfile::tempdir().unwrap();
        let mut request = profiled_request(root.path());
        let duplicate = request.profile.as_ref().unwrap().rules[0].clone();
        request.profile.as_mut().unwrap().rules.push(duplicate);
        assert!(matches!(create_backup(request), Err(ArchiveError::Invalid)));
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
        // The preview only reads the manifest; the full restore below must reject the body.
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
    fn helper_summary_keeps_counts_and_digest_without_the_entry_list() {
        let root = tempfile::tempdir().unwrap();
        let request = request(root.path(), false);
        let output = request.output_path.clone();
        let manifest = create_backup(request).unwrap();

        let summary = manifest.summary().unwrap();
        let account = &manifest.accounts[0];
        assert_eq!(
            summary.accounts[0].ce.entry_count,
            account.ce.entries.len() as u64
        );
        assert_eq!(
            summary.accounts[0].de.entry_count,
            account.de.entries.len() as u64
        );
        assert_eq!(summary.logical_size, manifest.logical_size);
        assert_eq!(
            inspect_backup(&output, None).unwrap().summary().unwrap(),
            summary
        );
        let json = serde_json::to_string(&HelperResponse::Manifest {
            manifest: Box::new(summary),
        })
        .unwrap();
        assert!(!json.contains("\"entries\""));
        assert!(json.contains("\"manifest_sha256\""));
    }

    #[test]
    fn progress_reports_both_backup_passes_and_the_restore() {
        let root = tempfile::tempdir().unwrap();
        let request = request(root.path(), false);
        let output = request.output_path.clone();
        let mut backup_reports = Vec::new();
        let mut sink = |done, total| backup_reports.push((done, total));
        let manifest =
            create_backup_with_progress(request, None, &mut Progress::new(&mut sink)).unwrap();

        assert!(manifest.logical_size > 0);
        assert_eq!(
            backup_reports.last(),
            Some(&(manifest.logical_size * 2, manifest.logical_size * 2))
        );
        assert!(backup_reports.windows(2).all(|pair| pair[0].0 <= pair[1].0));

        let ce = root.path().join("restore/ce");
        let de = root.path().join("restore/de");
        fs::create_dir_all(&ce).unwrap();
        fs::create_dir_all(&de).unwrap();
        let mut restore_reports = Vec::new();
        let mut sink = |done, total| restore_reports.push((done, total));
        restore_backup_with_progress(
            RestoreRequest {
                input_path: output,
                password: None,
                destinations: vec![RestoreDestination {
                    archive_account_id: "account-0".to_owned(),
                    ce_path: ce,
                    de_path: de,
                }],
            },
            &mut Progress::new(&mut sink),
        )
        .unwrap();
        assert_eq!(
            restore_reports.last(),
            Some(&(manifest.logical_size, manifest.logical_size))
        );
    }

    #[test]
    fn an_account_with_tens_of_thousands_of_files_round_trips() {
        let root = tempfile::tempdir().unwrap();
        let mut request = request(root.path(), false);
        let images = request.sources[0]
            .ce_path
            .join("MicroMsg/0123456789abcdef0123456789abcdef/image2");
        // The old 4 MiB manifest limit rejected about 16,000 files with paths like these.
        for index in 0..30_000_u32 {
            let directory = images.join(format!("{:02x}", index / 128));
            if index % 128 == 0 {
                fs::create_dir_all(&directory).unwrap();
            }
            fs::write(
                directory.join(format!("th_{index:032x}")),
                index.to_le_bytes(),
            )
            .unwrap();
        }
        request.password = None;
        let output = request.output_path.clone();
        let manifest = create_backup(request).unwrap();
        assert!(serde_json::to_vec(&manifest).unwrap().len() > 4 * 1024 * 1024);

        let ce = root.path().join("restore/ce");
        let de = root.path().join("restore/de");
        fs::create_dir_all(&ce).unwrap();
        fs::create_dir_all(&de).unwrap();
        restore_backup(RestoreRequest {
            input_path: output,
            password: None,
            destinations: vec![RestoreDestination {
                archive_account_id: "account-0".to_owned(),
                ce_path: ce.clone(),
                de_path: de,
            }],
        })
        .unwrap();
        let last = ce.join(format!(
            "MicroMsg/0123456789abcdef0123456789abcdef/image2/{:02x}/th_{:032x}",
            29_999_u32 / 128,
            29_999_u32
        ));
        assert_eq!(fs::read(last).unwrap(), 29_999_u32.to_le_bytes());
    }

    #[test]
    fn archive_scope_requires_a_complete_active_account_and_base_contract() {
        let root = tempfile::tempdir().unwrap();
        let mut manifest = build_manifest(
            &request(root.path(), false),
            &mut Progress::new(&mut ignore_progress),
        )
        .unwrap();
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
