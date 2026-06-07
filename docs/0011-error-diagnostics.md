# ADR 0011 — Error diagnostics: source locations and backtraces

**Status:** Accepted (2026-06-07) **Bead:** [nscheme-tn3](../.beads/issues.jsonl) **Related:** [0001 — Tree-walking interpreter](0001-tree-walking-interpreter.md)

## Context

Errors told the user *what* went wrong (`undefined variable: foo`) but not *where*. The lexer and parser already carried byte [`Span`]s, but nothing threaded location through evaluation, and there was no notion of a call chain. This is bead `nscheme-tn3` — flagged as a "big change" because the naive approach (a `Span` on every `Value::Pair`, preserved through macro expansion) touches the parser, the macro expander, the value model, and the evaluator at once.

## Decision

A [`diagnostic`](../src/diagnostic.rs) module, surfaced by the CLI, built in three layers that avoid the invasive rewrite.

### 1. Located lex/parse errors

`LexError` / `ParseError` gained `span()` accessors; `diagnostic::locate(source, span)` renders a rustc-style block (`at line L:C`, the source line, a caret). `main.rs` prints it for parse/lex errors.

### 2. Runtime source locations via a pointer-keyed span sidecar

Rather than add a field to `Pair` (which would churn every construction), the parser records each list's span in a thread-local map keyed by the head pair's pointer identity (`Rc::as_ptr`). The evaluator's `step_eval`, on entering a pair, looks up that span and sets a thread-local "current form" (`note_current`). At error time, the current form *is* the location. The map and current form reset per `eval_source` (`begin_source`).

Macro-expanded forms that rebuild pairs have no recorded span, so they fall back to the nearest enclosing recorded form (typically the macro call site). Special forms preserve spans, since they evaluate the original parsed pairs.

### 3. A TCO-safe call-chain backtrace

At every error exit of the `eval` loop (a small `try_step!` macro around `step_eval`/`step_apply`/`resume`, plus the uncaught-raise arm), `capture_backtrace` snapshots the procedures of the pending `CallArg` frames (innermost first) into a thread-local. This is **read-only** — it adds no frames — so it cannot regress the verified TCO invariant ([0001], `nscheme-8g2`): tail calls leave no `CallArg`, so a tail-recursive loop yields a correctly *empty* trace.

### Surfacing without touching `EvalError`

Diagnostics ride thread-locals read by `main.rs::report_error`. `EvalError` is unchanged, so its ~10 `Raised(_)` match sites (tests, the `error` helper) need no churn, and embedders still get a clean error type — opting into diagnostics via the `diagnostic` API and `eval::take_backtrace` if they want.

## Consequences

### Positive

- Parse *and* runtime errors show `line:col` + a caret at the offending form; non-tail call chains show a "called from" trace.
- The location compensates where the backtrace can't: tail-structured code has an empty trace but still pins the failing form.
- No `Value`/`EvalError` churn; the core eval loop changed only by wrapping its three error-producing calls.

### Negative / limitations

- **Macro fidelity:** forms built fresh by a macro have no span and fall back to the call site. Full fidelity needs span-preserving macro expansion (a follow-up).
- **Tail calls are invisible to the backtrace** — inherent to proper TCO; the source location is the compensation.
- **Overhead:** `note_current` does a map lookup per evaluated pair. Acceptable for a tree-walker; a bytecode backend (`nscheme-6mp`) would fold spans into the instruction stream instead.
- **Thread-local side channel:** diagnostics surface in the CLI, not in the `EvalError` value. A deliberate trade to keep the programmatic API and tests stable.
