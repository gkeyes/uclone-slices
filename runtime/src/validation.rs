//! Validation rules shared by the runtime model, the archive format and the `uclone_archive`
//! helper, so each rule has exactly one implementation.

use std::collections::BTreeSet;

/// Longest relative path or path pattern accepted anywhere in the runtime or an archive.
pub const MAX_PATH_BYTES: usize = 4096;
/// Most signing certificate digests one signing identity may list.
pub const MAX_SIGNING_DIGESTS: usize = 16;
/// Most preserve or exclude rules one restore policy or backup profile may list.
pub const MAX_PATH_RULES: usize = 64;

/// An Android package name: at least two `.`-separated segments of ASCII letters, digits or `_`,
/// none of them empty. Only the characters that keep it one safe path component are checked;
/// there is no length limit, since Android itself decides what can be installed
/// (see `docs/SETTINGS_PROVENANCE.md`).
pub fn is_package_name(value: &str) -> bool {
    value.split('.').count() >= 2
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
}

/// A slot directory name: `base` or `slot-<digits>`.
pub fn is_slot_id(value: &str) -> bool {
    value == "base"
        || value.strip_prefix("slot-").is_some_and(|number| {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        })
}

/// An identifier that is also safe as one path component: archive account ids, transfer ids and
/// backup profile ids. 1 to 64 ASCII letters, digits, `-` or `_`.
pub fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// Exactly `len` lowercase hexadecimal digits.
pub fn is_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

/// An account I/O token: 32 lowercase hexadecimal digits.
pub fn is_io_token(value: &str) -> bool {
    is_lower_hex(value, 32)
}

/// A SHA-256 digest: 64 lowercase hexadecimal digits.
pub fn is_sha256(value: &str) -> bool {
    is_lower_hex(value, 64)
}

/// The certificate digests of one signing identity: 1 to 16 distinct SHA-256 digests. A
/// `multiple` signer set lists at least two, sorted ascending.
pub fn is_signing_digest_set(multiple: bool, digests: &[String]) -> bool {
    !digests.is_empty()
        && digests.len() <= MAX_SIGNING_DIGESTS
        && digests.iter().all(|digest| is_sha256(digest))
        && digests.iter().collect::<BTreeSet<_>>().len() == digests.len()
        && (!multiple
            || (digests.len() >= 2
                && digests
                    .windows(2)
                    .all(|pair| pair[0].as_str() < pair[1].as_str())))
}

/// A relative path pattern inside one app data domain, as used by restore preserve rules and
/// backup profile rules: `/`-separated segments of ASCII letters, digits, `.`, `_` or `-`, where a
/// segment may be exactly `*` to match any one name. `.`, `..` and `**` are refused.
pub fn is_path_pattern(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PATH_BYTES
        && !value.starts_with('/')
        && value.split('/').all(|segment| {
            !segment.is_empty()
                && !matches!(segment, "." | ".." | "**")
                && (segment == "*"
                    || segment.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                    }))
        })
}

/// Whether two path patterns could select the same path or one could select an ancestor of the
/// other.
pub fn path_patterns_overlap(left: &str, right: &str) -> bool {
    left.split('/')
        .zip(right.split('/'))
        .all(|(left, right)| left == right || left == "*" || right == "*")
}

/// Whether any two patterns of the same domain in `rules` overlap.
pub fn any_patterns_overlap<'a, D: PartialEq + 'a>(
    rules: impl IntoIterator<Item = (D, &'a str)>,
) -> bool {
    let rules = rules.into_iter().collect::<Vec<_>>();
    rules.iter().enumerate().any(|(index, (domain, path))| {
        rules.iter().skip(index + 1).any(|(other_domain, other)| {
            domain == other_domain && path_patterns_overlap(path, other)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_names_follow_one_rule() {
        // No length limit: names the previous model accepted must keep loading after an upgrade.
        let long_segment = format!("com.{}", "a".repeat(64));
        let long_name = format!("com.{}", ["a"; 130].join("."));
        for valid in [
            "com.example.app",
            "a.b",
            "com.example_1.App2",
            "_x.y",
            long_segment.as_str(),
            long_name.as_str(),
        ] {
            assert!(is_package_name(valid), "{valid}");
        }
        for invalid in [
            "",
            "app",
            ".com.example",
            "com.example.",
            "com..example",
            "com.exa-mple",
            "com/example.app",
        ] {
            assert!(!is_package_name(invalid), "{invalid}");
        }
    }

    #[test]
    fn slot_ids_are_base_or_numbered() {
        for valid in ["base", "slot-1", "slot-42"] {
            assert!(is_slot_id(valid), "{valid}");
        }
        for invalid in ["", "slot-", "slot-x", "Base", "slot-1/..", "../base"] {
            assert!(!is_slot_id(invalid), "{invalid}");
        }
    }

    #[test]
    fn identifiers_are_single_safe_path_components() {
        let max = "a".repeat(64);
        for valid in ["a", "account-1", "A_b-C", max.as_str()] {
            assert!(is_identifier(valid), "{valid}");
        }
        let too_long = "a".repeat(65);
        for invalid in ["", ".", "..", ".hidden", "a/b", "a.b", too_long.as_str()] {
            assert!(!is_identifier(invalid), "{invalid}");
        }
    }

    #[test]
    fn tokens_and_digests_are_lowercase_hex() {
        assert!(is_io_token(&"0123456789abcdef".repeat(2)));
        assert!(!is_io_token(&"0123456789ABCDEF".repeat(2)));
        assert!(!is_io_token("abc"));
        assert!(is_sha256(&"a".repeat(64)));
        assert!(!is_sha256(&"g".repeat(64)));
    }

    #[test]
    fn signing_digest_sets_are_distinct_and_sorted_for_multiple_signers() {
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        assert!(is_signing_digest_set(false, &[b.clone(), a.clone()]));
        assert!(is_signing_digest_set(true, &[a.clone(), b.clone()]));
        assert!(!is_signing_digest_set(true, &[b.clone(), a.clone()]));
        assert!(!is_signing_digest_set(true, std::slice::from_ref(&a)));
        assert!(!is_signing_digest_set(false, &[a.clone(), a.clone()]));
        assert!(!is_signing_digest_set(false, &[]));
        assert!(!is_signing_digest_set(false, &vec![a; 17]));
    }

    #[test]
    fn path_patterns_and_overlap() {
        for valid in ["files", "shared_prefs/*", "files/*/cache", "a-b.c_d"] {
            assert!(is_path_pattern(valid), "{valid}");
        }
        let too_long = "a".repeat(MAX_PATH_BYTES + 1);
        for invalid in [
            "",
            "/files",
            "files/",
            "a//b",
            "./a",
            "a/..",
            "**",
            "a b",
            too_long.as_str(),
        ] {
            assert!(!is_path_pattern(invalid), "{invalid}");
        }
        assert!(path_patterns_overlap("files", "files/a"));
        assert!(path_patterns_overlap("*/a", "files"));
        assert!(!path_patterns_overlap("files/a", "files/b"));
        assert!(any_patterns_overlap([
            (1, "files"),
            (2, "files"),
            (1, "files/x")
        ]));
        assert!(!any_patterns_overlap([(1, "files"), (2, "files")]));
    }
}
