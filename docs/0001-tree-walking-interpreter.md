# ADR 0001 — Tree-walking interpreter with explicit step-loop

**Status:** Accepted (2026-05-21) **Bead:** [nscheme-6g4](../.beads/issues.jsonl) **Supersedes:** —

## Context

`nscheme` is an R7RS-small Scheme interpreter written in Rust. The single biggest up-front architectural decision is the shape of the evaluator. The realistic options are:

1. **Recursive tree-walking** — `fn eval(expr, env) -> Value`, recursing on sub-expressions via Rust function calls.
2. **Iterative tree-walking with explicit continuation frames** — `eval` is a `loop { ... }` that dispatches on an `EvalStep` enum, with pending work represented as a `Vec<Frame>`.
3. **Bytecode VM** — compile the AST to a custom instruction stream and execute via a fetch-decode-dispatch loop.
4. **Threaded code / direct threading** — a variation of (3) that uses computed-gotos or function pointers.

R7RS-small mandates two language features that interact violently with shape (1):

- **Proper tail calls** (R7RS §3.5) — Scheme guarantees that tail calls must not grow the call stack. A `(define (loop n) (if (= n 0) 'done (loop (- n 1))))` with `n = 10^6` must complete without overflow.
- **First-class continuations** (`call-with-current-continuation`, R7RS §6.10) — the evaluator must be able to *capture* its remaining work as a reified value, then later resume from it (possibly more than once).

Both features are *not* feature additions to an evaluator. They are properties of the evaluator's overall shape. Adding them late means rewriting the spine.

## Decision

**We choose option (2): an iterative tree-walking evaluator with explicit continuation frames.**

Concretely:

```rust
enum Step {
    Eval(Datum, EnvRef),    // evaluate this expression in this env
    Apply(Value, Vec<Value>), // apply this procedure to these args
    Return(Value),          // a sub-expression produced this value
}

enum Frame {
    // Pending: evaluate the rest of (begin e1 e2 e3) after e1 completes
    Begin { rest: Vec<Datum>, env: EnvRef },

    // Pending: choose between consequent/alternate based on the value
    If { consequent: Datum, alternate: Option<Datum>, env: EnvRef },

    // Pending: evaluate next argument, then apply
    ArgEval { proc: Value, evaluated: Vec<Value>, remaining: Vec<Datum>, env: EnvRef },

    // Pending: bind name to value, then continue with body
    Define { name: Symbol, env: EnvRef },

    // ... and so on per special form
}

fn eval(expr: Datum, env: EnvRef) -> Value {
    let mut step = Step::Eval(expr, env);
    let mut frames: Vec<Frame> = Vec::new();
    loop {
        step = match step {
            Step::Eval(d, e)    => step_eval(d, e, &mut frames),
            Step::Apply(p, a)   => step_apply(p, a, &mut frames),
            Step::Return(v)     => match frames.pop() {
                Some(frame) => resume(frame, v, &mut frames),
                None => return v,
            },
        };
    }
}
```

### Why this shape

**Tail calls become free.** When `step_eval` enters a tail position (the last expression of a `begin`, the chosen branch of an `if`, the body of a `lambda` applied via `Apply`), it transitions to `Step::Eval(tail_expr, env)` *without pushing a frame*. The Rust stack does not grow. R7RS §3.5 tail positions are simply the cases where we don't push a new frame — they're handled by omission, not by a separate mechanism.

**`call/cc` becomes a clone.** A continuation, in this representation, is exactly `Vec<Frame>` plus the current `Step`. `call-with-current-continuation` clones the frame stack into a `Continuation` value; invoking that continuation replaces the current frame stack with the cloned one and resumes with the supplied value. The hard work of "what is a continuation" is already done structurally.

**`dynamic-wind` becomes a frame type.** The before/after thunks are just extra `Frame` variants. The standard "unwind to common ancestor" semantics of dynamic-wind on continuation invocation falls out by walking the frame vectors.

**Exception handling becomes a frame search.** `with-exception-handler` installs a `Frame::ExceptionHandler` frame; `raise` walks the frame stack to find the nearest handler. `guard` is sugar over the same mechanism.

### Why not a bytecode VM

A bytecode VM is the right choice for a *production* Scheme. For `nscheme` v1, the goals are correctness, conformance, and clarity. A tree-walking evaluator:

- has a more direct correspondence to the semantic rules in the report;
- needs no separate compilation phase;
- is easier to step-debug;
- can be replaced wholesale later — the public library API (`parse`, `eval`, `Value`) does not depend on the evaluation strategy.

If/when performance becomes a real concern, a bytecode VM can be added as an alternate evaluation backend without breaking the public surface.

### Why not pure recursive eval

The advisor reviewing the initial plan flagged this directly: writing `eval` as recursive Rust calls would force a complete rewrite when implementing TCO (bead nscheme-6lm) and again for `call/cc` (bead nscheme-0xn). The work to do the explicit-loop shape *now* is the same work in either case; doing it later requires also undoing the recursive version.

## Consequences

### Positive

- TCO and `call/cc` are structural, not bolt-ons.
- `dynamic-wind` and exception handling slot into the same `Frame` enum.
- Public library API is stable across future evaluator backends.
- Stepping/debugging is easier (each loop iteration is a discrete step).

### Negative

- Slightly more verbose than recursive `eval` for the simple cases.
- Each special form requires a corresponding `Frame` variant.
- Performance is bounded by tree-walking — no JIT, no inline caches. Acceptable for v1; revisit only if a real workload demands it.

### Open questions deferred to later beads

- **Hygiene in `syntax-rules`** (T15, nscheme-2p2): the standard alpha-renaming approach (gensym all introduced identifiers) interacts with how environments are looked up. Decide when T15 starts.
- **Numeric tower representation** (T12, nscheme-c92): when to promote `i64` → `BigInt` → `BigRational` → `f64`. Decided in T12's design.
- **Performance** — explicitly out of scope for v1. No premature optimization.

## Related decisions

- Value representation: `Rc<RefCell<...>>` with documented cycle leak. See bead nscheme-aof's design field.
- Single Cargo crate with `lib.rs` + `main.rs` (no workspace for v1). See bead nscheme-khs notes.
