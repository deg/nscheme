//! R7RS library / module system (§5.6), with a filesystem loader.
//!
//! Beyond the R7RS-small core forms, this module discovers libraries on
//! disk: an `(import (foo bar))` that isn't built in is resolved to
//! `foo/bar.sld` on a search path, which is how the R7RS-large SRFI
//! libraries under `lib/` are loaded on demand. "On disk" means the
//! installed [`crate::files`] filesystem, which an embedder can replace
//! (with an in-memory one, say). The same sources are also compiled
//! into the binary as a last-resort fallback.
//!
//! ## What you'll learn here
//!
//! - **Scheme**: how R7RS's `define-library` works. A library is a
//!   named bundle of bindings with an explicit export list; user
//!   code names the library with a symbol-list path (`(scheme
//!   base)`, `(srfi 1)`) and the `import` form copies the exported
//!   bindings into the importing scope.
//! - **Two kinds of library** in this interpreter:
//!     - **Built-in libraries** (`(scheme base)`, `(scheme
//!       write)`, …). Their bindings are already installed in the
//!       global env at startup by [`crate::builtins::install_base`]
//!       and [`crate::io::install_io`], so an `import` of these
//!       library names is a no-op. [`is_builtin_library`] is the
//!       recognition function.
//!     - **User libraries** defined via `define-library`. Their
//!       bindings go into a thread-local registry keyed by the
//!       canonical name, then get copied to the importing env on
//!       `import`.
//!
//! ## Read alongside
//!
//! - [`crate::eval`] — `step_define_library`, `step_import`, and
//!   `step_cond_expand` are in there.
//! - R7RS §5.6.
//!
//! ## Libraries compiled into the binary
//!
//! `build.rs` also compiles every `lib/` source into the binary (the
//! `EMBEDDED_LIBS` table). The loader tries it last, after every disk
//! directory, so a library is still found where there is no filesystem
//! (the browser build) or when an installed binary's `lib/` has moved.
//! A file on disk always wins, so editing `lib/` takes effect without
//! a rebuild during development. See ADR 0012.
//!
//! ## Standard and user libraries
//!
//! The registry remembers which libraries are *standard*: loaded from
//! the compiled-in copy or from the crate's own `lib/` directory.
//! [`forget_user_libraries`] drops all the others (saved files, inline
//! `define-library` forms), so the browser playground's Reset makes an
//! edited library reload while the standard ones stay cached (ADR 0013).
//!
//! ## Mutable bindings are shared, not copied
//!
//! A library's exports are stored as the *cells* that back them (see
//! [`crate::env::Cell`]), so `import` aliases the importer's name to
//! the same location. A `set!` in the library is visible to importers
//! and vice versa — the semantics R7RS expects for libraries that
//! expose mutable state (counters, registries, on-load hooks). This
//! resolved the former value-copy gap (bead `nscheme-q1c`).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::env::{Cell, Env, EnvRef};
use crate::eval::EvalError;
use crate::value::{Symbol, Value};

/// Library name as a vector of identifier names. `(scheme base)`
/// becomes `["scheme", "base"]`.
pub type LibraryName = Vec<String>;

thread_local! {
    /// Registry of user-defined libraries by name. The implementation-
    /// supplied libraries (`(scheme base)` etc.) are NOT stored here
    /// because their bindings are already installed in the global env.
    static LIBRARIES: RefCell<HashMap<LibraryName, HashMap<Symbol, Cell>>> =
        RefCell::new(HashMap::new());
}

/// Parse a library-name form. R7RS allows numbers as components
/// (e.g. `(srfi 1)`); we accept symbols and exact integers and render
/// them to strings for the canonical key.
pub fn parse_library_name(form: &Value) -> Result<LibraryName, EvalError> {
    let parts =
        collect_list(form).ok_or_else(|| malformed("library name must be a proper list"))?;
    if parts.is_empty() {
        return Err(malformed("library name cannot be empty"));
    }
    let mut out = Vec::with_capacity(parts.len());
    for p in parts {
        match p {
            Value::Symbol(s) => out.push(s.name().to_string()),
            Value::Int(n) => out.push(n.to_string()),
            _ => {
                return Err(malformed(
                    "library-name component must be an identifier or integer",
                ));
            }
        }
    }
    Ok(out)
}

/// The implementation-supplied libraries we recognize as no-op imports
/// because their bindings are already in the global env.
pub fn is_builtin_library(name: &LibraryName) -> bool {
    match name.as_slice() {
        [s, b] if s == "scheme" => matches!(
            b.as_str(),
            "base"
                | "write"
                | "read"
                | "char"
                | "file"
                | "inexact"
                | "complex"
                | "cxr"
                | "lazy"
                | "load"
                | "process-context"
                | "repl"
                | "time"
                | "case-lambda"
                | "eval"
                | "r5rs"
        ),
        // The chibi r7rs-tests.scm corpus imports `(chibi test)`.
        // We preload that shim into the global env outside the
        // library system, so the import is a no-op.
        [s, b] if s == "chibi" && b == "test" => true,
        // NOTE: `(srfi 64)` is a real on-disk library (lib/srfi/64.sld),
        // not a builtin no-op, so reference test suites that import it
        // get the actual harness.
        _ => false,
    }
}

/// Static feature list returned by `(features)` and consulted by
/// `cond-expand`.
pub fn features() -> Vec<&'static str> {
    vec![
        "r7rs",
        "nscheme",
        "nscheme-0.1",
        "exact-closed",
        "ratios",
        "ieee-float",
    ]
}

pub fn library_exists(name: &LibraryName) -> bool {
    LIBRARIES.with(|r| r.borrow().contains_key(name))
}

#[allow(clippy::implicit_hasher)]
pub fn register_library(name: LibraryName, bindings: HashMap<Symbol, Cell>) {
    // Registered while loading a compiled-in source: a standard library.
    // Anything else, including a user library that re-registers a
    // standard name, is not, so the mark is set or cleared every time.
    let standard = LOAD_KINDS.with(|k| k.borrow().last().copied()) == Some(LoadKind::Standard);
    STANDARD.with(|s| {
        let mut s = s.borrow_mut();
        if standard {
            s.insert(name.clone());
        } else {
            s.remove(&name);
        }
    });
    LIBRARIES.with(|r| r.borrow_mut().insert(name, bindings));
}

/// Forget every registered library except the standard ones (those
/// loaded from the copy of `lib/` compiled into the binary), so the next
/// `import` of any other reloads it from its source. Environments that
/// already imported a library keep the bindings they have.
///
/// The browser playground's Reset calls this, so a library the user
/// edited (a saved file, or an inline `define-library`) takes effect,
/// while the standard libraries stay cached: reloading those costs up to
/// a second (bead nscheme-yd0.2.3). Standard libraries import only one
/// another, so none of them holds a forgotten library, unless a saved
/// file shadows a standard library's name; the standard libraries that
/// imported the original keep it until the interpreter restarts.
pub fn forget_user_libraries() {
    STANDARD.with(|standard| {
        let standard = standard.borrow();
        LIBRARIES.with(|r| r.borrow_mut().retain(|name, _| standard.contains(name)));
    });
}

pub fn library_bindings(name: &LibraryName) -> Option<HashMap<Symbol, Cell>> {
    // Cloning the map clones the `Rc` cell handles, not the values, so
    // importers receive shared cells.
    LIBRARIES.with(|r| r.borrow().get(name).cloned())
}

// ---------------------------------------------------------------------
// Filesystem loader (bead nscheme-9q5)
//
// When `import` names a library that is neither built-in nor already
// registered, we search a load path for a matching source file, load
// it (which runs its `define-library` form and registers it), then
// retry. This is what lets nscheme adopt R7RS-large reference
// libraries from disk instead of pasting them into the program.
// ---------------------------------------------------------------------

thread_local! {
    /// Hermetic root environment for loading library files. It has the
    /// base library installed so a loaded `(define-library …)` body can
    /// resolve `cons`, `define-record-type`, `syntax-rules`, etc. up
    /// the parent chain. Built lazily and reused: loads are cached by
    /// the registry, but a few libraries may load before the cache
    /// fills, and they all share this root. Kept separate from any
    /// program env so libraries can't see the importer's local defines.
    static LOADER_ROOT: RefCell<Option<EnvRef>> = const { RefCell::new(None) };

    /// Test/embedding hook: when set, fully replaces the search path.
    /// Thread-local, so tests running in parallel don't interfere and
    /// no `unsafe` env mutation is needed. See [`set_search_path`].
    static SEARCH_PATH_OVERRIDE: RefCell<Option<Vec<PathBuf>>> = const { RefCell::new(None) };

    /// Libraries whose load is in progress on this thread. A library is
    /// only `register`ed *after* its `define-library` form finishes
    /// evaluating, so `library_exists` stays false for the whole load —
    /// which is fine for a diamond (A→B, A→C, B→C: C just loads once)
    /// but would recurse forever on a true cycle (A→B, B→A). This set
    /// catches the cycle and turns it into a clean error.
    static LOADING: RefCell<Vec<LibraryName>> = const { RefCell::new(Vec::new()) };

    /// Where each load in progress comes from, innermost last, so
    /// [`register_library`] can tell a standard library from a user's
    /// one even when one load triggers another.
    static LOAD_KINDS: RefCell<Vec<LoadKind>> = const { RefCell::new(Vec::new()) };

    /// The libraries registered by loading a compiled-in source: what
    /// [`forget_user_libraries`] keeps.
    static STANDARD: RefCell<HashSet<LibraryName>> = RefCell::new(HashSet::new());
}

/// The crate's `lib/` directory as of the build: where the standard
/// libraries live in a development checkout.
const COMPILED_LIB_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/lib");

/// Whether a load in progress reads a standard library (compiled in, or
/// from [`COMPILED_LIB_DIR`]) or a file the user supplied.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LoadKind {
    Standard,
    User,
}

thread_local! {
    /// Stack of directories of the files currently being loaded, so
    /// `include` resolves a relative path against the including file's
    /// directory (R7RS) rather than the process working directory.
    static LOAD_DIR: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

/// Push the directory of a file about to be loaded. Pair with
/// [`pop_load_dir`]. Used by the loader and by program runners so
/// `include` inside the file resolves relative to it.
pub fn push_load_dir(dir: PathBuf) {
    LOAD_DIR.with(|s| s.borrow_mut().push(dir));
}

/// Pop the most recently pushed load directory.
pub fn pop_load_dir() {
    LOAD_DIR.with(|s| {
        s.borrow_mut().pop();
    });
}

/// Resolve an `include` target. Absolute paths are used as-is; a
/// relative path is joined to the directory of the file currently being
/// loaded (or the process working directory if none).
pub fn resolve_include(path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        return p;
    }
    LOAD_DIR.with(|s| match s.borrow().last() {
        Some(dir) => dir.join(&p),
        None => p,
    })
}

/// Replace the library search path for the current thread. Primarily
/// for tests and embedders that ship libraries in a known location;
/// when set, it takes precedence over `NSCHEME_LIB_PATH` and the
/// compiled-in default directory. Pass an empty vector to disable all
/// *disk* lookup; the libraries compiled into the binary are still
/// found (see ADR 0012).
pub fn set_search_path(dirs: Vec<PathBuf>) {
    SEARCH_PATH_OVERRIDE.with(|s| *s.borrow_mut() = Some(dirs));
}

/// The directories searched for library files. If [`set_search_path`]
/// installed an override for this thread, that is used verbatim.
/// Otherwise, in order:
/// 1. `NSCHEME_LIB_PATH` (colon-separated), if set;
/// 2. a compiled-in default (`<crate>/lib`, baked at build time);
/// 3. `./lib` relative to the current directory.
fn library_search_path() -> Vec<PathBuf> {
    if let Some(dirs) = SEARCH_PATH_OVERRIDE.with(|s| s.borrow().clone()) {
        return dirs;
    }
    let mut dirs = Vec::new();
    if let Ok(path) = std::env::var("NSCHEME_LIB_PATH") {
        for entry in path.split(':') {
            if !entry.is_empty() {
                dirs.push(PathBuf::from(entry));
            }
        }
    }
    dirs.push(PathBuf::from(COMPILED_LIB_DIR));
    dirs.push(PathBuf::from("./lib"));
    dirs
}

// The `EMBEDDED_LIBS` table generated by `build.rs`: every `lib/`
// source, keyed by its path under `lib/` (`"scheme/list.sld"`).
include!(concat!(env!("OUT_DIR"), "/embedded_lib.rs"));

/// Where a library's source was found.
enum LibrarySource {
    /// A file on the disk search path.
    Disk(PathBuf),
    /// A source compiled into the binary; `rel` is its path under `lib/`.
    Embedded {
        rel: &'static str,
        text: &'static str,
    },
}

/// Locate the source for `name`. A name like `(scheme list)` maps to
/// the relative path `scheme/list`, tried with `.sld` then `.scm` in
/// each search-path directory; the first existing file wins. If no
/// directory has it, fall back to the copy compiled into the binary.
fn find_library_source(name: &LibraryName) -> Option<LibrarySource> {
    let rel: PathBuf = name.iter().collect();
    for dir in library_search_path() {
        for ext in ["sld", "scm"] {
            let candidate = dir.join(&rel).with_extension(ext);
            if crate::files::is_file(&candidate) {
                return Some(LibrarySource::Disk(candidate));
            }
        }
    }
    find_embedded(name).map(|(rel, text)| LibrarySource::Embedded { rel, text })
}

/// The compiled-in source for `name`, if `build.rs` embedded one.
fn find_embedded(name: &LibraryName) -> Option<(&'static str, &'static str)> {
    let rel = name.join("/");
    ["sld", "scm"].into_iter().find_map(|ext| {
        let key = format!("{rel}.{ext}");
        EMBEDDED_LIBS
            .binary_search_by(|(k, _)| (*k).cmp(key.as_str()))
            .ok()
            .map(|i| EMBEDDED_LIBS[i])
    })
}

/// Whether `name` could be loaded (a matching `.sld`/`.scm` exists on
/// the search path or is compiled in), without actually loading it. Used by
/// `cond-expand`'s `(library …)` test so an as-yet-unloaded on-disk
/// library counts as available.
pub fn library_findable(name: &LibraryName) -> bool {
    find_library_source(name).is_some()
}

/// Try to load `name` from the search path or the compiled-in copy.
/// Returns `Ok(true)` if a source was found and evaluated (which
/// registers the library via its `define-library` form), `Ok(false)` if
/// there is none, or an error if one was found but failed to read or
/// evaluate.
pub fn try_load_library(name: &LibraryName) -> Result<bool, EvalError> {
    // Already registered (e.g. a diamond import resolved earlier) —
    // nothing to do.
    if library_exists(name) {
        return Ok(true);
    }
    // A library that (transitively) imports itself would otherwise
    // re-read and re-eval forever, since it isn't registered until its
    // load completes. Detect the cycle and report it.
    if LOADING.with(|s| s.borrow().contains(name)) {
        return Err(malformed(&format!(
            "circular library dependency: ({})",
            name.join(" ")
        )));
    }
    let Some(found) = find_library_source(name) else {
        return Ok(false);
    };
    // An `include` inside the library resolves relative to the library
    // file's own directory. A compiled-in library claims the directory
    // it was compiled from, which exists in a development checkout and
    // fails cleanly where it doesn't.
    let (source, path, kind) = match found {
        LibrarySource::Disk(path) => {
            let source = crate::files::read_to_string(&path)
                .map_err(|e| malformed(&format!("reading library ({}): {e}", name.join(" "))))?;
            // The compiled-in `lib/` directory, found on disk in a
            // development checkout, holds the standard libraries too.
            let kind = if path.starts_with(COMPILED_LIB_DIR) {
                LoadKind::Standard
            } else {
                LoadKind::User
            };
            (std::borrow::Cow::Owned(source), path, kind)
        }
        LibrarySource::Embedded { rel, text } => (
            std::borrow::Cow::Borrowed(text),
            PathBuf::from(COMPILED_LIB_DIR).join(rel),
            LoadKind::Standard,
        ),
    };
    // Everything that can fail comes before the pushes below, so an error
    // can't leave the load stacks pushed.
    let root = loader_root()?;
    LOADING.with(|s| s.borrow_mut().push(name.clone()));
    LOAD_KINDS.with(|k| k.borrow_mut().push(kind));
    let dir = path
        .parent()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    push_load_dir(dir);
    let result = crate::eval::eval_source(&source, root);
    pop_load_dir();
    LOADING.with(|s| {
        s.borrow_mut().pop();
    });
    LOAD_KINDS.with(|k| {
        k.borrow_mut().pop();
    });
    result?;
    // The file should have registered the library. If it named a
    // different library than the one we were asked for, this is false
    // and the caller reports "unknown library".
    Ok(library_exists(name))
}

/// The hermetic, base-installed root environment used both to evaluate
/// loaded library files and to resolve qualified imports of built-in
/// libraries. Built lazily on first use and reused for the thread.
pub fn loader_root() -> Result<EnvRef, EvalError> {
    LOADER_ROOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            let env = Env::new_global();
            crate::builtins::install_base(&env)?;
            *slot = Some(env);
        }
        Ok(slot.as_ref().unwrap().clone())
    })
}

/// Enumerate the bindings of a built-in library as shared cells. Since
/// `install_base` installs every built-in library into one global
/// frame, this returns the full built-in surface rather than just one
/// library's exports — exact enough for `only`/`rename` (which name
/// identifiers explicitly) and a slight over-approximation for
/// `prefix`/`except` on a built-in (uncommon in practice).
pub fn builtin_bindings() -> Result<HashMap<Symbol, Cell>, EvalError> {
    Ok(loader_root()?.frame_cells().into_iter().collect())
}

fn collect_list(v: &Value) -> Option<Vec<Value>> {
    let mut out = Vec::new();
    let mut cur = v.clone();
    loop {
        match cur {
            Value::Null => return Some(out),
            Value::Pair(p) => {
                let pair = p.borrow();
                out.push(pair.car.clone());
                cur = pair.cdr.clone();
            }
            _ => return None,
        }
    }
}

fn malformed(msg: &str) -> EvalError {
    EvalError::MalformedForm {
        form: "library",
        message: msg.to_string(),
    }
}
