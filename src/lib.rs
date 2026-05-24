//! nscheme — an R7RS-small Scheme interpreter.
//!
//! Library entry point. The command-line REPL is a thin binary in
//! `src/main.rs` that consumes this library; all language behavior
//! lives here so the interpreter can be embedded in other Rust
//! applications.
//!
//! ## Module map
//!
//! - [`value`] — runtime data model. The [`Value`](value::Value)
//!   enum defines what a Scheme value *is*. Start here when reading
//!   the codebase: nearly every other module produces or consumes
//!   these values.
//! - [`env`] — lexical environments. A small linked-frame structure
//!   that maps symbols to values.
//! - [`lex`] — source text → tokens. R7RS-small lexical syntax.
//! - [`parse`] — tokens → [`Value`](value::Value). Scheme programs
//!   *are* data, so the parser emits the same value type the
//!   evaluator walks.
//! - [`eval`] — the evaluator. A step-loop machine over a `Vec<Frame>`
//!   continuation stack. The heart of the interpreter.
//! - [`macros`] — `syntax-rules` hygienic macro expander.
//! - [`builtins`] — the `(scheme base)` library of primitive
//!   procedures (`+`, `car`, `string-length`, …) implemented in Rust.
//! - [`io`] — port primitives (`open-input-file`, `read-char`,
//!   `display`, …).
//! - [`library`] — `define-library`, `import`, `cond-expand`. The
//!   R7RS module system.
//!
//! See `TOUR.md` at the repo root for a guided reading order; see
//! `docs/STYLE.md` for the in-code commentary conventions; see
//! `docs/0001-…06-…md` for the architecture decision records.

pub mod builtins;
pub mod env;
pub mod eval;
pub mod io;
pub mod lex;
pub mod library;
pub mod macros;
pub mod parse;
pub mod value;

/// Crate version, taken from `Cargo.toml` at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
