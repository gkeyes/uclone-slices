#![allow(
    unreachable_pub,
    dead_code,
    unused_imports,
    reason = "path-included integration fixtures exercise strict parser helpers"
)]

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt as _;

use crate::android::MountCounts;
use crate::domain::{DataInodes, PackageName, SlotId};
use crate::layout::RuntimeLayout;

use super::facts::FactError;

pub const MAX_MOUNTINFO_BYTES: usize = 1_048_576;

/// Parses an exact Linux mount-namespace symbolic-link target.
pub fn parse_mount_namespace(link: &OsStr) -> Result<u64, FactError> {
    let digits = link
        .as_bytes()
        .strip_prefix(b"mnt:[")
        .and_then(|remainder| remainder.strip_suffix(b"]"))
        .ok_or(FactError::Invalid)?;
    parse_nonzero_decimal(digits)
}

/// Counts exact canonical CE and DE targets in caller-bounded mountinfo bytes.
pub fn parse_canonical_mount_counts(
    input: &[u8],
    package: &PackageName,
) -> Result<MountCounts, FactError> {
    if input.is_empty() || input.len() > MAX_MOUNTINFO_BYTES {
        return Err(FactError::Invalid);
    }

    let base = RuntimeLayout::slot_paths(package, &SlotId::base());
    let ce_target = base.ce().as_os_str().as_bytes();
    let de_target = base.de().as_os_str().as_bytes();
    let body = input.strip_suffix(b"\n").unwrap_or(input);
    if body.is_empty() {
        return Err(FactError::Invalid);
    }

    let mut ce = 0_u32;
    let mut de = 0_u32;
    for line in body.split(|byte| *byte == b'\n') {
        let target = parse_mount_target(line)?;
        if decoded_field_matches(target, ce_target)? {
            ce = ce.checked_add(1).ok_or(FactError::Invalid)?;
        }
        if decoded_field_matches(target, de_target)? {
            de = de.checked_add(1).ok_or(FactError::Invalid)?;
        }
    }
    Ok(MountCounts::new(ce, de))
}

/// Parses the exact CE-then-DE inode output emitted by the fixed stat command.
pub fn parse_inode_pair(input: &[u8]) -> Result<DataInodes, FactError> {
    let body = input.strip_suffix(b"\n").ok_or(FactError::Invalid)?;
    let mut lines = body.split(|byte| *byte == b'\n');
    let ce = lines.next().ok_or(FactError::Invalid)?;
    let de = lines.next().ok_or(FactError::Invalid)?;
    if lines.next().is_some() {
        return Err(FactError::Invalid);
    }

    DataInodes::new(parse_nonzero_decimal(ce)?, parse_nonzero_decimal(de)?)
        .map_err(|_| FactError::Invalid)
}

/// Returns the first nonempty UTF-8 field from a procfs cmdline record.
pub fn parse_process_name(input: &[u8]) -> Result<&str, FactError> {
    let first = input
        .split(|byte| *byte == 0)
        .next()
        .ok_or(FactError::Invalid)?;
    let name = std::str::from_utf8(first).map_err(|_| FactError::Invalid)?;
    if name.is_empty() || name.bytes().any(|byte| matches!(byte, b'\n' | b'\r')) {
        Err(FactError::Invalid)
    } else {
        Ok(name)
    }
}

fn parse_mount_target(line: &[u8]) -> Result<&[u8], FactError> {
    let mut fields = line.split(|byte| *byte == b' ');
    let mount_id = fields.next().ok_or(FactError::Invalid)?;
    let parent_id = fields.next().ok_or(FactError::Invalid)?;
    let device = fields.next().ok_or(FactError::Invalid)?;
    let root = fields.next().ok_or(FactError::Invalid)?;
    let target = fields.next().ok_or(FactError::Invalid)?;
    let mount_options = fields.next().ok_or(FactError::Invalid)?;

    parse_nonzero_decimal(mount_id)?;
    parse_nonzero_decimal(parent_id)?;
    validate_device(device)?;
    validate_absolute_mount_field(root)?;
    validate_absolute_mount_field(target)?;
    validate_token(mount_options)?;

    let mut separator_found = false;
    for field in fields.by_ref() {
        validate_token(field)?;
        if field == b"-" {
            separator_found = true;
            break;
        }
    }
    if !separator_found {
        return Err(FactError::Invalid);
    }

    let filesystem = fields.next().ok_or(FactError::Invalid)?;
    let source = fields.next().ok_or(FactError::Invalid)?;
    let super_options = fields.next().ok_or(FactError::Invalid)?;
    if fields.next().is_some() {
        return Err(FactError::Invalid);
    }
    validate_token(filesystem)?;
    validate_kernel_field(source)?;
    validate_token(super_options)?;
    Ok(target)
}

fn validate_device(field: &[u8]) -> Result<(), FactError> {
    let mut components = field.split(|byte| *byte == b':');
    let major = components.next().ok_or(FactError::Invalid)?;
    let minor = components.next().ok_or(FactError::Invalid)?;
    if components.next().is_some() {
        return Err(FactError::Invalid);
    }
    parse_decimal(major)?;
    parse_decimal(minor)?;
    Ok(())
}

fn validate_absolute_mount_field(field: &[u8]) -> Result<(), FactError> {
    if !field.starts_with(b"/") {
        return Err(FactError::Invalid);
    }
    validate_kernel_field(field)
}

fn validate_kernel_field(field: &[u8]) -> Result<(), FactError> {
    validate_token(field)?;
    let mut bytes = field.iter().copied();
    while next_decoded_byte(&mut bytes)?.is_some() {}
    Ok(())
}

fn decoded_field_matches(field: &[u8], expected: &[u8]) -> Result<bool, FactError> {
    let mut bytes = field.iter().copied();
    let mut expected = expected.iter().copied();
    let mut matches = true;
    while let Some(actual) = next_decoded_byte(&mut bytes)? {
        if expected.next() != Some(actual) {
            matches = false;
        }
    }
    Ok(matches && expected.next().is_none())
}

fn next_decoded_byte(bytes: &mut impl Iterator<Item = u8>) -> Result<Option<u8>, FactError> {
    let Some(byte) = bytes.next() else {
        return Ok(None);
    };
    if byte != b'\\' {
        return Ok(Some(byte));
    }
    let escape = (
        bytes.next().ok_or(FactError::Invalid)?,
        bytes.next().ok_or(FactError::Invalid)?,
        bytes.next().ok_or(FactError::Invalid)?,
    );
    match escape {
        (b'0', b'4', b'0') => Ok(Some(b' ')),
        (b'0', b'1', b'1') => Ok(Some(b'\t')),
        (b'0', b'1', b'2') => Ok(Some(b'\n')),
        (b'1', b'3', b'4') => Ok(Some(b'\\')),
        _ => Err(FactError::Invalid),
    }
}

fn validate_token(field: &[u8]) -> Result<(), FactError> {
    if field.is_empty()
        || field
            .iter()
            .any(|byte| byte.is_ascii_control() || *byte == b' ')
    {
        Err(FactError::Invalid)
    } else {
        Ok(())
    }
}

fn parse_nonzero_decimal(input: &[u8]) -> Result<u64, FactError> {
    let value = parse_decimal(input)?;
    if value == 0 {
        Err(FactError::Invalid)
    } else {
        Ok(value)
    }
}

fn parse_decimal(input: &[u8]) -> Result<u64, FactError> {
    if input.is_empty() {
        return Err(FactError::Invalid);
    }
    input.iter().try_fold(0_u64, |value, byte| {
        if !byte.is_ascii_digit() {
            return Err(FactError::Invalid);
        }
        value
            .checked_mul(10)
            .and_then(|scaled| scaled.checked_add(u64::from(*byte - b'0')))
            .ok_or(FactError::Invalid)
    })
}
