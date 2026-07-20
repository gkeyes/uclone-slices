#![doc = "Bounded Android filesystem and process fact tests."]
#![allow(
    clippy::redundant_pub_crate,
    clippy::unwrap_used,
    reason = "included production module keeps its original crate-relative visibility"
)]
#![allow(
    unreachable_pub,
    reason = "production module is included as a test fixture"
)]

use std::fs;
use std::os::unix::fs::{MetadataExt as _, symlink};

pub use uclone_slot_runtime::{android, bridge, domain, layout};

#[path = "../src/android/system/mod.rs"]
mod system;

use domain::PackageName;
use system::{FactError, ProcessSet};

fn package() -> PackageName {
    PackageName::parse("com.uclone.slotprobe").unwrap()
}

#[test]
fn arm64_zygote_facts_ignore_the_unrelated_32_bit_zygote() {
    let temporary = tempfile::tempdir().unwrap();
    let zygote64 = temporary.path().join("1913");
    let zygote32 = temporary.path().join("32525");
    fs::create_dir(&zygote64).unwrap();
    fs::create_dir(&zygote32).unwrap();
    fs::write(zygote64.join("cmdline"), b"zygote64\0start-system-server").unwrap();
    fs::write(zygote32.join("cmdline"), b"zygote\0start-system-server").unwrap();

    let processes = system::arm64_zygote_processes_at(temporary.path()).unwrap();

    assert_eq!(processes.pids(), &[1913]);
}

#[test]
fn process_set_accepts_an_empty_package_process_sample() {
    // Given an observation with no running package processes.
    let pids = Vec::new();

    // When the bounded set is constructed.
    let processes = ProcessSet::new(pids).unwrap();

    // Then the empty sample remains representable.
    assert!(processes.pids().is_empty());
}

#[test]
fn process_set_canonicalizes_unique_nonzero_pids() {
    // Given unique process identifiers in filesystem enumeration order.
    let pids = vec![42, 7, 19];

    // When the bounded set is constructed.
    let processes = ProcessSet::new(pids).unwrap();

    // Then callers receive one deterministic ascending set.
    assert_eq!(processes.pids(), &[7, 19, 42]);
}

#[test]
fn process_set_rejects_zero_pid() {
    // Given a PID list containing the kernel sentinel.
    let pids = vec![0];

    // When the bounded set is constructed.
    let result = ProcessSet::new(pids);

    // Then the unaddressable PID is rejected.
    assert_eq!(result, Err(FactError::Invalid));
}

#[test]
fn process_set_rejects_duplicate_pid() {
    // Given a PID repeated by an incoherent observation.
    let pids = vec![91, 91];

    // When the bounded set is constructed.
    let result = ProcessSet::new(pids);

    // Then the sample is rejected rather than silently deduplicated.
    assert_eq!(result, Err(FactError::Invalid));
}

#[test]
fn process_set_enforces_the_4096_process_bound() {
    // Given exactly the maximum number of valid process identifiers.
    let maximum = (1..=4096).collect();

    // When the bounded set is constructed.
    let processes = ProcessSet::new(maximum).unwrap();

    // Then the inclusive boundary is accepted.
    assert_eq!(processes.pids().len(), 4096);
}

#[test]
fn process_set_rejects_more_than_4096_processes() {
    // Given one process beyond the fixed observation capacity.
    let excessive = (1..=4097).collect();

    // When the bounded set is constructed.
    let result = ProcessSet::new(excessive);

    // Then the oversized observation is rejected.
    assert_eq!(result, Err(FactError::Invalid));
}

#[test]
fn mirror_facts_accept_one_matching_volume() {
    // Given one well-formed CE/DE mirror volume containing the fixed package.
    let temporary = tempfile::tempdir().unwrap();
    let ce_root = temporary.path().join("data_ce");
    let de_root = temporary.path().join("data_de");
    let ce_package = ce_root.join("volume-a/0/com.uclone.slotprobe");
    let de_package = de_root.join("volume-a/0/com.uclone.slotprobe");
    fs::create_dir_all(&ce_package).unwrap();
    fs::create_dir_all(&de_package).unwrap();

    // When the mirror roots are sampled.
    let inodes = system::mirror_inodes_at(&ce_root, &de_root, &package()).unwrap();

    // Then the exact two directory inodes are returned.
    assert_eq!(inodes.ce().get(), fs::metadata(ce_package).unwrap().ino());
    assert_eq!(inodes.de().get(), fs::metadata(de_package).unwrap().ino());
}

#[test]
fn mirror_facts_reject_multiple_matching_volumes() {
    // Given two CE volumes exposing the package and one DE match.
    let temporary = tempfile::tempdir().unwrap();
    let ce_root = temporary.path().join("data_ce");
    let de_root = temporary.path().join("data_de");
    fs::create_dir_all(ce_root.join("volume-a/0/com.uclone.slotprobe")).unwrap();
    fs::create_dir_all(ce_root.join("volume-b/0/com.uclone.slotprobe")).unwrap();
    fs::create_dir_all(de_root.join("volume-a/0/com.uclone.slotprobe")).unwrap();

    // When the ambiguous roots are sampled.
    let result = system::mirror_inodes_at(&ce_root, &de_root, &package());

    // Then ambiguity fails closed.
    assert_eq!(result, Err(FactError::Invalid));
}

#[test]
fn mirror_facts_reject_a_package_symlink() {
    // Given a mirror package entry that redirects outside its volume.
    let temporary = tempfile::tempdir().unwrap();
    let ce_root = temporary.path().join("data_ce");
    let de_root = temporary.path().join("data_de");
    let outside = temporary.path().join("outside");
    fs::create_dir_all(ce_root.join("volume-a/0")).unwrap();
    fs::create_dir_all(de_root.join("volume-a/0/com.uclone.slotprobe")).unwrap();
    fs::create_dir(&outside).unwrap();
    symlink(&outside, ce_root.join("volume-a/0/com.uclone.slotprobe")).unwrap();

    // When the unsafe root is sampled.
    let result = system::mirror_inodes_at(&ce_root, &de_root, &package());

    // Then the symlink is rejected.
    assert_eq!(result, Err(FactError::Invalid));
}

#[test]
fn slot_facts_reject_a_partial_ce_de_pair() {
    // Given only the CE half of a derived slot pair.
    let temporary = tempfile::tempdir().unwrap();
    let ce = temporary.path().join("ce");
    let de = temporary.path().join("de");
    fs::create_dir(&ce).unwrap();

    // When the exact pair is sampled.
    let result = system::slot_inodes_at(&ce, &de);

    // Then a partial slot is invalid.
    assert_eq!(result, Err(FactError::Invalid));
}

#[test]
fn slot_facts_report_none_only_when_both_paths_are_absent() {
    // Given two absent derived slot paths.
    let temporary = tempfile::tempdir().unwrap();
    let ce = temporary.path().join("ce");
    let de = temporary.path().join("de");

    // When the exact pair is sampled.
    let result = system::slot_inodes_at(&ce, &de);

    // Then absence is represented without inventing an inode pair.
    assert_eq!(result, Ok(None));
}
