use std::path::{Component, Path};

use super::{BridgeError, invalid_response};

pub(super) fn validate_request_id(value: &str) -> Result<(), BridgeError> {
    if !(1..=128).contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(invalid_response("requestId is not bounded ASCII"));
    }
    Ok(())
}

pub(super) fn validate_package_name(value: &str) -> Result<(), BridgeError> {
    if value.is_empty()
        || value.len() > 255
        || value.split('.').any(|segment| {
            segment.is_empty()
                || !segment
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_alphabetic())
                || !segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
        || value.split('.').count() < 2
    {
        return Err(invalid_response(
            "packageName is not a safe package identifier",
        ));
    }
    Ok(())
}

pub(super) fn validate_text(
    value: &str,
    max: usize,
    allow_empty: bool,
    field: &str,
) -> Result<(), BridgeError> {
    if value.len() > max
        || (!allow_empty && value.is_empty())
        || value.chars().any(|character| {
            character == '\0' || character == '\n' || character == '\r' || character.is_control()
        })
    {
        return Err(invalid_response(format!(
            "{field} is malformed or too long"
        )));
    }
    Ok(())
}

pub(super) fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn validate_code_path(value: &str) -> Result<(), BridgeError> {
    if value.len() > 4096
        || !value.starts_with("/data/app/")
        || value.contains('\0')
        || value.contains('\n')
        || value.contains('\r')
        || value.contains('\\')
    {
        return Err(invalid_response("codePath is outside /data/app"));
    }
    let path = Path::new(value);
    if !path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::CurDir | Component::Prefix(_)
            )
        })
        || value.ends_with('/')
        || value
            .strip_prefix("/data/app/")
            .is_none_or(|suffix| suffix.split('/').any(str::is_empty))
    {
        return Err(invalid_response(
            "codePath contains traversal or empty segments",
        ));
    }
    Ok(())
}

pub(super) fn validate_data_path(
    value: &str,
    root: &str,
    package_name: &str,
) -> Result<(), BridgeError> {
    let expected = format!("{root}{package_name}");
    if value != expected {
        return Err(invalid_response("package data path is not canonical"));
    }
    Ok(())
}
