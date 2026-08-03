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
    use std::rc::Rc;

    /// One workspace's files.
    ///
    /// A handle rather than one global map, because a page can hold more than one
    /// workspace at a time and used to do so wrongly: loading a second repository
    /// replaced the bytes the first one's index was measured against, and every span
    /// the older handle held then pointed into somebody else's file. Nothing failed —
    /// the answers were just quietly about the wrong text.
    pub type Handle = Rc<RefCell<BTreeMap<PathBuf, String>>>;

    thread_local! {
        /// Whose files the analysis is currently reading. A browser tab is
        /// single-threaded, so a thread-local is the whole story.
        static ACTIVE: RefCell<Handle> = RefCell::new(Rc::new(RefCell::new(BTreeMap::new())));
    }

    /// Put a set of files together into a handle nothing else can reach.
    pub fn new_handle(files: impl IntoIterator<Item = (PathBuf, String)>) -> Handle {
        Rc::new(RefCell::new(files.into_iter().collect()))
    }

    /// Read and write through these files until told otherwise.
    pub fn activate(handle: &Handle) {
        ACTIVE.with(|a| *a.borrow_mut() = Rc::clone(handle));
    }

    fn with_active<T>(f: impl FnOnce(&BTreeMap<PathBuf, String>) -> T) -> T {
        ACTIVE.with(|a| {
            let handle = Rc::clone(&a.borrow());
            let files = handle.borrow();
            f(&files)
        })
    }

    pub fn paths() -> Vec<PathBuf> {
        with_active(|files| files.keys().cloned().collect())
    }

    pub fn read_to_string(path: &Path) -> io::Result<String> {
        with_active(|files| {
            files.get(path).cloned().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{} is not in the loaded workspace", path.display()),
                )
            })
        })
    }

    pub fn write(path: &Path, contents: &str) -> io::Result<()> {
        ACTIVE.with(|a| {
            let handle = Rc::clone(&a.borrow());
            let mut files = handle.borrow_mut();
            files.insert(path.to_path_buf(), contents.to_string());
        });
        Ok(())
    }

    pub fn exists(path: &Path) -> bool {
        with_active(|files| files.contains_key(path))
    }

    pub fn read_dir(dir: &Path) -> io::Result<Vec<PathBuf>> {
        Ok(with_active(|files| {
            files
                .keys()
                .filter(|path| path.parent() == Some(dir))
                .cloned()
                .collect()
        }))
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

    pub fn read_dir(dir: &Path) -> io::Result<Vec<std::path::PathBuf>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            out.push(entry?.path());
        }
        Ok(out)
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

/// The files directly inside a directory, in no particular order.
///
/// Directories themselves are not returned: a browser workspace is a flat map of
/// paths, so a directory only exists as the prefix of a file that is in it.
pub fn read_dir(dir: impl AsRef<Path>) -> io::Result<Vec<std::path::PathBuf>> {
    backing::read_dir(dir.as_ref())
}

/// Every file the workspace holds, where that is a knowable thing.
#[cfg(target_arch = "wasm32")]
pub fn paths() -> Vec<std::path::PathBuf> {
    backing::paths()
}

/// A set of in-memory files, owned by whoever asked for it.
///
/// Only meaningful where there is no filesystem. The owner passes it to
/// [`activate`] before every operation, so two workspaces can exist at once without
/// either one reading the other's bytes.
#[cfg(target_arch = "wasm32")]
pub type Handle = backing::Handle;

#[cfg(target_arch = "wasm32")]
pub fn new_handle(files: impl IntoIterator<Item = (std::path::PathBuf, String)>) -> Handle {
    backing::new_handle(files)
}

#[cfg(target_arch = "wasm32")]
pub fn activate(handle: &Handle) {
    backing::activate(handle)
}

/// A directory, as a sentence can name it.
///
/// `Path::display` renders the workspace root — the parent of a top-level file — as
/// the empty string, so a message built from it comes out with a hole in it: *"no .go
/// file in declares a package"*. That is what `fr move x.go` to a file at the root
/// actually printed. Fifteen messages across move, signature and provenance
/// interpolate a directory this way, and any of them can be handed the root.
pub fn describe_dir(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    if path.as_os_str().is_empty() || path == Path::new(".") {
        "the workspace root".to_string()
    } else {
        path.display().to_string()
    }
}
