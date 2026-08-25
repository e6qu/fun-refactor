//! Dump every carried construct across the corpus, with file and note.
use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/corpus"));
    let mut stack = vec![root];
    let mut files = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let language = match path.extension().and_then(|e| e.to_str()) {
                Some("py") => Language::Python,
                Some("java") => Language::Java,
                Some("zig") => Language::Zig,
                Some("ts") | Some("tsx") => Language::TypeScript,
                _ => continue,
            };
            files.push((path, language));
        }
    }
    let tmp = std::env::temp_dir().join("fr-carried-dump");
    std::fs::create_dir_all(&tmp).unwrap();
    let mut n = 0usize;
    for (path, from) in files {
        for target in transpile::SUPPORTED {
            if *target == from {
                continue;
            }
            n += 1;
            let ext = match target {
                Language::Rust => "rs",
                Language::Go => "go",
                Language::Java => "java",
                Language::Python => "py",
                Language::TypeScript => "ts",
                Language::Zig => "zig",
                other => panic!("no extension for {other}"),
            };
            let out = tmp.join(format!("D{n}.{ext}"));
            let plan = transpile::plan_to(&path, *target, Some(&out), false).unwrap();
            for note in &plan.fidelity.notes {
                if note.contains("carried over unchanged") {
                    println!(
                        "{}\t{}\t{}",
                        path.display(),
                        target,
                        note.replace('\n', "\\n")
                    );
                }
            }
        }
    }
}
