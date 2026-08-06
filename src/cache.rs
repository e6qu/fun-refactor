//! A persistent, content-addressed cache of extracted facts.
//!
//! Extraction is the expensive half of indexing, and its result depends on exactly
//! two things: the bytes of the file and the queries used to read them. So entries are
//! keyed by a hash of the content and stored under a directory named for a hash of the
//! query set — change a query file and every stale entry becomes unreachable rather
//! than wrong.
//!
//! Because the key is the content, two files with identical bytes share one entry, and
//! moving a file costs nothing to re-index.

use crate::lang::Language;
use crate::model::FileFacts;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Bumped when [`FileFacts`] changes shape in a way old entries cannot satisfy.
const SCHEMA_VERSION: u32 = 2;

/// A content-addressed store of per-file facts.
pub struct Cache {
    root: PathBuf,
    // Indexing extracts files in parallel, so the counters are shared across threads.
    hits: AtomicUsize,
    misses: AtomicUsize,
    /// Set when the store turns out to be unusable, so we stop trying.
    disabled: AtomicBool,
}

impl Cache {
    /// Open the cache for the current query set, or `None` when no location is
    /// available. A missing cache is never an error: it only costs time.
    pub fn open() -> Option<Cache> {
        Cache::open_at(&cache_root()?)
    }

    /// Open the cache under an explicit base directory.
    ///
    /// The query fingerprint still names the subdirectory, so a caller cannot
    /// accidentally read entries produced by a different query set.
    pub fn open_at(base: &Path) -> Option<Cache> {
        // Three things decide whether an entry is still meaningful: the schema, the
        // queries that produced the facts, and the extractor that interpreted them.
        // The third is a compile-time hash of the sources that define extraction —
        // see build.rs for why leaving it out is not survivable.
        let root = base.join(format!(
            "v{SCHEMA_VERSION}-{}-{}",
            query_fingerprint(),
            env!("FUN_REFACTOR_EXTRACTOR_FINGERPRINT")
        ));
        std::fs::create_dir_all(&root).ok()?;
        Some(Cache {
            root,
            hits: AtomicUsize::new(0),
            misses: AtomicUsize::new(0),
            disabled: AtomicBool::new(false),
        })
    }

    /// The key for a file's contents under a language.
    pub fn key(language: Language, source: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(language.name().as_bytes());
        hasher.update([0]);
        hasher.update(source.as_bytes());
        let digest = hasher.finalize();
        digest.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn entry_path(&self, key: &str) -> PathBuf {
        // Two levels of fan-out keeps directories small on large workspaces.
        self.root.join(&key[..2]).join(&key[2..])
    }

    /// Look up facts for a key, rewriting their path to the file being indexed.
    ///
    /// The stored facts came from whichever file first had these bytes, so the path
    /// they carry is not necessarily this one.
    pub fn get(&self, key: &str, path: &Path) -> Option<FileFacts> {
        if self.disabled.load(Ordering::Relaxed) {
            return None;
        }
        let bytes = std::fs::read(self.entry_path(key)).ok()?;
        match postcard::from_bytes::<FileFacts>(&bytes) {
            Ok(mut facts) => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                facts.path = path.to_path_buf();
                for symbol in &mut facts.symbols {
                    symbol.file = path.to_path_buf();
                }
                for reference in &mut facts.references {
                    reference.file = path.to_path_buf();
                }
                for import in &mut facts.imports {
                    import.file = path.to_path_buf();
                }
                Some(facts)
            }
            // A corrupt or outdated entry is a miss, not a failure.
            Err(_) => {
                let _ = std::fs::remove_file(self.entry_path(key));
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Store facts under a key. Failure to write only costs time, so it is ignored
    /// beyond disabling further attempts.
    pub fn put(&self, key: &str, facts: &FileFacts) {
        if self.disabled.load(Ordering::Relaxed) {
            return;
        }
        self.misses.fetch_add(1, Ordering::Relaxed);

        let Ok(bytes) = postcard::to_allocvec(facts) else {
            return;
        };
        let path = self.entry_path(key);
        let Some(dir) = path.parent() else { return };
        if std::fs::create_dir_all(dir).is_err() {
            self.disabled.store(true, Ordering::Relaxed);
            return;
        }
        // Write through a temporary file so a concurrent reader never sees a
        // half-written entry.
        let Ok(mut tmp) = tempfile::NamedTempFile::new_in(dir) else {
            self.disabled.store(true, Ordering::Relaxed);
            return;
        };
        use std::io::Write;
        if tmp.write_all(&bytes).is_err() {
            return;
        }
        let _ = tmp.persist(&path);
    }

    pub fn stats(&self) -> (usize, usize) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
        )
    }

    pub fn location(&self) -> &Path {
        &self.root
    }

    /// Delete every entry for the current query set.
    pub fn clear(&self) -> std::io::Result<()> {
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root)?;
        }
        std::fs::create_dir_all(&self.root)
    }

    /// Total bytes stored, for reporting.
    pub fn size_bytes(&self) -> u64 {
        fn walk(dir: &Path) -> u64 {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return 0;
            };
            entries
                .filter_map(|e| e.ok())
                .map(|e| match e.file_type() {
                    Ok(t) if t.is_dir() => walk(&e.path()),
                    Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
                    Err(_) => 0,
                })
                .sum()
        }
        walk(&self.root)
    }
}

/// Where cache entries live.
///
/// `FUN_REFACTOR_CACHE` overrides everything, which is what tests use. Otherwise the
/// usual per-user cache location, never the workspace itself — a refactoring tool has
/// no business writing into the repository it is reading.
fn cache_root() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("FUN_REFACTOR_CACHE") {
        return Some(PathBuf::from(explicit));
    }
    if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
        return Some(PathBuf::from(xdg).join("fun-refactor"));
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return Some(PathBuf::from(home).join("Library/Caches/fun-refactor"));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache/fun-refactor"))
}

/// A fingerprint of every query file, so editing one invalidates its entries.
fn query_fingerprint() -> String {
    let mut hasher = Sha256::new();
    for language in Language::ALL {
        if let Some(source) = crate::extract::query_source_for(*language) {
            hasher.update(language.name().as_bytes());
            hasher.update(source.as_bytes());
        }
    }
    let digest = hasher.finalize();
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Symbol, SymbolId, SymbolKind};
    use crate::span::Span;

    /// A cache in its own directory, so parallel tests never share state.
    fn scratch() -> (tempfile::TempDir, Cache) {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open_at(dir.path()).expect("cache should open");
        (dir, cache)
    }

    fn facts_for(path: &str, name: &str) -> FileFacts {
        FileFacts {
            path: PathBuf::from(path),
            symbols: vec![Symbol {
                id: SymbolId(0),
                name: name.to_string(),
                kind: SymbolKind::Function,
                name_span: Span::new(3, 3 + name.len()),
                full_span: Span::new(0, 20),
                file: PathBuf::from(path),
                language: Language::Rust,
                scope: crate::model::ScopeId(0),
                container: None,
                qualifier: None,
                exported: false,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn stores_and_retrieves_facts() {
        let (_dir, cache) = scratch();
        let key = Cache::key(Language::Rust, "fn alpha() {}\n");

        assert!(cache.get(&key, Path::new("a.rs")).is_none(), "starts empty");
        cache.put(&key, &facts_for("a.rs", "alpha"));

        let loaded = cache.get(&key, Path::new("a.rs")).expect("a hit");
        assert_eq!(loaded.symbols.len(), 1);
        assert_eq!(loaded.symbols[0].name, "alpha");
    }

    #[test]
    fn identical_content_in_another_file_reuses_the_entry() {
        // The key is the content, so a copy costs nothing to index — but the facts
        // must come back pointing at the file that asked for them.
        let (_dir, cache) = scratch();
        let key = Cache::key(Language::Rust, "fn alpha() {}\n");
        cache.put(&key, &facts_for("original.rs", "alpha"));

        let loaded = cache.get(&key, Path::new("copy.rs")).expect("a hit");
        assert_eq!(loaded.path, PathBuf::from("copy.rs"));
        assert_eq!(loaded.symbols[0].file, PathBuf::from("copy.rs"));
    }

    #[test]
    fn different_content_gets_a_different_key() {
        let a = Cache::key(Language::Rust, "fn alpha() {}\n");
        let b = Cache::key(Language::Rust, "fn beta() {}\n");
        assert_ne!(a, b);
    }

    #[test]
    fn the_same_bytes_in_another_language_get_a_different_key() {
        // `x = 1` means different things in different grammars.
        let a = Cache::key(Language::Python, "x = 1\n");
        let b = Cache::key(Language::Rust, "x = 1\n");
        assert_ne!(a, b);
    }

    #[test]
    fn a_corrupt_entry_is_a_miss_and_is_removed() {
        let (_dir, cache) = scratch();
        let key = Cache::key(Language::Rust, "fn alpha() {}\n");
        cache.put(&key, &facts_for("a.rs", "alpha"));

        // Truncate the stored entry.
        let path = cache.entry_path(&key);
        std::fs::write(&path, b"nonsense").unwrap();

        assert!(cache.get(&key, Path::new("a.rs")).is_none());
        assert!(!path.exists(), "a corrupt entry should be deleted");
    }

    #[test]
    fn hits_and_misses_are_counted() {
        let (_dir, cache) = scratch();
        let key = Cache::key(Language::Rust, "fn alpha() {}\n");

        cache.get(&key, Path::new("a.rs"));
        cache.put(&key, &facts_for("a.rs", "alpha"));
        cache.get(&key, Path::new("a.rs"));

        let (hits, _) = cache.stats();
        assert_eq!(hits, 1);
    }

    #[test]
    fn clearing_removes_every_entry() {
        let (_dir, cache) = scratch();
        let key = Cache::key(Language::Rust, "fn alpha() {}\n");
        cache.put(&key, &facts_for("a.rs", "alpha"));
        assert!(cache.size_bytes() > 0);

        cache.clear().unwrap();
        assert!(cache.get(&key, Path::new("a.rs")).is_none());
    }

    #[test]
    fn the_fingerprint_covers_the_query_set() {
        // Two calls agree, and the value is short enough to name a directory.
        let a = query_fingerprint();
        assert_eq!(a, query_fingerprint());
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn the_fingerprint_would_change_if_a_query_did() {
        // The one above checks the fingerprint is *stable*. A function that ignored the
        // queries entirely and hashed a constant would pass it, and the cache tells
        // every reader that "editing a query file makes every stale entry unreachable
        // rather than wrong" — which would then be false, and false in the direction
        // that returns confident answers computed by code that no longer exists.
        //
        // The queries are compiled in, so this cannot edit one. What it can do is
        // recompute the fingerprint the same way over a set with one query altered, and
        // insist the answer differs.
        fn over(pairs: &[(&str, &str)]) -> String {
            let mut hasher = Sha256::new();
            for (name, source) in pairs {
                hasher.update(name.as_bytes());
                hasher.update(source.as_bytes());
            }
            hasher
                .finalize()
                .iter()
                .take(8)
                .map(|b| format!("{b:02x}"))
                .collect()
        }

        let real: Vec<(&str, &str)> = Language::ALL
            .iter()
            .filter_map(|l| crate::extract::query_source_for(*l).map(|s| (l.name(), s)))
            .collect();
        assert_eq!(over(&real), query_fingerprint(), "the recipe has drifted");
        assert!(real.len() > 8, "only {} languages have queries", real.len());

        // Change one query, and the name of the directory has to change with it.
        for index in 0..real.len() {
            let mut altered = real.clone();
            altered[index].1 = "; a capture nobody wrote";
            assert_ne!(
                over(&altered),
                query_fingerprint(),
                "changing {}'s query leaves the cache key alone",
                real[index].0
            );
        }

        // And the language's name is in it, so two languages swapping queries is a
        // different set even though the same bytes went in.
        let mut swapped = real.clone();
        swapped.swap(0, 1);
        assert_ne!(
            over(&swapped),
            query_fingerprint(),
            "the fingerprint does not depend on which language a query belongs to"
        );
    }
}
