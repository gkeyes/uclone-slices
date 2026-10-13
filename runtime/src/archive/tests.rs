#![allow(clippy::unwrap_used)]

use super::*;

fn ignore_progress(_done: u64, _total: u64) {}

fn create_backup(request: BackupRequest) -> Result<ArchiveManifest, ArchiveError> {
    create_backup_with_progress(request, None, &mut Progress::new(&mut ignore_progress))
}

fn create_backup_for_owner(
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

fn restore_backup(request: RestoreRequest) -> Result<ArchiveManifest, ArchiveError> {
    restore_backup_with_progress(request, &mut Progress::new(&mut ignore_progress))
}

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
    filetime::set_symlink_file_times(ce.join("files/account-link"), link_time, link_time).unwrap();
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
    fs::create_dir_all(ce.join("files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer/version/patch"))
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
fn restore_that_does_not_fit_is_refused_before_writing() -> Result<(), ArchiveError> {
    let temp = tempfile::tempdir().map_err(io_error)?;
    let root = temp.path().to_path_buf();

    let domain = |sizes: &[u64]| DomainManifest {
        state: DomainState::Data,
        logical_size: sizes.iter().sum(),
        entries: sizes
            .iter()
            .enumerate()
            .map(|(index, size)| ManifestEntry {
                path: format!("file{index}"),
                kind: ManifestEntryKind::File,
                size: *size,
                sha256: None,
                link_target: None,
                mode: 0o600,
                mtime_seconds: 0,
            })
            .collect(),
    };
    let small = domain(&[0, 1]);
    ensure_restore_capacity([(&root, &small), (&root, &small)])?;
    let huge = domain(&[u64::MAX / 2]);
    assert!(matches!(
        ensure_restore_capacity([(&root, &small), (&root, &huge)]),
        Err(ArchiveError::InsufficientStorage),
    ));
    // Empty files and directories still cost a block, and partial blocks round up.
    assert_eq!(allocated_bytes(0, 4096), 4096);
    assert_eq!(allocated_bytes(1, 4096), 8192);
    assert_eq!(allocated_bytes(4096, 4096), 8192);
    assert_eq!(allocated_bytes(u64::MAX, 4096), u64::MAX);
    // `a/b/c` alone implies `a` and `a/b`; a listed directory is not counted twice.
    let mut nested = domain(&[1]);
    nested.entries[0].path = "a/b/c".to_owned();
    assert_eq!(implied_parent_count(&nested), 2);
    let mut listed_parent = nested.clone();
    listed_parent.entries.push(ManifestEntry {
        path: "a".to_owned(),
        kind: ManifestEntryKind::Directory,
        ..nested.entries[0].clone()
    });
    assert_eq!(implied_parent_count(&listed_parent), 1);
    Ok(())
}

#[test]
fn file_swapped_for_a_symlink_after_the_scan_is_not_read() -> Result<(), ArchiveError> {
    let temp = tempfile::tempdir().map_err(io_error)?;
    fs::write(temp.path().join("secret"), b"secret").map_err(io_error)?;
    symlink("secret", temp.path().join("swapped")).map_err(io_error)?;

    fs::create_dir(temp.path().join("dir")).map_err(io_error)?;
    fs::write(temp.path().join("dir/inner"), b"inner").map_err(io_error)?;
    symlink("dir", temp.path().join("linked-dir")).map_err(io_error)?;
    rustix::fs::mknodat(
        rustix::fs::CWD,
        temp.path().join("fifo"),
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .map_err(io_error)?;

    let root = temp.path();
    assert!(open_regular_beneath(root, Path::new("secret")).is_ok());
    assert!(open_regular_beneath(root, Path::new("dir/inner")).is_ok());
    assert!(open_regular_beneath(root, Path::new("swapped")).is_err());
    // A parent swapped for a link is refused even though the final name is a regular file.
    assert!(open_regular_beneath(root, Path::new("linked-dir/inner")).is_err());
    // A FIFO fails the type check at once instead of blocking for a writer.
    assert!(matches!(
        open_regular_beneath(root, Path::new("fifo")),
        Err(ArchiveError::Invalid),
    ));
    Ok(())
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

#[test]
fn deep_app_trees_are_scanned_without_recursion() {
    // A recursive walk of this depth needs far more than the small stack below, while an
    // iterative walk needs only one directory level of it.
    const DEPTH: usize = 1500;
    const SMALL_STACK_BYTES: usize = 256 * 1024;
    let root = tempfile::tempdir().unwrap();
    let ce = root.path().join("ce");
    let deep = ["files", "cache"].map(|top| {
        let mut path = ce.join(top);
        for _ in 0..DEPTH {
            path.push("d");
        }
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("leaf"), b"leaf").unwrap();
        path
    });
    assert!(deep.iter().all(|path| path.join("leaf").is_file()));

    let scan = std::thread::Builder::new()
        .stack_size(SMALL_STACK_BYTES)
        .spawn(move || scan_domain(&ce, ArchiveDomain::Ce, &[], true))
        .unwrap()
        .join()
        .unwrap()
        .unwrap();

    // files + DEPTH directories + leaf; the excluded cache tree counts the same way.
    assert_eq!(scan.manifest.entries.len(), DEPTH + 2);
    assert_eq!(scan.manifest.logical_size, 4);
    assert_eq!(scan.excluded.entries, (DEPTH + 2) as u64);
    assert_eq!(scan.excluded.logical_size, 4);
}

/// Rewrites the tar body of a plain archive: `edit` gets each data entry (path, header, data)
/// in order and returns the entries to write in its place.
fn rewrite_plain_archive(
    path: &Path,
    mut edit: impl FnMut(String, tar::Header, Vec<u8>) -> Vec<(tar::Header, Vec<u8>)>,
) {
    let bytes = fs::read(path).unwrap();
    assert_eq!(&bytes[..MAGIC.len()], MAGIC);
    assert_eq!(bytes[MAGIC.len()], 0);
    let body = zstd::stream::decode_all(&bytes[MAGIC.len() + 1..]).unwrap();
    let mut archive = tar::Archive::new(body.as_slice());
    let mut builder = tar::Builder::new(Vec::new());
    for (index, entry) in archive.entries().unwrap().enumerate() {
        let mut entry = entry.unwrap();
        let entry_path = entry.path().unwrap().to_string_lossy().into_owned();
        let header = entry.header().clone();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        let replacements = if index == 0 {
            vec![(header, data)]
        } else {
            edit(entry_path, header, data)
        };
        for (header, data) in replacements {
            builder.append(&header, data.as_slice()).unwrap();
        }
    }
    let tar = builder.into_inner().unwrap();
    let mut output = MAGIC.to_vec();
    output.push(0);
    output.extend(zstd::stream::encode_all(tar.as_slice(), 0).unwrap());
    fs::write(path, output).unwrap();
}

fn restore_plain_into_fresh_staging(root: &Path, input: PathBuf) -> Result<(), ArchiveError> {
    let ce = root.join("restore/ce");
    let de = root.join("restore/de");
    let _ = fs::remove_dir_all(root.join("restore"));
    fs::create_dir_all(&ce).unwrap();
    fs::create_dir_all(&de).unwrap();
    let result = restore_backup(RestoreRequest {
        input_path: input,
        password: None,
        destinations: vec![RestoreDestination {
            archive_account_id: "account-0".to_owned(),
            ce_path: ce.clone(),
            de_path: de.clone(),
        }],
    });
    if result.is_err() {
        assert!(fs::read_dir(&ce).unwrap().next().is_none());
        assert!(fs::read_dir(&de).unwrap().next().is_none());
    }
    result.map(|_manifest| ())
}

#[test]
fn restore_requires_each_manifest_entry_exactly_once() {
    const LINK: &str = "accounts/account-0/ce/files/account-link";
    let root = tempfile::tempdir().unwrap();
    let request = request(root.path(), false);
    let original = request.output_path.clone();
    create_backup(request).unwrap();
    restore_plain_into_fresh_staging(root.path(), original.clone()).unwrap();

    let duplicated = root.path().join("duplicated.ucsbackup");
    fs::copy(&original, &duplicated).unwrap();
    rewrite_plain_archive(&duplicated, |path, header, data| {
        if path == LINK {
            vec![(header.clone(), data.clone()), (header, data)]
        } else {
            vec![(header, data)]
        }
    });
    assert!(matches!(
        restore_plain_into_fresh_staging(root.path(), duplicated),
        Err(ArchiveError::Invalid)
    ));

    let missing = root.path().join("missing.ucsbackup");
    fs::copy(&original, &missing).unwrap();
    let mut dropped = false;
    rewrite_plain_archive(&missing, |path, header, data| {
        if path == LINK {
            dropped = true;
            Vec::new()
        } else {
            vec![(header, data)]
        }
    });
    assert!(dropped);
    assert!(matches!(
        restore_plain_into_fresh_staging(root.path(), missing),
        Err(ArchiveError::Invalid)
    ));

    let undeclared = root.path().join("undeclared.ucsbackup");
    fs::copy(&original, &undeclared).unwrap();
    rewrite_plain_archive(&undeclared, |path, header, data| {
        let mut entries = vec![(header.clone(), data)];
        if path == LINK {
            let mut extra = header;
            extra
                .set_path("accounts/account-0/ce/files/undeclared-link")
                .unwrap();
            extra.set_cksum();
            entries.push((extra, Vec::new()));
        }
        entries
    });
    assert!(matches!(
        restore_plain_into_fresh_staging(root.path(), undeclared),
        Err(ArchiveError::Invalid)
    ));
}
