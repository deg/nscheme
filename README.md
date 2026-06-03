# nscheme

An [R7RS](https://standards.scheme.org/) Scheme interpreter written in Rust — the R7RS-small (2013) core plus the ratified **R7RS-large** libraries (Red + Tangerine editions). Built primarily as an experiment in autonomous AI-assisted development; not (yet) a production-grade implementation.

- **Reading the code?** Start with [TOUR.md](TOUR.md) — a guided tour of the source with suggested reading orders.
- **Curious how it was built?** [PROJECT.md](PROJECT.md) is the retrospective on the AI-assisted build process.
- **What can it do?** [docs/showcase/](docs/showcase/) is a dozen tiny programs that show off the R7RS-large libraries.
- **How conformant is it?** [docs/CONFORMANCE.md](docs/CONFORMANCE.md) — each library runs its actual upstream SRFI reference suite.
- **Contributing or extending it?** [docs/STYLE.md](docs/STYLE.md) describes the in-code commentary conventions; [docs/](docs/) holds the architecture decision records.

## What this is

- A **tree-walking interpreter** for Scheme — Lisp's oldest standardized dialect.
- The language target is **R7RS-small (2013)** — the 88-page revised report most modern Schemes track — plus the **R7RS-large** library set: 21 SRFI libraries from the ratified Red (2016) and Tangerine (2019) editions, loaded from `lib/` as `.sld` files. Examples: `(scheme list)`, `(scheme comparator)`, `(scheme mapping)`, `(scheme set)`, `(scheme regex)`, `(scheme generator)`, `(scheme stream)`, `(scheme show)`. See [docs/CONFORMANCE.md](docs/CONFORMANCE.md) for the full roster.
- Implemented in Rust, single binary, no runtime dependencies beyond what `cargo` pulls in at build time.

## Installation

### Step 1: Install Rust

If you don't already have Rust, the official installer is `rustup`:

```bash
# Pulls the rustup script and runs it. Installs Rust into ~/.cargo
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

This installs three commands you'll need:

- `rustc` — the Rust compiler
- `cargo` — Rust's build tool and package manager (analogous to `npm`, `pip`, or `go`)
- `rustup` — the toolchain manager itself

After install, restart your shell (or `source ~/.cargo/env`) so the new commands are on your `PATH`. Verify with:

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

The first build downloads dependencies (`thiserror`, `rustyline`, and a few transitive crates) and may take a minute or two. Subsequent builds are incremental.

The compiled binary lands at:

```
target/release/nscheme
```

You can copy it anywhere on your `PATH`, or run it directly from that path. For the rest of this README we'll write `./target/release/nscheme` — substitute `nscheme` if you've put it on your `PATH`.

> **Note**: `cargo build` (without `--release`) produces a debug binary at `target/debug/nscheme`. It's slower but compiles faster — useful while editing the interpreter itself.

## Running nscheme

### Interactive REPL

Run with no arguments to enter the read-eval-print loop:

```bash
./target/release/nscheme
```

```
nscheme 0.1.0 — R7RS Scheme interpreter. Type (exit) or press Ctrl-D to quit.
> (+ 1 2 3)
6
> (define (square x) (* x x))
> (square 7)
49
> (exit)
```

The REPL handles **multi-line input** automatically — if your parens aren't balanced when you hit Enter, the prompt changes to `…` and waits for the rest:

```
> (define (factorial n)
…   (if (= n 0)
…       1
…       (* n (factorial (- n 1)))))
> (factorial 10)
3628800
```

History is saved to `~/.nscheme_history`. Press the up-arrow to recall previous expressions.

### Run a file

```bash
./target/release/nscheme path/to/program.scm
```

Evaluates each top-level expression in the file in order. `(display …)` / `(write …)` print to stdout; errors go to stderr.

### Run a file, then stay in the REPL

```bash
./target/release/nscheme -i path/to/program.scm
```

Evaluates the file (one or more files) into the session, then drops you into the REPL with all of its definitions — and its imports — still live. You can also `(load "file.scm")` from inside a running REPL. See [docs/showcase/](docs/showcase/) for worked examples.

### Evaluate a one-shot expression

```bash
./target/release/nscheme -e '(* 6 7)'
# prints: 42
```

## A short tour of Scheme

If you've never used a Lisp before: every expression is a parenthesized list where the first element is the *operator* and the rest are *operands*. `(+ 1 2)` means "call `+` with arguments `1` and `2`". There is no precedence and no infix notation — every operator is prefix.

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

A more thorough Scheme primer: [*The Scheme Programming Language*](https://www.scheme.com/tspl4/) by Kent Dybvig — free online, written for the previous revision (R6RS) but the core language is similar.

## What's implemented

All of R7RS-small, plus the R7RS-large library set. See the `bd` issue tracker (`bd list --status=open`) for the remaining backlog (mostly refinements — see "Known gaps" below).

Currently working:

- Lexer + parser (including `[ ]` as a synonym for `( )`)
- Evaluator with proper tail calls and lexical closures
- Special forms: `quote`, `if`, `lambda`, `define`, `set!`, `begin`, `let`, `let*`, `letrec`/`letrec*`, named `let`, `cond` (with `=>` clauses), `case`, `and`/`or`, `when`/`unless`, `do`, `quasiquote`, `case-lambda`, `define-values`, `define-record-type`, `let-values`/`let*-values`, `parameterize`, `guard`, `delay`/`delay-force`, `eval`, `load`
- Base library: arithmetic with exact/inexact promotion, all the type predicates, equality (`eq?`/`eqv?`/`equal?`), list operations (`cons`, `car`, `cdr`, `length`, `reverse`, `append`, `list-ref`, `member`/`assoc` families), `map`, `for-each`

### Also implemented

- Full numeric tower (`i64` / arbitrary-precision `BigInt` / exact `BigRational` / `f64` / `Complex`) with R7RS exact/inexact promotion; complex literals (`1+2i`) parse and evaluate
- String / char / symbol / vector / bytevector operations with Unicode-aware string indexing
- Textual ports (string and file), `display` / `write` / `read-char` / datum `read`, `eof-object`, `call-with-output-file` / `call-with-input-file`
- Hygienic `syntax-rules` (scope-based) — `define-syntax`, `let-syntax`, `letrec-syntax`
- `define-library`, `import` (with `only` / `except` / `prefix` / `rename`), `cond-expand`, and a **filesystem loader** that finds `(import (foo bar))` on disk as `foo/bar.sld` (search path: `NSCHEME_LIB_PATH` → compiled-in default → `./lib`)
- `call/cc`, `call-with-current-continuation`, `dynamic-wind`, `apply`
- Exception handling: `raise`, `raise-continuable`, `with-exception-handler`, `guard`, error objects
- `delay` / `force` / `make-promise`, lazy evaluation
- `values` / `call-with-values` / `let-values` / `let*-values`
- `make-parameter` / `parameterize`

### R7RS-large libraries

21 SRFI libraries from the ratified Red (2016) and Tangerine (2019) editions ship as `.sld` files under `lib/` and are loaded on demand. Among them: list (SRFI 1), comparator (128), set/bag (113), hash-table (125), mapping (146), regex (115), generator (158), stream (41), ideque (134), lseq (127), list-queue (117), bitwise (151), fixnum/flonum (143/144), division (141), box (111), charset (14), show (159), sort (132), bytevector (160). 18 of the 21 run their actual upstream SRFI reference suites green (~5,400 assertions); see [docs/CONFORMANCE.md](docs/CONFORMANCE.md).

### Known gaps

Tracked in the `bd` issue tracker, with deeper detail in [`docs/`](docs/):

- `eval` is a special form, not yet a first-class procedure you can pass as a value (`nscheme-iii`)
- Exact-complex arithmetic falls through to inexact — `1+2i` works but `(* 1+2i 1+2i)` is computed in floats (`nscheme-5mn`)
- Hygiene is scope-based and passes the canonical tests, but the full sets-of-scopes algorithm is not yet complete (`nscheme-d6o`)
- Performance: it's a tree-walking interpreter with no bytecode VM, so heavy loops are slow (`nscheme-6mp`)
- Smaller reader / Unicode-case-folding / error-category refinements (`nscheme-9gy`, `nscheme-vfp`, `nscheme-1o2`)

## Design

See [`docs/`](docs/) for architecture decision records:

- 0001 — Tree-walking interpreter with explicit step-loop
- 0002 — Numeric tower
- 0003 — `syntax-rules` hygiene (originally alpha-renaming; since reworked to scope-based — see the ADR's update note)
- 0004 — Continuations as cloned frame stacks
- 0005 — Exception handling (incl. how primitive errors flow as raises)
- 0006 — Library / module system

ADR 0001 is the load-bearing one: it explains why the evaluator is a step-loop with continuation frames rather than recursive `eval` calls, and why that choice makes TCO and `call/cc` cheap.

## Testing

### Running all tests

```bash
cargo test
```

That runs about **624 tests across ~40 files** in a few seconds (the slow SRFI 132 sort suite is `#[ignore]`d — run it with `-- --ignored`). The suite covers each module's unit tests, end-to-end integration tests (evaluation, tail calls, special forms, the base library, I/O, macros, libraries, continuations, exceptions, lazy evaluation, multiple values, parameters), the in-house R7RS conformance corpus, and the R7RS-large reference-suite harness described below.

### R7RS-large reference suites

`tests/conformance.rs` runs each R7RS-large library against its **actual upstream SRFI reference test suite** (vendored under `tests/r7rs-large-corpus/`, adapted only at the non-portable preamble). 18 of the 21 libraries have a portable upstream suite and all 18 run green — roughly **5,400 verbatim upstream assertions**. The full story, including the bugs these suites surfaced, is in [docs/CONFORMANCE.md](docs/CONFORMANCE.md).

```bash
cargo test --test conformance              # the fast suites
cargo test --test conformance -- --ignored # also the ~2-min SRFI 132 sort suite
```

### Running specific tests

```bash
cargo test --test r7rs_conformance      # the in-house R7RS suite
cargo test --test tail_calls            # tail-position regression suite
cargo test --test syntax_rules          # macros
cargo test --test continuations         # call/cc
cargo test factorial                    # any test name matching "factorial"
cargo test -- --nocapture               # show println! / eprintln! output
```

### Standard R7RS conformance corpus (chibi-scheme)

`tests/r7rs_chibi.rs` runs chibi-scheme's [`r7rs-tests.scm`](tests/r7rs-corpus/chibi-r7rs-tests.scm) — the de facto standard R7RS-small conformance suite — through nscheme. To see the baseline:

```bash
cargo test --test r7rs_chibi -- --nocapture
```

Sample output:

```
=== chibi r7rs-tests.scm baseline ===
Top-level forms in corpus:
  total:               1180
  evaluated cleanly:   1180
  raised an error:     0
Test assertions run inside those forms:
  total:               1225
  passed:              1225
  failed:              0
Duration: ~230ms
```

All 1225 chibi `(test …)` / `(test-assert …)` / `(test-error …)` assertions pass; every one of the 1180 top-level forms evaluates without raising an uncaught exception.

The `BASELINE_MIN_PASSES` constant in the test guards against regressions — lowering it requires triage in [bead `nscheme-i0p`](.beads/issues.jsonl).

### Linting / formatting

```bash
cargo clippy --all-targets -- -D warnings   # lint (treats warnings as errors)
cargo fmt                                   # auto-format
cargo fmt --check                           # verify formatted without changing
```

### Backlog

```bash
bd ready              # tasks ready to work on
bd list --status=open # all open tasks
bd show <id>          # details on a specific task
```

## License

MIT OR Apache-2.0. The chibi-scheme test corpus under `tests/r7rs-corpus/chibi-r7rs-tests.scm` is redistributed under its original BSD 3-clause license — see [`tests/r7rs-corpus/COPYING-chibi-scheme`](tests/r7rs-corpus/COPYING-chibi-scheme).
