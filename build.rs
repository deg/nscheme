//! Build script: compile the `lib/` library sources into the binary.
//!
//! Walks `lib/` and writes `$OUT_DIR/embedded_lib.rs`, a sorted table of
//! `("scheme/list.sld", include_str!(".../lib/scheme/list.sld"))` pairs.
//! [`crate::library`] consults it as the last fallback after the disk
//! search path, so `(import (scheme list))` works where there is no
//! filesystem (the browser build, bead nscheme-yd0.1) and from an
//! installed binary whose `lib/` has moved.
//!
//! **Rust note**: a `build.rs` runs on the host before the crate
//! compiles. `cargo:rerun-if-changed=lib` makes Cargo rerun it whenever
//! anything under `lib/` changes, so the table never goes stale.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let lib = manifest.join("lib");
    println!("cargo:rerun-if-changed=lib");

    let mut files = Vec::new();
    collect(&lib, &mut files);
    let mut entries: Vec<(String, PathBuf)> = files
        .into_iter()
        .map(|path| {
            let rel = path
                .strip_prefix(&lib)
                .expect("file under lib/")
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            (rel, path)
        })
        .collect();
    // Sort by the key *string*: the loader binary-searches it that way.
    // (Path order differs: it puts `srfi/146/hash.sld` before
    // `srfi/146.sld`.)
    entries.sort();

    let mut out = String::from(
        "/// Library sources compiled in by `build.rs`: (path relative to\n\
         /// `lib/`, with extension and `/` separators; source text),\n\
         /// sorted by path string.\n\
         pub static EMBEDDED_LIBS: &[(&str, &str)] = &[\n",
    );
    for (rel, path) in &entries {
        writeln!(
            out,
            "    ({rel:?}, include_str!({:?})),",
            path.display().to_string()
        )
        .expect("write to String");
    }
    out.push_str("];\n");

    let dest = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("embedded_lib.rs");
    fs::write(dest, out).expect("write embedded_lib.rs");
}

/// Recursively gather every `.sld` / `.scm` file under `dir`.
fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|e| e == "sld" || e == "scm") {
            files.push(path);
        }
    }
}
