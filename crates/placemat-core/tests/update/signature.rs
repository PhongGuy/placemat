//! `verify_manifest`: any listed key, the refusals, and the legacy algorithm.

use crate::support::{TestKey, retag_legacy};
use placemat_core::update::{CheckError, verify_manifest};

const M: &[u8] = b"{\"version\":\"1.0.0\"}";

#[test]
fn verify_accepts_a_release_style_signature() {
    let k = TestKey::generate();
    assert_eq!(
        verify_manifest(M, k.sign(M).as_bytes(), &[k.key_line()]).map(|v| v.as_bytes().to_vec()),
        Ok(M.to_vec())
    );
}

#[test]
fn verify_accepts_any_listed_key() {
    let (old, new) = (TestKey::generate(), TestKey::generate());
    let keys = [old.key_line(), new.key_line()];
    assert_eq!(
        verify_manifest(M, old.sign(M).as_bytes(), &keys).map(|v| v.as_bytes().to_vec()),
        Ok(M.to_vec())
    );
    assert_eq!(
        verify_manifest(M, new.sign(M).as_bytes(), &keys).map(|v| v.as_bytes().to_vec()),
        Ok(M.to_vec())
    );
}

#[test]
fn verify_ignores_whitespace_around_signature_and_key() {
    let k = TestKey::generate();
    let sig = format!("{}\n", k.sign(M));
    let key = format!("  {}\r\n", k.key_line());
    assert_eq!(
        verify_manifest(M, sig.as_bytes(), &[key]).map(|v| v.as_bytes().to_vec()),
        Ok(M.to_vec())
    );
}

#[test]
fn verify_refuses_unknown_key() {
    let (listed, other) = (TestKey::generate(), TestKey::generate());
    assert_eq!(
        verify_manifest(M, other.sign(M).as_bytes(), &[listed.key_line()]),
        Err(CheckError::UnknownKey)
    );
}

#[test]
fn verify_skips_malformed_key_line() {
    let k = TestKey::generate();
    let sig = k.sign(M);
    let keys = ["garbage".to_owned(), "aGVsbG8=".to_owned(), k.key_line()];
    assert_eq!(
        verify_manifest(M, sig.as_bytes(), &keys).map(|v| v.as_bytes().to_vec()),
        Ok(M.to_vec())
    );
    assert_eq!(
        verify_manifest(M, sig.as_bytes(), &keys[..2]),
        Err(CheckError::UnknownKey)
    );
}

#[test]
fn verify_refuses_edited_manifest_in_one_sentence() {
    let k = TestKey::generate();
    let got = verify_manifest(
        b"{\"version\":\"9.0.0\"}",
        k.sign(M).as_bytes(),
        &[k.key_line()],
    );
    assert_eq!(
        got.map_err(|e| e.to_string()),
        Err(
            "the update manifest signature is invalid: the signature verification failed"
                .to_owned()
        )
    );
}

#[test]
fn verify_refuses_malformed_signature_files() {
    let k = TestKey::generate();
    for sig in ["", "not base64!", "aGVsbG8="] {
        let got = verify_manifest(M, sig.as_bytes(), &[k.key_line()]);
        assert!(
            matches!(got, Err(CheckError::ManifestSignature(_))),
            "{sig:?}: {got:?}"
        );
    }
}

#[test]
fn verify_refuses_legacy_signature() {
    let k = TestKey::generate();
    let legacy = retag_legacy(&k.sign(M));
    assert_eq!(
        verify_manifest(M, legacy.as_bytes(), &[k.key_line()]),
        Err(CheckError::ManifestSignature(
            "unexpected signature algorithm".to_owned()
        ))
    );
}
