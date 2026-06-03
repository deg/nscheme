# ADR 0007 — Filesystem-loaded libraries for R7RS-large

**Status:** Accepted (2026-06-04) **Bead:** [nscheme-9q5](../.beads/issues.jsonl) **Related:** [0006 — Library / module system](0006-library-system.md), epics [nscheme-lul](../.beads/issues.jsonl) (Red) / [nscheme-oeg](../.beads/issues.jsonl) (Tangerine)

## Context

After the R7RS-small core was complete, the target expanded to **R7RS-large**: the ratified Red (2016) and Tangerine (2019) editions, ~21 SRFI libraries (`(scheme list)`, `(scheme comparator)`, `(scheme mapping)`, `(scheme regex)`, …). ADR 0006 built `define-library` / `import` / `cond-expand` but left libraries to be defined *inline* — it explicitly listed "no file-based library discovery (`.sld` files)" as a gap. Twenty-one libraries totalling ~20,000 lines could not live inline in user programs or be hand-installed in Rust without enormous cost.

Two questions had to be answered:

1. **In what language are the libraries written** — native Rust primitives, or portable Scheme?
2. **How does `(import (scheme mapping))` find its definition** when it isn't already in the env?

## Decision

### Libraries are portable Scheme source, not Rust

Each R7RS-large library is an ordinary `.sld` file (`define-library` form) under [`lib/`](../lib/), written in portable R7RS on top of the base primitives. `lib/scheme/mapping.sld`, `lib/srfi/1.sld`, and so on. Only the genuinely primitive operations live in Rust (`builtins.rs`, `io.rs`); everything expressible in Scheme is Scheme.

This keeps the Rust core small, makes each library auditable against its SRFI document, and — critically — lets us run each library's **actual upstream reference test suite** unmodified (see [docs/CONFORMANCE.md](CONFORMANCE.md)).

### A search-path filesystem loader resolves imports on demand

A library name maps to a relative path: `(scheme mapping)` → `scheme/mapping`, `(srfi 1)` → `srfi/1` (numeric components become their decimal string). When `import` encounters a name that is neither built-in nor already registered, the loader searches, in order:

1. `NSCHEME_LIB_PATH` (colon-separated directories), if set;
2. a **compiled-in default** — `<CARGO_MANIFEST_DIR>/lib`, baked in at build time so an installed binary finds its libraries without configuration;
3. `./lib` relative to the working directory.

The first `scheme/mapping.sld` (then `.scm`) that exists is read and evaluated, which runs its `define-library` form and registers it. Loading is recursive (a library's own imports load the same way), guarded against cycles by a thread-local `LOADING` stack, and pushes the file's directory so an `include` inside it resolves relative to the library file.

### Loads run in a hermetic root env, with shared primitive identity

A loaded library is evaluated in a fresh loader-root env rather than the user's program env, so a library can't accidentally see or clobber user bindings. But this raised a subtlety: a primitive like `eq?` installed into the program env and again into the loader root must be the **same object**, or code that compares procedures (SRFI 125 infers a hash from an equality predicate by identity) breaks across the import boundary. A thread-local `PRIMITIVE_CACHE` makes every `install_base` bind the identical singleton for each primitive name.

### Import qualifiers

`import` gained the R7RS modifiers `only` / `except` / `prefix` / `rename` (`resolve_import_set` in `eval.rs`), which several SRFI suites and libraries rely on — closing the other ADR 0006 follow-up.

## Consequences

### Positive

- R7RS-large is implementable at all, and at reasonable cost: a new library is a `.sld` file, no Rust required (see the "extend nscheme" note in `TOUR.md`).
- Libraries are validated by their verbatim upstream suites, not hand-rolled tests — far stronger conformance evidence.
- An installed binary works out of the box (compiled-in default path) while developers and embedders can redirect with `NSCHEME_LIB_PATH`.
- The hermetic loader root plus shared primitive identity means libraries compose without leaking state and without breaking procedure equality.

### Negative

- Library load is interpreted at first import — there's no precompilation or caching of expanded libraries across process runs. Acceptable for a tree-walking interpreter (ADR 0001); a bytecode backend (`nscheme-6mp`) would revisit it.
- The search path is process/thread-global state (thread-locals), which the test suite manages explicitly via `set_search_path`.
- Built-in `(scheme …)` libraries remain no-op imports (ADR 0006); the on-disk libraries are real, so the two kinds coexist and a reader must know which is which.

### Follow-ups

- Per-library strict export sets for the built-in libraries (carried over from ADR 0006).
- Optional compiled/cached library images if startup cost ever matters.
