use serde::Serialize;
use sha2::{Digest as _, Sha256};

#[allow(
    clippy::redundant_pub_crate,
    reason = "shared by independent secure persistence readers"
)]
pub(crate) fn digest_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len().saturating_mul(2));
    for byte in digest {
        output.push(hex_digit(byte >> 4));
        output.push(hex_digit(byte & 0x0f));
    }
    output
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "shared by sibling persistence modules while this support module stays internal"
)]
pub(crate) fn digest_json(value: &impl Serialize) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(value)?;
    Ok(digest_bytes(&bytes))
}

fn hex_digit(nibble: u8) -> char {
    char::from_digit(u32::from(nibble), 16).unwrap_or('x')
}
