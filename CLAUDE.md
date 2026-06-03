# Project Instructions for AI Agents

This file provides instructions and context for AI coding agents working on this project.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:7510c1e2 -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->


## What This Is

`nscheme` is an R7RS Scheme interpreter in Rust: the R7RS-small (2013) core
plus 21 R7RS-large SRFI libraries (Red 2016 + Tangerine 2019). Tree-walking,
single binary, no bytecode VM. See `README.md` for the user-facing overview,
`TOUR.md` for a source reading guide, and `docs/` for the ADRs.

## Build & Test

```bash
cargo build                         # debug binary at target/debug/nscheme
cargo build --release               # optimized binary at target/release/nscheme

cargo test                          # full suite (~624 tests; SRFI 132 sort is #[ignore]d)
cargo test --test conformance       # R7RS-large upstream SRFI reference suites
cargo test --test conformance -- --ignored   # also the ~2-min sort suite
cargo test --test r7rs_chibi -- --nocapture  # chibi R7RS-small corpus (1180 forms / 1225 assertions)
cargo test <name>                   # any test whose name matches

cargo clippy --all-targets -- -D warnings    # lint (warnings are errors)
cargo fmt                           # format;  cargo fmt --check to verify
```

Run a program: `./target/debug/nscheme FILE`. Stay in the REPL after loading:
`./target/debug/nscheme -i FILE`. The interpreter finds `lib/` automatically;
override with `NSCHEME_LIB_PATH=lib` if you've moved things.

## Architecture Overview

- The crate is a **library + thin binary**: all language behavior lives in
  `src/*.rs`; `src/main.rs` is just the CLI/REPL frontend.
- Pipeline: `lex.rs` (hand-written state machine) → `parse.rs` (produces
  `Value`s directly — code is data) → `eval.rs`.
- `eval.rs` is a **step-loop, not recursive `eval`**: a `Step`/`Frame` state
  machine driven by a `loop`. This is what makes TCO and `call/cc` (a
  `frames.clone()`) cheap. Three dispatch tables: `step_eval` (special forms),
  `step_apply` (callables), `resume` (pending frames). See ADR 0001.
- `value.rs` is the data model (the `Value` enum + the three equality
  predicates + the numeric tower). `builtins.rs` is the flat catalog of
  primitives; `io.rs` is ports. `macros.rs` is scope-based `syntax-rules`.
- `library.rs` is `define-library`/`import`/`cond-expand` **plus a filesystem
  loader**: `(import (foo bar))` resolves to `foo/bar.sld` on the search path
  (`NSCHEME_LIB_PATH` → compiled-in `<crate>/lib` → `./lib`). The 21
  R7RS-large libraries live in `lib/` as `.sld` source.

## Conventions & Patterns

- **Issue tracking is `bd` (beads), not markdown TODOs** — see the Beads
  section above. File/claim a bead before non-trivial work.
- **In-code teaching commentary**: every source file has an annotated header;
  inline "Rust note" / "Scheme note" comments explain an idiom or concept once
  at its first appearance. Match this style. Conventions are in `docs/STYLE.md`.
- **A new primitive** → add to `builtins.rs` (copy a same-shape `define(...)`).
  **A new special form** → add an arm to `eval.rs`'s `dispatch_special_form`
  and a sibling `step_*` helper, and register the name in `is_special_form_name`.
  **A new library** → drop a `.sld` under `lib/` (e.g. `lib/scheme/foo.sld` or
  `lib/srfi/N.sld`); the loader finds it. Mirror an existing one.
- **ADRs are history**: `docs/000N-*.md` record decisions as of their date.
  When a decision changes, add a dated status-note at the top rather than
  rewriting the body.
- **Conformance over hand-rolled tests**: a library's coverage should be its
  real upstream SRFI suite (`tests/conformance.rs` + `tests/r7rs-large-corpus/`)
  where one exists; hand-mined tests only where upstream ships none.
