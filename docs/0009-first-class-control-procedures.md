# ADR 0009 — First-class control procedures: `apply` / `eval` / `load`

**Status:** Accepted (2026-06-07) **Bead:** [nscheme-iii](../.beads/issues.jsonl) **Related:** [0001 — Tree-walking interpreter](0001-tree-walking-interpreter.md), [0007 — Filesystem library loader](0007-filesystem-library-loader.md)

## Context

`apply`, `eval`, and `load` were implemented as **special forms**, not procedures, for one structural reason: a primitive's signature is `fn(&[Value]) -> Result<Value, …>` (see [`PrimitiveFn`](../src/value.rs)). It receives only its evaluated arguments — it cannot reach the evaluator's control state (to schedule a tail call or evaluate a datum) nor the calling environment. `apply` needs to re-dispatch into the evaluator; `eval` needs to evaluate a datum in an environment; `load` needs to read a file and evaluate its forms.

But R7RS makes all three **procedures**. As special forms they were second-class: you could not write `(map (lambda (e) (eval e env)) forms)`, pass `apply` to a higher-order function, or `(apply apply …)`. That gap is bead `nscheme-iii`.

## Decision

Promote them to real `Procedure` values, dispatched in `step_apply` — the exact mechanism `Procedure::DynamicWindStart` already used for `dynamic-wind`. Three new variants in [`value.rs`](../src/value.rs):

```rust
Procedure::Apply,              // unit; re-dispatches
Procedure::Eval { env: EnvRef },
Procedure::Load { env: EnvRef },
```

Because they are procedures, their arguments arrive **already evaluated** when `step_apply` sees them, which makes each case small:

- **`Apply`** — splice the final argument (which must be a proper list) onto the leading args and return `Step::Apply(proc, combined)`. Returning `Step::Apply` keeps the call in **tail position** (verified by a 100k-deep `(apply loop …)` in `tests/tail_calls.rs`).
- **`Eval { env }`** — return `Step::Eval(datum, env)`. The `datum` is already evaluated (it's the quoted/constructed code); `env` is the captured environment.
- **`Load { env }`** — resolve the filename, read the file, `eval_source` it in `env`, return unspecified.

`eval` and `load` capture the **install-time environment** as their interaction environment.

### Environment reification is deliberately out of scope

`eval`'s environment-specifier argument and `(null-environment …)` / `(scheme-report-environment …)` / `(environment …)` all resolve to the one interaction environment — nscheme does not reify environments as distinct first-class values. This matches the pre-existing behavior and is corpus-safe (the chibi suite calls `eval` only with self-contained expressions plus an explicit, ignored env spec). Reifying environments is a separate, larger change.

## Consequences

### Positive

- `apply` / `eval` / `load` are values: `(procedure? apply)` is `#t`, they pass through `map`/`fold`, and `(apply apply (list + '(1 2 3)))` works.
- TCO is preserved through `apply` (the conversion's main risk; tested).
- Arity and type errors raise **catchable** conditions (`guard` sees them), matching other procedures.
- The special-form table shrank, and the now-unreachable `ApplySpread` / `EvalAfter` frames and `step_apply_form` / `step_eval_form` / `step_load` helpers were removed.

### Negative

- No environment reification: `eval` cannot evaluate in a restricted or alternate environment (`null-environment` does not actually restrict bindings).
- Applying a non-procedure (e.g. `(apply 5 '(1))`) is still an *uncatchable* evaluator error rather than a catchable condition — pre-existing behavior for all application, not specific to `apply`.
