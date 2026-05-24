# nscheme — design documentation

This directory holds architecture decision records (ADRs) and other long-form design notes for the `nscheme` Scheme interpreter.

Per-task implementation notes live in beads (`bd show <id>`) — only durable decisions land here. An ADR is added when a decision is expensive to change later or affects how multiple modules fit together.

## Index

- [0001 — Tree-walking interpreter with explicit step-loop](0001-tree-walking-interpreter.md)
- [0002 — Numeric tower (Int / BigInt / Rational / Float)](0002-numeric-tower.md)
- [0003 — Hygienic `syntax-rules` via alpha-renaming](0003-syntax-rules-hygiene.md)
- [0004 — Continuations as cloned frame stacks](0004-continuations.md)
- [0005 — Exception handling via the frame stack](0005-exception-handling.md)
- [0006 — Library / module system](0006-library-system.md)
