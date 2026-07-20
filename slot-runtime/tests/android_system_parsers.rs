#![doc = "Strict parser tests for Android procfs and namespace facts."]
#![allow(
    clippy::redundant_pub_crate,
    clippy::unwrap_used,
    reason = "private parser harness mirrors crate module visibility for included production code"
)]

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStringExt as _;

use uclone_slot_runtime as runtime;

mod android {
    pub(crate) use crate::runtime::android::MountCounts;
}

mod domain {
    pub(crate) use crate::runtime::domain::{DataInodes, PackageName, SlotId};
}

mod layout {
    pub(crate) use crate::runtime::layout::RuntimeLayout;
}

mod facts {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum FactError {
        Invalid,
    }
}

#[path = "../src/android/system/parse.rs"]
mod parse;

use facts::FactError;
use runtime::domain::PackageName;

fn package() -> PackageName {
    PackageName::parse("com.uclone.slotprobe").unwrap()
}

fn mount_line(target: &str) -> String {
    format!("31 24 0:27 / {target} rw,nosuid shared:7 - ext4 /dev/block/dm-7 rw")
}

#[test]
fn parses_mount_namespace_when_link_is_exact() {
    let parsed = parse::parse_mount_namespace(OsStr::new("mnt:[4026531840]"));

    assert_eq!(parsed, Ok(4_026_531_840));
}

#[test]
fn rejects_mount_namespace_when_link_is_malformed() {
    let malformed = [
        "",
        "mnt:[]",
        "mnt:[0]",
        "mnt:[+1]",
        "mnt:[1x]",
        "mnt:[1]\n",
        "pid:[1]",
        "mnt:1",
        "mnt:[18446744073709551616]",
    ];

    for raw in malformed {
        assert_eq!(
            parse::parse_mount_namespace(OsStr::new(raw)),
            Err(FactError::Invalid),
            "unexpectedly accepted {raw:?}"
        );
    }
    let non_utf8 = OsString::from_vec(vec![b'm', b'n', b't', b':', b'[', 0xff, b']']);
    assert_eq!(
        parse::parse_mount_namespace(&non_utf8),
        Err(FactError::Invalid)
    );
}

#[test]
fn counts_only_exact_canonical_mount_targets() {
    let mountinfo = [
        mount_line("/data/user/0/com.uclone.slotprobe"),
        mount_line("/data/user/0/com.uclone.slotprobe"),
        mount_line("/data/user_de/0/com.uclone.slotprobe"),
        mount_line("/data/user/0/com.uclone.slotprobe.child"),
        mount_line("/prefix/data/user_de/0/com.uclone.slotprobe"),
    ]
    .join("\n");

    let counts = parse::parse_canonical_mount_counts(mountinfo.as_bytes(), &package()).unwrap();

    assert_eq!(counts.ce(), 2);
    assert_eq!(counts.de(), 1);
}

#[test]
fn accepts_only_kernel_mount_escapes() {
    let mountinfo = format!(
        "{}\n{}\n",
        mount_line("/unrelated\\040space\\011tab\\012line\\134slash"),
        mount_line("/data/user/0/com.uclone.slotprobe")
    );

    let counts = parse::parse_canonical_mount_counts(mountinfo.as_bytes(), &package()).unwrap();

    assert_eq!(counts, android::MountCounts::new(1, 0));
}

#[test]
fn ignores_other_packages_and_rejects_invalid_mountinfo_lines() {
    let valid = mount_line("/data/user/0/com.uclone.slotprobe");
    let other = PackageName::parse("com.example.other").unwrap();
    assert_eq!(
        parse::parse_canonical_mount_counts(valid.as_bytes(), &other),
        Ok(android::MountCounts::new(0, 0))
    );

    let malformed = [
        "",
        "31 24 0:27 / /target rw,nosuid ext4 /dev/block/dm-7 rw",
        "31 24 0:27 / /target rw,nosuid - ext4 source",
        "31 24 0:27 / /target rw,nosuid - ext4 source rw extra",
        "x 24 0:27 / /target rw,nosuid - ext4 source rw",
        "31 24 0:x / /target rw,nosuid - ext4 source rw",
        "31 24 0:27 / /bad\\777path rw,nosuid - ext4 source rw",
        "31 24 0:27 / /bad\\04 rw,nosuid - ext4 source rw",
        "31  24 0:27 / /target rw,nosuid - ext4 source rw",
        "31 24 0:27 / /target rw,nosuid - ext4 source rw\n\n",
    ];
    for raw in malformed {
        assert_eq!(
            parse::parse_canonical_mount_counts(raw.as_bytes(), &package()),
            Err(FactError::Invalid),
            "unexpectedly accepted {raw:?}"
        );
    }
}

#[test]
fn rejects_mountinfo_larger_than_the_reader_contract() {
    let oversized = vec![b'x'; 1_048_577];

    let parsed = parse::parse_canonical_mount_counts(&oversized, &package());

    assert_eq!(parsed, Err(FactError::Invalid));
}

#[test]
fn parses_exactly_two_nonzero_inode_lines() {
    let parsed = parse::parse_inode_pair(b"101\n202\n").unwrap();

    assert_eq!(parsed.ce().get(), 101);
    assert_eq!(parsed.de().get(), 202);
}

#[test]
fn rejects_inode_output_when_arity_or_decimal_is_invalid() {
    let malformed: [&[u8]; 11] = [
        b"",
        b"101\n",
        b"101\n202\n303\n",
        b"101\n202\n\n",
        b"0\n202\n",
        b"101\n0\n",
        b"+101\n202\n",
        b"101 \n202\n",
        b"101\r\n202\r\n",
        b"101\n2x2\n",
        b"18446744073709551616\n202\n",
    ];

    for raw in malformed {
        assert_eq!(parse::parse_inode_pair(raw), Err(FactError::Invalid));
    }
}

#[test]
fn parses_first_nonempty_utf8_cmdline_field() {
    assert_eq!(
        parse::parse_process_name(b"com.uclone.slotprobe:worker\0--flag\0"),
        Ok("com.uclone.slotprobe:worker")
    );
    assert_eq!(parse::parse_process_name(b"zygote64"), Ok("zygote64"));
    assert_eq!(
        parse::parse_process_name(b"zygote\0ignored\nargument\0"),
        Ok("zygote")
    );
}

#[test]
fn rejects_cmdline_when_first_field_is_invalid() {
    let malformed: [&[u8]; 5] = [
        b"",
        b"\0second\0",
        b"bad\nname\0",
        b"bad\rname\0",
        b"bad\xffname\0",
    ];

    for raw in malformed {
        assert_eq!(parse::parse_process_name(raw), Err(FactError::Invalid));
    }
}
