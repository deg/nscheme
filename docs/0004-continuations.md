# ADR 0004 — Continuations as cloned frame stacks

**Status:** Accepted (2026-05-21) **Bead:** [nscheme-0xn](../.beads/issues.jsonl) **Related:** [0001 — Tree-walking interpreter](0001-tree-walking-interpreter.md)

## Context

R7RS-small §6.10 requires first-class continuations via `call-with-current-continuation` (a.k.a. `call/cc`). A captured continuation is a *value* representing "the rest of the computation that was waiting for `call/cc`'s result." Invoking the continuation later — possibly many times, possibly long after `call/cc` returned — resumes that computation with the value passed to it.

This is the form that breaks most amateur interpreters. The interpreter has to have a representation for "the rest of the computation" that can be:

1. Captured cheaply at the call/cc site.
2. Reified as a runtime value (boxable, storable, passable).
3. Re-entered later, possibly from a completely different control context.

## Decision

A continuation is exactly the evaluator's frame stack at the moment of capture.

```rust
Procedure::Continuation { frames: Vec<Frame> }
```

`Frame` is `#[derive(Clone)]`. Capturing a continuation is a `frames.clone()` — fast, O(stack-depth) memory.

### Capture

`call/cc` is a special form (not a primitive — primitives can't read the eval state). At capture time:

```rust
let saved = frames.clone();  // snapshot taken BEFORE we set up our own work
let cont = Value::Procedure(Rc::new(Procedure::Continuation { frames: saved }));
frames.push(Frame::CallOp { args: vec![cont], env });
Step::Eval(proc_expr, env)
```

The snapshot is taken *before* `call/cc` pushes its own `CallOp` so the continuation represents what was going to happen *after* `call/cc` produced a value — not how `call/cc` was about to set itself up.

### Invocation

`step_apply` recognizes `Procedure::Continuation` and emits a special step:

```rust
Step::InvokeContinuation(saved, value)
```

The eval loop handles this by *replacing* the live frame stack with the saved one and then transitioning to `Step::Return(value)`:

```rust
Step::InvokeContinuation(saved, value) => {
    frames = saved;
    Step::Return(value)
}
```

This is the entire mechanism. Replacing the frame stack means the next iteration's `Step::Return` pops the next frame from the *saved* stack, which is exactly the original pending work.

### Multi-shot

Because we capture by `.clone()`, the saved stack is independent of the live stack. Invoking the continuation twice clones the saved state again each time (`Procedure::Continuation` is stored behind `Rc`, so the `Vec<Frame>` is shared until the eval loop assigns it into the live `frames` variable, at which point Rust would clone-on- borrow… in our implementation we `clone()` explicitly when emitting `InvokeContinuation` from `step_apply`).

### Top-level continuity

For continuations captured at the top level to behave per R7RS, all top-level forms must share a single `eval()` invocation. We accomplish this in `eval_source` by wrapping the parsed datums in an implicit `(begin …)` and evaluating the whole thing. Without this, each top-level form would have its own local `frames` vector and a continuation captured in one form couldn't jump into another.

## Consequences

### Positive

- `call/cc` is ~30 lines of new code on top of the existing evaluator. The architecture from ADR 0001 was the load-bearing decision; this is just the surface.
- Multi-shot continuations work for free.
- Mutual recursion across continuation jumps works because closures carry their `EnvRef` by `Rc` — re-entering an old continuation re-enters the right environment.
- `dynamic-wind` and exception handlers slot into the same `Frame` enum; see [0005](0005-exception-handling.md).

### Negative

- A captured continuation holds all the `Frame` storage indefinitely. For long-lived continuations that's a memory cost (no GC).

### Dynamic-wind interaction

`dynamic-wind` *is* wired into continuation invocation (R7RS §6.10): the body thunk runs under a `Frame::DynamicWind { id, before, after }` marker, so the saved continuation captures which wind extents were active. When a continuation is invoked, the eval loop diffs the current wind chain against the saved one, finds the longest common prefix, and schedules `after` thunks (innermost first) and `before` thunks (outermost first) via a `Frame::WindJump` before installing the saved frames and returning the supplied value.

### Open follow-ups

- Delimited continuations (`shift`/`reset`) — not R7RS but useful.
