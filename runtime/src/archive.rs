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

use crate::links::link_resolves_inside;
use crate::validation::{self, MAX_PATH_BYTES};

const MAGIC: &[u8; 8] = b"UCSBKP01";
const FLAG_ENCRYPTED: u8 = 1;
const MAX_CONTROL_FRAME_BYTES: u64 = 4 * 1024 * 1024;
// A manifest line is about 260 bytes for a typical app data path (measured: 4 MiB held about
// 16,000 files). 64 MiB keeps roughly 250,000 entries in one in-memory manifest.
const MAX_MANIFEST_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ACCOUNTS: usize = 256;
const MAX_ENTRIES: usize = 1_000_000;
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
        ensure_restore_capacity(manifest.accounts.iter().flat_map(|account| {
            let destination = destination_map[account.archive_account_id.as_str()];
            [
                (&destination.ce_path, &account.ce),
                (&destination.de_path, &account.de),
            ]
        }))?;
        // Entries still to be extracted. Each archive entry takes its manifest entry out, so a
        // duplicate or undeclared path finds nothing, and anything left at the end is missing.
        let mut expected = manifest_entry_index(&manifest);
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
            let domain_index = if domain == "ce" { 0 } else { 1 };
            let expected_entry = expected
                .get_mut(account_id)
                .and_then(|domains| domains[domain_index].remove(relative.as_path()))
                .ok_or(ArchiveError::Invalid)?;
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
        if expected
            .values()
            .flatten()
            .any(|entries| !entries.is_empty())
            || tally.extracted != manifest.logical_size
        {
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
                crate::model::BASE_ACCOUNT_NAME.to_owned()
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
        let file = open_regular_beneath(root, Path::new(&entry.path))?;
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
    collect_entries(&context, &mut entries, &mut excluded)?;
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

/// Walks the domain with an explicit stack: the tree belongs to the app, so its depth must
/// not translate into daemon or helper stack depth.
fn collect_entries(
    context: &ScanContext<'_>,
    entries: &mut Vec<ManifestEntry>,
    excluded: &mut ExcludedStats,
) -> Result<(), ArchiveError> {
    let mut pending = vec![(context.root.to_path_buf(), 0_usize)];
    while let Some((directory, depth)) = pending.pop() {
        collect_directory_entries(context, &directory, depth, entries, excluded, &mut pending)?;
    }
    Ok(())
}

fn collect_directory_entries(
    context: &ScanContext<'_>,
    directory: &Path,
    depth: usize,
    entries: &mut Vec<ManifestEntry>,
    excluded: &mut ExcludedStats,
    pending: &mut Vec<(PathBuf, usize)>,
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
            pending.push((path, depth.saturating_add(1)));
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
            if target.is_absolute()
                || !relative_link_stays_inside(context.root, &path, &target)
                || !link_resolves_inside(context.root, &path).map_err(io_error)?
            {
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
    let mut pending = vec![(path.to_path_buf(), metadata.clone())];
    while let Some((path, metadata)) = pending.pop() {
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
            for entry in fs::read_dir(&path).map_err(io_error)? {
                let child = entry.map_err(io_error)?.path();
                let child_metadata = fs::symlink_metadata(&child).map_err(io_error)?;
                pending.push((child, child_metadata));
            }
        } else if metadata.file_type().is_symlink() {
            let target = fs::read_link(&path).map_err(io_error)?;
            if target.is_absolute()
                || !relative_link_stays_inside(root, &path, &target)
                || !link_resolves_inside(root, &path).map_err(io_error)?
            {
                return Err(ArchiveError::Invalid);
            }
        } else {
            return Err(ArchiveError::Invalid);
        }
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
                let file = open_regular_beneath(root, Path::new(&entry.path))?;
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
    validation::is_signing_digest_set(kind == ArchiveSigningKind::Multiple, digests)
        .then_some(())
        .ok_or(ArchiveError::Invalid)
}

fn validate_backup_profile(profile: &BackupProfile) -> Result<(), ArchiveError> {
    validate_profile_identity(&profile.id, profile.revision, &profile.rules_digest)?;
    if profile.rules.is_empty()
        || profile.rules.len() > validation::MAX_PATH_RULES
        || !profile
            .rules
            .iter()
            .all(|rule| validation::is_path_pattern(&rule.path))
        || validation::any_patterns_overlap(
            profile
                .rules
                .iter()
                .map(|rule| (rule.domain, rule.path.as_str())),
        )
    {
        return Err(ArchiveError::Invalid);
    }
    Ok(())
}

fn validate_profile_identity(
    id: &str,
    revision: u32,
    rules_digest: &str,
) -> Result<(), ArchiveError> {
    validate_archive_id(id)?;
    if revision == 0 || !validation::is_sha256(rules_digest) {
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

fn validate_manifest_entry(entry: &ManifestEntry) -> Result<(), ArchiveError> {
    if entry.mtime_seconds > i64::MAX as u64 {
        return Err(ArchiveError::Invalid);
    }
    match entry.kind {
        ManifestEntryKind::File => {
            let digest = entry.sha256.as_deref().ok_or(ArchiveError::Invalid)?;
            if !validation::is_sha256(digest) || entry.link_target.is_some() {
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

/// Manifest entries by account id, then domain (`[ce, de]`), then relative path, all
/// borrowed from the manifest. Paths compare by component, as the archive paths they are
/// matched against do.
type ManifestEntryIndex<'a> = BTreeMap<&'a str, [BTreeMap<&'a Path, &'a ManifestEntry>; 2]>;

fn manifest_entry_index(manifest: &ArchiveManifest) -> ManifestEntryIndex<'_> {
    let mut index = ManifestEntryIndex::new();
    for account in &manifest.accounts {
        let domains = index
            .entry(account.archive_account_id.as_str())
            .or_default();
        for (entries, content) in domains.iter_mut().zip([&account.ce, &account.de]) {
            for entry in &content.entries {
                entries.insert(Path::new(entry.path.as_str()), entry);
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
    validation::is_identifier(value)
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

/// Opens a file that the scan saw as a regular file at `relative` under `root`, refusing it if
/// anything on the way has since been replaced. Every directory below `root` is opened with
/// `O_NOFOLLOW` relative to the one before it, so a parent the app swaps for a symbolic link
/// cannot redirect root outside the tree, and the file itself is opened non-blocking so a FIFO
/// swapped in for it fails the type check instead of hanging the helper.
fn open_regular_beneath(root: &Path, relative: &Path) -> Result<File, ArchiveError> {
    use rustix::fs::{Mode, OFlags};

    validate_relative_path(relative)?;
    let directory_flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
    let mut directory = rustix::fs::open(root, directory_flags, Mode::empty()).map_err(io_error)?;
    let mut components = relative.components().peekable();
    let name = loop {
        let Some(Component::Normal(name)) = components.next() else {
            return Err(ArchiveError::Invalid);
        };
        if components.peek().is_none() {
            break name;
        }
        directory = rustix::fs::openat(
            &directory,
            name,
            directory_flags | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .map_err(io_error)?;
    };
    let file = File::from(
        rustix::fs::openat(
            &directory,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::NOCTTY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(io_error)?,
    );
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.file_type().is_file() || metadata.nlink() != 1 {
        return Err(ArchiveError::Invalid);
    }
    Ok(file)
}

/// Free space a restore must leave on each filesystem so the system stays usable afterwards.
const RESTORE_RESERVE_BYTES: u64 = 64 * 1024 * 1024;

/// Refuses a restore that cannot fit before anything is written. Each domain is charged to the
/// filesystem it lands on, every file is rounded up to whole blocks, every entry also costs one
/// block of metadata and one inode, and a reserve keeps the filesystem usable afterwards.
fn ensure_restore_capacity<'a>(
    domains: impl IntoIterator<Item = (&'a PathBuf, &'a DomainManifest)>,
) -> Result<(), ArchiveError> {
    struct Demand {
        stats: rustix::fs::StatVfs,
        bytes: u64,
        inodes: u64,
    }
    let mut devices = BTreeMap::<u64, Demand>::new();
    for (root, manifest) in domains {
        let device = fs::metadata(root).map_err(io_error)?.dev();
        let demand = match devices.entry(device) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => entry.insert(Demand {
                stats: rustix::fs::statvfs(root.as_path()).map_err(io_error)?,
                bytes: RESTORE_RESERVE_BYTES,
                inodes: 0,
            }),
        };
        let block = demand.stats.f_frsize;
        for entry in &manifest.entries {
            demand.bytes = demand
                .bytes
                .saturating_add(allocated_bytes(entry.size, block));
        }
        // Extraction also creates any parent directory the manifest does not list itself.
        let implied = implied_parent_count(manifest);
        demand.bytes = demand
            .bytes
            .saturating_add(implied.saturating_mul(allocated_bytes(0, block)));
        demand.inodes = demand
            .inodes
            .saturating_add(manifest.entries.len() as u64)
            .saturating_add(implied);
    }
    for demand in devices.values() {
        let free_bytes = demand.stats.f_bavail.saturating_mul(demand.stats.f_frsize);
        // Filesystems without a fixed inode table report zero total inodes.
        let inodes_short = demand.stats.f_files != 0 && demand.inodes > demand.stats.f_favail;
        if demand.bytes > free_bytes || inodes_short {
            return Err(ArchiveError::InsufficientStorage);
        }
    }
    Ok(())
}

/// Number of parent directories extraction must create because the manifest has no entry for them.
fn implied_parent_count(manifest: &DomainManifest) -> u64 {
    let listed = manifest
        .entries
        .iter()
        .map(|entry| Path::new(entry.path.as_str()))
        .collect::<BTreeSet<_>>();
    let mut implied = BTreeSet::new();
    for entry in &manifest.entries {
        for parent in Path::new(entry.path.as_str()).ancestors().skip(1) {
            if parent.as_os_str().is_empty() || listed.contains(parent) {
                continue;
            }
            implied.insert(parent);
        }
    }
    implied.len() as u64
}

/// Bytes a file of `size` occupies: whole data blocks plus one block of metadata, so even an
/// empty file or a directory is never free.
fn allocated_bytes(size: u64, block: u64) -> u64 {
    let block = block.max(1);
    size.div_ceil(block).saturating_add(1).saturating_mul(block)
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
mod tests;
