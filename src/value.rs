//! Runtime value types for nscheme.
//!
//! This module defines the [`Value`] enum, which represents every kind of
//! Scheme object the interpreter can hold at runtime, along with the
//! supporting types ([`Symbol`], [`Pair`], [`Procedure`], [`Port`]) and
//! the three R7RS equality predicates (`eq?`, `eqv?`, `equal?`).
//!
//! ## Cycle handling
//!
//! Mutable compound values (pairs, vectors, strings, bytevectors, ports)
//! are wrapped in `Rc<RefCell<…>>`. This is the simplest representation
//! that supports `set-car!` / `set-cdr!` / `vector-set!` / `string-set!`,
//! but it does **not** reclaim cyclic structures — a circular list will
//! leak until process exit. This is documented as a known v1 limitation;
//! a proper garbage collector is out of scope for the epic.
//!
//! ## Procedures
//!
//! The [`Procedure`] enum currently has only the [`Procedure::Primitive`]
//! variant — closures are added when the evaluator lands (see bead
//! `nscheme-1no`). Extending the enum is backwards-compatible for
//! internal callers.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use num_bigint::BigInt;
use num_rational::BigRational;
use thiserror::Error;

use crate::env::EnvRef;

// ---------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------

/// Runtime errors raised by primitives, the evaluator, and helpers.
/// Variants will grow as later beads need finer-grained reporting.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum RuntimeError {
    #[error("type error: expected {expected}, got {got}")]
    Type { expected: String, got: String },

    #[error("arity error in `{procedure}`: expected {expected}, got {got}")]
    Arity {
        procedure: String,
        expected: String,
        got: usize,
    },

    #[error("undefined variable: {0}")]
    Undefined(String),

    #[error("not a procedure: {0}")]
    NotProcedure(String),

    #[error("division by zero")]
    DivisionByZero,

    #[error("integer overflow in `{op}`")]
    Overflow { op: &'static str },

    #[error("{0}")]
    Other(String),
}

/// Convenience alias for runtime results.
pub type Result<T> = std::result::Result<T, RuntimeError>;

// ---------------------------------------------------------------------
// Symbol interning
// ---------------------------------------------------------------------

thread_local! {
    static SYMBOLS: RefCell<HashMap<String, Rc<str>>> = RefCell::new(HashMap::new());
}

/// An interned identifier. Two `Symbol`s with the same name share storage
/// and are equal by pointer, which is what R7RS requires for `eq?` on
/// symbols (§6.5).
#[derive(Clone)]
pub struct Symbol(Rc<str>);

impl Symbol {
    /// Interning constructor: returns the canonical symbol for `name`,
    /// creating it on first use.
    pub fn intern(name: &str) -> Self {
        SYMBOLS.with(|table| {
            let mut table = table.borrow_mut();
            if let Some(existing) = table.get(name) {
                return Self(existing.clone());
            }
            let rc: Rc<str> = Rc::from(name);
            table.insert(name.to_string(), rc.clone());
            Self(rc)
        })
    }

    /// The symbol's printable name.
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl PartialEq for Symbol {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Symbol {}

impl Hash for Symbol {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash by interned pointer so HashMap lookups are O(1) on
        // identity, matching Eq. Cast through a thin pointer first
        // because `Rc<str>` is a fat pointer.
        (Rc::as_ptr(&self.0).cast::<()>() as usize).hash(state);
    }
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Symbol({:?})", self.name())
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

// ---------------------------------------------------------------------
// Pair
// ---------------------------------------------------------------------

/// A cons cell. Mutable for `set-car!` / `set-cdr!`.
#[derive(Debug)]
pub struct Pair {
    pub car: Value,
    pub cdr: Value,
}

// ---------------------------------------------------------------------
// Procedure
// ---------------------------------------------------------------------

/// Signature of a Rust-implemented primitive.
pub type PrimitiveFn = fn(&[Value]) -> Result<Value>;

/// A procedure value. Either implemented in Rust ([`Self::Primitive`])
/// or constructed from a `lambda` form ([`Self::Closure`]).
pub enum Procedure {
    Primitive {
        /// Display name (e.g. `"car"`, `"+"`).
        name: &'static str,
        /// Calling-convention description for error messages.
        arity: Arity,
        /// Rust implementation.
        body: PrimitiveFn,
    },
    Closure {
        /// Positional parameter names.
        params: Vec<Symbol>,
        /// `rest` parameter from `(lambda (a b . rest) ...)` or
        /// `(lambda args ...)`; `None` for fixed-arity closures.
        rest: Option<Symbol>,
        /// Raw body datums. The evaluator re-walks the body each call;
        /// no precompilation in v1.
        body: Vec<Value>,
        /// Defining environment, kept alive by `Rc`. Closures over the
        /// same scope share this `Rc`, so mutations to enclosed
        /// variables are visible across them (needed for `letrec` /
        /// mutual recursion).
        env: EnvRef,
        /// Optional name attached at `(define (f …) …)` for nicer
        /// `#<procedure foo>` rendering and error messages.
        name: Option<String>,
    },
}

impl Procedure {
    pub fn name(&self) -> &str {
        match self {
            Self::Primitive { name, .. } => name,
            Self::Closure { name, .. } => name.as_deref().unwrap_or("anonymous"),
        }
    }
}

impl fmt::Debug for Procedure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Primitive { name, arity, .. } => f
                .debug_struct("Primitive")
                .field("name", name)
                .field("arity", arity)
                .finish(),
            Self::Closure {
                params, rest, name, ..
            } => f
                .debug_struct("Closure")
                .field("name", name)
                .field("params", params)
                .field("rest", rest)
                .finish_non_exhaustive(),
        }
    }
}

/// Calling-convention description used by primitives for arity checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Arity {
    /// Exactly this many arguments.
    Exact(usize),
    /// At least this many arguments; no upper bound.
    AtLeast(usize),
    /// Between `min` and `max` inclusive.
    Range { min: usize, max: usize },
}

impl Arity {
    pub fn matches(&self, n: usize) -> bool {
        match self {
            Self::Exact(k) => n == *k,
            Self::AtLeast(k) => n >= *k,
            Self::Range { min, max } => n >= *min && n <= *max,
        }
    }
}

impl fmt::Display for Arity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact(k) => write!(f, "exactly {k}"),
            Self::AtLeast(k) => write!(f, "at least {k}"),
            Self::Range { min, max } => write!(f, "between {min} and {max}"),
        }
    }
}

// ---------------------------------------------------------------------
// Port
// ---------------------------------------------------------------------

/// I/O port. Each variant captures the kind of underlying source/sink
/// plus enough mutable state to track read position or write buffer.
///
/// Binary ports are deferred — every variant here is a textual port.
/// Output ports write through `print!` rather than buffering; that
/// keeps interactive use snappy at the cost of a flush per write.
#[derive(Debug)]
pub enum Port {
    /// Reads from process stdin one line at a time.
    StdIn { buffer: String, pos: usize },
    /// Writes to process stdout.
    StdOut,
    /// Writes to process stderr.
    StdErr,
    /// Reads from an in-memory string. `pos` tracks the next byte index
    /// to consume; we use byte-indexed slicing rather than char indexing
    /// because Rust strings already store UTF-8.
    StringInput { content: String, pos: usize },
    /// Accumulates writes into a buffer; `get-output-string` returns it.
    StringOutput { buffer: String },
    /// Reads the entire file into memory at open time. Big-file
    /// streaming would need a separate variant.
    FileInput {
        content: String,
        pos: usize,
        path: String,
    },
    /// Appends to a file. The buffer is flushed on close.
    FileOutput { buffer: String, path: String },
    /// A port that has been explicitly closed.
    Closed,
}

impl Port {
    pub fn is_input(&self) -> bool {
        matches!(
            self,
            Self::StdIn { .. } | Self::StringInput { .. } | Self::FileInput { .. }
        )
    }

    pub fn is_output(&self) -> bool {
        matches!(
            self,
            Self::StdOut | Self::StdErr | Self::StringOutput { .. } | Self::FileOutput { .. }
        )
    }
}

// ---------------------------------------------------------------------
// Value
// ---------------------------------------------------------------------

/// Every runtime Scheme value.
#[derive(Clone)]
pub enum Value {
    /// The empty list `()`.
    Null,
    /// `(if #f #f)` — the conventional Scheme "unspecified" value.
    Unspecified,
    /// `(eof-object)`.
    Eof,
    /// `#t` / `#f`.
    Bool(bool),
    /// A character.
    Char(char),
    /// A symbol (interned).
    Symbol(Symbol),
    /// An exact fixnum (fast path for small integers).
    Int(i64),
    /// An exact arbitrary-precision integer. Only used when a result
    /// overflows `i64`; smaller values stay as `Int` for speed.
    BigInt(Rc<BigInt>),
    /// An exact rational number with arbitrary-precision numerator and
    /// denominator. Always in lowest terms with a positive denominator
    /// (the invariant `num_rational::BigRational` maintains).
    Rational(Rc<BigRational>),
    /// An inexact real (R7RS `flonum`).
    Float(f64),
    /// A mutable string.
    String(Rc<RefCell<String>>),
    /// A pair (cons cell).
    Pair(Rc<RefCell<Pair>>),
    /// A vector.
    Vector(Rc<RefCell<Vec<Value>>>),
    /// A bytevector.
    Bytevector(Rc<RefCell<Vec<u8>>>),
    /// A procedure (primitive or, eventually, closure).
    Procedure(Rc<Procedure>),
    /// An I/O port.
    Port(Rc<RefCell<Port>>),
}

impl Value {
    // -- constructors -----------------------------------------------

    pub fn string(s: impl Into<String>) -> Self {
        Self::String(Rc::new(RefCell::new(s.into())))
    }

    pub fn cons(car: Self, cdr: Self) -> Self {
        Self::Pair(Rc::new(RefCell::new(Pair { car, cdr })))
    }

    pub fn vector(items: Vec<Value>) -> Self {
        Self::Vector(Rc::new(RefCell::new(items)))
    }

    pub fn bytevector(bytes: Vec<u8>) -> Self {
        Self::Bytevector(Rc::new(RefCell::new(bytes)))
    }

    /// Build a proper list from an iterator of values.
    pub fn list_from(items: impl IntoIterator<Item = Value>) -> Self {
        let mut items: Vec<Value> = items.into_iter().collect();
        let mut acc = Self::Null;
        while let Some(v) = items.pop() {
            acc = Self::cons(v, acc);
        }
        acc
    }

    // -- predicates -------------------------------------------------

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    pub fn is_pair(&self) -> bool {
        matches!(self, Self::Pair(_))
    }

    pub fn is_boolean(&self) -> bool {
        matches!(self, Self::Bool(_))
    }

    pub fn is_symbol(&self) -> bool {
        matches!(self, Self::Symbol(_))
    }

    pub fn is_number(&self) -> bool {
        matches!(
            self,
            Self::Int(_) | Self::BigInt(_) | Self::Rational(_) | Self::Float(_)
        )
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Self::String(_))
    }

    pub fn is_vector(&self) -> bool {
        matches!(self, Self::Vector(_))
    }

    pub fn is_procedure(&self) -> bool {
        matches!(self, Self::Procedure(_))
    }

    /// R7RS "truthy": every value except `#f` is true.
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Self::Bool(false))
    }

    /// Walk a proper list, returning `Some(length)` if every cdr is a
    /// pair or `Null`. Returns `None` for improper lists, including
    /// cyclic ones (detected with Floyd's algorithm).
    pub fn list_length(&self) -> Option<usize> {
        let mut slow = self.clone();
        let mut fast = self.clone();
        let mut len = 0usize;
        loop {
            match fast {
                Self::Null => return Some(len),
                Self::Pair(p) => {
                    let next = p.borrow().cdr.clone();
                    fast = match next {
                        Self::Pair(p2) => {
                            len += 2;
                            p2.borrow().cdr.clone()
                        }
                        Self::Null => return Some(len + 1),
                        _ => return None,
                    };
                }
                _ => return None,
            }
            // Advance slow by one — used for cycle detection.
            slow = match slow {
                Self::Pair(p) => p.borrow().cdr.clone(),
                _ => return None,
            };
            if let (Self::Pair(a), Self::Pair(b)) = (&slow, &fast)
                && Rc::ptr_eq(a, b)
            {
                return None;
            }
        }
    }

    /// Return `(car, cdr)` if this is a pair.
    pub fn as_pair(&self) -> Option<(Value, Value)> {
        if let Self::Pair(p) = self {
            let cell = p.borrow();
            Some((cell.car.clone(), cell.cdr.clone()))
        } else {
            None
        }
    }

    /// Friendly type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Unspecified => "unspecified",
            Self::Eof => "eof-object",
            Self::Bool(_) => "boolean",
            Self::Char(_) => "char",
            Self::Symbol(_) => "symbol",
            Self::Int(_) | Self::BigInt(_) | Self::Rational(_) | Self::Float(_) => "number",
            Self::String(_) => "string",
            Self::Pair(_) => "pair",
            Self::Vector(_) => "vector",
            Self::Bytevector(_) => "bytevector",
            Self::Procedure(_) => "procedure",
            Self::Port(_) => "port",
        }
    }
}

// ---------------------------------------------------------------------
// Equality
// ---------------------------------------------------------------------

/// R7RS `eq?` (§6.1). Identity equality. For atoms, equality is by
/// value (booleans, the empty list, characters, symbols, fixnums).
/// For heap-allocated values, equality is by `Rc` pointer.
pub fn eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null)
        | (Value::Unspecified, Value::Unspecified)
        | (Value::Eof, Value::Eof) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Char(x), Value::Char(y)) => x == y,
        (Value::Symbol(x), Value::Symbol(y)) => x == y,
        // R7RS §6.1: eq? on numbers is implementation-defined. We use
        // value equality on unboxed numbers (Int / Float) and pointer
        // equality on heap-allocated numbers (BigInt / Rational).
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(),
        (Value::BigInt(x), Value::BigInt(y)) => Rc::ptr_eq(x, y),
        (Value::Rational(x), Value::Rational(y)) => Rc::ptr_eq(x, y),
        (Value::String(x), Value::String(y)) => Rc::ptr_eq(x, y),
        (Value::Pair(x), Value::Pair(y)) => Rc::ptr_eq(x, y),
        (Value::Vector(x), Value::Vector(y)) => Rc::ptr_eq(x, y),
        (Value::Bytevector(x), Value::Bytevector(y)) => Rc::ptr_eq(x, y),
        (Value::Procedure(x), Value::Procedure(y)) => Rc::ptr_eq(x, y),
        (Value::Port(x), Value::Port(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}

/// R7RS `eqv?` (§6.1). Like `eq?` but with numeric equality across
/// exact representations. Numbers are `eqv?` iff they have the same
/// exactness and represent the same mathematical value, so e.g.
/// `(eqv? 1 1)` is `#t` but `(eqv? 1 1.0)` is `#f`.
pub fn eqv(a: &Value, b: &Value) -> bool {
    match (a, b) {
        // Within the exact tower — promote both to BigRational for the
        // mathematical compare (cheap because num-rational normalizes).
        (
            Value::Int(_) | Value::BigInt(_) | Value::Rational(_),
            Value::Int(_) | Value::BigInt(_) | Value::Rational(_),
        ) => exact_to_rational(a) == exact_to_rational(b),
        (Value::Float(x), Value::Float(y)) => {
            // R7RS §6.1: NaN is eqv? to itself.
            x.to_bits() == y.to_bits()
        }
        // Cross-exactness comparisons are always #f.
        _ => eq(a, b),
    }
}

/// Promote any of `Int` / `BigInt` / `Rational` to a `BigRational`.
fn exact_to_rational(v: &Value) -> BigRational {
    use num_traits::FromPrimitive;
    match v {
        Value::Int(n) => BigRational::from_i64(*n).expect("i64 in BigRational"),
        Value::BigInt(b) => BigRational::from_integer((**b).clone()),
        Value::Rational(r) => (**r).clone(),
        _ => unreachable!("exact_to_rational called on inexact"),
    }
}

/// R7RS `equal?` (§6.1). Structural equality: recurses through pairs,
/// vectors, bytevectors, and strings. R7RS requires `equal?` to
/// terminate on cyclic inputs; for v1 we use a visited set keyed by
/// `Rc` pointer identity.
pub fn equal(a: &Value, b: &Value) -> bool {
    use std::collections::HashSet;
    let mut visited: HashSet<(*const (), *const ())> = HashSet::new();
    equal_inner(a, b, &mut visited)
}

fn equal_inner(
    a: &Value,
    b: &Value,
    visited: &mut std::collections::HashSet<(*const (), *const ())>,
) -> bool {
    match (a, b) {
        (Value::String(x), Value::String(y)) => *x.borrow() == *y.borrow(),
        (Value::Bytevector(x), Value::Bytevector(y)) => *x.borrow() == *y.borrow(),
        (Value::Pair(x), Value::Pair(y)) => {
            let key = (Rc::as_ptr(x).cast::<()>(), Rc::as_ptr(y).cast::<()>());
            if !visited.insert(key) {
                // Already comparing this pair of pairs — break the cycle.
                return true;
            }
            let xb = x.borrow();
            let yb = y.borrow();
            equal_inner(&xb.car, &yb.car, visited) && equal_inner(&xb.cdr, &yb.cdr, visited)
        }
        (Value::Vector(x), Value::Vector(y)) => {
            let key = (Rc::as_ptr(x).cast::<()>(), Rc::as_ptr(y).cast::<()>());
            if !visited.insert(key) {
                return true;
            }
            let xb = x.borrow();
            let yb = y.borrow();
            xb.len() == yb.len()
                && xb
                    .iter()
                    .zip(yb.iter())
                    .all(|(a, b)| equal_inner(a, b, visited))
        }
        _ => eqv(a, b),
    }
}

// ---------------------------------------------------------------------
// Display / Debug
// ---------------------------------------------------------------------

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Debug uses the same `write` form as Display by default —
        // useful in test failure messages.
        write_value(self, f, /*display=*/ false)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display uses R7RS `display` semantics (strings without quotes,
        // characters without `#\`).
        write_value(self, f, /*display=*/ true)
    }
}

/// Render a value. When `display` is false this is R7RS `write`;
/// when true it is R7RS `display`.
fn write_value(v: &Value, f: &mut fmt::Formatter<'_>, display: bool) -> fmt::Result {
    match v {
        Value::Null => f.write_str("()"),
        Value::Unspecified => f.write_str("#<unspecified>"),
        Value::Eof => f.write_str("#<eof>"),
        Value::Bool(true) => f.write_str("#t"),
        Value::Bool(false) => f.write_str("#f"),
        Value::Char(c) => {
            if display {
                f.write_str(&c.to_string())
            } else {
                write_char_literal(*c, f)
            }
        }
        Value::Symbol(s) => f.write_str(s.name()),
        Value::Int(n) => write!(f, "{n}"),
        Value::BigInt(b) => write!(f, "{b}"),
        Value::Rational(r) => {
            // num-rational's Display already produces `n/d`.
            write!(f, "{r}")
        }
        Value::Float(x) => write_float(*x, f),
        Value::String(s) => {
            if display {
                f.write_str(&s.borrow())
            } else {
                write_string_literal(&s.borrow(), f)
            }
        }
        Value::Pair(_) => write_list(v, f, display),
        Value::Vector(items) => {
            f.write_str("#(")?;
            let items = items.borrow();
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    f.write_str(" ")?;
                }
                write_value(item, f, display)?;
            }
            f.write_str(")")
        }
        Value::Bytevector(bytes) => {
            f.write_str("#u8(")?;
            let bytes = bytes.borrow();
            for (i, b) in bytes.iter().enumerate() {
                if i > 0 {
                    f.write_str(" ")?;
                }
                write!(f, "{b}")?;
            }
            f.write_str(")")
        }
        Value::Procedure(p) => write!(f, "#<procedure {}>", p.name()),
        Value::Port(_) => f.write_str("#<port>"),
    }
}

fn write_char_literal(c: char, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match c {
        '\u{0007}' => f.write_str("#\\alarm"),
        '\u{0008}' => f.write_str("#\\backspace"),
        '\u{007F}' => f.write_str("#\\delete"),
        '\u{001B}' => f.write_str("#\\escape"),
        '\n' => f.write_str("#\\newline"),
        '\0' => f.write_str("#\\null"),
        '\r' => f.write_str("#\\return"),
        ' ' => f.write_str("#\\space"),
        '\t' => f.write_str("#\\tab"),
        c if c.is_control() => write!(f, "#\\x{:X}", c as u32),
        c => write!(f, "#\\{c}"),
    }
}

fn write_string_literal(s: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("\"")?;
    for c in s.chars() {
        match c {
            '"' => f.write_str("\\\"")?,
            '\\' => f.write_str("\\\\")?,
            '\n' => f.write_str("\\n")?,
            '\r' => f.write_str("\\r")?,
            '\t' => f.write_str("\\t")?,
            c if c.is_control() => write!(f, "\\x{:X};", c as u32)?,
            c => f.write_str(&c.to_string())?,
        }
    }
    f.write_str("\"")
}

fn write_float(x: f64, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if x.is_nan() {
        return f.write_str("+nan.0");
    }
    if x.is_infinite() {
        return f.write_str(if x > 0.0 { "+inf.0" } else { "-inf.0" });
    }
    // R7RS requires a decimal point or exponent so that the reader
    // round-trips it as inexact.
    let s = format!("{x}");
    if s.contains('.') || s.contains('e') || s.contains('E') {
        f.write_str(&s)
    } else {
        write!(f, "{s}.0")
    }
}

fn write_list(v: &Value, f: &mut fmt::Formatter<'_>, display: bool) -> fmt::Result {
    f.write_str("(")?;
    let mut cur = v.clone();
    let mut first = true;
    loop {
        match cur {
            Value::Pair(p) => {
                if !first {
                    f.write_str(" ")?;
                }
                first = false;
                let cell = p.borrow();
                write_value(&cell.car, f, display)?;
                cur = cell.cdr.clone();
            }
            Value::Null => break,
            other => {
                // Improper list — print the tail after a dot.
                f.write_str(" . ")?;
                write_value(&other, f, display)?;
                break;
            }
        }
    }
    f.write_str(")")
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(name: &str) -> Value {
        Value::Symbol(Symbol::intern(name))
    }

    // -- symbol interning --------------------------------------------

    #[test]
    fn symbols_intern_to_same_storage() {
        let a = Symbol::intern("foo");
        let b = Symbol::intern("foo");
        assert!(a == b);
        // Pointer-identical:
        assert!(std::ptr::eq(a.name(), b.name()));
    }

    #[test]
    fn distinct_names_differ() {
        assert!(Symbol::intern("foo") != Symbol::intern("bar"));
    }

    // -- list construction and length -------------------------------

    #[test]
    fn list_from_iter() {
        let list = Value::list_from([sym("a"), sym("b"), sym("c")]);
        assert_eq!(list.list_length(), Some(3));
        // (a b c) prints correctly.
        assert_eq!(format!("{list}"), "(a b c)");
    }

    #[test]
    fn empty_list_has_length_zero() {
        assert_eq!(Value::Null.list_length(), Some(0));
    }

    #[test]
    fn improper_list_returns_none() {
        let pair = Value::cons(sym("a"), sym("b")); // (a . b)
        assert_eq!(pair.list_length(), None);
    }

    #[test]
    fn cyclic_list_returns_none() {
        // Build (a . <self>): a one-element cycle.
        let cell = Rc::new(RefCell::new(Pair {
            car: sym("a"),
            cdr: Value::Null,
        }));
        let val = Value::Pair(cell.clone());
        cell.borrow_mut().cdr = val.clone();
        assert_eq!(val.list_length(), None);
    }

    // -- equality predicates ----------------------------------------

    #[test]
    fn eq_on_atoms() {
        assert!(eq(&Value::Null, &Value::Null));
        assert!(eq(&Value::Bool(true), &Value::Bool(true)));
        assert!(!eq(&Value::Bool(true), &Value::Bool(false)));
        assert!(eq(&Value::Char('a'), &Value::Char('a')));
        assert!(eq(&sym("x"), &sym("x")));
        assert!(eq(&Value::Int(42), &Value::Int(42)));
    }

    #[test]
    fn eq_on_heap_uses_pointer_equality() {
        let s1 = Value::string("hello");
        let s2 = Value::string("hello"); // distinct allocation
        assert!(!eq(&s1, &s2));
        assert!(eq(&s1, &s1.clone()));
    }

    #[test]
    fn eqv_on_floats_distinguishes_negative_zero() {
        assert!(!eqv(&Value::Float(0.0), &Value::Float(-0.0)));
        assert!(eqv(&Value::Float(1.5), &Value::Float(1.5)));
    }

    #[test]
    fn equal_is_structural() {
        let a = Value::list_from([Value::Int(1), Value::Int(2), Value::Int(3)]);
        let b = Value::list_from([Value::Int(1), Value::Int(2), Value::Int(3)]);
        // eq? false (different allocations), equal? true.
        assert!(!eq(&a, &b));
        assert!(equal(&a, &b));
    }

    #[test]
    fn equal_on_strings() {
        let a = Value::string("hi");
        let b = Value::string("hi");
        assert!(!eq(&a, &b));
        assert!(equal(&a, &b));
    }

    #[test]
    fn equal_on_vectors() {
        let a = Value::vector(vec![Value::Int(1), Value::Int(2)]);
        let b = Value::vector(vec![Value::Int(1), Value::Int(2)]);
        assert!(equal(&a, &b));
        let c = Value::vector(vec![Value::Int(1), Value::Int(3)]);
        assert!(!equal(&a, &c));
    }

    #[test]
    fn equal_terminates_on_cycles() {
        let cell_a = Rc::new(RefCell::new(Pair {
            car: Value::Int(1),
            cdr: Value::Null,
        }));
        let cell_b = Rc::new(RefCell::new(Pair {
            car: Value::Int(1),
            cdr: Value::Null,
        }));
        let va = Value::Pair(cell_a.clone());
        let vb = Value::Pair(cell_b.clone());
        cell_a.borrow_mut().cdr = va.clone();
        cell_b.borrow_mut().cdr = vb.clone();
        assert!(equal(&va, &vb));
    }

    // -- display formatting -----------------------------------------

    #[test]
    fn display_nested_list() {
        let inner = Value::list_from([Value::Int(2), Value::Int(3)]);
        let outer = Value::list_from([Value::Int(1), inner, Value::Int(4)]);
        assert_eq!(format!("{outer}"), "(1 (2 3) 4)");
    }

    #[test]
    fn display_improper_list() {
        let pair = Value::cons(sym("a"), sym("b"));
        assert_eq!(format!("{pair}"), "(a . b)");
    }

    #[test]
    fn display_vector_and_bytevector() {
        assert_eq!(
            format!("{}", Value::vector(vec![Value::Int(1), Value::Int(2)])),
            "#(1 2)",
        );
        assert_eq!(format!("{}", Value::bytevector(vec![1, 255])), "#u8(1 255)");
    }

    #[test]
    fn display_string_write_vs_display() {
        let s = Value::string(r#"a "b" c"#);
        // {:?} (Debug) uses write semantics with escapes.
        assert_eq!(format!("{s:?}"), r#""a \"b\" c""#);
        // {} (Display) is bare.
        assert_eq!(format!("{s}"), r#"a "b" c"#);
    }

    #[test]
    fn display_floats() {
        assert_eq!(format!("{}", Value::Float(1.0)), "1.0");
        assert_eq!(format!("{}", Value::Float(0.5)), "0.5");
        assert_eq!(format!("{}", Value::Float(f64::INFINITY)), "+inf.0");
        assert_eq!(format!("{}", Value::Float(f64::NEG_INFINITY)), "-inf.0");
        assert_eq!(format!("{}", Value::Float(f64::NAN)), "+nan.0");
    }

    #[test]
    fn display_characters() {
        // {:?} (Debug = write) shows the #\ form.
        assert_eq!(format!("{:?}", Value::Char('a')), "#\\a");
        assert_eq!(format!("{:?}", Value::Char(' ')), "#\\space");
        assert_eq!(format!("{:?}", Value::Char('\n')), "#\\newline");
        // {} (Display) shows the bare character.
        assert_eq!(format!("{}", Value::Char('a')), "a");
    }

    // -- predicates --------------------------------------------------

    #[test]
    fn truthy() {
        assert!(Value::Bool(true).is_truthy());
        assert!(!Value::Bool(false).is_truthy());
        // R7RS: everything except #f is truthy, including ().
        assert!(Value::Null.is_truthy());
        assert!(Value::Int(0).is_truthy());
        assert!(Value::string("").is_truthy());
    }

    #[test]
    fn type_name_matches_value() {
        assert_eq!(Value::Int(1).type_name(), "number");
        assert_eq!(Value::Bool(true).type_name(), "boolean");
        assert_eq!(Value::Null.type_name(), "null");
        assert_eq!(sym("x").type_name(), "symbol");
    }

    // -- procedures --------------------------------------------------

    #[test]
    fn primitive_procedure_display() {
        // PrimitiveFn must return Result; the body's success/error split
        // isn't relevant to this display test.
        fn dummy(args: &[Value]) -> Result<Value> {
            if args.is_empty() {
                Ok(Value::Unspecified)
            } else {
                Err(RuntimeError::Other("dummy".into()))
            }
        }
        let p = Procedure::Primitive {
            name: "test-prim",
            arity: Arity::Exact(0),
            body: dummy,
        };
        let v = Value::Procedure(Rc::new(p));
        assert_eq!(format!("{v}"), "#<procedure test-prim>");
    }

    #[test]
    fn arity_matches() {
        assert!(Arity::Exact(2).matches(2));
        assert!(!Arity::Exact(2).matches(3));
        assert!(Arity::AtLeast(1).matches(5));
        assert!(!Arity::AtLeast(1).matches(0));
        assert!(Arity::Range { min: 1, max: 3 }.matches(2));
        assert!(!Arity::Range { min: 1, max: 3 }.matches(4));
    }
}
