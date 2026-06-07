# A guided tour of nscheme

This document is a reading guide for the nscheme source. It exists for one reason: a Scheme interpreter is a moderately large piece of software, and opening any file cold is harder than it has to be. There are productive orders in which to read these ~14,000 lines of Rust (plus the R7RS-large libraries — another ~20,000 lines of Scheme under `lib/`); this is a map to them.

If you have not yet read [`PROJECT.md`](PROJECT.md) — the retrospective on how this codebase was built — read it first. The rest of this guide assumes you've already decided you want to read the code.

## Who this is for

A strong programmer who is new to *both* Rust and Scheme.

The annotations in the source assume you know what `Result` and `Option` are, what a closed enum is, what `HashMap<K, V>` is, what an iterator is. You don't need to know what `Rc<RefCell<T>>` means, what hygienic macros are, what a tail call is, or what `call/cc` does — those things get introduced at the first place in the code where they appear.

If you're a Rust expert, the `Rc<RefCell<T>>` explanations will be over-detailed for you; skip them. If you're a Scheme expert, the same goes for the Scheme-concept notes. Either group has plenty of material elsewhere.

## What to read first, and why

The code has three concentric layers. The outer layer (the data model) is the smallest and the foundation for everything else. Read outward from there.

### 1. The data model

Start at [`src/value.rs`](src/value.rs).

This is where every runtime Scheme object is defined — the `Value` enum, its variants (pairs, vectors, numbers, procedures, ports, …), and the three R7RS equality predicates that compare them. Almost every other file in the project produces or consumes these values.

While you're there:

- The `Rc<RefCell<T>>` pattern shows up here for the first time, at the `Pair` section. That comment is the canonical explanation for the pattern; everywhere else it's used (`String`, `Vector`, `Bytevector`, `Port`, the `Env` map in `env.rs`), it's the same pattern.
- The three equality predicates (`eq?` / `eqv?` / `equal?`) are the foundation of nearly every R7RS test. Understand the layering.
- The numeric tower (`Int` → `BigInt` → `Rational` → `Float`, with separate `Complex` for `1+2i`-style values) is presented here as data; the *arithmetic* over it lives in `src/builtins.rs`. The reasoning behind the four-rung tower is in [`docs/0002-numeric-tower.md`](docs/0002-numeric-tower.md).

### 2. The pipeline from text to value

These three small files form the path that turns source text into runtime values. Read them in order:

[`src/lex.rs`](src/lex.rs) → [`src/parse.rs`](src/parse.rs) → [`src/env.rs`](src/env.rs)

The lexer is a hand-coded state machine. The parser is the place where the central trick of Lisp — *code is data* — shows up: the parser doesn't produce its own AST type, it produces `Value`s, the same enum the evaluator walks. `env.rs` is the small data structure that maps symbols to values in a lexical scope.

If you're a programmer who has never written a language before, this is the order in which most language books introduce the topic: lexer, parser, then "now what?" The "now what" is the evaluator, which is layer 3.

### 3. The evaluator

This is the heart of the interpreter. Three files:

[`src/eval.rs`](src/eval.rs) — the main file. Read this in the order the file's own header recommends:

1. The `Step` and `Frame` enums. These are the state machine. Read with care; you won't understand the rest until you understand these.
2. The `eval()` function. Five match arms, one per `Step` variant. The whole architecture is visible in those forty lines.
3. `step_eval` — the syntax dispatcher. One arm per special form.
4. `step_apply` — the procedure dispatcher. One arm per kind of callable thing.
5. `resume` — the frame dispatcher. One arm per kind of pending work.

[`src/macros.rs`](src/macros.rs) — `syntax-rules` hygienic macro expansion. The architectural decision that drives the whole module — `Value::SyntaxRef` and the `VarKey` keying — is explained in the module header. The rationale is in [`docs/0003-syntax-rules-hygiene.md`](docs/0003-syntax-rules-hygiene.md).

[`src/builtins.rs`](src/builtins.rs) — the catalog of primitive procedures (`+`, `car`, `string-length`, `error`, …). Not meant to be read front-to-back; treat it as a reference. Skim the module header for the install order, then search for the procedure you care about.

### 4. The peripherals

Once you've read the evaluator, the remaining files are small and self-contained:

[`src/io.rs`](src/io.rs) — port I/O. Same flat-catalog shape as `builtins.rs`.

[`src/library.rs`](src/library.rs) — `define-library`, `import` (with `only`/`except`/`prefix`/`rename`), `cond-expand`, and the **filesystem loader**: an `(import (foo bar))` that isn't built in is resolved to `foo/bar.sld` on a search path. This is how the 21 R7RS-large libraries under [`lib/`](lib/) get loaded on demand.

[`src/main.rs`](src/main.rs) — the CLI frontend. The thin binary that consumes the library above. Wraps an interactive REPL with `rustyline` for line editing.

## Suggested reading orders

### "I want to understand how interpreters work"

Pure architecture-first.

1. `src/value.rs` — what is a Scheme value?
2. `src/lex.rs` and `src/parse.rs` — how does text become values?
3. `src/env.rs` — how do variables resolve?
4. `src/eval.rs` — read the module header, then [`docs/0001-tree-walking-interpreter.md`](docs/0001-tree-walking-interpreter.md), then the file in the order recommended above.
5. [`docs/0004-continuations.md`](docs/0004-continuations.md) and [`docs/0005-exception-handling.md`](docs/0005-exception-handling.md) — both fall out of the step-loop architecture.

At this point you understand what a tree-walking interpreter is.

### "I want to understand Scheme"

Concept-first. Use the ADRs as your concept-by-concept walkthrough; dip into the source where they cite it.

1. [`docs/0002-numeric-tower.md`](docs/0002-numeric-tower.md) — what makes Scheme numbers different from most languages.
2. [`docs/0003-syntax-rules-hygiene.md`](docs/0003-syntax-rules-hygiene.md) and [`docs/0008-hygiene-scope-and-syntaxref.md`](docs/0008-hygiene-scope-and-syntaxref.md) — pattern-based macros and hygiene (0008 is how hygiene grew past plain alpha-renaming).
3. [`docs/0004-continuations.md`](docs/0004-continuations.md) — first-class continuations.
4. [`docs/0005-exception-handling.md`](docs/0005-exception-handling.md) — how exceptions live on the same frame stack.
5. [`docs/0006-library-system.md`](docs/0006-library-system.md) and [`docs/0007-filesystem-library-loader.md`](docs/0007-filesystem-library-loader.md) — modules, and how the R7RS-large libraries load from disk.

Then read R7RS-small itself for the bits the ADRs don't cover (control flow, basic types, the standard procedures in §6).

### "I want to learn Rust patterns from a real codebase"

Pattern-first. nscheme uses a deliberately small slice of Rust — almost no traits, modest generics, no async, no `unsafe`. What it does use, it uses repeatedly and in the open. Each file below introduces specific idioms at their canonical first-appearance site; subsequent uses elsewhere are deliberately terse.

1. [`src/lib.rs`](src/lib.rs) — crate organization. Library-plus-binary layout, explicit `pub mod` declarations, the discipline of keeping all language behavior in the library and the CLI thin.
2. [`src/value.rs`](src/value.rs) — the densest Rust-idiom file in the project. In one read you see:
   - The closed-enum-with-exhaustive-matching pattern (`Value`, `Procedure`, `Port`) and why it's worth more than dynamic dispatch when the alternatives are known.
   - `Rc<RefCell<T>>` for shared mutable state (introduced at `Pair`).
   - `thread_local!` plus `Rc<str>` for a singleton interner without `static mut` or a lock.
   - `thiserror` for ergonomic error enums.
   - `Hash` by pointer identity (used by interned symbols for `HashMap` key efficiency).
   - Manual `Display` and `Debug` impls, with `Debug` reused as Scheme's `write` and `Display` as `display`.
   - The orphan rule worked around with a newtype wrapper (`WriteShared`).
3. [`src/env.rs`](src/env.rs) — small enough to read in five minutes; reinforces the `Rc<RefCell<T>>` pattern from `value.rs` in a different shape (`HashMap` interior).
4. [`src/lex.rs`](src/lex.rs) — a hand-written state-machine parser. `Span`-bearing errors. The `#[error("…")]` strings in `LexError` show how `thiserror` handles formatting.
5. [`src/parse.rs`](src/parse.rs) — error composition via `#[from] LexError` (a `?` chain that propagates lex errors out as parse errors). The placeholder-pair trick for cyclic literals is also a nice example of `RefCell::borrow_mut` doing real work.
6. [`src/eval.rs`](src/eval.rs) — the architecturally interesting Rust file:
   - A state machine encoded as an enum (`Step`) plus a `Vec<Frame>` driving a `loop`.
   - The companion pattern of "closed enum + dispatch function": `step_eval` over syntactic forms, `step_apply` over procedure kinds, `resume` over frame kinds. Three big match tables, all clean.
   - The first-class continuation is a `frames.clone()`; you can see how cheap that is in Rust once everything heap-allocated is behind `Rc`.
7. [`src/macros.rs`](src/macros.rs) — newtypes for distinguishing same-named things (`VarKey { name, scope }`). When `HashMap<Symbol, _>` isn't precise enough, you wrap.
8. [`src/builtins.rs`](src/builtins.rs) — `fn` pointers as a registration mechanism. A flat catalog of `define(env, "name", arity, |args| { … })` calls; no trait objects, no boxing, no virtual dispatch.

After this, you've seen what idiomatic Rust looks like in a ~14,000-line single-author project. The style is conservative; what you don't see — heavy generics, traits everywhere, async — is itself a design statement worth noticing.

### "I want to extend nscheme"

Goal-first.

- **Add a new primitive procedure**: read `src/builtins.rs` and pick an existing primitive with a similar shape. Copy its `define(env, "name", arity, |args| { … })` pattern.
- **Add a new special form**: read `src/eval.rs`'s `step_eval` dispatcher, then pick a small existing `step_X_form` helper as a template. Add an arm to the dispatcher's match and a sibling `step_yourform` function.
- **Implement a new library**: drop a `.sld` file under [`lib/`](lib/) (e.g. `lib/scheme/foo.sld` or `lib/srfi/N.sld`) and the filesystem loader in `src/library.rs` will find it on `(import (scheme foo))`. Mirror an existing one — the 21 R7RS-large libraries there (Red + Tangerine editions) are worked examples.
- **Change the evaluator architecture** (e.g. compile to bytecode): read [`docs/0001-tree-walking-interpreter.md`](docs/0001-tree-walking-interpreter.md) first; the architectural-decision record explains what choosing a tree-walking step-loop bought us and what a bytecode VM would change.

The `bd` issue tracker (`bd ready`) lists all open work with rationale.

## In-code commentary

Every source file has an annotated header explaining what it teaches. Within the file, you'll find two kinds of inline notes:

- **Rust note**: a Rust idiom being introduced for the first time in the codebase. Each pattern is explained once at its canonical first-appearance site; subsequent occurrences stay terse. The conventions are in [`docs/STYLE.md`](docs/STYLE.md).
- **Scheme note**: a Scheme concept being introduced for the first time. Same rule — explained once, then trusted.

If you find yourself confused by a Rust idiom or a Scheme concept, search the codebase for the same idiom — the first occurrence will have the explanation.

## Architecture decision records

The `docs/000N-*.md` files are the ADRs — durable records of the load-bearing design decisions. They are not tutorials; they are the historical "why we built it this way" record. The TOUR cites them where relevant; you can also read them as a sequence:

- [0001 — Tree-walking interpreter with explicit step-loop](docs/0001-tree-walking-interpreter.md)
- [0002 — Numeric tower](docs/0002-numeric-tower.md)
- [0003 — `syntax-rules` hygiene](docs/0003-syntax-rules-hygiene.md)
- [0004 — Continuations as cloned frame stacks](docs/0004-continuations.md)
- [0005 — Exception handling on the frame stack](docs/0005-exception-handling.md)
- [0006 — Library / module system](docs/0006-library-system.md)
- [0007 — Filesystem-loaded libraries for R7RS-large](docs/0007-filesystem-library-loader.md)
- [0008 — Hygiene beyond alpha-renaming](docs/0008-hygiene-scope-and-syntaxref.md) (supersedes 0003's mechanism)
- [0009 — First-class control procedures: `apply` / `eval` / `load`](docs/0009-first-class-control-procedures.md)
- [0010 — Current ports as parameters](docs/0010-current-ports-as-parameters.md)

## Test corpus

[`tests/r7rs-corpus/`](tests/r7rs-corpus/) holds the chibi-scheme conformance suite — 1180 top-level forms running 1225 test assertions, all of which pass. The corpus file (`chibi-r7rs-tests.scm`) is vendored verbatim from upstream; only our test-framework shim (`chibi-test-shim.scm`) is local. Run with:

```bash
cargo test --test r7rs_chibi -- --nocapture
```

It's the most direct way to confirm that a change you make hasn't broken something for the R7RS-small core.

For the R7RS-large libraries, [`tests/conformance.rs`](tests/conformance.rs) runs each library's *actual upstream SRFI reference suite* (vendored under [`tests/r7rs-large-corpus/`](tests/r7rs-large-corpus/)) — 18 of 21 libraries, ~5,400 verbatim assertions. [`docs/CONFORMANCE.md`](docs/CONFORMANCE.md) is the map of which suite covers which library and what each one surfaced. And [`docs/showcase/`](docs/showcase/) is a dozen short programs that exercise those libraries in anger — a good way to get a feel for what they offer.

## Where to go next

After you've worked through this tour, the natural next steps are:

- Pick an open bead (`bd ready`) and implement it. The beads are small enough to be self-contained, and each lists what files it touches and what it'll teach you about the codebase.
- Read the chibi corpus directly. It's the best summary of what R7RS-small actually demands of an implementation.
- Read R7RS-small itself (small.r7rs.org). 88 pages. The spec is unusually pleasant.

If you got this far and the tour helped, that's the point. If you got this far and it didn't, the conventions in [`docs/STYLE.md`](docs/STYLE.md) tell you what each comment in the code is *trying* to do; the failure to land is something we want to know about.
