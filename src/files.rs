//! The interpreter's file access, behind a replaceable filesystem.
//!
//! Every file operation a Scheme program can cause goes through the
//! per-thread [`FileSystem`] installed here: `open-input-file`, the write
//! that `close-port` performs for an output file, `file-exists?`,
//! `delete-file`, `load`, `include`, and the library loader's lookup of
//! `.sld` files. By default that is [`StdFileSystem`], the real disk. An
//! embedder can install another with [`set_file_system`]. The browser
//! playground is to install a [`MemoryFileSystem`] seeded from the files
//! the page has saved, and post each change back to the page to persist
//! (nscheme-yd0.2.4); until it does, file access in a browser fails as
//! it always has (bead nscheme-yd0.2.1, ADR 0012).
//!
//! The command-line frontend reading the program you named (`nscheme
//! FILE`) is the host reading its input, not the program touching
//! files, so `main.rs` still uses `std::fs` directly.
//!
//! ## What you'll learn here
//!
//! - **Rust**: a trait object as a plug-in point. [`FileSystem`] is a
//!   trait; the installed one is stored as `Box<dyn FileSystem>`, so the
//!   call sites don't know or care which implementation they reach.
//! - **Rust**: interior mutability behind `&self`. The trait's methods
//!   take `&self` so the thread-local can lend the filesystem out
//!   without a mutable borrow; [`MemoryFileSystem`] keeps its map in a
//!   `RefCell` to change it anyway.
//! - **Scheme**: R7RS files are named by strings and hold text. Ports
//!   read a whole file in on open and write it out on close, so a
//!   filesystem only needs whole-file operations.
//!
//! ## Read alongside
//!
//! - [`crate::io`] — the file ports, and the console sink this mirrors.
//! - [`crate::library`] — the loader's search path.
//! - R7RS §6.13 (ports) and §6.14 (`file-exists?`, `delete-file`).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

// ---------------------------------------------------------------------
// The trait and the installed filesystem
// ---------------------------------------------------------------------
//
// The interpreter calls the five free functions at the bottom of this
// section (`read_to_string`, `write`, …), never `std::fs`. Each borrows
// the thread's installed filesystem for the length of one call.

/// Whole-file text operations: all the interpreter ever needs.
///
/// **Rust note**: this is the first trait in the crate written to be used
/// as a *trait object*. `Box<dyn FileSystem>` is a pointer to some type
/// that implements the trait, plus a table of its methods, so code can
/// call `fs.write(..)` without knowing the concrete type, the way an
/// interface works in Java or Go. The cost is one indirect call per file
/// operation, which is nothing next to the I/O itself.
pub trait FileSystem {
    /// The whole file as text.
    fn read_to_string(&self, path: &Path) -> io::Result<String>;

    /// Create or replace the file with `contents`.
    fn write(&self, path: &Path, contents: &str) -> io::Result<()>;

    /// Whether anything (a file or a directory) exists at `path`; this is
    /// what `file-exists?` reports.
    fn exists(&self, path: &Path) -> bool;

    /// Whether `path` is a file, which is what the library loader asks of
    /// each candidate `.sld`.
    fn is_file(&self, path: &Path) -> bool;

    /// Delete the file.
    fn remove_file(&self, path: &Path) -> io::Result<()>;
}

thread_local! {
    /// The installed filesystem, or `None` for the default (the disk).
    static FILE_SYSTEM: RefCell<Option<Box<dyn FileSystem>>> = const { RefCell::new(None) };
}

/// Make `fs` the filesystem every file operation on this thread uses;
/// `None` restores the default, [`StdFileSystem`].
///
/// The installed filesystem stays borrowed while each of its methods
/// runs, so a method (or anything it calls, such as a
/// [`MemoryFileSystem`]'s observer) must not call `set_file_system`:
/// that would panic. The filesystem being replaced is dropped after the
/// swap, so its `Drop` may use the new one.
pub fn set_file_system(fs: Option<Box<dyn FileSystem>>) {
    let old = FILE_SYSTEM.with(|slot| slot.replace(fs));
    drop(old);
}

/// Run `f` against the installed filesystem, or the disk if none is.
fn with_file_system<T>(f: impl FnOnce(&dyn FileSystem) -> T) -> T {
    FILE_SYSTEM.with(|slot| match slot.borrow().as_deref() {
        Some(fs) => f(fs),
        None => f(&StdFileSystem),
    })
}

/// Read `path` through the installed filesystem.
pub(crate) fn read_to_string(path: &Path) -> io::Result<String> {
    with_file_system(|fs| fs.read_to_string(path))
}

/// Write `path` through the installed filesystem.
pub(crate) fn write(path: &Path, contents: &str) -> io::Result<()> {
    with_file_system(|fs| fs.write(path, contents))
}

/// Ask the installed filesystem whether anything exists at `path`.
pub(crate) fn exists(path: &Path) -> bool {
    with_file_system(|fs| fs.exists(path))
}

/// Ask the installed filesystem whether `path` is a file.
pub(crate) fn is_file(path: &Path) -> bool {
    with_file_system(|fs| fs.is_file(path))
}

/// Delete `path` through the installed filesystem.
pub(crate) fn remove_file(path: &Path) -> io::Result<()> {
    with_file_system(|fs| fs.remove_file(path))
}

/// A shared filesystem is one too, so an embedder can install an
/// `Rc<MemoryFileSystem>` and keep a clone to change the files between
/// runs (the browser does, when the page saves or deletes a file).
impl<T: FileSystem + ?Sized> FileSystem for Rc<T> {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        (**self).read_to_string(path)
    }

    fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
        (**self).write(path, contents)
    }

    fn exists(&self, path: &Path) -> bool {
        (**self).exists(path)
    }

    fn is_file(&self, path: &Path) -> bool {
        (**self).is_file(path)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        (**self).remove_file(path)
    }
}

// ---------------------------------------------------------------------
// The disk
// ---------------------------------------------------------------------

/// The real filesystem, through `std::fs`. The default. (In a browser,
/// where there is no disk, reads, writes and deletes fail with
/// `Unsupported`, and `exists` / `is_file` are `false`.)
#[derive(Clone, Copy, Debug, Default)]
pub struct StdFileSystem;

impl FileSystem for StdFileSystem {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
        std::fs::write(path, contents)
    }

    fn exists(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }
}

// ---------------------------------------------------------------------
// An in-memory filesystem
// ---------------------------------------------------------------------
//
// A map from path to text, for hosts with no disk (such as a browser)
// and for tests. Directories are implicit: one exists wherever a file's
// path passes through it, so writing `a/b/c` needs no `mkdir` first,
// which is looser than a disk. Otherwise it fails where a disk would: a
// path cannot be both a file and a directory, so writing `a/b` fails
// when `a` is a file (`NotADirectory`) or when `a/b` already holds files
// (`IsADirectory`). Paths are normalized lexically, since there is no
// real filesystem underneath to resolve them against: `./a/b`, `a/./b`
// and `a/c/../b` all name the file `a/b`.

/// What changed in a [`MemoryFileSystem`], as reported to its observer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileChange {
    /// The file at `path` was created or replaced with `contents`.
    Written { path: String, contents: String },
    /// The file at `path` was deleted.
    Removed { path: String },
}

/// A change observer; see [`MemoryFileSystem::with_observer`].
pub type FileObserver = Box<dyn FnMut(&FileChange)>;

/// Files held in memory, keyed by normalized path.
#[derive(Default)]
pub struct MemoryFileSystem {
    files: RefCell<BTreeMap<PathBuf, String>>,
    observer: RefCell<Option<FileObserver>>,
}

impl MemoryFileSystem {
    /// An empty filesystem.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add (or replace) a file without telling the observer: this is for
    /// seeding the filesystem with files that are already saved.
    pub fn insert(&self, path: impl AsRef<Path>, contents: impl Into<String>) {
        self.files
            .borrow_mut()
            .insert(normalize(path.as_ref()), contents.into());
    }

    /// Remove a file without telling the observer, the mirror of
    /// [`insert`](Self::insert): for a deletion that came from outside the
    /// program. Returns whether the file was there.
    pub fn discard(&self, path: impl AsRef<Path>) -> bool {
        self.files
            .borrow_mut()
            .remove(&normalize(path.as_ref()))
            .is_some()
    }

    /// Call `observer` after every write and delete, with the normalized
    /// path, so a host can persist each change (the browser playground
    /// posts it to the page). The observer runs while it is borrowed, so
    /// it must not write or delete files itself: that would panic.
    #[must_use]
    pub fn with_observer(self, observer: FileObserver) -> Self {
        *self.observer.borrow_mut() = Some(observer);
        self
    }

    /// Tell the observer, if there is one, about `change`.
    fn notify(&self, change: &FileChange) {
        if let Some(observer) = self.observer.borrow_mut().as_mut() {
            observer(change);
        }
    }
}

impl FileSystem for MemoryFileSystem {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        let key = normalize(path);
        let files = self.files.borrow();
        match files.get(&key) {
            Some(text) => Ok(text.clone()),
            None if is_directory(&files, &key) => Err(is_a_directory()),
            None => Err(not_found()),
        }
    }

    fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
        let key = normalize(path);
        if key.as_os_str().is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty path"));
        }
        {
            let mut files = self.files.borrow_mut();
            if key.ancestors().skip(1).any(|dir| files.contains_key(dir)) {
                return Err(io::Error::new(
                    io::ErrorKind::NotADirectory,
                    "Not a directory",
                ));
            }
            if is_directory(&files, &key) {
                return Err(is_a_directory());
            }
            files.insert(key.clone(), contents.to_string());
        }
        self.notify(&FileChange::Written {
            path: key.to_string_lossy().into_owned(),
            contents: contents.to_string(),
        });
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        // An empty name exists nowhere, as on a disk. (`/` or `..` is the
        // root, which does.)
        if path.as_os_str().is_empty() {
            return false;
        }
        let key = normalize(path);
        let files = self.files.borrow();
        key.as_os_str().is_empty() || files.contains_key(&key) || is_directory(&files, &key)
    }

    fn is_file(&self, path: &Path) -> bool {
        self.files.borrow().contains_key(&normalize(path))
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        let key = normalize(path);
        let removed = self.files.borrow_mut().remove(&key).is_some();
        if !removed {
            let files = self.files.borrow();
            return Err(if is_directory(&files, &key) {
                is_a_directory()
            } else {
                not_found()
            });
        }
        self.notify(&FileChange::Removed {
            path: key.to_string_lossy().into_owned(),
        });
        Ok(())
    }
}

/// Whether some file sits under `key`, which makes `key` a directory.
fn is_directory(files: &BTreeMap<PathBuf, String>, key: &Path) -> bool {
    files
        .keys()
        .any(|file| file != key && file.starts_with(key))
}

/// The `NotFound` error a disk gives. Callers already name the path.
fn not_found() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "No such file or directory")
}

/// The error a disk gives for a file operation on a directory.
fn is_a_directory() -> io::Error {
    io::Error::new(io::ErrorKind::IsADirectory, "Is a directory")
}

/// Resolve `.` and `..` and drop a leading `/`, without consulting any
/// filesystem: an in-memory filesystem has one root, and `..` above it
/// stays at it.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lexical normalization is what lets the loader's `./lib/x.sld`,
    /// a program's `lib/x.sld` and an include's `lib/sub/../x.sld` all
    /// reach the same in-memory file.
    #[test]
    fn paths_that_name_the_same_file_normalize_alike() {
        for p in ["a/b", "./a/b", "a/./b", "a/c/../b", "/a/b", "a//b"] {
            assert_eq!(normalize(Path::new(p)), PathBuf::from("a/b"), "{p}");
        }
        assert_eq!(normalize(Path::new("../../a")), PathBuf::from("a"));
    }

    /// Directories have no entries of their own, so `exists` must see
    /// one wherever a file lives under it, while `is_file` must not
    /// mistake it for a file (the loader asks `is_file`).
    #[test]
    fn directories_exist_only_where_files_live() {
        let fs = MemoryFileSystem::new();
        fs.insert("lib/mine/util.sld", "x");
        assert!(fs.exists(Path::new("lib")));
        assert!(fs.exists(Path::new("./lib/mine")));
        assert!(!fs.is_file(Path::new("lib/mine")));
        assert!(fs.is_file(Path::new("lib/mine/util.sld")));
        assert!(
            !fs.exists(Path::new("li")),
            "a name prefix is not a directory"
        );
    }

    /// A path can't be both a file and a directory, or the observer would
    /// hand the page a tree it can't store. Each conflict fails with the
    /// error kind a disk gives.
    #[test]
    fn a_path_cannot_be_both_a_file_and_a_directory() {
        let fs = MemoryFileSystem::new();
        fs.write(Path::new("a"), "file").unwrap();
        let e = fs.write(Path::new("a/b"), "x").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotADirectory);
        fs.write(Path::new("d/e/f"), "x").unwrap();
        for e in [
            fs.write(Path::new("d/e"), "x").unwrap_err(),
            fs.read_to_string(Path::new("d")).unwrap_err(),
            fs.remove_file(Path::new("d/e")).unwrap_err(),
        ] {
            assert_eq!(e.kind(), io::ErrorKind::IsADirectory);
        }
        assert_eq!(fs.read_to_string(Path::new("a")).unwrap(), "file");
    }

    /// `file-exists?` must agree with a disk on the edge cases: an empty
    /// name exists nowhere; the root (`/`, `..`) always exists.
    #[test]
    fn the_empty_name_does_not_exist_but_the_root_does() {
        let fs = MemoryFileSystem::new();
        assert!(!fs.exists(Path::new("")));
        assert!(fs.exists(Path::new("/")));
        assert!(fs.exists(Path::new("..")));
    }

    /// The observer is how the browser persists changes, so it must see
    /// every write and delete with the normalized path, and must *not*
    /// see the seeding of already-saved files.
    #[test]
    fn the_observer_sees_writes_and_deletes_but_not_seeding() {
        let seen = std::rc::Rc::new(RefCell::new(Vec::new()));
        let log = std::rc::Rc::clone(&seen);
        let fs = MemoryFileSystem::new()
            .with_observer(Box::new(move |c| log.borrow_mut().push(c.clone())));
        fs.insert("saved.txt", "old");
        fs.write(Path::new("./out/new.txt"), "hello").unwrap();
        fs.remove_file(Path::new("saved.txt")).unwrap();
        assert!(fs.remove_file(Path::new("saved.txt")).is_err());
        assert_eq!(
            *seen.borrow(),
            vec![
                FileChange::Written {
                    path: "out/new.txt".into(),
                    contents: "hello".into()
                },
                FileChange::Removed {
                    path: "saved.txt".into()
                },
            ]
        );
    }
}
