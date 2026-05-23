# ADR 0005 — Exception handling via the frame stack

**Status:** Accepted (2026-05-21)
**Bead:** [nscheme-76n](../.beads/issues.jsonl)
**Related:** [0001](0001-tree-walking-interpreter.md), [0004](0004-continuations.md)

## Context

R7RS-small §6.11 specifies an exception system:

- `raise expr` — propagate an exception. If a handler returns, its
  result is re-raised (the handler couldn't satisfy the request).
- `raise-continuable expr` — same propagation, but if the handler
  returns normally, its value substitutes for the `raise` expression
  (the handler *did* satisfy the request).
- `with-exception-handler handler thunk` — install `handler` for the
  dynamic extent of `(thunk)`.
- `guard` — structured handler form analogous to `try`/`catch`.

The two raise variants differ in one place: where the handler's
return value ends up.

## Decision

Exceptions ride the same frame stack as everything else. No
side-channel, no Rust panic semantics.

### Primitives

- `Step::Raise(Value, bool)` — the bool flags `raise-continuable`.
- `Frame::ExceptionHandler { handler, env }` — installed by
  `with-exception-handler`.
- `Frame::ReRaise` — helper that re-raises whatever value flows
  through it (used after a non-continuable handler returns).
- `Frame::RaiseAfter { continuable }` — pending raise after the
  operand of `raise` finishes evaluating.
- `Frame::InstallHandler { thunk_expr, env }` — helper that, after the
  handler expression evaluates, installs an `ExceptionHandler` frame
  and calls the thunk.

### Propagation algorithm

```
Step::Raise(value, continuable):
    popped := []
    handler := None
    while frames is non-empty:
        f := frames.pop()
        if f is ExceptionHandler { handler: h }:
            handler := Some(h)
            break
        popped.push(f)
    if handler is None:
        return Err(EvalError::Raised(value))
    if continuable:
        for f in popped.reverse(): frames.push(f)    # preserve frames
                                                     # ABOVE the handler
    else:
        frames.push(Frame::ReRaise)                  # re-raise handler's
                                                     # return value
    Step::Apply(handler, [value])
```

The continuable / non-continuable distinction is the only complexity.
For continuable, the frames between the raise site and the handler
are *preserved* so the handler's return value flows back to the
raise expression's position in the AST. For non-continuable, those
frames are discarded and a `ReRaise` frame ensures the handler's
return becomes the next raise.

### `guard` desugaring

`(guard (var clause...) body)` desugars to:

```scheme
(call/cc
  (lambda (k)
    (with-exception-handler
      (lambda (var) (k (cond clause... (else (raise var)))))
      (lambda () body))))
```

The escape continuation `k` jumps out of the handler's dynamic extent
before the cond's `else` clause re-raises — important because R7RS
requires that the handler is not active during its own invocation, so
a re-raise from inside the handler must reach the *next outer*
handler. Our handler frame is popped when the handler is invoked, so
that behavior is automatic.

### Error objects

`Value::ErrorObject(Rc<ErrorObject>)` carries `{ message, irritants,
kind }` where `kind` is one of `User`/`Read`/`File` (the last two tag
errors raised by the I/O subsystem). `error-object?`,
`error-object-message`, `error-object-irritants`, `read-error?`,
`file-error?` test/extract these.

The `error` procedure is defined in the Scheme bootstrap:

```scheme
(define (error msg . irritants)
  (raise (apply make-error-object msg irritants)))
```

## Errors from primitives become raises

R7RS §6.11 requires that exceptional conditions ("an error has been
detected") flow through the same handler mechanism as user-level
`raise`. So a built-in like `(/ 1 0)` or `(car '())` must be
catchable by `(guard …)` rather than escaping as a host-language
error.

`step_apply` for `Procedure::Primitive` converts any `RuntimeError`
the body returns into a `Step::Raise(error-object, continuable=false)`
via `runtime_error_to_value`. Two variants of `RuntimeError`,
`FileError` and `ReadError`, are tagged on conversion with
`ErrorKind::File` / `ErrorKind::Read` so the R7RS predicates
`file-error?` and `read-error?` discriminate them. Primitives that
need this routing (`delete-file`, `open-input-file`, `(read)`)
return those variants explicitly.

Arity mismatches at primitive call sites also use this path: they
construct a `RuntimeError::Arity` and route it through the same
raise machinery.

The same conversion applies inside `apply`'s splice when the last
argument isn't a proper list — that's a runtime condition, not a
compile-time malformation, so it's a raise rather than an
`EvalError::MalformedForm`.

## Consequences

### Positive

- Exception propagation reuses the frame mechanism — no new control
  pathway in the evaluator.
- Continuable vs non-continuable is a one-line difference (preserve
  vs discard the popped frames).
- `guard` is a desugaring, not a primitive — its behavior is exactly
  whatever `with-exception-handler` and `call/cc` do.
- Handlers are not active during their own invocation (since they're
  popped from the frame stack when invoked) — R7RS-correct.
- Built-in errors are catchable by user code, matching R7RS §6.11.

### Negative

- Uncaught raises produce `EvalError::Raised(value)`. Pretty-printing
  for end users could be richer.
- We don't currently distinguish the dynamic extent for nested
  `with-exception-handler` forms in error-trace reporting — there's
  no stack trace beyond what the host Rust panic gives. Could be
  improved with a backtrace mechanism wired through the eval loop.

### Open follow-ups

- Backtrace / source-span tracking for raised exceptions.
- `error-object-type` (R7RS has the predicate functions for `read`
  and `file` errors, but Schemes often add an arbitrary type
  hierarchy).
