//! Fingerprint the things that decide what a cached fact *means*.
//!
//! The cache is content-addressed: a file's facts are keyed by the hash of its bytes
//! and of the query set. That is correct only while "the extractor" is a constant.
//! It is not — changing `src/extract.rs` changes what a `Reference` is, and an entry
//! written by yesterday's extractor then deserializes cleanly into today's struct
//! with `#[serde(default)]` filling the new fields. The result is silently wrong
//! answers from a cache that looks healthy, which cost an afternoon of bisecting a
//! test failure that was not in the code being bisected.
//!
//! So the key includes a hash of the sources that define extraction. Editing any of
//! them moves every entry to a new namespace, and the stale ones are simply never
//! looked up again.
//!
//! The grammars under `grammars/` are in it for the same reason. A patched grammar
//! changes what the tree looks like, and a fact written from the old tree says nothing
//! about that: raising one token's precedence moved `theme.$brand` from a plain value to
//! a variable, and every file already scanned kept the answer that had no variable in it.
//! What is hashed is what the parse is generated *from*. `src/parser.c` is left out: it
//! is derived from `grammar.js`, and it is a megabyte of table per language.

use std::fmt::Write as _;
use std::path::Path;

/// Files whose contents change the meaning of a cached fact.
///
/// A cached fact is not only what extraction read out of a file. The cache also
/// holds the resolution snapshot, the target and tier of every reference, and
/// skips resolving at all when it hits. So the resolver belongs here beside the
/// extractor, and so does everything the resolver asks: a receiver's type comes
/// from the type analysis, through the bridge in `refactor`. Without them, a
/// change to how a member access resolves is invisible on any machine that has
/// run the tool before, which is every machine except a fresh clone. That is how
/// a green local gate and a red one in CI came to disagree about the same commit.
const INPUTS: &[&str] = &[
    "src/extract.rs",
    "src/model.rs",
    "src/parse.rs",
    "src/helm.rs",
    "src/lang.rs",
    "src/index.rs",
    "src/analysis/types.rs",
    "src/refactor/mod.rs",
];

fn main() {
    let mut hasher = Fnv::new();
    for input in INPUTS {
        println!("cargo:rerun-if-changed={input}");
        match std::fs::read(input) {
            Ok(bytes) => hasher.write(&bytes),
            // A missing input is a build-script bug, not something to paper over: a
            // fingerprint that silently ignores a file is worse than none.
            Err(e) => panic!("cache fingerprint input {input} is unreadable: {e}"),
        }
    }
    // Every grammar this project compiles itself: its rules, its scanner, the patch
    // applied to it and the release it was taken from.
    let grammars = Path::new("grammars");
    println!("cargo:rerun-if-changed=grammars");
    let mut sources: Vec<String> = Vec::new();
    collect(grammars, &mut sources);
    sources.sort();
    for source in &sources {
        if source.ends_with("parser.c") || source.contains("tree_sitter/") {
            continue;
        }
        println!("cargo:rerun-if-changed={source}");
        match std::fs::read(source) {
            Ok(bytes) => hasher.write(&bytes),
            Err(e) => panic!("cache fingerprint input {source} is unreadable: {e}"),
        }
    }

    // The query files are hashed at runtime already, but a renamed or deleted one
    // must move the namespace too.
    let queries = Path::new("queries");
    println!("cargo:rerun-if-changed=queries");
    let mut names: Vec<String> = Vec::new();
    collect(queries, &mut names);
    names.sort();
    for name in &names {
        hasher.write(name.as_bytes());
    }

    let mut fingerprint = String::new();
    write!(fingerprint, "{:016x}", hasher.finish()).unwrap();
    println!("cargo:rustc-env=FUN_REFACTOR_EXTRACTOR_FINGERPRINT={fingerprint}");
}

fn collect(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else {
            out.push(path.display().to_string());
        }
    }
}

struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= *byte as u64;
            self.0 = self.0.wrapping_mul(0x1000_0000_01b3);
        }
    }
    fn finish(&self) -> u64 {
        self.0
    }
}
