//! R7RS-small library / module system (§5.6).
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
        // Same for SRFI 64 (the upstream test framework chibi's
        // suite was derived from).
        [s, b] if s == "srfi" && b == "64" => true,
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

/// Try to load `name` from disk. Returns `Ok(true)` if a file was found
/// and evaluated (which registers the library via its `define-library`
/// form), `Ok(false)` if no file exists on the path, or an error if a
/// file was found but failed to read or evaluate.
pub fn try_load_library(name: &LibraryName) -> Result<bool, EvalError> {
    // Already registered (e.g. a diamond import resolved earlier) —
    // nothing to do. This also makes the DAG of inter-library imports
    // terminate without a separate cycle guard for the common case.
    if library_exists(name) {
        return Ok(true);
    }
    let Some(path) = find_library_file(name) else {
        return Ok(false);
    };
    let source = std::fs::read_to_string(&path)
        .map_err(|e| malformed(&format!("reading library ({}): {e}", name.join(" "))))?;
    let root = LOADER_ROOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            let env = Env::new_global();
            crate::builtins::install_base(&env)?;
            *slot = Some(env);
        }
        Ok::<EnvRef, EvalError>(slot.as_ref().unwrap().clone())
    })?;
    crate::eval::eval_source(&source, root)?;
    // The file should have registered the library. If it named a
    // different library than the one we were asked for, this is false
    // and the caller reports "unknown library".
    Ok(library_exists(name))
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
