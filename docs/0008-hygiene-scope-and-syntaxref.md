# ADR 0008 — Hygiene beyond alpha-renaming: def-site `SyntaxRef` + per-expansion scope

**Status:** Accepted (2026-06-04) **Bead:** [nscheme-d6o](../.beads/issues.jsonl) **Supersedes (in part):** [0003 — `syntax-rules` hygiene via alpha-renaming](0003-syntax-rules-hygiene.md)

## Context

ADR 0003 implemented `syntax-rules` hygiene by **alpha-renaming**: gensym every identifier the template introduces into a core binding position (`let`/`lambda`/`define` LHS). It passed the canonical four hygiene tests but documented two real limitations:

1. **No definition-site resolution.** A template's *free* identifiers resolved in the call-site env, so
   ```scheme
   (define-syntax m (syntax-rules () ((_ x) (+ x 1))))
   (let ((+ -)) (m 5))
   ```
   returned **4** (call-site `+` = `-`) where R7RS requires **6** (the template's `+` is the global `+` captured at definition time).

2. **Template binders that only become binders downstream.** Alpha-renaming only renames identifiers in *core* binding positions. When a template-introduced identifier instead becomes a binder via a *downstream* macro — the recursive tree-matcher in SRFI 146 (`mapping` delete-min), `stream-let` in SRFI 41 — two expansions' same-named identifiers collided. These were live bugs blocking the R7RS-large libraries, not theoretical corners.

Both had to be fixed to ship the libraries. The question was whether to jump straight to Racket-style full sets-of-scopes (Flatt 2016) or add the minimum that resolves these cases.

## Decision

Keep alpha-renaming and layer two more mechanisms on top, giving a **three-part hybrid** in `macros.rs`. Implementation: `Value::SyntaxRef` (in `value.rs`) plus `VarKey { name, scope }` keying.

1. **Gensym for core binders** (unchanged from ADR 0003) — handles the classic `swap!` case.

2. **`SyntaxRef` carries the definition-site env.** A template's free identifier is wrapped in a `Value::SyntaxRef` holding the macro's def-site env; the evaluator resolves it there, not at the call site. This is the syntactic-closures idea applied narrowly to free template identifiers, and it makes the `(let ((+ -)) (m 5))` example return **6**.

3. **A per-expansion `scope` on each `SyntaxRef`.** Every expansion stamps the identifiers it introduces with a scope unique to that expansion. When such an identifier later lands in a binding position via a downstream macro, the evaluator binds and resolves it under a hygienic name derived from `(name, scope)`, so two expansions stay disjoint. Pattern variables are likewise keyed by `VarKey { name, scope }` so a user-supplied `x` and a template-introduced `x` never alias.

## Consequences

### Positive

- The ADR 0003 call-site-capture limitation is gone: free template identifiers resolve at definition site (verified — the `+`/`-` example returns 6).
- The SRFI 146 (mapping) and SRFI 41 (stream) hygiene bugs are fixed, which unblocked those reference suites.
- It's an additive layering, not a rewrite — the alpha-renaming path and the existing `Env` integration (macros are `Value::Macro`, `let-syntax` shadows globals) are preserved.

### Negative

- This is **not** full sets-of-scopes. It resolves the cases that arose in practice via three cooperating mechanisms rather than one uniform scope-set model. Adversarial nesting that the three layers don't cover is possible; the complete Flatt algorithm remains future work and `nscheme-d6o` stays open to track it.
- Three interacting mechanisms are more to hold in your head than one. The `macros.rs` header documents how they divide the work; read it before touching the expander.
- Nested-ellipsis depth > 1 and custom-ellipsis `(syntax-rules ellipsis …)` are still unsupported (also from ADR 0003).

### Why this point on the spectrum

Shipping 21 libraries needed mechanisms (2) and (3); it did not need a from-scratch expander. A full sets-of-scopes rewrite is a large, self-contained change best done deliberately against a hygiene-specific test corpus — not smuggled in while chasing library bugs. This ADR records the intermediate, working state; the rewrite, if it happens, supersedes this one.
