//! R7RS library / module system (§5.6), with a filesystem loader.
//!
//! Beyond the R7RS-small core forms, this module discovers libraries on
//! disk: an `(import (foo bar))` that isn't built in is resolved to
//! `foo/bar.sld` on a search path, which is how the R7RS-large SRFI
//! libraries under `lib/` are loaded on demand.
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
//! ## Mutable bindings are shared, not copied
//!
//! A library's exports are stored as the *cells* that back them (see
//! [`crate::env::Cell`]), so `import` aliases the importer's name to
//! the same location. A `set!` in the library is visible to importers
//! and vice versa — the semantics R7RS expects for libraries that
//! expose mutable state (counters, registries, on-load hooks). This
//! resolved the former value-copy gap (bead `nscheme-q1c`).

use std::cell::RefCell;
use std::collections::HashMap;
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
    LIBRARIES.with(|r| r.borrow_mut().insert(name, bindings));
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
/// compiled-in defaults. Pass an empty vector to disable all lookup.
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
    dirs.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/lib")));
    dirs.push(PathBuf::from("./lib"));
    dirs
}

/// Locate the source file for `name` on the search path. A name like
/// `(scheme list)` maps to the relative path `scheme/list`, tried with
/// `.sld` then `.scm`. Returns the first existing file.
fn find_library_file(name: &LibraryName) -> Option<PathBuf> {
    let rel: PathBuf = name.iter().collect();
    for dir in library_search_path() {
        for ext in ["sld", "scm"] {
            let candidate = dir.join(&rel).with_extension(ext);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Whether `name` could be loaded from the search path (a matching
/// `.sld`/`.scm` exists), without actually loading it. Used by
/// `cond-expand`'s `(library …)` test so an as-yet-unloaded on-disk
/// library counts as available.
pub fn library_findable(name: &LibraryName) -> bool {
    find_library_file(name).is_some()
}

/// Try to load `name` from disk. Returns `Ok(true)` if a file was found
/// and evaluated (which registers the library via its `define-library`
/// form), `Ok(false)` if no file exists on the path, or an error if a
/// file was found but failed to read or evaluate.
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
    let Some(path) = find_library_file(name) else {
        return Ok(false);
    };
    let source = std::fs::read_to_string(&path)
        .map_err(|e| malformed(&format!("reading library ({}): {e}", name.join(" "))))?;
    LOADING.with(|s| s.borrow_mut().push(name.clone()));
    // So an `include` inside the library resolves relative to the
    // library file's own directory.
    let dir = path
        .parent()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    push_load_dir(dir);
    let result = crate::eval::eval_source(&source, loader_root()?);
    pop_load_dir();
    LOADING.with(|s| {
        s.borrow_mut().pop();
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
