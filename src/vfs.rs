//! Where source text comes from.

use std::io;
use std::path::Path;

/// A workspace held in memory and not on disk.
mod memory {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    /// One workspace's files.
    pub type Handle = Rc<RefCell<BTreeMap<PathBuf, String>>>;

    thread_local! {
        /// Whose files the analysis is currently reading.
        static ACTIVE: RefCell<Handle> = RefCell::new(Rc::new(RefCell::new(BTreeMap::new())));
    }

    /// Put a set of files together into a handle nothing else can reach.
    pub fn new_handle(files: impl IntoIterator<Item = (PathBuf, String)>) -> Handle {
        Rc::new(RefCell::new(files.into_iter().collect()))
    }

    // Only a host build has two backings to choose between.
    #[cfg(not(target_arch = "wasm32"))]
    thread_local! {
        /// Whether anyone has handed over a workspace.
        static HANDED_OVER: RefCell<bool> = const { RefCell::new(false) };
    }

    /// Read and write through these files until told otherwise.
    pub fn activate(handle: &Handle) {
        ACTIVE.with(|a| *a.borrow_mut() = Rc::clone(handle));
        #[cfg(not(target_arch = "wasm32"))]
        HANDED_OVER.with(|h| *h.borrow_mut() = true);
    }

    /// Has a workspace been handed over on this thread?
    #[cfg(not(target_arch = "wasm32"))]
    pub fn is_active() -> bool {
        HANDED_OVER.with(|h| *h.borrow())
    }

    /// Go back to reading the disk.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn deactivate() {
        HANDED_OVER.with(|h| *h.borrow_mut() = false);
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

#[cfg(target_arch = "wasm32")]
use memory as backing;

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

/// Run this against the handed-over workspace instead of the disk, where there is one.
macro_rules! through_memory {
    ($call:ident($($arg:expr),*)) => {{
        #[cfg(not(target_arch = "wasm32"))]
        if memory::is_active() {
            return memory::$call($($arg),*);
        }
    }};
}

/// Are reads and writes going to a workspace held in memory and not to the disk?
pub fn is_in_memory() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        true
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        memory::is_active()
    }
}

/// Read and write the real filesystem again.
#[cfg(not(target_arch = "wasm32"))]
pub fn use_filesystem() {
    memory::deactivate()
}

/// Read a file's text.
pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    let path = path.as_ref();
    through_memory!(read_to_string(path));
    backing::read_to_string(path)
}

/// Replace a file's text.
pub fn write(path: impl AsRef<Path>, contents: impl AsRef<str>) -> io::Result<()> {
    let (path, contents) = (path.as_ref(), contents.as_ref());
    through_memory!(write(path, contents));
    backing::write(path, contents)
}

/// Is there a file here?
pub fn exists(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    through_memory!(exists(path));
    backing::exists(path)
}

/// The files directly inside a directory, in no particular order.
pub fn read_dir(dir: impl AsRef<Path>) -> io::Result<Vec<std::path::PathBuf>> {
    let dir = dir.as_ref();
    through_memory!(read_dir(dir));
    backing::read_dir(dir)
}

/// Every file the workspace holds, where that is a knowable thing.
pub fn paths() -> Vec<std::path::PathBuf> {
    memory::paths()
}

/// A set of in-memory files, owned by whoever asked for it.
pub type Handle = memory::Handle;

pub fn new_handle(files: impl IntoIterator<Item = (std::path::PathBuf, String)>) -> Handle {
    memory::new_handle(files)
}

pub fn activate(handle: &Handle) {
    memory::activate(handle)
}

/// A directory, as a sentence can name it.
pub fn describe_dir(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    if path.as_os_str().is_empty() || path == Path::new(".") {
        "the workspace root".to_string()
    } else {
        path.display().to_string()
    }
}

/// Resolve `.` and `..` without touching the filesystem, so two spellings of one path compare
/// equal.
pub fn normalise(path: impl AsRef<Path>) -> std::path::PathBuf {
    use std::path::Component;
    let mut out = std::path::PathBuf::new();
    for component in path.as_ref().components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
