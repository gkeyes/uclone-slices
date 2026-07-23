use std::collections::BTreeSet;
use std::fs;

use crate::domain::PackageName;

use super::GateLeaseError;
use super::filesystem::existing_gate_root;

pub(super) fn package_names() -> Result<Vec<PackageName>, GateLeaseError> {
    let Some(root) = existing_gate_root()? else {
        return Ok(Vec::new());
    };
    let entries = fs::read_dir(root).map_err(|_| GateLeaseError::Unavailable)?;
    let mut packages = BTreeSet::new();
    for entry in entries {
        let entry = entry.map_err(|_| GateLeaseError::Unavailable)?;
        if !entry.file_type().is_ok_and(|file_type| file_type.is_file()) {
            return Err(GateLeaseError::UnsafeArtifact);
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| GateLeaseError::InvalidArtifact)?;
        if let Some(package) = active_package(&name)? {
            packages.insert(package);
        }
    }
    Ok(packages.into_iter().collect())
}

fn active_package(name: &str) -> Result<Option<PackageName>, GateLeaseError> {
    if matches!(
        name,
        "startup-gate.pending" | "startup-gate.ready" | "emergency-containment.request"
    ) {
        return Ok(None);
    }
    if let Some(raw) = name
        .strip_suffix(".gate")
        .filter(|raw| !raw.starts_with('.'))
    {
        return parse(raw).map(Some);
    }
    if let Some(raw) = name
        .strip_prefix('.')
        .and_then(|value| value.strip_suffix(".gate.retiring"))
    {
        return parse(raw).map(Some);
    }
    if name
        .strip_prefix('.')
        .and_then(|value| value.strip_suffix(".gate.retired"))
        .is_some()
    {
        return Ok(None);
    }
    if let Some((raw, suffix)) = name
        .strip_prefix('.')
        .and_then(|value| value.split_once(".gate.upgrade-"))
    {
        let mut numbers = suffix.split('-');
        let valid = numbers.next().is_some_and(|value| {
            !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
        }) && numbers.next().is_some_and(|value| {
            !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
        }) && numbers.next().is_none();
        return valid
            .then(|| parse(raw))
            .ok_or(GateLeaseError::InvalidArtifact)?
            .map(Some);
    }
    Err(GateLeaseError::InvalidArtifact)
}

fn parse(raw: &str) -> Result<PackageName, GateLeaseError> {
    PackageName::parse(raw).map_err(|_| GateLeaseError::InvalidArtifact)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_live_and_transitional_artifacts_only() {
        for name in [
            "com.example.app.gate",
            ".com.example.app.gate.retiring",
            ".com.example.app.gate.upgrade-42-7",
        ] {
            assert_eq!(
                active_package(name).unwrap().unwrap().as_str(),
                "com.example.app"
            );
        }
        assert_eq!(
            active_package(".com.example.app.gate.retired").unwrap(),
            None
        );
        for name in [
            "startup-gate.pending",
            "startup-gate.ready",
            "emergency-containment.request",
        ] {
            assert_eq!(active_package(name).unwrap(), None);
        }
        assert!(active_package("../bad.gate").is_err());
        assert!(active_package("unknown.request").is_err());
        assert!(active_package(".com.example.app.gate.upgrade-x-1").is_err());
    }
}
