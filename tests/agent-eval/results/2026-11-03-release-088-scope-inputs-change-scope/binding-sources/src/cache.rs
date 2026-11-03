//! A persistent, content-addressed cache of extracted facts.

use crate::lang::Language;
use crate::model::FileFacts;
use serde::de::DeserializeOwned;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

/// Bumped when [`FileFacts`] changes shape in a way old entries cannot satisfy.
const SCHEMA_VERSION: u32 = 6;

/// Hits and misses since this cache was opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub resolution_hits: usize,
    pub resolution_misses: usize,
    pub resolution_owners: usize,
    pub resolution_waits: usize,
    pub resolution_timeouts: usize,
}

/// A content-addressed store of per-file facts.
pub struct Cache {
    root: PathBuf,
    // Indexing extracts files in parallel, so the counters are shared across threads.
    hits: AtomicUsize,
    misses: AtomicUsize,
    resolution_hits: AtomicUsize,
    resolution_misses: AtomicUsize,
    resolution_owners: AtomicUsize,
    resolution_waits: AtomicUsize,
    resolution_timeouts: AtomicUsize,
    /// Set when the store turns out to be unusable, so we stop trying.
    disabled: AtomicBool,
    fallback: bool,
}

#[derive(Serialize, serde::Deserialize)]
struct ResolutionSnapshot {
    basis: String,
    entries_sha256: String,
    entries: Vec<(Option<crate::model::SymbolId>, crate::model::Confidence)>,
}

pub enum ResolutionAccess {
    Ready(Vec<(Option<crate::model::SymbolId>, crate::model::Confidence)>),
    Owner(ResolutionLease),
    TimedOut,
}

pub struct ResolutionLease {
    path: PathBuf,
    token: Vec<u8>,
}

impl Drop for ResolutionLease {
    fn drop(&mut self) {
        if std::fs::read(&self.path).is_ok_and(|bytes| bytes == self.token) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

static LEASE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const RESOLUTION_WAIT: Duration = Duration::from_secs(310);
const RESOLUTION_STALE: Duration = Duration::from_secs(300);
const RESOLUTION_POLL: Duration = Duration::from_millis(25);

pub fn resolution_wait_action(stale: bool, expired: bool) -> usize {
    if stale {
        1
    } else if expired {
        2
    } else {
        0
    }
}

pub fn resolution_snapshot_publishable(owner: bool, admitted: bool) -> bool {
    owner && admitted
}

impl Cache {
    /// Open the cache for the current query set, or `None` when no location is available.
    pub fn open() -> Option<Cache> {
        if let Some(explicit) = std::env::var_os("FUN_REFACTOR_CACHE") {
            return Cache::open_at(Path::new(&explicit));
        }
        if let Some(cache) = default_cache_root().and_then(|root| Cache::open_at(&root)) {
            return Some(cache);
        }
        for root in fallback_cache_roots() {
            if std::fs::create_dir_all(&root).is_err() {
                continue;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).is_err()
                {
                    continue;
                }
            }
            if let Some(mut cache) = Cache::open_at(&root) {
                cache.fallback = true;
                return Some(cache);
            }
        }
        None
    }

    /// Open the cache under an explicit base directory.
    pub fn open_at(base: &Path) -> Option<Cache> {
        // Three things decide whether an entry is still meaningful: the schema, the queries
        // that produced the facts, and the extractor that interpreted them.
        let root = base.join(fact_semantics_fingerprint());
        std::fs::create_dir_all(&root).ok()?;
        // Probe now so `open` can fall back before the first cache write.
        tempfile::NamedTempFile::new_in(&root).ok()?;
        Some(Cache {
            root,
            hits: AtomicUsize::new(0),
            misses: AtomicUsize::new(0),
            resolution_hits: AtomicUsize::new(0),
            resolution_misses: AtomicUsize::new(0),
            resolution_owners: AtomicUsize::new(0),
            resolution_waits: AtomicUsize::new(0),
            resolution_timeouts: AtomicUsize::new(0),
            disabled: AtomicBool::new(false),
            fallback: false,
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

    /// Look up facts for a key, repointing their path at this file.
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
            // A corrupt or outdated entry is a miss.
            Err(_) => {
                let _ = std::fs::remove_file(self.entry_path(key));
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Store facts under a key.
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

    /// The stored resolution snapshot for a workspace key, when one exists.
    pub fn get_resolutions(
        &self,
        key: &str,
    ) -> Option<Vec<(Option<crate::model::SymbolId>, crate::model::Confidence)>> {
        self.read_resolutions(key, true, true)
    }

    fn read_resolutions(
        &self,
        key: &str,
        count_miss: bool,
        count_hit: bool,
    ) -> Option<Vec<(Option<crate::model::SymbolId>, crate::model::Confidence)>> {
        if self.disabled.load(Ordering::Relaxed) {
            return None;
        }
        let bytes = match std::fs::read(self.entry_path(key)) {
            Ok(bytes) => bytes,
            Err(_) => {
                if count_miss {
                    self.resolution_misses.fetch_add(1, Ordering::Relaxed);
                }
                return None;
            }
        };
        match postcard::from_bytes::<ResolutionSnapshot>(&bytes) {
            Ok(snapshot)
                if snapshot.basis == key
                    && serialized_digest(&snapshot.entries).as_deref()
                        == Some(snapshot.entries_sha256.as_str()) =>
            {
                if count_hit {
                    self.resolution_hits.fetch_add(1, Ordering::Relaxed);
                }
                Some(snapshot.entries)
            }
            Err(_) => {
                if count_miss {
                    self.resolution_misses.fetch_add(1, Ordering::Relaxed);
                }
                let _ = std::fs::remove_file(self.entry_path(key));
                None
            }
            Ok(_) => {
                if count_miss {
                    self.resolution_misses.fetch_add(1, Ordering::Relaxed);
                }
                let _ = std::fs::remove_file(self.entry_path(key));
                None
            }
        }
    }

    pub fn acquire_resolutions(
        &self,
        key: &str,
        admitted: impl Fn(&[(Option<crate::model::SymbolId>, crate::model::Confidence)]) -> bool,
        progress: Option<&dyn Fn(usize, usize)>,
    ) -> ResolutionAccess {
        self.acquire_resolutions_with(
            key,
            admitted,
            RESOLUTION_WAIT,
            RESOLUTION_STALE,
            RESOLUTION_POLL,
            progress,
        )
    }

    fn acquire_resolutions_with(
        &self,
        key: &str,
        admitted: impl Fn(&[(Option<crate::model::SymbolId>, crate::model::Confidence)]) -> bool,
        wait: Duration,
        stale: Duration,
        poll: Duration,
        progress: Option<&dyn Fn(usize, usize)>,
    ) -> ResolutionAccess {
        if self.disabled.load(Ordering::Relaxed) {
            return ResolutionAccess::TimedOut;
        }
        let started = std::time::Instant::now();
        let mut first_read = true;
        let mut waiting = false;
        let lock = self.entry_path(key).with_extension("resolution-lock");
        let Some(parent) = lock.parent() else {
            return ResolutionAccess::TimedOut;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return ResolutionAccess::TimedOut;
        }
        loop {
            if let Some(entries) = self.read_resolutions(key, first_read, false) {
                if admitted(&entries) {
                    self.resolution_hits.fetch_add(1, Ordering::Relaxed);
                    return ResolutionAccess::Ready(entries);
                }
                let _ = std::fs::remove_file(self.entry_path(key));
            }
            first_read = false;

            let sequence = LEASE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let token = format!(
                "{}:{}:{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                sequence
            )
            .into_bytes();
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock)
            {
                Ok(mut file) => {
                    use std::io::Write;
                    if file.write_all(&token).is_err() || file.sync_all().is_err() {
                        let _ = std::fs::remove_file(&lock);
                        return ResolutionAccess::TimedOut;
                    }
                    let lease = ResolutionLease { path: lock, token };
                    if let Some(entries) = self.read_resolutions(key, false, false) {
                        if admitted(&entries) {
                            self.resolution_hits.fetch_add(1, Ordering::Relaxed);
                            return ResolutionAccess::Ready(entries);
                        }
                        let _ = std::fs::remove_file(self.entry_path(key));
                    }
                    self.resolution_owners.fetch_add(1, Ordering::Relaxed);
                    return ResolutionAccess::Owner(lease);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if !waiting {
                        self.resolution_waits.fetch_add(1, Ordering::Relaxed);
                        waiting = true;
                    }
                }
                Err(_) => {
                    self.resolution_timeouts.fetch_add(1, Ordering::Relaxed);
                    return ResolutionAccess::TimedOut;
                }
            }

            let stale_lock = std::fs::metadata(&lock)
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                .is_some_and(|age| age >= stale);
            match resolution_wait_action(stale_lock, started.elapsed() >= wait) {
                1 => {
                    let observed = std::fs::read(&lock).ok();
                    if observed.is_some() && observed == std::fs::read(&lock).ok() {
                        let _ = std::fs::remove_file(&lock);
                    }
                    continue;
                }
                2 => {
                    self.resolution_timeouts.fetch_add(1, Ordering::Relaxed);
                    return ResolutionAccess::TimedOut;
                }
                _ => {}
            }
            let elapsed = started.elapsed().as_secs() as usize;
            if elapsed > 0 {
                if let Some(report) = progress {
                    report(
                        elapsed.min(wait.as_secs() as usize),
                        wait.as_secs() as usize,
                    );
                }
            }
            std::thread::sleep(poll);
        }
    }

    /// Store a resolution snapshot under a workspace key.
    pub fn put_resolutions(
        &self,
        key: &str,
        entries: &[(Option<crate::model::SymbolId>, crate::model::Confidence)],
    ) {
        if self.disabled.load(Ordering::Relaxed) {
            return;
        }
        let Some(entries_sha256) = serialized_digest(entries) else {
            return;
        };
        let snapshot = ResolutionSnapshot {
            basis: key.to_owned(),
            entries_sha256,
            entries: entries.to_vec(),
        };
        let Ok(bytes) = postcard::to_allocvec(&snapshot) else {
            return;
        };
        let path = self.entry_path(key);
        let Some(dir) = path.parent() else { return };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        let Ok(mut tmp) = tempfile::NamedTempFile::new_in(dir) else {
            return;
        };
        use std::io::Write;
        if tmp.write_all(&bytes).is_err() {
            return;
        }
        let _ = tmp.persist(&path);
    }

    pub fn get_analysis<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        if self.disabled.load(Ordering::Relaxed) {
            return None;
        }
        let path = self.entry_path(key);
        let bytes = std::fs::read(&path).ok()?;
        match postcard::from_bytes(&bytes) {
            Ok(value) => Some(value),
            Err(_) => {
                let _ = std::fs::remove_file(path);
                None
            }
        }
    }

    pub fn put_analysis<T: Serialize>(&self, key: &str, value: &T) {
        if self.disabled.load(Ordering::Relaxed) {
            return;
        }
        let Ok(bytes) = postcard::to_allocvec(value) else {
            return;
        };
        let path = self.entry_path(key);
        let Some(dir) = path.parent() else { return };
        if std::fs::create_dir_all(dir).is_err() {
            self.disabled.store(true, Ordering::Relaxed);
            return;
        }
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

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            resolution_hits: self.resolution_hits.load(Ordering::Relaxed),
            resolution_misses: self.resolution_misses.load(Ordering::Relaxed),
            resolution_owners: self.resolution_owners.load(Ordering::Relaxed),
            resolution_waits: self.resolution_waits.load(Ordering::Relaxed),
            resolution_timeouts: self.resolution_timeouts.load(Ordering::Relaxed),
        }
    }

    pub fn location(&self) -> &Path {
        &self.root
    }

    pub fn is_fallback(&self) -> bool {
        self.fallback
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

pub(crate) fn serialized_digest<T: Serialize + ?Sized>(value: &T) -> Option<String> {
    let bytes = postcard::to_allocvec(value).ok()?;
    let digest = Sha256::digest(bytes);
    Some(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Where cache entries live.
fn default_cache_root() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
        return Some(PathBuf::from(xdg).join("fun-refactor"));
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return Some(PathBuf::from(home).join("Library/Caches/fun-refactor"));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache/fun-refactor"))
}

fn fallback_cache_roots() -> Vec<PathBuf> {
    let identity = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USER"))
        .unwrap_or_default();
    let digest = Sha256::digest(identity.to_string_lossy().as_bytes());
    let suffix = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let runtimes = std::env::var_os("XDG_RUNTIME_DIR")
        .into_iter()
        .chain(std::env::var_os("TMPDIR"))
        .map(PathBuf::from)
        .chain(std::iter::once(std::env::temp_dir()));
    let mut roots = Vec::new();
    for runtime in runtimes {
        let root = runtime.join(format!("fun-refactor-cache-{suffix}"));
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots
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

pub(crate) fn fact_semantics_fingerprint() -> String {
    format!(
        "v{SCHEMA_VERSION}-{}-{}",
        query_fingerprint(),
        env!("FUN_REFACTOR_EXTRACTOR_FINGERPRINT")
    )
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
        // The key is the content, so a copy costs nothing to index.
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

        let hits = cache.stats().hits;
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
    fn resolution_snapshots_bind_their_basis_and_entry_digest() {
        let (_dir, cache) = scratch();
        let key = "resolved-v2-workspace";
        let entries = vec![(
            Some(crate::model::SymbolId(0)),
            crate::model::Confidence::Exact,
        )];
        cache.put_resolutions(key, &entries);
        assert_eq!(cache.get_resolutions(key), Some(entries.clone()));

        let wrong_basis = ResolutionSnapshot {
            basis: "another-workspace".to_owned(),
            entries_sha256: serialized_digest(&entries).unwrap(),
            entries: entries.clone(),
        };
        std::fs::write(
            cache.entry_path(key),
            postcard::to_allocvec(&wrong_basis).unwrap(),
        )
        .unwrap();
        assert!(cache.get_resolutions(key).is_none());

        cache.put_resolutions(key, &entries);
        let mut bytes = std::fs::read(cache.entry_path(key)).unwrap();
        let last = bytes.last_mut().unwrap();
        *last ^= 1;
        std::fs::write(cache.entry_path(key), bytes).unwrap();
        assert!(cache.get_resolutions(key).is_none());
    }

    #[test]
    fn one_resolution_owner_publishes_to_concurrent_waiters() {
        let dir = tempfile::tempdir().unwrap();
        let owner_cache = Cache::open_at(dir.path()).unwrap();
        let waiter_cache = Cache::open_at(dir.path()).unwrap();
        let key = "resolved-v2-concurrent";
        let ResolutionAccess::Owner(owner) = owner_cache.acquire_resolutions(key, |_| true, None)
        else {
            panic!("the first process should own an empty resolution key.");
        };
        let waiter = std::thread::spawn(move || {
            let access = waiter_cache.acquire_resolutions_with(
                key,
                |_| true,
                Duration::from_secs(1),
                Duration::from_secs(1),
                Duration::from_millis(1),
                None,
            );
            (access, waiter_cache.stats())
        });
        std::thread::sleep(Duration::from_millis(20));
        let entries = vec![(
            Some(crate::model::SymbolId(0)),
            crate::model::Confidence::Exact,
        )];
        owner_cache.put_resolutions(key, &entries);
        drop(owner);

        let (access, stats) = waiter.join().unwrap();
        let ResolutionAccess::Ready(loaded) = access else {
            panic!("the waiter should consume the completed snapshot.");
        };
        assert_eq!(loaded, entries);
        assert!(stats.resolution_waits > 0);
        assert_eq!(stats.resolution_owners, 0);
        assert_eq!(owner_cache.stats().resolution_owners, 1);
    }

    #[test]
    fn stale_resolution_owners_are_recovered_and_live_owners_time_out() {
        let (_dir, cache) = scratch();
        let key = "resolved-v2-stale-owner";
        let lock = cache.entry_path(key).with_extension("resolution-lock");
        std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
        std::fs::write(&lock, b"dead-owner").unwrap();
        let access = cache.acquire_resolutions_with(
            key,
            |_| true,
            Duration::from_millis(20),
            Duration::ZERO,
            Duration::from_millis(1),
            None,
        );
        assert!(matches!(access, ResolutionAccess::Owner(_)));

        let other = "resolved-v2-live-owner";
        let ResolutionAccess::Owner(_owner) = cache.acquire_resolutions(other, |_| true, None)
        else {
            panic!("empty key should have an owner.");
        };
        let timed_out = cache.acquire_resolutions_with(
            other,
            |_| true,
            Duration::from_millis(5),
            Duration::from_secs(1),
            Duration::from_millis(1),
            None,
        );
        assert!(matches!(timed_out, ResolutionAccess::TimedOut));
        assert_eq!(cache.stats().resolution_timeouts, 1);
    }

    #[test]
    fn structurally_invalid_resolution_snapshots_are_replaced() {
        let (_dir, cache) = scratch();
        let key = "resolved-v2-invalid-structure";
        let invalid = vec![(
            Some(crate::model::SymbolId(8)),
            crate::model::Confidence::Exact,
        )];
        cache.put_resolutions(key, &invalid);
        let ResolutionAccess::Owner(owner) = cache.acquire_resolutions(
            key,
            |entries| entries.iter().all(|(target, _)| target.is_none()),
            None,
        ) else {
            panic!("admission must remove an invalid snapshot before ownership.");
        };
        let valid = vec![(None, crate::model::Confidence::NameOnly)];
        cache.put_resolutions(key, &valid);
        drop(owner);
        assert_eq!(cache.get_resolutions(key), Some(valid));
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
        // The one above checks the fingerprint is *stable*.
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
