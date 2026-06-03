# R7RS-large conformance: reference test suites

nscheme runs each R7RS-large library against its **actual upstream SRFI
reference test suite** where one exists, via `tests/conformance.rs` and
the vendored suites under `tests/r7rs-large-corpus/`. Only each suite's
non-portable preamble (a foreign test framework, an `import` of a
non-R7RS library, a relative `load`, a shebang) is adapted; every
assertion is verbatim. The `(srfi 64)` library (`lib/srfi/64.sld`) plus a
few small dependencies (`(srfi 8)`, `(srfi 27)`, `(srfi 126)`) provide
what the suites need.

## Suites running green

| Library | SRFI | Assertions | Notes |
|---------|------|-----------:|-------|
| comparator | 128 | 144 | |
| bitwise | 151 | 246 | |
| set / bag | 113 | 282 | upstream comparators-shim inlined |
| fixnum | 143 | 141 | |
| list-queue | 117 | 34 | |
| lseq | 127 | 109 | |
| charset | 14 | ~140 | self-checking (`assert`-or-raise) |
| regex | 115 | 66 | unused `(srfi 130)` import dropped |
| hash-table | 125 | ~700 | needed a real `(srfi 126)` + a builtin-identity fix |
| mapping (tree) | 146 | 97 | exercises the macro-hygiene fix (delete) |
| mapping (hash) | 146 | 77 | |
| flonum | 144 | 1276 | surfaced float-div-by-zero, `expt`, flnumerator fixes |
| stream | 41 | 174 | R6RS-adapted; exercises the stream-of hygiene fix |
| **sort** | 132 | ~1400 | passes, but `#[ignore]`d — ~2 min on the tree-walking interpreter |

**≈ 3,500 verbatim upstream assertions across 14 libraries.**

Real interpreter/library bugs these suites surfaced and that were fixed:
multi-value `call/cc`, the full `(scheme cxr)` family, scope-based macro
hygiene (mapping + stream), builtin procedure identity across the
loader's hermetic env, inexact division by zero, `expt` via repeated
squaring, `flnumerator`/`fldenominator` on infinities, `include`
resolving relative to the loading file, and `[ ]` bracket syntax.

## Suites not yet running

| Library | SRFI | Blocker |
|---------|------|---------|
| show | 159 | Suite tests the **columnar/unicode** sub-libraries (`show-columns`, `tabular`, `wrapped`); we vendored base+color only. Completing it needs `(srfi 130)` (string cursors) plus the show `internal/*` sub-libraries. |
| bytevector | 160 | Suite tests the **`s16`** (signed-16-bit) vector variant with negative/large values; we implemented the **`u8`** variant (a full SRFI 160 surface, but the wrong element type for this suite). Needs the `s16` typed-vector family. |

## Libraries with no portable upstream suite

These SRFIs ship no portable reference test file in their repository, so
coverage is the hand-mined `tests/srfi_*_*.rs` suites (cases translated
from the SRFI document's worked examples):

| Library | SRFI | What exists upstream |
|---------|------|----------------------|
| ideque | 134 | only a Guile-specific doctest extractor (`(ice-9 …)`, GOOPS) |
| list | 1 | none |
| box | 111 | none (the library is the canonical 5-line record) |
| division | 141 | none |
| generator | 158 | only Chicken/Gauche-specific test files |

## Running

```
cargo test --test conformance              # the 13 fast suites
cargo test --test conformance -- --ignored # also the slow SRFI 132 sort suite
```
