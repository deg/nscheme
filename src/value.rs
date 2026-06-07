//! Runtime value types for nscheme.
//!
//! This is the central data-definition file of the interpreter: it says
//! what a runtime Scheme value *is*. Almost every other module either
//! produces these values (the lexer / parser), consumes them (the
//! evaluator and primitives), or both.
//!
//! ## What you'll learn here
//!
//! - **Scheme**: what kinds of values R7RS distinguishes — null, the
//!   unspecified value, booleans, characters, symbols, the four-rung
//!   numeric tower (fixnum / bignum / rational / float) plus complex,
//!   pairs, vectors, bytevectors, strings, ports, procedures, records,
//!   promises, multiple-value packets, error objects.
//! - **The three equality predicates** (`eq?`, `eqv?`, `equal?`) and
//!   why each gives a different answer for "the same" inputs.
//! - **Symbol interning** and why R7RS demands it (so that `eq?` on
//!   symbols is just a pointer comparison).
//! - **Rust**: the `Rc<RefCell<T>>` pattern for shared mutable state,
//!   which is how Scheme's `set-car!`-style mutation lives inside
//!   Rust's ownership model.
//! - **`thiserror`** for ergonomic error enums.
//! - The discipline a closed enum gives you: every new kind of value
//!   has to be added here, and `rustc`'s exhaustiveness checker then
//!   makes you handle it at every match site in the codebase.
//!
//! ## Read alongside
//!
//! - `docs/0002-numeric-tower.md` — why the four-rung numeric tower.
//! - `docs/0004-continuations.md` — why [`Procedure::Continuation`] is
//!   a captured frame stack.
//! - `docs/0005-exception-handling.md` — how [`ErrorObject`] flows
//!   through `with-exception-handler` / `guard`.
//! - R7RS §6.1 (equivalence predicates), §6.4 (pairs and lists), §6.2
//!   (numbers).
//!
//! ## A note on cycles and garbage collection
//!
//! Mutable compound values (pairs, vectors, strings, bytevectors, ports)
//! are wrapped in `Rc<RefCell<…>>`. This is the simplest representation
//! that supports `set-car!` / `set-cdr!` / `vector-set!` / `string-set!`,
//! but it does **not** reclaim cyclic structures — a circular list will
//! leak until process exit. A proper garbage collector is documented as
//! a v1 limitation (bead `nscheme-3gv`). The `equal?` and `write` paths
//! cope with cycles by tracking visited `Rc` pointers in a set; see
//! `equal_inner` and `count_refs` below.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
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
//
// Rust note (`thiserror`): each variant carries an `#[error("…")]`
// attribute that the `thiserror` macro turns into a `Display` impl.
// Variants with placeholders like `{0}` or `{expected}` reference the
// payload by field name or tuple index. This gives us human-readable
// error formatting without a hand-written `match` in `Display`, while
// `#[derive(Error)]` itself only adds the `std::error::Error` trait
// (which `?` propagation needs).
//
// Scheme note: most R7RS implementations conflate "raised by Scheme
// code with `(raise …)`" and "the host language hit a problem" into
// the same `ErrorObject` type. We keep them as two separate Rust
// types — `RuntimeError` (this enum, host-side) and `ErrorObject`
// (Scheme-side, defined below) — and have a small bridge function
// in eval.rs (`runtime_error_to_value`) that converts the host
// kind into the Scheme kind. See ADR 0005.

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

    /// Tagged file I/O error so `file-error?` predicates can catch it.
    #[error("file error: {0}")]
    FileError(String),

    /// Tagged read/parser error so `read-error?` predicates can catch it.
    #[error("read error: {0}")]
    ReadError(String),

    #[error("{0}")]
    Other(String),
}

/// Convenience alias for runtime results.
pub type Result<T> = std::result::Result<T, RuntimeError>;

// ---------------------------------------------------------------------
// Symbol interning
// ---------------------------------------------------------------------
//
// Scheme note: in Scheme, `(eq? 'foo 'foo)` must be `#t`. The two
// occurrences of `foo` were written separately by the programmer but
// they refer to the same *identifier*. This is the defining property
// of a symbol versus a string: symbols are compared by identity, not
// content. The traditional implementation technique — used here — is
// *interning*: the first time anyone asks for the symbol `foo`, we
// allocate one piece of storage holding the name; every subsequent
// request for `foo` returns a handle to that same storage.
//
// Rust note: the interning table lives in a `thread_local!` because
// `Rc<str>` (and `Symbol` itself) is not thread-safe. `nscheme` is a
// single-threaded interpreter; if we ever made it multi-threaded we
// would switch to `Arc<str>` and a `OnceLock<Mutex<HashMap<…>>>`.
//
// We store `Rc<str>` rather than `Rc<String>` because `str` is the
// unsized string slice — exactly what we want for read-only names.
// An `Rc<str>` is two machine words (data pointer + length) and never
// re-allocates; an `Rc<String>` would be one machine word but every
// access goes through a level of indirection and the `String`'s
// capacity field is wasted.

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
//
// Scheme note: a *pair* (also called a *cons cell*) is the
// two-element building block of Scheme's lists. `(car p)` returns
// the first element, `(cdr p)` returns the rest. A proper list `(a
// b c)` is shorthand for `(cons a (cons b (cons c '())))` — a chain
// of pairs ending in the empty list.
//
// Rust note (the `Rc<RefCell<T>>` pattern, appearing for the first
// time below): R7RS pairs are *mutable* — `(set-car! p v)` replaces
// the first slot. But the same pair can be referenced from many
// places (lists are shared all the time in Scheme), so it can't live
// behind a unique mutable reference. We need:
//
//   - shared ownership: many `Value`s can refer to the same pair
//     and the storage stays alive until all of them go away. That's
//     `Rc<T>` (reference-counted).
//   - mutability through a shared reference: `&Rc<T>` only gives
//     immutable access. To get mutation we wrap the inner type in
//     `RefCell<T>`, which moves the borrow check from compile time
//     to runtime — `.borrow_mut()` panics if anyone else is holding
//     a `.borrow()` simultaneously.
//
// `Rc<RefCell<Pair>>` is what those two together look like:
// "shareable, reassignable cell." You'll see this pattern repeated
// for `String`, `Vector`, `Bytevector`, and `Port` — every Scheme
// value that is both shareable and mutable.

/// A cons cell. Mutable for `set-car!` / `set-cdr!`.
#[derive(Debug)]
pub struct Pair {
    pub car: Value,
    pub cdr: Value,
}

/// Components of a Cartesian complex number. Each field carries
/// its own exactness as the underlying `Value` form.
///
/// Scheme note: R7RS allows a complex value's real and imaginary
/// parts to have *independent* exactness. `1+2i` is exact-complex
/// (both parts are exact integers); `1.0+2i` is mixed (inexact real,
/// exact imaginary); `1.0+2.0i` is fully inexact. Each form should
/// round-trip through `(string->number (number->string z))`. We
/// preserve that distinction by storing each component as a full
/// `Value` and letting the writer inspect their variants when
/// rendering the lexeme.
#[derive(Debug, Clone)]
pub struct ComplexValue {
    pub re: Value,
    pub im: Value,
}

impl ComplexValue {
    /// Promote the real part to `f64`.
    pub fn re_f64(&self) -> f64 {
        Self::component_f64(&self.re)
    }
    /// Promote the imaginary part to `f64`.
    pub fn im_f64(&self) -> f64 {
        Self::component_f64(&self.im)
    }
    fn component_f64(v: &Value) -> f64 {
        match v {
            #[allow(clippy::cast_precision_loss)]
            Value::Int(n) => *n as f64,
            Value::BigInt(b) => {
                use num_traits::ToPrimitive;
                b.to_f64().unwrap_or(f64::NAN)
            }
            Value::Rational(r) => {
                use num_traits::ToPrimitive;
                r.to_f64().unwrap_or(f64::NAN)
            }
            Value::Float(f) => *f,
            _ => f64::NAN,
        }
    }
}

// ---------------------------------------------------------------------
// Procedure
// ---------------------------------------------------------------------
//
// This is the closed set of all *callable* things in nscheme. The
// evaluator's `step_apply` function dispatches on this enum: each
// variant has its own apply rule. The variants are not all
// user-visible — some (e.g. `DynamicWindStart`) are internal control
// procedures used by the evaluator to implement R7RS semantics.
//
// Reading note: skip ahead to `Value::Procedure` and `step_apply` in
// `eval.rs` if you want to see how a call site dispatches on this
// enum.

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
    /// A reified continuation, produced by `call/cc`. Invoking it
    /// replaces the evaluator's frame stack with `frames` and resumes
    /// by returning the supplied value to whatever was about to happen
    /// when the continuation was captured.
    Continuation { frames: Vec<crate::eval::Frame> },
    /// An R7RS parameter object (§4.2.6, §6.10). Calling it with no
    /// args returns the current value; calling with one arg sets it.
    /// `parameterize` save/restores via the cell.
    Parameter { cell: Rc<ParameterCell> },
    /// R7RS §4.2.9 `case-lambda`: a procedure with multiple
    /// arity-dispatched clauses. At apply time we walk `clauses` and
    /// pick the first one whose arity matches.
    CaseLambda {
        clauses: Vec<CaseLambdaClause>,
        env: EnvRef,
        name: Option<String>,
    },
    /// R7RS §5.5 record constructor: applies its args to the given
    /// field indices in declaration order; other fields default to
    /// `Value::Unspecified`.
    RecordConstructor {
        type_id: Rc<RecordTypeId>,
        field_count: usize,
        ctor_field_indices: Vec<usize>,
    },
    /// R7RS §5.5 record predicate: `#t` iff the argument is a Record
    /// whose `type_id` is this one.
    RecordPredicate { type_id: Rc<RecordTypeId> },
    /// R7RS §5.5 record accessor: returns the given field of a record
    /// of the matching type.
    RecordAccessor {
        type_id: Rc<RecordTypeId>,
        field_index: usize,
    },
    /// R7RS §5.5 record mutator: sets the given field of a record of
    /// the matching type.
    RecordMutator {
        type_id: Rc<RecordTypeId>,
        field_index: usize,
    },
    /// Internal control procedure that drives `dynamic-wind`. When
    /// applied with `(before thunk after)`, sets up the wind-aware
    /// extent and runs the three thunks in the right order. Not
    /// exposed as a bindable procedure; see
    /// `step_dynamic_wind` in the evaluator.
    DynamicWindStart,
    /// First-class `apply` (R7RS §6.10). `step_apply` splices the final
    /// argument list onto the leading args and re-dispatches, preserving
    /// the tail call. A procedure (not a special form) so it can be
    /// passed as a value — `(map (lambda (p) (apply + p)) …)`.
    Apply,
    /// First-class `eval` (R7RS §6.12). Evaluates its first argument (a
    /// datum) in the captured environment. The optional env-specifier
    /// argument is accepted but, since nscheme does not reify
    /// environments, treated as the interaction environment.
    Eval { env: EnvRef },
    /// First-class `load` (R7RS §6.12). Reads the file named by its
    /// string argument and evaluates its forms in the captured
    /// environment.
    Load { env: EnvRef },
}

/// One arity-arm of a `case-lambda` procedure.
#[derive(Clone, Debug)]
pub struct CaseLambdaClause {
    pub params: Vec<Symbol>,
    pub rest: Option<Symbol>,
    pub body: Vec<Value>,
}

/// The mutable interior of a [`Procedure::Parameter`].
#[derive(Debug)]
pub struct ParameterCell {
    pub value: RefCell<Value>,
}

impl Procedure {
    pub fn name(&self) -> &str {
        match self {
            Self::Primitive { name, .. } => name,
            Self::Closure { name, .. } => name.as_deref().unwrap_or("anonymous"),
            Self::Continuation { .. } => "continuation",
            Self::Parameter { .. } => "parameter",
            Self::CaseLambda { name, .. } => name.as_deref().unwrap_or("case-lambda"),
            Self::RecordConstructor { type_id, .. }
            | Self::RecordPredicate { type_id, .. }
            | Self::RecordAccessor { type_id, .. }
            | Self::RecordMutator { type_id, .. } => &type_id.name,
            Self::DynamicWindStart => "%dynamic-wind-apply",
            Self::Apply => "apply",
            Self::Eval { .. } => "eval",
            Self::Load { .. } => "load",
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
            Self::Continuation { frames } => f
                .debug_struct("Continuation")
                .field("frame_count", &frames.len())
                .finish_non_exhaustive(),
            Self::Parameter { .. } => f.debug_struct("Parameter").finish_non_exhaustive(),
            Self::CaseLambda { clauses, name, .. } => f
                .debug_struct("CaseLambda")
                .field("name", name)
                .field("clauses", &clauses.len())
                .finish_non_exhaustive(),
            Self::RecordConstructor { type_id, .. } => f
                .debug_struct("RecordConstructor")
                .field("type", &type_id.name)
                .finish_non_exhaustive(),
            Self::RecordPredicate { type_id } => f
                .debug_struct("RecordPredicate")
                .field("type", &type_id.name)
                .finish(),
            Self::RecordAccessor {
                type_id,
                field_index,
            } => f
                .debug_struct("RecordAccessor")
                .field("type", &type_id.name)
                .field("field", field_index)
                .finish(),
            Self::RecordMutator {
                type_id,
                field_index,
            } => f
                .debug_struct("RecordMutator")
                .field("type", &type_id.name)
                .field("field", field_index)
                .finish(),
            Self::DynamicWindStart => f.write_str("%dynamic-wind-apply"),
            Self::Apply => f.write_str("apply"),
            Self::Eval { .. } => f.write_str("eval"),
            Self::Load { .. } => f.write_str("load"),
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
    /// Reads from an in-memory string but flagged as a binary port
    /// (R7RS `open-input-bytevector`). All textual operations decode
    /// the bytes as UTF-8.
    BinaryInput { content: String, pos: usize },
    /// Like `StringOutput` but flagged as a binary port.
    BinaryOutput { buffer: String },
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
            Self::StdIn { .. }
                | Self::StringInput { .. }
                | Self::BinaryInput { .. }
                | Self::FileInput { .. }
        )
    }

    pub fn is_output(&self) -> bool {
        matches!(
            self,
            Self::StdOut
                | Self::StdErr
                | Self::StringOutput { .. }
                | Self::BinaryOutput { .. }
                | Self::FileOutput { .. }
        )
    }

    /// R7RS `binary-port?` — true iff this port is one of the
    /// bytevector-backed variants.
    pub fn is_binary(&self) -> bool {
        matches!(self, Self::BinaryInput { .. } | Self::BinaryOutput { .. })
    }
}

// ---------------------------------------------------------------------
// Value
// ---------------------------------------------------------------------
//
// The central enum of the interpreter. Every runtime Scheme value is
// one of these variants. Three things to notice:
//
// 1. **Atoms vs. heap values**: Atoms (`Null`, `Unspecified`, `Eof`,
//    `Bool`, `Char`, `Symbol`, `Int`, `Float`) are copied by value
//    when the `Value` is cloned. Heap values (`Pair`, `Vector`,
//    `String`, `Procedure`, `Port`, …) live behind an `Rc`, so
//    cloning a `Value` just bumps a refcount.
//
// 2. **The closed-enum discipline**: Adding a new kind of Scheme
//    value means adding a variant here. `rustc` will then refuse to
//    compile every `match` site that hasn't handled it. This is
//    deliberate — it's the easiest way to keep the interpreter
//    correct as the language grows. Whenever you add a variant,
//    expect to update: `is_number` / `is_string` / etc, `type_name`,
//    the `eq` / `eqv` / `equal` machinery, the `write_value_body`
//    dispatch, and any primitive that needs to know about the new
//    kind.
//
// 3. **Why `Clone` is cheap**: every heap-allocated payload is
//    behind `Rc`, so `Value::clone()` is a small handful of
//    refcount bumps in the worst case. The evaluator clones values
//    liberally; this is what makes that affordable.

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
    /// A macro-introduced identifier reference (R7RS hygiene
    /// §4.3.2): like `Symbol(name)` but carries the definition-site
    /// environment so the evaluator can resolve it there, plus a
    /// `scope` that is unique to the macro *expansion* that introduced
    /// it. The scope makes a template-introduced binding identifier
    /// distinct across separate (including recursive) expansions of the
    /// same macro, so two expansions that both introduce `tmp` don't
    /// collide. Produced only by the `syntax-rules` expander; user code
    /// never constructs one directly.
    SyntaxRef {
        name: Symbol,
        scope: u64,
        env: EnvRef,
    },
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
    /// A Cartesian complex number (R7RS §6.2). Each component is
    /// itself any of the real-numeric slots (`Int`, `BigInt`,
    /// `Rational`, `Float`), preserving exactness per part — so
    /// `1+2i` keeps both components exact while `1.0+2i` keeps the
    /// real inexact and the imag exact. The reader and
    /// `make-rectangular` decide what to store; arithmetic typically
    /// promotes to inexact floats.
    Complex(Rc<ComplexValue>),
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
    /// A `syntax-rules` macro transformer. When `step_eval` encounters
    /// `(head args...)` where `head` resolves to a `Macro`, it expands
    /// the call by matching `args...` against the patterns and emits
    /// `Step::Eval(expanded, env)` — no frame pushed, expansion is
    /// purely syntactic. See `crate::macros::SyntaxRules`.
    Macro(Rc<crate::macros::SyntaxRules>),
    /// An R7RS error object (`error`, `error-object?`, etc.).
    ErrorObject(Rc<ErrorObject>),
    /// A delayed computation, R7RS §4.2.5. Until forced, the promise
    /// holds an unevaluated expression and the env it was created in.
    /// `force` runs the expression once and caches the result; later
    /// `force` calls return the cached value.
    Promise(Rc<RefCell<PromiseState>>),
    /// A multiple-value packet, R7RS §6.10. Produced by `(values v
    /// ...)` with non-1 arity; consumed by `call-with-values` and the
    /// `let-values` family. R7RS's `(values v)` collapses to just `v`,
    /// so a singleton packet is never observed.
    Values(Rc<Vec<Value>>),
    /// An R7RS record (§5.5). The `type_id` is shared between the
    /// record value and the predicate/accessor/mutator procedures
    /// produced by `define-record-type`. Each field is in its own
    /// `RefCell` so individual mutators can update independently.
    Record {
        type_id: Rc<RecordTypeId>,
        fields: Rc<Vec<RefCell<Value>>>,
    },
}

/// Identity marker for a record type. Two records share a type iff
/// their `type_id` `Rc`s are `Rc::ptr_eq`.
#[derive(Debug)]
pub struct RecordTypeId {
    pub name: String,
}

/// State of a [`Value::Promise`]. Pending promises remember their
/// suspended expression and defining env; once forced they collapse
/// to the resulting value.
#[derive(Debug)]
pub enum PromiseState {
    Pending { expr: Value, env: EnvRef },
    Forced(Value),
}

/// Payload behind [`Value::ErrorObject`].
#[derive(Debug)]
pub struct ErrorObject {
    pub message: String,
    pub irritants: Vec<Value>,
    pub kind: ErrorKind,
}

/// Tag used by R7RS predicates like `read-error?` and `file-error?`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    User,
    Read,
    File,
}

impl Value {
    // -- constructors -----------------------------------------------

    pub fn string(s: impl Into<String>) -> Self {
        Self::String(Rc::new(RefCell::new(s.into())))
    }

    /// Construct an inexact complex value from two `f64`s — both
    /// parts become `Value::Float`. Use `Value::Complex(Rc::new(
    /// ComplexValue { re, im }))` directly when component exactness
    /// matters (parser, `make-rectangular`).
    pub fn complex_inexact(re: f64, im: f64) -> Self {
        Self::Complex(Rc::new(ComplexValue {
            re: Self::Float(re),
            im: Self::Float(im),
        }))
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
            Self::Int(_) | Self::BigInt(_) | Self::Rational(_) | Self::Float(_) | Self::Complex(_)
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

    /// Treat this value as an identifier and return its underlying
    /// symbol. Accepts both plain `Symbol` and macro-introduced
    /// `SyntaxRef` — the wrapping env carries hygiene info but the
    /// name itself is the same.
    pub fn as_identifier(&self) -> Option<&Symbol> {
        match self {
            Self::Symbol(s) => Some(s),
            Self::SyntaxRef { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Like `as_identifier`, but also returns the env where the
    /// identifier originated when it's a `SyntaxRef`. Use this in
    /// variable-reference and `set!` positions where hygiene
    /// dictates that the binding lives in the macro's def-site env.
    pub fn as_identifier_ref(&self) -> Option<(&Symbol, u64, Option<&EnvRef>)> {
        match self {
            Self::Symbol(s) => Some((s, 0, None)),
            Self::SyntaxRef { name, scope, env } => Some((name, *scope, Some(env))),
            _ => None,
        }
    }

    /// True iff this value is an identifier (`Symbol` or `SyntaxRef`)
    /// whose underlying name is `keyword`. Used to recognise
    /// syntactic-position keywords (`else`, `=>`, `syntax-rules`, …)
    /// regardless of macro-introduction wrapping.
    pub fn is_keyword(&self, keyword: &str) -> bool {
        self.as_identifier().is_some_and(|s| s.name() == keyword)
    }

    /// Friendly type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Unspecified => "unspecified",
            Self::Eof => "eof-object",
            Self::Bool(_) => "boolean",
            Self::Char(_) => "char",
            Self::Symbol(_) | Self::SyntaxRef { .. } => "symbol",
            Self::Int(_)
            | Self::BigInt(_)
            | Self::Rational(_)
            | Self::Float(_)
            | Self::Complex(_) => "number",
            Self::String(_) => "string",
            Self::Pair(_) => "pair",
            Self::Vector(_) => "vector",
            Self::Bytevector(_) => "bytevector",
            Self::Procedure(_) => "procedure",
            Self::Port(_) => "port",
            Self::Macro(_) => "macro",
            Self::ErrorObject(_) => "error-object",
            Self::Promise(_) => "promise",
            Self::Values(_) => "values-packet",
            Self::Record { .. } => "record",
        }
    }
}

// ---------------------------------------------------------------------
// Equality
// ---------------------------------------------------------------------
//
// Scheme has three equality predicates of increasing "depth":
//
//   `eq?`     — identity. Cheap. Two heap-allocated values are
//               `eq?` only if they share an `Rc` pointer; atoms
//               compare by value. Two distinct allocations of
//               the same string are NOT `eq?`.
//
//   `eqv?`    — like `eq?`, plus a special rule for numbers:
//               `(eqv? 1 1.0)` is `#f` (different exactness),
//               but `(eqv? 1 1)` and `(eqv? 1.0 1.0)` are `#t`.
//               NaN is `eqv?` to itself (a bit-for-bit compare).
//
//   `equal?`  — structural recursion. Two pairs are `equal?` if
//               their cars and cdrs are `equal?`. Two strings are
//               `equal?` if their character contents match. R7RS
//               requires `equal?` to terminate on cyclic input,
//               which we handle with a visited set keyed on
//               `Rc` pointers.
//
// The three predicates are layered: `eqv?` falls back to `eq?` for
// non-numeric cases, and `equal?` falls back to `eqv?` for atoms.
// See R7RS §6.1.

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
        // R7RS `eq?` on identifiers compares by name; whether the
        // symbol carries hygienic env metadata or not doesn't
        // change identity for user-visible equality.
        (
            Value::Symbol(x) | Value::SyntaxRef { name: x, .. },
            Value::Symbol(y) | Value::SyntaxRef { name: y, .. },
        ) => x == y,
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
        (Value::Macro(x), Value::Macro(y)) => Rc::ptr_eq(x, y),
        (Value::ErrorObject(x), Value::ErrorObject(y)) => Rc::ptr_eq(x, y),
        (Value::Promise(x), Value::Promise(y)) => Rc::ptr_eq(x, y),
        (Value::Values(x), Value::Values(y)) => Rc::ptr_eq(x, y),
        (Value::Record { fields: fx, .. }, Value::Record { fields: fy, .. }) => Rc::ptr_eq(fx, fy),
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
        (Value::Complex(x), Value::Complex(y)) => {
            // eqv? on a complex compares each component eqv?-wise.
            eqv(&x.re, &y.re) && eqv(&x.im, &y.im)
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
//
// Scheme has two ways of rendering a value to text. `write` produces
// output that the reader can parse back: strings get quotes and
// escape sequences, characters use `#\`, and so on. `display` is the
// human-friendly variant: strings appear without quotes, characters
// appear as themselves. R7RS §6.13.3 covers both.
//
// We map `write` onto Rust's `Debug` and `display` onto `Display`.
// That lets `format!("{x:?}")` produce write semantics and
// `format!("{x}")` produce display semantics, which is convenient
// inside the interpreter (every primitive's error message just
// embeds `{value}` and gets the right thing for free).
//
// The shared-structure machinery (`count_refs`, `LabelState`,
// `write_value`) implements R7RS *datum labels* (`#0=`, `#0#`).
// Without them a cyclic list would loop forever in `write`. The
// algorithm is a two-pass count-then-emit:
//   1. Walk the value, counting how often each `Rc`-shared node is
//      reached. Note which ones are reached *while we're already in
//      the middle of visiting them* — those are the cyclic nodes.
//   2. Emit. The first time we hit a cyclic node, emit `#N=` and
//      assign label N. Subsequent visits emit `#N#`.
// `write-shared` extends the labeling to merely-shared (non-cyclic)
// substructure too; plain `write` only labels cycles.

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_top_level(self, f, /*display=*/ false)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_top_level(self, f, /*display=*/ true)
    }
}

/// Wrapper that selects R7RS `write-shared` semantics (label shared
/// substructure as well as cyclic).
pub struct WriteShared<'a>(pub &'a Value);

impl fmt::Display for WriteShared<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_shared_top_level(self.0, f, /*display=*/ false)
    }
}

/// Top-level entry for write/display. Builds a structure table so
/// cyclic references are emitted with R7RS `#N=` / `#N#` datum
/// labels (§6.13.3) rather than recursing forever. R7RS `write`
/// labels only nodes that participate in a cycle; merely shared
/// (non-cyclic) substructure prints twice. The shared-too variant
/// is exposed via `write_shared_top_level`.
fn write_top_level(v: &Value, f: &mut fmt::Formatter<'_>, display: bool) -> fmt::Result {
    write_top_level_mode(v, f, display, /*label_shared=*/ false)
}

/// Like `write_top_level`, but emit datum labels for shared structure
/// too (R7RS `write-shared`).
pub(crate) fn write_shared_top_level(
    v: &Value,
    f: &mut fmt::Formatter<'_>,
    display: bool,
) -> fmt::Result {
    write_top_level_mode(v, f, display, /*label_shared=*/ true)
}

fn write_top_level_mode(
    v: &Value,
    f: &mut fmt::Formatter<'_>,
    display: bool,
    label_shared: bool,
) -> fmt::Result {
    let mut counts: HashMap<usize, usize> = HashMap::new();
    let mut cyclic: HashSet<usize> = HashSet::new();
    let mut visiting: HashSet<usize> = HashSet::new();
    count_refs(v, &mut counts, &mut cyclic, &mut visiting);
    let mut state = LabelState {
        counts,
        cyclic,
        label_shared,
        labels: HashMap::new(),
        next_label: 0,
    };
    write_value(v, f, display, &mut state)
}

#[derive(Debug)]
struct LabelState {
    /// Reference count per shared structure (by Rc pointer).
    counts: HashMap<usize, usize>,
    /// Set of keys that are part of a cycle.
    cyclic: HashSet<usize>,
    /// If true, label nodes whose `counts` > 1 in addition to cyclic
    /// nodes (used by `write-shared`). Plain `write` labels only
    /// cyclic nodes.
    label_shared: bool,
    /// Labels assigned so far (Rc pointer → integer label).
    labels: HashMap<usize, usize>,
    next_label: usize,
}

fn count_refs(
    v: &Value,
    counts: &mut HashMap<usize, usize>,
    cyclic: &mut HashSet<usize>,
    visiting: &mut HashSet<usize>,
) {
    let key = shared_key(v);
    if let Some(k) = key {
        let n = counts.entry(k).and_modify(|c| *c += 1).or_insert(1);
        // Already visited above us on the recursion stack — this is a
        // cycle. Record and don't recurse.
        if visiting.contains(&k) {
            cyclic.insert(k);
            return;
        }
        // Already counted (sharing without cycle) — don't recurse
        // again. Bump count only.
        if *n > 1 {
            return;
        }
        visiting.insert(k);
    }
    match v {
        Value::Pair(p) => {
            let cell = p.borrow();
            count_refs(&cell.car, counts, cyclic, visiting);
            count_refs(&cell.cdr, counts, cyclic, visiting);
        }
        Value::Vector(items) => {
            for item in items.borrow().iter() {
                count_refs(item, counts, cyclic, visiting);
            }
        }
        _ => {}
    }
    if let Some(k) = key {
        visiting.remove(&k);
    }
}

/// Return a pointer-as-usize key for values that can be shared
/// (Pairs and Vectors). Other values are atomic / immutable from
/// the cycle-detection perspective.
fn shared_key(v: &Value) -> Option<usize> {
    match v {
        Value::Pair(p) => Some(Rc::as_ptr(p) as usize),
        Value::Vector(p) => Some(Rc::as_ptr(p) as usize),
        _ => None,
    }
}

/// Render a value. When `display` is false this is R7RS `write`;
/// when true it is R7RS `display`. Uses `state` to emit datum labels
/// for shared and cyclic structures.
fn write_value(
    v: &Value,
    f: &mut fmt::Formatter<'_>,
    display: bool,
    state: &mut LabelState,
) -> fmt::Result {
    // If this is a shareable Pair/Vector that needs a label (cyclic,
    // or shared when in write-shared mode), emit a label on first
    // visit and a backref on subsequent visits.
    if let Some(key) = shared_key(v)
        && (state.cyclic.contains(&key)
            || (state.label_shared && state.counts.get(&key).copied().unwrap_or(0) > 1))
    {
        if let Some(label) = state.labels.get(&key).copied() {
            return write!(f, "#{label}#");
        }
        let label = state.next_label;
        state.next_label += 1;
        state.labels.insert(key, label);
        write!(f, "#{label}=")?;
    }
    write_value_body(v, f, display, state)
}

#[allow(clippy::too_many_lines)]
fn write_value_body(
    v: &Value,
    f: &mut fmt::Formatter<'_>,
    display: bool,
    state: &mut LabelState,
) -> fmt::Result {
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
        Value::SyntaxRef { name, .. } => {
            // Render a macro-introduced reference using the same
            // rules as a plain symbol — the carried env is metadata
            // for the evaluator and isn't user-visible.
            let n = name.name();
            if display || !symbol_needs_pipes(n) {
                f.write_str(n)
            } else {
                write_pipe_quoted_symbol(n, f)
            }
        }
        Value::Symbol(s) => {
            // R7RS write must produce a representation that the
            // reader can parse back. For symbols whose name looks
            // like a number (`+inf.0`), contains forbidden
            // characters (spaces, `|`, `\\`, `"`, `;`, parens), or
            // is empty, wrap in `|…|` with escapes.
            // `display` (the non-debug path) doesn't need round-trip
            // preservation; we still write the bare name for
            // `display`.
            let name = s.name();
            if display || !symbol_needs_pipes(name) {
                f.write_str(name)
            } else {
                write_pipe_quoted_symbol(name, f)
            }
        }
        Value::Int(n) => write!(f, "{n}"),
        Value::BigInt(b) => write!(f, "{b}"),
        Value::Rational(r) => {
            // num-rational's Display already produces `n/d`.
            write!(f, "{r}")
        }
        Value::Float(x) => write_float(*x, f),
        Value::Complex(c) => write_complex(c, f),
        Value::String(s) => {
            if display {
                f.write_str(&s.borrow())
            } else {
                write_string_literal(&s.borrow(), f)
            }
        }
        Value::Pair(_) => write_list(v, f, display, state),
        Value::Vector(items) => {
            f.write_str("#(")?;
            let items = items.borrow();
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    f.write_str(" ")?;
                }
                write_value(item, f, display, state)?;
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
        Value::Macro(_) => f.write_str("#<macro>"),
        Value::ErrorObject(e) => {
            write!(f, "#<error: {}", e.message)?;
            for i in &e.irritants {
                write!(f, " {i}")?;
            }
            f.write_str(">")
        }
        Value::Promise(_) => f.write_str("#<promise>"),
        Value::Values(vs) => {
            f.write_str("#<values")?;
            for v in vs.iter() {
                write!(f, " {v:?}")?;
            }
            f.write_str(">")
        }
        Value::Record { type_id, fields } => {
            write!(f, "#<{}", type_id.name)?;
            for field in fields.iter() {
                write!(f, " {:?}", field.borrow())?;
            }
            f.write_str(">")
        }
    }
}

/// Does this symbol name need `|…|` quoting for round-trip
/// preservation under R7RS write?
fn symbol_needs_pipes(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    // Forbidden chars in unquoted identifiers (R7RS §7.1.1):
    // whitespace, `|`, `\`, `"`, `;`, parens, `#`, `'`, `,`, `` ` ``.
    if name.chars().any(|c| {
        c.is_whitespace()
            || matches!(
                c,
                '|' | '\\' | '"' | ';' | '(' | ')' | '#' | '\'' | ',' | '`' | '[' | ']' | '{' | '}'
            )
    }) {
        return true;
    }
    // If it looks like a number, we'd parse it back as one.
    if looks_numeric(name) {
        return true;
    }
    // Otherwise check the identifier grammar.
    !is_plain_identifier(name)
}

fn looks_numeric(s: &str) -> bool {
    // Approximations: starts with digit; OR sign-then-digit/dot;
    // OR `+inf.0` / `-inf.0` / `+nan.0` / `-nan.0` (any case);
    // OR `+i` / `-i` (the complex unit imaginary).
    let lower = s.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "+inf.0" | "-inf.0" | "+nan.0" | "-nan.0" | "+i" | "-i"
    ) {
        return true;
    }
    let bytes = s.as_bytes();
    let first = bytes[0];
    if first.is_ascii_digit() {
        return true;
    }
    if matches!(first, b'+' | b'-' | b'.') && bytes.len() >= 2 {
        let next = bytes[1];
        if next.is_ascii_digit() || next == b'.' {
            return true;
        }
    }
    // `+inf.0xyz` / `-nan.0xyz` etc.: starts like an infinity/NaN
    // lexeme and trails extra subsequent characters. R7RS doesn't
    // make this a numeric lexeme, but the writer pipe-quotes such
    // names to avoid the reader misinterpreting them.
    if (lower.starts_with("+inf.0")
        || lower.starts_with("-inf.0")
        || lower.starts_with("+nan.0")
        || lower.starts_with("-nan.0"))
        && s.len() > 6
    {
        return true;
    }
    false
}

fn is_plain_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    let initial_ok = first.is_ascii_alphabetic()
        || matches!(
            first,
            '!' | '$' | '%' | '&' | '*' | '/' | ':' | '<' | '=' | '>' | '?' | '^' | '_' | '~'
        );
    // Peculiar identifiers: `+`, `-`, `...`, `->...`
    if !initial_ok {
        if s == "+" || s == "-" || s == "..." {
            return true;
        }
        if (first == '+' || first == '-') && s.len() >= 2 {
            // `->foo` and similar
            let next = s.chars().nth(1).unwrap();
            if next == '>' || next.is_ascii_alphabetic() {
                return chars.all(is_subsequent);
            }
            return false;
        }
        return false;
    }
    chars.all(is_subsequent)
}

fn is_subsequent(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '!' | '$'
                | '%'
                | '&'
                | '*'
                | '/'
                | ':'
                | '<'
                | '='
                | '>'
                | '?'
                | '^'
                | '_'
                | '~'
                | '+'
                | '-'
                | '.'
                | '@'
        )
}

fn write_pipe_quoted_symbol(name: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("|")?;
    for c in name.chars() {
        match c {
            '|' => f.write_str("\\|")?,
            '\\' => f.write_str("\\\\")?,
            c => f.write_str(&c.to_string())?,
        }
    }
    f.write_str("|")
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

/// R7RS lexeme `<re><sign><|im|>i`. Each component carries its own
/// exactness, so a part with `Value::Float` renders with a decimal
/// point and `Value::Int` / `Value::Rational` render without. The
/// special unit cases `+i` / `-i` (imag ±1, no real shown) and the
/// pure-imaginary cases (`0+1.0i`-style suppression of the leading
/// zero) follow R7RS §6.2.6.
fn write_complex(c: &ComplexValue, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    use num_traits::Zero;
    let re_is_negative_zero = matches!(c.re, Value::Float(x) if x == 0.0 && x.is_sign_negative());
    let re_is_zero = !re_is_negative_zero
        && match &c.re {
            Value::Int(0) => true,
            Value::BigInt(b) => b.is_zero(),
            Value::Rational(r) => r.numer().is_zero(),
            Value::Float(x) => *x == 0.0,
            _ => false,
        };
    let im_is_neg = match &c.im {
        Value::Int(n) => *n < 0,
        Value::BigInt(b) => b.sign() == num_bigint::Sign::Minus,
        Value::Rational(r) => r.numer().sign() == num_bigint::Sign::Minus,
        Value::Float(x) => x.is_sign_negative() || x.is_nan(),
        _ => false,
    };
    // R7RS `+i` / `-i` is exact only — `+1.0i` keeps the decimal
    // point because the imag part is inexact. So shorten the unit
    // form only when imag is an exact ±1.
    let im_is_unit = matches!(&c.im, Value::Int(1 | -1));
    // The sign of the imaginary part may be carried by its writer
    // (special values like `+inf.0` / `+nan.0` always emit one) or
    // we may need to prepend it ourselves. Compute it once.
    let im_emits_own_sign =
        matches!(&c.im, Value::Float(x) if !x.is_finite() || x.is_sign_negative());
    if !re_is_zero {
        write_complex_part(&c.re, f)?;
    }
    // Print the sign between real and imag (or as leading sign on a
    // purely imaginary value) unless the imag writer will emit it
    // for us.
    // Only emit `+` for non-negative imag (negative imag's writer
    // prints the `-` itself; +inf / +nan also self-prefix).
    if !im_emits_own_sign && !im_is_neg {
        f.write_str("+")?;
    }
    if im_is_unit {
        if im_is_neg {
            f.write_str("-")?;
        }
        // Drop the magnitude `1` for unit-magnitude imag.
    } else {
        write_complex_part(&c.im, f)?;
    }
    f.write_str("i")
}

fn write_complex_part(v: &Value, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match v {
        Value::Int(n) => write!(f, "{n}"),
        Value::BigInt(b) => write!(f, "{b}"),
        Value::Rational(r) => write!(f, "{r}"),
        Value::Float(x) => write_float(*x, f),
        _ => Ok(()),
    }
}

fn write_float(x: f64, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if x.is_nan() {
        return f.write_str("+nan.0");
    }
    if x.is_infinite() {
        return f.write_str(if x > 0.0 { "+inf.0" } else { "-inf.0" });
    }
    if x == 0.0 {
        return f.write_str(if x.is_sign_negative() { "-0.0" } else { "0.0" });
    }
    // Get the *shortest* decimal that round-trips (R7RS §6.2.6 requires
    // `(string->number (number->string x))` to recover x). Ryu computes
    // the canonical shortest digits directly — the same set chibi/Racket
    // print — instead of the old "try 15 then widen to 16/17" heuristic,
    // which could emit a non-canonical digit at boundary values
    // (nscheme-ecg). We take Ryu's digits + exponent and reformat them
    // with our own Scheme conventions (fixed vs scientific, `e+`, `.0`).
    let mut buf = ryu::Buffer::new();
    let (negative, digits, exp) = shortest_digits(buf.format_finite(x));
    f.write_str(&render_decimal(&negative, &digits, exp))
}

/// Parse Ryu's shortest representation into `(sign, significant-digits,
/// exponent)` in normalized scientific form: the value equals
/// `digits[0].digits[1..] × 10^exp`, with no leading/trailing zeros in
/// `digits`. Ryu emits either `int.frac` or `int.frac e±NN` (lowercase
/// `e`, `-` only for negative exponents), which this normalizes.
fn shortest_digits(s: &str) -> (String, String, i32) {
    let negative = s.starts_with('-');
    let body = s.strip_prefix('-').unwrap_or(s);
    let (mantissa, exp_part) = match body.split_once('e') {
        Some((m, e)) => (m, e.parse::<i32>().expect("ryu integer exponent")),
        None => (body, 0),
    };
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    // value = (int_part ++ frac_part) × 10^(exp_part - frac_part.len())
    let raw: String = format!("{int_part}{frac_part}");
    let mut power = exp_part - i32::try_from(frac_part.len()).expect("frac length fits i32");
    // Strip leading zeros (don't change value), then trailing zeros
    // (each one raises the power by 1).
    let mut digits = raw.trim_start_matches('0').to_string();
    while digits.ends_with('0') {
        digits.pop();
        power += 1;
    }
    // value = digits_as_int × 10^power; in `d.ddd × 10^exp` form,
    // exp = power + (len(digits) - 1).
    let exp = power + i32::try_from(digits.len()).expect("digit count fits i32") - 1;
    let sign = if negative { "-" } else { "" };
    (sign.to_string(), digits, exp)
}

/// Render normalized `(sign, digits, exp)` — where the value is
/// `digits[0].digits[1..] × 10^exp` — as a Scheme inexact literal. The
/// output always contains a decimal point or an exponent marker so the
/// reader reconstructs it as inexact.
fn render_decimal(sign: &str, digits: &str, exp: i32) -> String {
    // Place the decimal point. The original scientific form had it
    // after one digit, so the value is `digits[0].digits[1..] * 10^exp`.
    // For the fixed form we want `whole.frac`, where `whole` and
    // `frac` are slices into `digits` shifted by `exp+1`.
    // Choose between fixed and scientific based on exponent.
    // Heuristic matches what chibi prints: fixed for -4 <= exp <= 15.
    if (-4..=15).contains(&exp) {
        let mut out = String::new();
        out.push_str(sign);
        if exp >= 0 {
            #[allow(clippy::cast_sign_loss)]
            let dpos = exp as usize + 1;
            if dpos >= digits.len() {
                out.push_str(digits);
                for _ in digits.len()..dpos {
                    out.push('0');
                }
                out.push_str(".0");
            } else {
                out.push_str(&digits[..dpos]);
                out.push('.');
                out.push_str(&digits[dpos..]);
            }
        } else {
            out.push_str("0.");
            #[allow(clippy::cast_sign_loss)]
            let zeros = (-exp) as usize - 1;
            for _ in 0..zeros {
                out.push('0');
            }
            out.push_str(digits);
        }
        out
    } else {
        // Scientific form. `digits[0].rest e exp`. R7RS-canonical
        // marker is `e`.
        let mut out = String::new();
        out.push_str(sign);
        if digits.len() == 1 {
            out.push_str(digits);
            out.push_str(".0");
        } else {
            out.push(digits.as_bytes()[0] as char);
            out.push('.');
            out.push_str(&digits[1..]);
        }
        // R7RS readers accept both `e123` and `e+123`. Chibi and
        // Racket emit the `+` for positive exponents, so we do too.
        out.push('e');
        if exp >= 0 {
            out.push('+');
        }
        out.push_str(&exp.to_string());
        out
    }
}

/// Emit the inline body of a pair-chain inside `(…)`. The caller
/// has already emitted any leading `#N=` label for the outermost
/// pair. We walk the spine via `cdr`; on encountering a shared
/// cdr we emit it as a `. #N#` (or `. #N=(…)` if first visit)
/// dotted tail so the structure round-trips.
fn write_list(
    v: &Value,
    f: &mut fmt::Formatter<'_>,
    display: bool,
    state: &mut LabelState,
) -> fmt::Result {
    f.write_str("(")?;
    let mut cur = v.clone();
    let mut first = true;
    loop {
        // End-of-list cases.
        match &cur {
            Value::Null => break,
            Value::Pair(_) => { /* fall through */ }
            other => {
                f.write_str(" . ")?;
                write_value(other, f, display, state)?;
                break;
            }
        }
        let Value::Pair(p) = cur else { unreachable!() };
        let key = Rc::as_ptr(&p) as usize;
        // For elements AFTER the first, if this cdr-pair is shared,
        // emit it via the labeled path (dotted tail).
        if !first && state.counts.get(&key).copied().unwrap_or(0) > 1 {
            f.write_str(" . ")?;
            write_value(&Value::Pair(p), f, display, state)?;
            break;
        }
        if !first {
            f.write_str(" ")?;
        }
        first = false;
        let cell = p.borrow();
        let car = cell.car.clone();
        let cdr = cell.cdr.clone();
        drop(cell);
        write_value(&car, f, display, state)?;
        cur = cdr;
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
