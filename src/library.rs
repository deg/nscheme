//! R7RS-small library / module system (§5.6).
//!
//! Implements `define-library`, `import`, and `cond-expand` plus the
//! ability for the implementation-supplied libraries (`(scheme base)`,
//! `(scheme write)`, …) to be imported as no-ops because nscheme
//! already installs their bindings in the global env at startup.
//!
//! User-defined libraries are registered in a thread-local registry
//! keyed by the canonical library name (a `Vec<String>` of identifier
//! names).

use std::cell::RefCell;
use std::collections::HashMap;

use crate::eval::EvalError;
use crate::value::{Symbol, Value};

/// Library name as a vector of identifier names. `(scheme base)`
/// becomes `["scheme", "base"]`.
pub type LibraryName = Vec<String>;

thread_local! {
    /// Registry of user-defined libraries by name. The implementation-
    /// supplied libraries (`(scheme base)` etc.) are NOT stored here
    /// because their bindings are already installed in the global env.
    static LIBRARIES: RefCell<HashMap<LibraryName, HashMap<Symbol, Value>>> =
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
    matches!(
        name.as_slice(),
        [s, b] if s == "scheme"
            && matches!(
                b.as_str(),
                "base" | "write" | "read" | "char" | "file" | "inexact"
                | "complex" | "cxr" | "lazy" | "load" | "process-context"
                | "repl" | "time" | "case-lambda" | "eval" | "r5rs"
            )
    )
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
pub fn register_library(name: LibraryName, bindings: HashMap<Symbol, Value>) {
    LIBRARIES.with(|r| r.borrow_mut().insert(name, bindings));
}

pub fn library_bindings(name: &LibraryName) -> Option<HashMap<Symbol, Value>> {
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
