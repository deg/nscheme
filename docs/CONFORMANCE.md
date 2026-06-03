# R7RS-large conformance: reference test suites

nscheme runs each R7RS-large library against its **actual upstream SRFI
reference test suite** where one exists, via `tests/conformance.rs` and
the vendored suites under `tests/r7rs-large-corpus/`. Only each suite's
non-portable preamble (a foreign test framework, an `import` of a
non-R7RS library, a relative `load`/`include`, a shebang) is adapted;
every assertion is verbatim. The `(srfi 64)` library (`lib/srfi/64.sld`)
plus small dependencies (`(srfi 8)`, `(srfi 27)`, `(srfi 126)`) provide
what the suites need.

**18 of the 21 libraries have a portable upstream suite, and all 18 run
green** (≈ 5,400 verbatim upstream assertions). The other 3 ship no
upstream test file at all.

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
| regex | 115 | 66 | |
| hash-table | 125 | ~700 | needed a real `(srfi 126)` + a builtin-identity fix |
| mapping (tree) | 146 | 97 | exercises the macro-hygiene fix (delete) |
| mapping (hash) | 146 | 77 | |
| flonum | 144 | 1276 | surfaced float-div-by-zero, `expt`, `flnumerator` fixes |
| stream | 41 | 174 | R6RS-adapted; exercises the stream-of hygiene fix |
| bytevector | 160 | 110 | the `s16` variant — added `(srfi 160 s16)` |
| show | 159 | 313 | completed the columnar + unicode sub-libraries |
| generator | 158 | 80 | |
| ideque | 134 | 119 | |
| **sort** | 132 | ~1400 | passes, but `#[ignore]`d — ~2 min on the tree-walking interpreter |

Real interpreter/library bugs these suites surfaced and that were fixed:
multi-value `call/cc`; the full `(scheme cxr)` family; scope-based macro
hygiene (mapping + stream); builtin procedure identity across the
loader's hermetic env; inexact division by zero; `expt` via repeated
squaring; `flnumerator`/`fldenominator` on infinities; `include`
resolving relative to the loading file; `[ ]` bracket syntax;
`call-with-output-file`/`call-with-input-file`.

To run them all (including the slow sort suite):

```
cargo test --test conformance              # the 17 fast suites
cargo test --test conformance -- --ignored # also SRFI 132 sort
```

## Libraries with no upstream suite

These three SRFIs ship **no test file** in their repository, so coverage
is the hand-mined `tests/srfi_*_*.rs` suites (cases translated from each
SRFI document's worked examples). There is nothing upstream to vendor.

| Library | SRFI | |
|---------|------|--|
| list | 1 | classic SRFI; no test file in the repo |
| box | 111 | the library is the canonical 5-line record |
| division | 141 | no test file in the repo |
