//! Where source text comes from.
//!
//! Every analysis in this crate re-reads a file's bytes when it needs them, the
//! index keeps facts and spans, not contents, so a span is only useful against the
//! source it was measured on. Until now each of those reads went straight to
//! `std::fs`, which is correct in a terminal and impossible in a browser.
//!
//! This is the one place that decides. On a normal build it delegates to the
//! filesystem and costs nothing. On `wasm32` there is no filesystem, so it reads
//! from a map the host loaded, a repository fetched from GitHub, say, and writes
//! back into the same map, which is what makes a refactoring in the playground a
//! real edit against real bytes instead of a rendering of one.
//!
//! Having a single choke point is worth something on native too: it is now possible
//! to say exactly what this crate reads, and to answer it from somewhere else.

use std::io;
use std::path::Path;

/// A workspace held in memory and not on disk.
///
/// Compiled everywhere, because "read through this map instead of the disk" has three
/// callers and only one of them is the browser. The recipe runner is the other: it
/// holds the workspace as each step left it and the refactorings read through *this*,
/// so a plan made after one step is measured against the text that step produced. It
/// was gated on the browser build, and the recipe runner's first two-step run failed
/// with `edit at 1..301 extends past end of file (226 bytes)`, spans computed against
/// the file on disk, applied to the file in memory.
///
/// Nothing in here is wasm-specific; it was only ever gated to pick a backend.
mod memory {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    /// One workspace's files.
    ///
    /// A handle instead of one global map, because a page can hold more than one
    /// workspace at a time and used to do so wrongly: loading a second repository
    /// replaced the bytes the first one's index was measured against, and every span
    /// the older handle held then pointed into somebody else's file. Nothing failed,
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

    // Only a host build has two backings to choose between. On wasm there is no disk,
    // so nothing ever asks and the flag would be written and never read.
    #[cfg(not(target_arch = "wasm32"))]
    thread_local! {
        /// Whether anyone has handed over a workspace.
        ///
        /// What decides between the two backings on a host build. Without it
        /// `activate` would be a no-op there, the call would succeed and the reads
        /// would go to the filesystem, which is the sort of quietly-wrong answer this
        /// module exists to prevent.
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
    ///
    /// The recipe runner plans against an in-memory workspace and then writes the
    /// result to the real one; without this the write would land back in the map it
    /// had just finished reading.
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
///
/// Only a host build compiled with the browser API has both backings; on wasm there is
/// no disk and on an ordinary build there is no handing over, so in both of those this
/// expands to nothing at all.
///
/// A macro instead of a function because the two backings share their names and only
/// one of them exists in most configurations.
macro_rules! through_memory {
    ($call:ident($($arg:expr),*)) => {{
        #[cfg(not(target_arch = "wasm32"))]
        if memory::is_active() {
            return memory::$call($($arg),*);
        }
    }};
}

/// Are reads and writes going to a workspace held in memory and not to the disk?
///
/// The question every caller that cares about *how* a write lands has to ask, and it
/// is a fact about the active backing and not about which features were compiled.
/// Asking `cfg!(feature = "cli")` instead is what made `commit` stage temporary files
/// beside a path that exists only in a browser's memory, on any build with both
/// features, a failure that could not happen in either build shipped today and was
/// waiting for the first one that had both.
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
///
/// Paired with [`activate`]: a caller that hands over a workspace to plan against has
/// to hand it back before anything is written, or the write lands in the map.
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
///
/// Used for the questions a language asks of its neighbours, whether a `Chart.yaml`
/// sits beside a YAML file, which is what makes it a Helm template and not plain
/// YAML.
pub fn exists(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    through_memory!(exists(path));
    backing::exists(path)
}

/// The files directly inside a directory, in no particular order.
///
/// Directories themselves are not returned: a browser workspace is a flat map of
/// paths, so a directory only exists as the prefix of a file that is in it.
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
///
/// Only meaningful where there is no filesystem. The owner passes it to
/// [`activate`] before every operation, so two workspaces can exist at once without
/// either one reading the other's bytes.
pub type Handle = memory::Handle;

pub fn new_handle(files: impl IntoIterator<Item = (std::path::PathBuf, String)>) -> Handle {
    memory::new_handle(files)
}

pub fn activate(handle: &Handle) {
    memory::activate(handle)
}

/// A directory, as a sentence can name it.
///
/// `Path::display` renders the workspace root, the parent of a top-level file, as
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
