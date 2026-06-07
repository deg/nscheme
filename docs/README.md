# nscheme — design documentation

This directory holds architecture decision records (ADRs) and other long-form design notes for the `nscheme` Scheme interpreter.

Per-task implementation notes live in beads (`bd show <id>`) — only durable decisions land here. An ADR is added when a decision is expensive to change later or affects how multiple modules fit together.

For a reading guide to the source code, see [`../TOUR.md`](../TOUR.md).

## Architecture decision records

- [0001 — Tree-walking interpreter with explicit step-loop](0001-tree-walking-interpreter.md)
- [0002 — Numeric tower (Int / BigInt / Rational / Float)](0002-numeric-tower.md)
- [0003 — Hygienic `syntax-rules` via alpha-renaming](0003-syntax-rules-hygiene.md)
- [0004 — Continuations as cloned frame stacks](0004-continuations.md)
- [0005 — Exception handling via the frame stack](0005-exception-handling.md)
- [0006 — Library / module system](0006-library-system.md)
- [0007 — Filesystem-loaded libraries for R7RS-large](0007-filesystem-library-loader.md)
- [0008 — Hygiene beyond alpha-renaming: def-site `SyntaxRef` + per-expansion scope](0008-hygiene-scope-and-syntaxref.md) — supersedes (in part) 0003
- [0009 — First-class control procedures: `apply` / `eval` / `load`](0009-first-class-control-procedures.md)
- [0010 — Current ports as parameters](0010-current-ports-as-parameters.md)
- [0011 — Error diagnostics: source locations and backtraces](0011-error-diagnostics.md)

Each ADR records the decision as of its date. Where a decision was later
revised, a dated status-note at the top of the ADR points to what changed
(see 0002, 0003, and 0006) — the body is left as the historical record.

## Conformance & examples

- [CONFORMANCE.md](CONFORMANCE.md) — how each R7RS-large library is tested against its actual upstream SRFI reference suite (18/21 green, ~5,400 assertions).
- [showcase/](showcase/) — a dozen tiny programs showing off the R7RS-large libraries, with a README on how to run them.

## Style guide

- [STYLE.md](STYLE.md) — conventions for in-code teaching commentary. Tells you what each comment in the source is trying to accomplish.
