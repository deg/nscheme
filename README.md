# nscheme

An [R7RS-small](https://small.r7rs.org/) Scheme interpreter written in
Rust. Built primarily as an experiment in autonomous AI-assisted
development; not (yet) a production-grade implementation.

## What this is

- A **tree-walking interpreter** for a subset of Scheme — Lisp's
  oldest standardized dialect.
- The language target is **R7RS-small (2013)**, the 80-page revised
  report that most modern Scheme implementations follow as their
  baseline.
- Implemented in Rust, single binary, no runtime dependencies beyond
  what `cargo` pulls in at build time.

## Installation

### Step 1: Install Rust

If you don't already have Rust, the official installer is `rustup`:

```bash
# Pulls the rustup script and runs it. Installs Rust into ~/.cargo
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

This installs three commands you'll need:

- `rustc` — the Rust compiler
- `cargo` — Rust's build tool and package manager (analogous to
  `npm`, `pip`, or `go`)
- `rustup` — the toolchain manager itself

After install, restart your shell (or `source ~/.cargo/env`) so the new
commands are on your `PATH`. Verify with:

```bash
rustc --version   # Should print "rustc 1.85+" or newer
cargo --version
```

`nscheme` is developed against Rust **1.85 or newer** (edition 2024).

### Step 2: Build nscheme

Clone the repository and build a release binary:

```bash
git clone <repository-url>
cd nscheme
cargo build --release
```

The first build downloads dependencies (`thiserror`, `rustyline`, and a
few transitive crates) and may take a minute or two. Subsequent builds
are incremental.

The compiled binary lands at:

```
target/release/nscheme
```

You can copy it anywhere on your `PATH`, or run it directly from that
path. For the rest of this README we'll write `./target/release/nscheme`
— substitute `nscheme` if you've put it on your `PATH`.

> **Note**: `cargo build` (without `--release`) produces a debug binary
> at `target/debug/nscheme`. It's slower but compiles faster — useful
> while editing the interpreter itself.

## Running nscheme

### Interactive REPL

Run with no arguments to enter the read-eval-print loop:

```bash
./target/release/nscheme
```

```
nscheme 0.1.0 — R7RS-small interpreter. Type (exit) or press Ctrl-D to quit.
> (+ 1 2 3)
6
> (define (square x) (* x x))
> (square 7)
49
> (exit)
```

The REPL handles **multi-line input** automatically — if your parens
aren't balanced when you hit Enter, the prompt changes to `…` and waits
for the rest:

```
> (define (factorial n)
…   (if (= n 0)
…       1
…       (* n (factorial (- n 1)))))
> (factorial 10)
3628800
```

History is saved to `~/.nscheme_history`. Press the up-arrow to recall
previous expressions.

### Run a file

```bash
./target/release/nscheme path/to/program.scm
```

Evaluates each top-level expression in the file in order. Output is
silent (errors go to stderr); use `(display …)` for output once T14
lands.

### Evaluate a one-shot expression

```bash
./target/release/nscheme -e '(* 6 7)'
# prints: 42
```

## A short tour of Scheme

If you've never used a Lisp before: every expression is a
parenthesized list where the first element is the *operator* and the
rest are *operands*. `(+ 1 2)` means "call `+` with arguments `1` and
`2`". There is no precedence and no infix notation — every operator is
prefix.

```scheme
; Comments start with semicolons and run to end of line.

; Arithmetic
(+ 1 2 3 4)            ; → 10
(* (- 5 1) (+ 2 3))    ; → 20

; Defining a variable
(define greeting "hello, world")
greeting               ; → "hello, world"

; Defining a function (note the (name . params) shorthand)
(define (double x) (* x 2))
(double 21)            ; → 42

; Anonymous functions
((lambda (x y) (+ x y)) 3 4)   ; → 7

; Conditionals
(if (> 5 3) 'yes 'no)  ; → yes

(cond ((= 1 2) 'never)
      ((= 1 1) 'always)
      (else 'fallback))
; → always

; Lists are the central data structure
(define xs '(1 2 3 4 5))
(car xs)               ; → 1   (first element)
(cdr xs)               ; → (2 3 4 5)  (everything but first)
(length xs)            ; → 5
(reverse xs)           ; → (5 4 3 2 1)

; Higher-order functions
(map (lambda (x) (* x x)) xs)   ; → (1 4 9 16 25)

; Recursion is the canonical loop in Scheme
(define (sum-to n)
  (if (= n 0) 0 (+ n (sum-to (- n 1)))))
(sum-to 100)           ; → 5050

; nscheme has proper tail-call optimization, so this won't overflow:
(define (count-down n)
  (if (= n 0) 'done (count-down (- n 1))))
(count-down 1000000)   ; → done
```

A more thorough Scheme primer:
[*The Scheme Programming Language*](https://www.scheme.com/tspl4/)
by Kent Dybvig — free online, written for the previous revision (R6RS)
but the core language is similar.

## What's implemented

Roughly: most of R7RS-small except the items that intentionally needed
their own design effort. See the `bd` issue tracker (`bd list
--status=open`) for the current backlog.

Currently working:

- Lexer + parser
- Evaluator with proper tail calls and lexical closures
- Special forms: `quote`, `if`, `lambda`, `define`, `set!`, `begin`,
  `let`, `let*`, `letrec`/`letrec*`, named `let`, `cond` (with `=>`
  clauses), `case`, `and`/`or`, `when`/`unless`, `do`, `quasiquote`
- Base library: arithmetic with exact/inexact promotion, all the type
  predicates, equality (`eq?`/`eqv?`/`equal?`), list operations
  (`cons`, `car`, `cdr`, `length`, `reverse`, `append`, `list-ref`,
  `member`/`assoc` families), `map`, `for-each`

Not yet implemented (filed as `bd` issues):

- Full numeric tower (bignums, exact rationals)
- Rich string/char/vector/bytevector operations
- I/O ports and `display`/`write`/`read`
- `syntax-rules` macros
- R7RS `define-library` modules
- `call/cc` (architecturally ready — surface wiring TBD)
- Exception handling (`raise`/`guard`)

## Design

See [`docs/`](docs/) for architecture decision records. The most
important one is `0001-tree-walking-interpreter.md` — it explains why
the evaluator is a step-loop with continuation frames rather than a
recursive `eval` function, and why that choice matters for TCO and
future `call/cc`.

## Development

```bash
cargo test                          # Run all tests
cargo clippy --all-targets -- -D warnings   # Lint
cargo fmt                           # Format
bd ready                            # See what's next on the backlog
```

## License

MIT OR Apache-2.0
