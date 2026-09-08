#[path = "../src/project/digest.rs"]
mod digest;

use digest::RevisionDigest;
use serde::ser::{Error, SerializeSeq};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;

#[derive(Clone)]
enum Operation {
    Success(Value, String),
    Failure(String),
    Flush,
}

struct Fails<'a>(&'a str);

impl serde::Serialize for Fails<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(2))?;
        sequence.serialize_element(self.0)?;
        Err(S::Error::custom("injected serialization failure"))
    }
}

fn corpus() -> Vec<Vec<Operation>> {
    let alphabet = [
        Operation::Success(Value::Null, "null".into()),
        Operation::Success(json!(true), "true".into()),
        Operation::Success(json!(""), "\"\"".into()),
        Operation::Success(json!("λ🙂\n"), "\"λ🙂\\n\"".into()),
        Operation::Success(json!(u64::MAX), "18446744073709551615".into()),
        Operation::Failure("partial".into()),
        Operation::Flush,
    ];
    let mut cases = vec![vec![]];
    let mut words = cases.clone();
    for _ in 0..3 {
        words = words
            .iter()
            .flat_map(|prefix| {
                alphabet.iter().map(|op| {
                    let mut ops = prefix.clone();
                    ops.push(op.clone());
                    ops
                })
            })
            .collect();
        cases.extend(words.clone());
    }
    for width in [65529, 65530, 65531, 100000] {
        let text = "a".repeat(width);
        cases.push(vec![
            Operation::Success(Value::Null, "null".into()),
            Operation::Success(json!(text), format!("\"{text}\"")),
            Operation::Failure("b".repeat(100000)),
            Operation::Flush,
            Operation::Success(json!("λ🙂\n"), "\"λ🙂\\n\"".into()),
            Operation::Flush,
            Operation::Flush,
            Operation::Success(Value::Null, "null".into()),
        ]);
    }
    cases
}

#[test]
fn buffered_revision_states_match_lean_and_explicit_byte_oracle() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("kernels");
    let build = Command::new("lake")
        .args(["build", "--wfail", "fr-digest-kernel"])
        .current_dir(&root)
        .output()
        .expect("Lean is installed for the kernel gate");
    assert!(
        build.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let output = Command::new(root.join(".lake/build/bin/fr-digest-kernel"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let mut lines = text.lines();
    let cases = corpus();
    assert_eq!(cases.len(), 404);
    let mut states = 0;
    for ops in cases {
        let mut digest = RevisionDigest::default();
        let mut expected = Vec::new();
        let mut check = |digest: &RevisionDigest, expected: &[u8]| {
            let [emitted, pending]: [Vec<u8>; 2] =
                serde_json::from_str(lines.next().expect("one Lean state per Rust state")).unwrap();
            assert_eq!(digest.buffer, pending);
            assert_eq!(digest.digest.clone().finalize(), Sha256::digest(&emitted));
            assert_eq!([emitted, pending].concat(), expected);
            assert_eq!(
                digest.clone().finish(),
                format!("{:x}", Sha256::digest(expected))
            );
            assert!(digest.buffer.len() < 65536);
            states += 1;
        };
        check(&digest, &expected);
        for op in ops {
            match op {
                Operation::Success(value, bytes) => {
                    digest.update(value).unwrap();
                    expected.extend_from_slice(bytes.as_bytes());
                }
                Operation::Failure(incomplete) => {
                    assert!(digest.update(Fails(&incomplete)).is_err());
                }
                Operation::Flush => digest.flush(),
            }
            check(&digest, &expected);
        }
    }
    assert!(lines.next().is_none());
    assert_eq!(states, 1570);
}
