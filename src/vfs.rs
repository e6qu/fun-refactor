//! Where source text comes from.
//!
//! Every analysis in this crate re-reads a file's bytes when it needs them — the
//! index keeps facts and spans, not contents, so a span is only useful against the
//! source it was measured on. Until now each of those reads went straight to
//! `std::fs`, which is correct in a terminal and impossible in a browser.
//!
//! This is the one place that decides. On a normal build it delegates to the
//! filesystem and costs nothing. On `wasm32` there is no filesystem, so it reads
//! from a map the host loaded — a repository fetched from GitHub, say — and writes
//! back into the same map, which is what makes a refactoring in the playground a
//! real edit against real bytes rather than a rendering of one.
//!
//! Having a single choke point is worth something on native too: it is now possible
//! to say exactly what this crate reads, and to answer it from somewhere else.

use std::io;
use std::path::Path;

#[cfg(target_arch = "wasm32")]
mod backing {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::io;
    use std::path::{Path, PathBuf};

    thread_local! {
        /// The loaded workspace. A browser tab is single-threaded, so a thread-local
        /// is the whole story; nothing here is shared across workers.
        static FILES: RefCell<BTreeMap<PathBuf, String>> = RefCell::new(BTreeMap::new());
    }

    pub fn load(files: impl IntoIterator<Item = (PathBuf, String)>) {
        FILES.with(|f| {
            let mut map = f.borrow_mut();
            map.clear();
            map.extend(files);
        });
    }

    pub fn paths() -> Vec<PathBuf> {
        FILES.with(|f| f.borrow().keys().cloned().collect())
    }

    pub fn read_to_string(path: &Path) -> io::Result<String> {
        FILES.with(|f| {
            f.borrow().get(path).cloned().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{} is not in the loaded workspace", path.display()),
                )
            })
        })
    }

    pub fn write(path: &Path, contents: &str) -> io::Result<()> {
        FILES.with(|f| {
            f.borrow_mut()
                .insert(path.to_path_buf(), contents.to_string())
        });
        Ok(())
    }

    pub fn exists(path: &Path) -> bool {
        FILES.with(|f| f.borrow().contains_key(path))
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod backing {
    use std::io;
    use std::path::Path;

    pub fn read_to_string(path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    pub fn write(path: &Path, contents: &str) -> io::Result<()> {
        std::fs::write(path, contents)
    }

    pub fn exists(path: &Path) -> bool {
        path.exists()
    }
}

/// Read a file's text.
pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    backing::read_to_string(path.as_ref())
}

/// Replace a file's text.
pub fn write(path: impl AsRef<Path>, contents: impl AsRef<str>) -> io::Result<()> {
    backing::write(path.as_ref(), contents.as_ref())
}

/// Is there a file here?
///
/// Used for the questions a language asks of its neighbours — whether a `Chart.yaml`
/// sits beside a YAML file, which is what makes it a Helm template rather than plain
/// YAML.
pub fn exists(path: impl AsRef<Path>) -> bool {
    backing::exists(path.as_ref())
}

/// Every file the workspace holds, where that is a knowable thing.
#[cfg(target_arch = "wasm32")]
pub fn paths() -> Vec<std::path::PathBuf> {
    backing::paths()
}

/// Load a workspace into memory. Only meaningful where there is no filesystem.
#[cfg(target_arch = "wasm32")]
pub fn load(files: impl IntoIterator<Item = (std::path::PathBuf, String)>) {
    backing::load(files)
}
