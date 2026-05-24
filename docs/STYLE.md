# Teaching-comment style guide

How to write comments in nscheme so a reader who is strong at programming but new to *both* Rust and Scheme can learn the language from the code.

This guide is descriptive, not prescriptive — it captures the pattern that emerged when the style was applied to [`src/value.rs`](../src/value.rs) and the architectural core of [`src/eval.rs`](../src/eval.rs). Use those two files as exhibits.

## The audience

A reader who:

- Has shipped production software in some language (TypeScript, Go, Python, Java, …).
- Has used `Result` / `Option` / Vec / HashMap in some language even if not Rust.
- Has *not* written a language implementation before.
- Has *not* used Rust meaningfully — knows the headline features but is probably uncertain about lifetimes, interior mutability, the orphan rule, declarative macros.
- Has *not* written serious Scheme — knows it's Lisp-with-parens, may have heard of `call/cc`, has not built mental models for hygienic macros or the numeric tower.

We are not writing for Rust experts and we are not writing for Scheme experts. Either group has plenty of material elsewhere.

## The five categories of teaching comment

A teaching comment lands in one of five buckets. Don't add comments that don't fit a bucket; don't pad a bucket that doesn't need padding.

### 1. File-level header

Every source file gets a `//!` module doc that contains:

- **One-sentence summary** of what the file is.
- **"What you'll learn here"** — a short bullet list of the concepts this file teaches. Two or three Scheme concepts, two or three Rust patterns. Be specific: "the `Rc<RefCell<T>>` pattern" beats "interior mutability."
- **"Read alongside"** — links to ADRs, R7RS sections, and other modules that share context with this one.
- **Internal reading order** (for big files) — for a multi-concern file like `eval.rs`, name the order in which the items should be read. The reader will follow your suggestion.

Exhibit: the top of [`src/value.rs`](../src/value.rs).

### 2. Section narrative

The file is broken into sections by `// --- Section ---` dividers. Each divider gets a short prose intro: two to six sentences explaining what this section is *for*, what Scheme concept or Rust pattern it embodies, and what to look at to dig deeper.

Don't restate what the divider already says — *add* something the type signatures alone can't tell you.

Exhibit: the `Equality` and `Display / Debug` sections in `value.rs`. The first introduces the three R7RS equality predicates and their layering; the second explains the write-vs-display distinction and the datum-label algorithm.

### 3. First-appearance idiom notes

When a Rust pattern shows up for the first time in the codebase, add a "Rust note" at the appearance site explaining the pattern and why it's the right choice here. Subsequent appearances stay terse.

Examples (the canonical first-appearance sites are noted; do not re-explain at later sites):

- `Rc<RefCell<T>>` — `value.rs`, the `Pair` section.
- `thiserror::Error` — `value.rs`, the `Errors` section.
- `thread_local!` — `value.rs`, the `Symbol interning` section.
- Closed-enum exhaustiveness as a discipline — `value.rs`, the `Value` section.
- Pointer-equality `Hash` — `value.rs`, the `Symbol` impls.
- The step-loop architecture itself — `eval.rs`, the `eval()` function.

The voice: *"Rust note: here's the pattern, here's why, here's where it shows up again."* Don't be evangelical, just declarative.

### 4. Scheme-concept notes

The mirror of (3) for Scheme. When a Scheme concept first appears, add a "Scheme note" explaining the concept at the level a Lisp-curious programmer can absorb.

Examples (canonical first-appearance sites):

- `eq?` / `eqv?` / `equal?` — `value.rs`, the `Equality` section.
- Pair-and-list duality — `value.rs`, the `Pair` section.
- Symbol identity vs. string content — `value.rs`, the `Symbol interning` section.
- The numeric tower's exactness rule — `value.rs`, the `ComplexValue` and `Value::Int/BigInt/Rational/Float` items.
- `write` vs. `display` and datum labels — `value.rs`, the `Display / Debug` section.
- Tail positions and `call/cc` — `eval.rs`, the module header.
- Special-form dispatch — `eval.rs`, the `Step` section.

The voice: *"Scheme note: here's the concept, here's the relevant R7RS section, here's the file that implements it."*

### 5. Cross-references

When a section's full story lives somewhere else, link to it explicitly:

- ADRs in `docs/0001-…0006-…md` for architectural decisions.
- R7RS section numbers for spec questions.
- Sibling modules for code-level context.

Cross-references go inline in the relevant comment, not in a separate "See also" block at the end. The reader is reading a function; they want to know where to look *now*, not after they're done.

## Voice rules

- **First-person plural where appropriate.** "We use `Rc<str>` rather than `Rc<String>` because …" — the project is a single voice talking to one reader.
- **Why, not what.** Don't comment that `let mut x = Vec::new()` "creates an empty vector." Do comment that the vector is the continuation stack and that `frames.clone()` is the cheap-by-design way `call/cc` captures it.
- **Tight, not chatty.** Five lines is more useful than fifty. The reader is here to learn; their reading time is the cost.
- **Don't restate the spec.** Don't reproduce R7RS in comment form. Cite the section number and trust the reader to look it up.
- **Don't restate the code.** If the next three lines obviously do X, don't comment "do X."
- **No ASCII art.** No emoji.
- **Hard limit: no comment makes the reader scroll past the thing it documents.** If your block dwarfs its target, move the explanation to a doc file and cite it.

## What does *not* get a comment

- Self-explanatory boilerplate (`derive(Clone)`, getters).
- Code that the type signature fully explains.
- "Obvious" idioms after their first appearance — second `Rc<RefCell<T>>` site doesn't re-explain.
- Lint-suppression `#[allow(…)]` already attached to its target.
- TODOs, except when filed beads exist and you cite the bead ID.

## Structural discipline

Two rules that should hold across the codebase:

1. **Top-down within a file.** Public API at the top, callers above their callees. The reader meets the most general thing first and descends into details. The exception is the rare helper used by many sites — those can sit at the bottom alongside other "library-style" utilities.

2. **One concept per file.** The test: if you'd introduce the file to a new reader as "this teaches X *and* Y *and* Z," it should probably be split. If you'd introduce it as "this teaches X — with sub-concerns A, B, C — it stays as one file. The two extremes in this repo illustrate the cut: `eval.rs` is split because the step-loop architecture, the catalog of `Frame`s, and the per-special-form dispatch are three distinct concerns; `builtins.rs` stays as one file because it is a flat catalog of primitives that all do the same kind of thing.

## A note on what this guide does not cover

This is the in-code teaching layer. The companion piece is `TOUR.md` at the repo root — a guided reading of the whole codebase with suggested reading orders. The two reinforce each other: TOUR.md tells the reader *which* file to open and why; the in-code comments tell them what they're looking at once they're there.
