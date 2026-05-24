//! Lexical environment for the evaluator.
//!
//! An [`Env`] is the data structure that maps Scheme identifiers
//! (symbols) to values within a particular scope.
//!
//! ## What you'll learn here
//!
//! - **Scheme**: how lexical scope is modeled — a chain of frames
//!   with `define` binding in the current frame and `set!`
//!   mutating the nearest enclosing one. The same model handles
//!   global definitions, `let` bodies, lambda parameters, and
//!   `letrec` mutual recursion.
//! - **Rust**: a second appearance of the `Rc<RefCell<T>>` pattern
//!   (introduced in `value.rs` at `Pair`), this time wrapping a
//!   `HashMap`. Closures hang on to an `Rc<Env>` to keep their
//!   defining scope alive; the `RefCell` lets multiple closures
//!   over the same scope observe each other's mutations — the
//!   condition `letrec` and mutual recursion require.
//!
//! ## Read alongside
//!
//! - `value.rs` — defines [`Symbol`](crate::value::Symbol) (the
//!   keys) and [`Value`](crate::value::Value) (the values).
//! - `eval.rs` — every special form that creates a scope
//!   (`lambda`, `let`, …) calls [`Env::extend`].
//! - R7RS §5.3.2 (definitions), §4.1.6 (`set!`).
//!
//! ## Semantics in one paragraph
//!
//! `define` always binds in the *current* frame (so a top-level
//! `define` inside a `let` body creates a new local binding,
//! shadowing any same-named outer binding). `set!` walks up the
//! parent chain to mutate the *nearest enclosing* binding, raising
//! [`RuntimeError::Undefined`] if no such binding exists. `lookup`
//! also walks the parent chain. That's the whole module.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::value::{RuntimeError, Symbol, Value};

/// A shared, reference-counted handle to an environment.
pub type EnvRef = Rc<Env>;

/// One lexical scope: a frame plus an optional parent.
pub struct Env {
    frame: RefCell<HashMap<Symbol, Value>>,
    parent: Option<EnvRef>,
}

impl std::fmt::Debug for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Don't recurse into bindings (Values may contain procedures
        // closing over this env, which would loop). Just summarize.
        f.debug_struct("Env")
            .field("local_bindings", &self.frame.borrow().len())
            .field("has_parent", &self.parent.is_some())
            .finish()
    }
}

impl Env {
    /// Create a fresh global (parent-less) environment.
    pub fn new_global() -> EnvRef {
        Rc::new(Self {
            frame: RefCell::new(HashMap::new()),
            parent: None,
        })
    }

    /// Create a child environment that inherits from `parent`. Use this
    /// when entering a `lambda`, `let`, or other scope-introducing form.
    pub fn extend(parent: EnvRef) -> EnvRef {
        Rc::new(Self {
            frame: RefCell::new(HashMap::new()),
            parent: Some(parent),
        })
    }

    /// Bind `name` to `value` in *this* frame, replacing any existing
    /// binding in this frame. Bindings in ancestor frames are not
    /// affected.
    pub fn define(&self, name: Symbol, value: Value) {
        self.frame.borrow_mut().insert(name, value);
    }

    /// Look up `name`. Walks the parent chain. Returns `None` if no
    /// frame contains the binding.
    pub fn lookup(&self, name: &Symbol) -> Option<Value> {
        if let Some(v) = self.frame.borrow().get(name) {
            return Some(v.clone());
        }
        self.parent.as_ref().and_then(|p| p.lookup(name))
    }

    /// Mutate the nearest enclosing binding of `name`. Returns an
    /// `Undefined` error if no enclosing frame contains the binding.
    pub fn set(&self, name: &Symbol, value: Value) -> Result<(), RuntimeError> {
        if self.frame.borrow().contains_key(name) {
            self.frame.borrow_mut().insert(name.clone(), value);
            return Ok(());
        }
        if let Some(parent) = &self.parent {
            return parent.set(name, value);
        }
        Err(RuntimeError::Undefined(name.name().to_string()))
    }

    /// Number of bindings in this frame (does not count ancestors).
    /// Mostly for diagnostics and tests.
    pub fn local_len(&self) -> usize {
        self.frame.borrow().len()
    }
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::equal;

    fn s(name: &str) -> Symbol {
        Symbol::intern(name)
    }

    #[test]
    fn lookup_returns_defined_value() {
        let env = Env::new_global();
        env.define(s("x"), Value::Int(42));
        let got = env.lookup(&s("x")).expect("present");
        assert!(equal(&got, &Value::Int(42)));
    }

    #[test]
    fn lookup_unknown_returns_none() {
        let env = Env::new_global();
        assert!(env.lookup(&s("missing")).is_none());
    }

    #[test]
    fn child_inherits_parent_binding() {
        let parent = Env::new_global();
        parent.define(s("x"), Value::Int(1));
        let child = Env::extend(parent);
        let got = child.lookup(&s("x")).expect("present");
        assert!(equal(&got, &Value::Int(1)));
    }

    #[test]
    fn child_shadows_without_mutating_parent() {
        let parent = Env::new_global();
        parent.define(s("x"), Value::Int(1));
        let child = Env::extend(parent.clone());
        child.define(s("x"), Value::Int(2));
        // child sees 2.
        assert!(equal(&child.lookup(&s("x")).unwrap(), &Value::Int(2)));
        // parent still sees 1.
        assert!(equal(&parent.lookup(&s("x")).unwrap(), &Value::Int(1)));
    }

    #[test]
    fn set_mutates_current_frame() {
        let env = Env::new_global();
        env.define(s("x"), Value::Int(1));
        env.set(&s("x"), Value::Int(2)).expect("set ok");
        assert!(equal(&env.lookup(&s("x")).unwrap(), &Value::Int(2)));
    }

    #[test]
    fn set_walks_to_parent_frame() {
        let parent = Env::new_global();
        parent.define(s("x"), Value::Int(1));
        let child = Env::extend(parent.clone());
        // No binding for x in child; set! walks up.
        child.set(&s("x"), Value::Int(99)).expect("set ok");
        // Parent's binding has been mutated.
        assert!(equal(&parent.lookup(&s("x")).unwrap(), &Value::Int(99)));
        // Child observes the same value (via parent walk).
        assert!(equal(&child.lookup(&s("x")).unwrap(), &Value::Int(99)));
    }

    #[test]
    fn set_undefined_returns_error() {
        let env = Env::new_global();
        let err = env.set(&s("nope"), Value::Int(1)).expect_err("should fail");
        assert!(matches!(err, RuntimeError::Undefined(name) if name == "nope"));
    }

    #[test]
    fn set_uses_nearest_binding_not_global() {
        // R7RS: set! mutates the nearest enclosing binding.
        let parent = Env::new_global();
        parent.define(s("x"), Value::Int(1));
        let child = Env::extend(parent.clone());
        child.define(s("x"), Value::Int(2));
        child.set(&s("x"), Value::Int(3)).expect("set ok");
        assert!(equal(&child.lookup(&s("x")).unwrap(), &Value::Int(3)));
        // Parent still 1.
        assert!(equal(&parent.lookup(&s("x")).unwrap(), &Value::Int(1)));
    }

    #[test]
    fn closures_capturing_same_frame_observe_mutations() {
        // Two "closures" (here, plain EnvRef clones) over the same frame
        // both see updates: this is what lets recursive `letrec` and
        // mutually recursive definitions work.
        let env = Env::new_global();
        env.define(s("x"), Value::Int(0));
        let cap_a = env.clone();
        let cap_b = env.clone();
        cap_a.set(&s("x"), Value::Int(7)).expect("set ok");
        assert!(equal(&cap_b.lookup(&s("x")).unwrap(), &Value::Int(7)));
    }

    #[test]
    fn local_len_counts_current_frame_only() {
        let parent = Env::new_global();
        parent.define(s("x"), Value::Int(1));
        parent.define(s("y"), Value::Int(2));
        let child = Env::extend(parent);
        child.define(s("z"), Value::Int(3));
        assert_eq!(child.local_len(), 1);
    }
}
