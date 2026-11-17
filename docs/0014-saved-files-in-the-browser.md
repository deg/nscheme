# ADR 0014 — Saved files in the browser

**Status:** Accepted (2026-11-17) **Bead:** nscheme-yd0.2.4 (Browser demo saves files in IndexedDB, with a Files panel), parent nscheme-yd0.2 (Browser playground) **Related:** [0012 — Running in a web browser](0012-browser-target.md), [0013 — The browser playground](0013-browser-playground.md)

## Context

In the browser playground, file ports failed: there is no disk, so `open-input-file`, `load` and `include` had nothing to read, and a program's output files went nowhere. Saved files also had to:
- survive Stop and crashes, which replace the Worker and everything in it ([0013]);
- survive page reloads;
- be importable as libraries, so a user can write `lib/mine/greet.sld` and `(import (mine greet))`.

The interpreter's file operations are synchronous. Each entry runs as one synchronous call inside the Worker, so it can't wait for an asynchronous storage API halfway through. That ruled out each browser option used directly:
- `localStorage` is synchronous, but a Worker can't reach it, and it's capped at about 5MB.
- IndexedDB works in a Worker, but it's async-only.
- OPFS has a synchronous read/write handle in Workers, but opening a file is still async.

## Decision

### The page owns the files; the Worker works on an in-memory copy

`files::set_file_system` (nscheme-yd0.2.1) lets an embedder replace the filesystem every file operation uses. The Worker installs a `MemoryFileSystem` once (`installFiles` in the wrapper) and keeps a handle to it.

The page keeps the saved files in IndexedDB (database `nscheme`, store `files`, keyed by path), plus a `Map` mirror. When a Worker reports `ready` (at load, and after every Stop or crash), the page first sends it all the files (`{type: "files"}`), and only then enables Evaluate. A program therefore runs against a complete, synchronous filesystem.

### Changes flow both ways, without echoes

- **A program's writes and deletes** reach the filesystem's observer, which posts `file-written` / `file-removed` to the page. The page saves each to IndexedDB. A crash loses nothing: the writes reach the page before the `crashed` message does. Stop is slightly weaker. Terminating a Worker discards messages it posted that the page hasn't handled yet, so a write made in the last instant before Stop can be lost.
- **The Files panel's saves, deletes and uploads** go from the page to the Worker as `put-file` / `delete-file`. The Worker applies them with `insert` / `discard`, which don't tell the observer, so nothing echoes back.
- **The panel can't change files while an entry runs.** Save input as…, Upload and Delete are disabled until it finishes. Otherwise the Worker would apply a panel change only after the entry, and a write the entry made meanwhile would leave the page and the Worker holding different files.

### Saving a library forgets its loaded copy

The library registry caches loaded libraries, and since [0013] there is no fresh run, only Reset. So `putFile` and `removeFile` also call `library::forget_library` for a path that names a library by the loader's rule: `lib/mine/greet.sld` (or `.scm`) is `(mine greet)`. The next `import` then reads the new text. Only the saved library is forgotten: a library that imported it keeps the old bindings until Reset (or Stop) reloads it too. A program that writes a library file itself doesn't get this either, and must Reset. Reset is not enough when the file shadows a standard library's name (`lib/srfi/1.sld`), since Reset keeps the standard libraries cached; Stop reloads everything.

### The panel

If IndexedDB can't be used (it's unavailable, blocked by another tab, fails to read, or doesn't open within two seconds), the page keeps files for this visit only, and the panel says they aren't saved. The interpreter starts either way; the browser test checks this with IndexedDB stubbed to never answer. A failed save (over quota, say) is reported in the transcript.

A collapsed section under the toolbar lists the files, each with its size and Open / Download / Delete actions. It also has "Save input as…" (it suggests the last file opened), "Upload…" (one or more files from disk), and the storage the site uses (`navigator.storage.estimate()`).

The page asks the browser to keep its storage (`navigator.storage.persist()`) on the first save, so the files aren't evicted under disk pressure. Paths are normalized the way the interpreter does it (`./a//b` is `a/b`), and a path that would make a file and a directory collide is refused, as on a disk.

## Consequences

### Positive
- File ports, `load`, `include` and `import` of saved libraries work in the browser, and the files outlast Reset, Stop, crashes and reloads.
- The browser test checks each of these, and that the panel is locked while an entry runs:
  - a program's write shows up in the panel;
  - the file survives Stop;
  - a library saved from the panel imports, and an edit to it takes effect;
  - a delete from the panel is seen by `file-exists?`;
  - an upload works;
  - the files survive a reload.

### Negative / limitations
- **Files are text.** Binary ports were never file-backed, so nothing is lost there, but uploading a binary file would mangle it.
- **Files belong to one browser on one device,** and clearing the site's data deletes them. Download is the only export.
- **Every Worker holds a full copy of the files,** which is fine at playground sizes.
- **Two tabs don't see each other's changes:** each keeps its own list until it reloads, and the last write wins. Two tabs can even store a pair of paths that collide (`a` and `a/b`). Loading then keeps the first and notes the other in the transcript, rather than giving a program a path that is both a file and a directory.

[0013]: 0013-browser-playground.md
