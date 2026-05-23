# ADR 0003 — Hygienic `syntax-rules` via alpha-renaming

**Status:** Accepted (2026-05-21)
**Bead:** [nscheme-2p2](../.beads/issues.jsonl)
**Related:** [0001 — Tree-walking interpreter](0001-tree-walking-interpreter.md)

## Context

R7RS-small §4.3 defines `syntax-rules`, a hygienic pattern-matching
macro system. "Hygienic" means a macro can introduce its own local
identifiers without colliding with identifiers the user supplied.
The canonical test:

```scheme
(define-syntax swap!
  (syntax-rules ()
    ((_ a b) (let ((tmp a)) (set! a b) (set! b tmp)))))
(define tmp 1)
(define x   2)
(swap! tmp x)
```

A non-hygienic implementation expands the inner `tmp` to capture the
user's `tmp`, producing the wrong result. R7RS requires hygiene.

Three textbook approaches exist:

1. **Alpha-renaming (KFFD)** — gensym every template-introduced
   identifier; substitute pattern variables from input. ~200 lines.
2. **Syntactic closures** — associate each piece of syntax with its
   defining env; resolve references through the carried env.
3. **Sets of scopes (Flatt 2016)** — Racket's modern approach;
   thorough but heavy.

## Decision

We use **alpha-renaming** for nscheme v1. Implementation lives in
[`src/macros.rs`](../src/macros.rs).

### Algorithm

1. Parse `(syntax-rules (LITERALS...) CLAUSES...)` once at
   `define-syntax` time into a `SyntaxRules { literals, clauses }`
   struct.
2. At each call site, walk clauses in order. For each clause:
   1. **Pattern-match** the input against the pattern, collecting a
      `Bindings` map (pattern variable → matched value, or sequence
      of values under `...`).
   2. **Collect binders**: walk the template syntactically, gathering
      identifiers that appear in *binding positions* (`lambda`
      formals, `let`/`let*`/`letrec` LHS, `define` LHS, `do` vars).
   3. **Gensym the binders**: assign each a fresh `name#N` symbol.
   4. **Instantiate** the template by walking it: substitute pattern
      variables from `Bindings`, rename binders per the gensym map,
      and splice `...` patterns.
3. Return the expansion via `Step::Eval(expanded, env)` — re-evaluating
   through the loop lets nested macros expand naturally.

### Integration

Macros are first-class values: `Value::Macro(Rc<SyntaxRules>)`. The
evaluator detects them in `step_eval` after the special-form table:
when `(head args...)` and `head` looks up to a `Macro`, expand and
re-eval. This means macros share the regular `Env` mechanism;
`define-syntax`, `let-syntax`, `letrec-syntax` all just call
`env.define(name, Value::Macro(...))`.

## Consequences

### Positive

- The four canonical hygiene tests (swap!, my-let*, literals,
  underscore-as-wildcard) all pass.
- Macros integrate with `Env` so shadowing works: a local
  `let-syntax` can shadow a global macro.
- Expansion re-runs through `step_eval`, so a macro that expands to a
  macro call expands transitively without special handling.

### Negative

- Hygiene is alpha-rename-only, *not* true definition-site
  resolution. If a user does:
  ```scheme
  (define-syntax m (syntax-rules () ((_ x) (+ x 1))))
  (let ((+ -)) (m 5))
  ```
  most R7RS-correct implementations return `6` (the `+` in the
  template is the global `+`, captured at definition time). We
  return `4` because free identifiers in the template use the
  *call-site* env. Fixing this needs syntactic closures or
  sets-of-scopes — out of v1 scope.
- Nested ellipsis depth >1 isn't supported.
- The custom-ellipsis form `(syntax-rules ... () ...)` isn't
  supported.

### Why these limits are acceptable for v1

Definition-site capture matters for libraries that build macros over
specific primitives. For programs that just want `(when …)`,
`(unless …)`, `(swap! …)`-style syntactic sugar, alpha-renaming
catches the dangerous cases. The follow-up bead would replace this
module wholesale with a proper expander, not patch around the edges.
