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

use crate::env::Cell;
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
