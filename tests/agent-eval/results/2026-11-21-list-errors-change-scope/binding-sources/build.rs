#[path = "src/native_fingerprint.rs"]
mod native_fingerprint;

use std::collections::BTreeMap;
use std::path::Path;

fn main() {
    let mut configuration: BTreeMap<_, _> = std::env::vars_os()
        .filter_map(|(name, value)| {
            let name = name.into_string().ok()?;
            let relevant = name.starts_with("CARGO_CFG_")
                || name.starts_with("CARGO_FEATURE_")
                || matches!(
                    name.as_str(),
                    "TARGET" | "PROFILE" | "OPT_LEVEL" | "DEBUG" | "CARGO_ENCODED_RUSTFLAGS"
                );
            relevant.then(|| {
                (
                    name,
                    value
                        .into_string()
                        .expect("Native build settings must be UTF-8."),
                )
            })
        })
        .collect();
    let compiler = std::env::var_os("RUSTC").expect("Cargo must supply the Rust compiler");
    let output = std::process::Command::new(compiler)
        .args(["--version", "--verbose"])
        .output()
        .expect("read the Rust compiler identity");
    assert!(
        output.status.success(),
        "Rust compiler identity command failed"
    );
    configuration.insert(
        "compiler".into(),
        String::from_utf8(output.stdout).expect("Rust compiler identity must be UTF-8"),
    );
    let inputs = native_fingerprint::fingerprint(Path::new("."), &configuration)
        .expect("read every declared native source input");
    for path in inputs.watched {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    println!(
        "cargo:rustc-env=FUN_REFACTOR_EXTRACTOR_FINGERPRINT={}",
        inputs.digest
    );
}
