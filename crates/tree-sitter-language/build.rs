fn main() {
    if std::env::var("TARGET")
        .unwrap_or_default()
        .starts_with("wasm32-unknown")
    {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("manifest directory");
        let root = std::path::Path::new(&manifest_dir).join("wasm");
        println!(
            "cargo::metadata=wasm-headers={}",
            root.join("include").display()
        );
        println!("cargo::metadata=wasm-src={}", root.join("compat").display());
    }
}
