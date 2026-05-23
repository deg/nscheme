# ADR 0006 — Library / module system

**Status:** Accepted (2026-05-21)
**Bead:** [nscheme-08v](../.beads/issues.jsonl)

## Context

R7RS-small §5.6 specifies a library system with `define-library`,
`import`, and `cond-expand`. Each library has a name (a list of
identifiers — e.g. `(scheme base)`), a set of exported bindings, and
declarations that supply them (`begin`, `import`, `include`).

A complete library system has many corners (rename / only / except /
prefix import modifiers, library file discovery, version handling).
For v1 we want enough to:

1. Make `(import (scheme base))` work — so user programs can write
   the canonical import line at the top.
2. Let users define their own libraries and import them.
3. Let `cond-expand` gate code on whether features or libraries are
   present.

## Decision

Implementation in [`src/library.rs`](../src/library.rs).

### Library identity

```rust
pub type LibraryName = Vec<String>;
```

`(scheme base)` becomes `["scheme", "base"]`. Numeric components
(e.g. `(srfi 1)`) become their decimal string.

### Registry

A thread-local `HashMap<LibraryName, HashMap<Symbol, Value>>` holds
user-defined libraries.

### Built-in libraries are not in the registry

The eleven `(scheme …)` libraries that R7RS-small specifies
(`base`, `write`, `read`, `char`, `file`, `inexact`, `cxr`, `lazy`,
`load`, `process-context`, `repl`, `time`, `case-lambda`, `eval`,
`r5rs`) are recognized by name and treated as **no-op imports** —
their bindings are already in the global env from `install_base`.

This means a user can write `(import (scheme base) (scheme write))`
at the top of their program without nscheme having to know which
bindings are in which library. We trade a bit of pedantic accuracy
(in a strict R7RS, importing only `(scheme write)` would *not* give
you `+`) for a simpler v1 that runs more existing R7RS code.

### `define-library`

```scheme
(define-library (NAME ...)
  decl ...)
```

Each `decl` is one of:

| decl                  | what it does                                  |
|-----------------------|-----------------------------------------------|
| `(export id ...)`     | mark `id`s for export                         |
| `(import LIB ...)`    | import bindings into the library's own env    |
| `(begin expr ...)`    | evaluate `expr`s in the library's env         |
| `(include "file")`    | read and evaluate the file in the library env |
| `(cond-expand …)`     | conditional declarations                      |

The library env is created via `Env::extend(global)` so it inherits
the built-in bindings but defines added by `begin` stay private
unless `export`ed.

After all decls run, the named exports are looked up in the library
env and stored in the registry.

### `import`

For each library form, look up the name. If it's a built-in
library (recognized by `is_builtin_library`), no-op. Otherwise, copy
the registered bindings into the importing env via `env.define`.

### `cond-expand`

Standard R7RS feature-detection form. A feature requirement is one
of:

- An identifier (matches if the implementation declares the feature)
- `(library NAME)` (matches if the library is built-in or registered)
- `(and req...)`, `(or req...)`, `(not req)`
- `else` (always matches)

Features list is hard-coded in `crate::library::features()`:
`r7rs`, `nscheme`, `nscheme-0.1`, `exact-closed`, `ratios`,
`ieee-float`.

## Consequences

### Positive

- Real R7RS programs that start with `(import (scheme base))` just
  work.
- User libraries provide a clean way to package code with a private
  scope.
- `cond-expand` lets portable code branch on `nscheme` vs other
  implementations.

### Negative

- The built-in-library-as-noop trick means `(import (scheme inexact))`
  also gives you `+` and `car`, which is over-permissive. R7RS-strict
  implementations would only expose what each library declares. The
  fix is to bind each `(scheme …)` library to its actual export set
  and look it up properly; this is a mechanical refactor when needed.
- Import modifiers (`only`, `except`, `prefix`, `rename`) are not yet
  supported.
- No file-based library discovery (`.sld` files). Libraries must be
  defined inline or pulled in via `(include "file.scm")`.

### Open follow-ups

- Strict per-library export sets for the built-in libraries.
- Import modifiers.
- `.sld` file discovery for a `(import (foo bar))` that isn't yet
  defined inline.
