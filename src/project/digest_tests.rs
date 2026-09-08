use super::*;
use serde::ser::{Error, SerializeSeq};

#[test]
fn reused_digest_buffer_preserves_exact_serialized_bytes_across_sizes() {
    let mut digest = RevisionDigest::default();
    let mut expected = Vec::new();
    let large = "a".repeat(100_000);
    let cases = [
        (json!(large), format!("\"{large}\"")),
        (Value::Null, "null".to_owned()),
        (json!("λ🙂\n\0\"\\"), "\"λ🙂\\n\\u0000\\\"\\\\\"".to_owned()),
        (json!(u64::MAX), "18446744073709551615".to_owned()),
        (
            json!([true, false, null, {"a": []}]),
            "[true,false,null,{\"a\":[]}]".to_owned(),
        ),
    ];
    for (value, bytes) in cases {
        digest.update(value).unwrap();
        expected.extend_from_slice(bytes.as_bytes());
        assert_eq!(
            digest.clone().finish(),
            format!("{:x}", Sha256::digest(&expected))
        );
    }
    assert!(digest.buffer.capacity() >= large.len());
}

#[test]
fn revision_buffer_flushes_at_its_threshold_without_changing_the_hash() {
    for width in [65529, 65530, 65531] {
        let text = "a".repeat(width);
        let mut digest = RevisionDigest::default();
        digest.update(()).unwrap();
        digest.update(&text).unwrap();
        assert_eq!(digest.buffer.is_empty(), width >= 65530);
        digest.update(false).unwrap();
        let expected = format!("null\"{text}\"false");
        assert_eq!(
            digest.finish(),
            format!("{:x}", Sha256::digest(expected.as_bytes()))
        );
    }
}

#[test]
fn a_partial_serialization_error_leaves_the_digest_unchanged() {
    struct Fails;
    impl serde::Serialize for Fails {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut sequence = serializer.serialize_seq(Some(2))?;
            sequence.serialize_element("partial")?;
            Err(S::Error::custom("injected serialization failure"))
        }
    }
    let mut digest = RevisionDigest::default();
    digest.update("prefix").unwrap();
    assert!(digest.update(Fails).is_err());
    assert_eq!(digest.buffer, b"\"prefix\"");
    assert_eq!(
        digest.clone().finish(),
        format!("{:x}", Sha256::digest(b"\"prefix\""))
    );
    digest.update(()).unwrap();
    assert_eq!(
        digest.finish(),
        format!("{:x}", Sha256::digest(b"\"prefix\"null"))
    );
}
