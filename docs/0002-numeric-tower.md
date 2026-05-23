# ADR 0002 — R7RS exact numeric tower

**Status:** Accepted (2026-05-21)
**Bead:** [nscheme-c92](../.beads/issues.jsonl)
**Related:** [0001 — Tree-walking interpreter](0001-tree-walking-interpreter.md)

## Context

R7RS-small §6.2 defines a *numeric tower* with exact and inexact
representations and well-defined promotion rules. A faithful
implementation needs:

- Arbitrary-precision exact integers (so `(* 30 (fact 29))` doesn't
  silently wrap or lose digits).
- Exact rationals (so `(/ 1 3)` returns `1/3`, not a float
  approximation).
- Inexact reals (the IEEE-754 `f64`).
- Contagion rules: mixing exact and inexact produces inexact; mixed
  Int/BigInt/Rational stays exact.

The naïve "everything is `f64`" approach fails at `(fact 21)` and at
any exact-rational test. A v1 that only had `Int(i64) | Float(f64)`
would block too many R7RS forms.

## Decision

The `Value` enum carries four numeric variants:

```rust
Value::Int(i64)                  // fast path for small integers
Value::BigInt(Rc<BigInt>)        // exact integer beyond i64
Value::Rational(Rc<BigRational>) // exact rational in lowest terms
Value::Float(f64)                // inexact real
```

Arithmetic is driven by an internal `Num` enum in
[`src/builtins.rs`](../src/builtins.rs) that mirrors these four
variants, plus per-op promotion rules:

1. If either operand is `Float`, the result is `Float`.
2. Otherwise (both exact):
   - `Int + Int` → `Int`, falling back to `BigInt` on overflow.
   - Anything involving `BigInt` or `Rational` → promote both to
     `BigRational`, do the math, then normalize back.

Normalization always runs:

- `BigInt` whose value fits `i64` collapses to `Int`.
- `BigRational` with denominator `1` collapses through `BigInt` to
  `Int` if possible.

So users see the "smallest" canonical representation: `(/ 6 3)` is
`2` (an `Int`), not `2/1` (a `Rational`).

### Equality

- `eq?` uses pointer identity for `BigInt`/`Rational` (cheap), value
  equality for `Int`/`Float`.
- `eqv?` compares mathematical value within the exact tower (via
  `BigRational`); across exactness it's `#f` (so `(eqv? 1 1.0)` is
  `#f`).
- `equal?` falls through to `eqv?` for atoms.

### Parser

The lexer preserves number bodies verbatim and passes them to
`parse_number` which:

- Routes `a/b` to `BigRational`.
- Tries `i64` first for integers; falls back to `BigInt` on overflow.
- Routes decimals/exponents to `f64`.
- Honors `#e` / `#i` exactness prefixes (with `#e` on a decimal
  converting to an exact rational via `BigRational::from_f64`).

## Consequences

### Positive

- `(fact 50)` produces the exact 64-digit bignum.
- `(+ 1/2 1/3)` → `5/6` with no float drift.
- Comparisons like `(= 3 3.0)` work as R7RS specifies (cross-exactness
  goes via `f64`).
- The numeric helper centralizes promotion — adding a new arithmetic
  primitive is a small dispatch table update.

### Negative

- Three crates (`num-bigint`, `num-rational`, `num-traits`) joined the
  dependency tree. They compile in a few seconds and pull no native
  code.
- `BigRational` allocates per operation. Hot loops over integers stay
  fast because they ride the `Int(i64)` path; only overflow promotes.
- We don't yet recognize complex (`a+bi`, `a@b`) numbers — the lexer
  identifies them but the parser errors. R7RS-small doesn't require
  complex, but this gap is the obvious next addition.

### Open follow-ups

- Complex numbers.
- `number->string` with non-default radix for non-integers.
- `floor`/`ceiling`/`truncate`/`round`/`sqrt`/`exp`/`log` (most are
  trivial with `f64`; the exact-result rules for integer `sqrt` are
  the only subtlety).
