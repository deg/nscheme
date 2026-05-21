//! nscheme — an R7RS-small Scheme interpreter.
//!
//! Library entry point. The command-line REPL is a thin binary in `src/main.rs`
//! that consumes this library; all language behavior lives here so the
//! interpreter can be embedded in other Rust applications.

pub mod lex;
pub mod parse;
pub mod value;

/// Crate version, taken from `Cargo.toml` at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
