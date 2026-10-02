//! Checks the minisign signature on an app update before it is installed.
//!
//! The update is a plain exe. Its signature is a `tauri signer` / minisign
//! signature (base64 of the `.sig` file), made with a key whose public half
//! is built into the app -- so whoever controls the update feed (the gist, a
//! release page) cannot get code run unless they also hold the private key.
//!
//! `trusted_keys` is a list so a backup key can take over if the primary one
//! is ever lost or leaked: every shipped app already knows both.
//!
//! The signed "trusted comment" carries `version:<x.y.z>`; it must equal the
//! version the feed announced, so an old signed exe cannot be passed off as
//! the new release (a rollback to a build with a known hole).

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use minisign_verify::{PublicKey, Signature};
use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum UpdateSignatureError {
    /// The app has no public key to check against.
    NoTrustedKeys,
    /// The signature text is not a base64 minisign signature.
    MalformedSignature,
    /// None of the trusted keys signed these bytes.
    NotSignedByTrustedKey,
    /// The signature does not say which version it is for.
    MissingSignedVersion,
    /// The signature is for another version than the one announced.
    VersionMismatch { signed: String, announced: String },
}

impl fmt::Display for UpdateSignatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTrustedKeys => write!(f, "no update signing key is built into this app"),
            Self::MalformedSignature => write!(f, "the update signature is malformed"),
            Self::NotSignedByTrustedKey => {
                write!(f, "the update is not signed by a trusted key")
            }
            Self::MissingSignedVersion => {
                write!(f, "the update signature does not name its version")
            }
            Self::VersionMismatch { signed, announced } => write!(
                f,
                "the update is signed for version {signed}, not the announced {announced}"
            ),
        }
    }
}

/// Is `data` signed (`signature_b64`) by one of `trusted_keys` (base64
/// minisign public keys), for exactly `announced_version`?
pub fn verify_update(
    data: &[u8],
    signature_b64: &str,
    trusted_keys: &[&str],
    announced_version: &str,
) -> Result<(), UpdateSignatureError> {
    if trusted_keys.is_empty() {
        return Err(UpdateSignatureError::NoTrustedKeys);
    }
    let signature = decode_text(signature_b64)
        .and_then(|text| Signature::decode(&text).ok())
        .ok_or(UpdateSignatureError::MalformedSignature)?;

    // A key that does not decode is skipped, never trusted. `true`: accept
    // the non-prehashed signatures `tauri signer` writes.
    let signed_by_a_trusted_key = trusted_keys.iter().any(|key| {
        decode_text(key)
            .and_then(|text| PublicKey::decode(&text).ok())
            .is_some_and(|key| key.verify(data, &signature, true).is_ok())
    });
    if !signed_by_a_trusted_key {
        return Err(UpdateSignatureError::NotSignedByTrustedKey);
    }

    // Only now is the trusted comment trustworthy: minisign's global
    // signature covers it, and `verify` above checked that signature.
    let signed = signature
        .trusted_comment()
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"))
        .ok_or(UpdateSignatureError::MissingSignedVersion)?;
    if same_version(signed, announced_version) {
        Ok(())
    } else {
        Err(UpdateSignatureError::VersionMismatch {
            signed: signed.to_string(),
            announced: announced_version.to_string(),
        })
    }
}

/// The text inside a base64 blob (`tauri signer` base64-encodes the `.sig`
/// and `.pub` files whole).
fn decode_text(base64_text: &str) -> Option<String> {
    let bytes = STANDARD.decode(base64_text.trim()).ok()?;
    String::from_utf8(bytes).ok()
}

/// `0.7.0` and `v0.7.0` are one version; anything that is not semver must
/// match letter for letter.
fn same_version(a: &str, b: &str) -> bool {
    let parse = |v: &str| semver::Version::parse(v.trim().trim_start_matches('v')).ok();
    match (parse(a), parse(b)) {
        (Some(a), Some(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Generated once with the `minisign` crate (what `tauri signer` uses);
    // throwaway keys that sign nothing real. DATA is what was signed.
    const DATA: &[u8] = b"resonance-stream test exe";
    const KEY_A: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IENGRDIzQjMyODRDRUY4N0UKUldSKytNNkVNanZTenhiMnorMEY5c0c5clY5ekx5aVRwSWRwVDVtS0pNWlh4YzcrV1NXdkh0WUoK";
    const KEY_B: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDgwNkYyNDJDRTZCNzI4QzgKUldUSUtMZm1MQ1J2Z0wzKzFuZkExbi9sZHora0g1eHJPZHMzS0RPWmRrSndVWndPMTRpYlNGdnYK";
    // Signed by key A / key B, trusted comment "...version:0.7.0".
    const SIG_A: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHVudHJ1c3RlZApSVVIrK002RU1qdlN6d2V6U0k2ejRNb2FBZUlPVHFRUkxnU2dITGdYL3NLcTQwb2FzNlRKcFBUeHowS0swS0RhZWlnVDVKSVpiV2hqK3QwMXhBYkJxSUk4Qm5lMy9DdTh3QTA9CnRydXN0ZWQgY29tbWVudDogdGltZXN0YW1wOjEJZmlsZTphcHAuZXhlCXZlcnNpb246MC43LjAKOXdBYWlQZTErT3hiLzVFTTdwZXhqbmlaSUlLckFSNTVoQ3FFT0hvMDdEZHN1NzdVb0tRMzVYZ21RTG1aeXhCd0tsNnF2ZUljeHppdUhrNkIyQ2FrQUE9PQo=";
    const SIG_B: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHVudHJ1c3RlZApSVVRJS0xmbUxDUnZnSmNKOEQ3Ly9YdW8xVVhTVkZSK1ZsR0tUaEo1V2pVdENEeW9hWjVVNWVMVW5iN2JLUHJmdEpLS1gzb21IMHN3MUpVbTFtR2N1SFNOREdIeWJHL3BjQXM9CnRydXN0ZWQgY29tbWVudDogdGltZXN0YW1wOjEJZmlsZTphcHAuZXhlCXZlcnNpb246MC43LjAKNlErYUxobndTT0JIaXQyZFZib1BtalUyOVo1TjNJVFJuWjNWWXpVS1BXbGNmTytTT2wrdUFEbVZDZHpYRUdSZ3hlcGRPS21aRkI3bkVsc2J2YlhxRHc9PQo=";
    // Signed by key A, trusted comment without a version.
    const SIG_A_NO_VERSION: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHVudHJ1c3RlZApSVVIrK002RU1qdlN6elJEa0RwRS9ueGIyaWt3emRQWkc2MXQzOU1VVEd4Z1hBaEFhQloySjFxRW9LUDBBUDdWMlkzblpXclVSeWtIT1p3YlUxeFVxYW94OG50YkRXTzV6UTg9CnRydXN0ZWQgY29tbWVudDogdGltZXN0YW1wOjEJZmlsZTphcHAuZXhlCksweXN0bU9UNFZDcGZid24xUVptaU9pNXhvOE8rRDNidmliM2UyOCtPaGxsaHVTVGdBaTFOREtub2RwRCtSd1o5SC9VZ0Vybi9WTG1nd3BsaVVzTURnPT0K";

    #[test]
    fn a_signature_from_the_trusted_key_for_the_announced_version_passes() {
        assert_eq!(verify_update(DATA, SIG_A, &[KEY_A], "0.7.0"), Ok(()));
    }

    #[test]
    fn changed_bytes_fail() {
        let mut data = DATA.to_vec();
        data[0] ^= 1;
        assert_eq!(
            verify_update(&data, SIG_A, &[KEY_A], "0.7.0"),
            Err(UpdateSignatureError::NotSignedByTrustedKey)
        );
    }

    #[test]
    fn a_signature_from_an_untrusted_key_fails() {
        assert_eq!(
            verify_update(DATA, SIG_B, &[KEY_A], "0.7.0"),
            Err(UpdateSignatureError::NotSignedByTrustedKey)
        );
    }

    #[test]
    fn a_backup_key_in_the_list_can_sign_too() {
        assert_eq!(verify_update(DATA, SIG_B, &[KEY_A, KEY_B], "0.7.0"), Ok(()));
        assert_eq!(verify_update(DATA, SIG_A, &[KEY_B, KEY_A], "0.7.0"), Ok(()));
    }

    #[test]
    fn a_signature_for_another_version_is_a_rollback_and_fails() {
        assert_eq!(
            verify_update(DATA, SIG_A, &[KEY_A], "0.8.0"),
            Err(UpdateSignatureError::VersionMismatch {
                signed: "0.7.0".into(),
                announced: "0.8.0".into(),
            })
        );
    }

    #[test]
    fn a_leading_v_is_the_same_version() {
        assert_eq!(verify_update(DATA, SIG_A, &[KEY_A], "v0.7.0"), Ok(()));
    }

    #[test]
    fn a_signature_that_names_no_version_fails() {
        assert_eq!(
            verify_update(DATA, SIG_A_NO_VERSION, &[KEY_A], "0.7.0"),
            Err(UpdateSignatureError::MissingSignedVersion)
        );
    }

    #[test]
    fn no_trusted_key_means_nothing_is_accepted() {
        assert_eq!(
            verify_update(DATA, SIG_A, &[], "0.7.0"),
            Err(UpdateSignatureError::NoTrustedKeys)
        );
    }

    #[test]
    fn garbage_is_a_malformed_signature() {
        for bad in ["", "not base64!", "aGVsbG8="] {
            assert_eq!(
                verify_update(DATA, bad, &[KEY_A], "0.7.0"),
                Err(UpdateSignatureError::MalformedSignature),
                "{bad:?}"
            );
        }
    }
}
