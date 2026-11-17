//! The replaceable filesystem (bead nscheme-yd0.2.1): every file
//! operation a Scheme program can cause goes through the installed
//! `nscheme::files::FileSystem`. The browser installs an in-memory one
//! and persists what its observer reports, so any operation that still
//! reached `std::fs` would silently fail there: the program would see
//! `Unsupported` errors, and the page would never save the file.
//!
//! The filesystem is per thread, and every test installs its own before
//! running anything, so the tests stay independent whichever thread
//! they run on.

use std::cell::RefCell;
use std::rc::Rc;

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{EvalError, eval_source};
use nscheme::files::{FileChange, MemoryFileSystem, set_file_system};
use nscheme::value::Value;

/// Writing through an output port must land in the installed filesystem
/// and be reported to its observer, which is how the browser saves it.
/// Reading it back through an input port proves both directions use the
/// same filesystem.
#[test]
fn file_ports_read_and_write_the_installed_filesystem() {
    let changes = install_memory(&[]);
    let v = run(r#"(call-with-output-file "notes/today.txt"
                     (lambda (p) (display "hello, memory" p)))
                   (call-with-input-file "notes/today.txt" read-line)"#)
    .unwrap();
    assert_eq!(format!("{v:?}"), r#""hello, memory""#);
    assert_eq!(
        *changes.borrow(),
        vec![FileChange::Written {
            path: "notes/today.txt".into(),
            contents: "hello, memory".into()
        }]
    );
}

/// `with-output-to-file` is a different route to the same write (it
/// parameterizes the current output port, then closes the file), so it
/// is checked separately.
#[test]
fn with_output_to_file_writes_the_installed_filesystem() {
    install_memory(&[]);
    let v = run(
        r#"(with-output-to-file "out.txt" (lambda () (write '(1 2))))
                   (with-input-from-file "out.txt" read)"#,
    )
    .unwrap();
    assert_eq!(format!("{v:?}"), "(1 2)");
}

/// `file-exists?` and `delete-file` must ask the installed filesystem,
/// not the disk: on the disk none of these files exist. A directory
/// counts as existing (R7RS says "file", but the disk-backed version
/// has always reported directories too).
#[test]
fn file_exists_and_delete_file_use_the_installed_filesystem() {
    let changes = install_memory(&[("data/a.txt", "a")]);
    let v = run(r#"(list (file-exists? "data/a.txt")
                         (file-exists? "data")
                         (file-exists? "data/b.txt")
                         (begin (delete-file "data/a.txt")
                                (file-exists? "data/a.txt")))"#)
    .unwrap();
    assert_eq!(format!("{v:?}"), "(#t #t #f #f)");
    assert_eq!(
        *changes.borrow(),
        vec![FileChange::Removed {
            path: "data/a.txt".into()
        }]
    );
}

/// A missing file must still be a catchable `file-error?` condition, as
/// it is on the disk, and deleting one must fail the same way.
#[test]
fn a_missing_file_is_a_file_error() {
    install_memory(&[]);
    let v = run(
        r#"(list (guard (e ((file-error? e) 'open)) (open-input-file "nope.txt"))
                         (guard (e ((file-error? e) 'delete)) (delete-file "nope.txt")))"#,
    )
    .unwrap();
    assert_eq!(format!("{v:?}"), "(open delete)");
}

/// `load` reads its file through the filesystem too.
#[test]
fn load_reads_the_installed_filesystem() {
    install_memory(&[("prog.scm", "(define loaded-value 42)")]);
    let v = run(r#"(load "prog.scm") loaded-value"#).unwrap();
    assert_eq!(format!("{v:?}"), "42");
}

/// The library loader looks for `./lib/<name>.sld` in the installed
/// filesystem, and an `include` inside that library resolves next to
/// it. This is what lets a browser user save a library and import it.
#[test]
fn a_library_saved_in_the_filesystem_can_be_imported() {
    install_memory(&[
        (
            "lib/mine/greet.sld",
            "(define-library (mine greet) (export greet) (import (scheme base))
               (include \"greet-impl.scm\"))",
        ),
        (
            "lib/mine/greet-impl.scm",
            "(define (greet who) (string-append \"hi, \" who))",
        ),
    ]);
    let v = run(r#"(import (mine greet)) (greet "you")"#).unwrap();
    assert_eq!(format!("{v:?}"), r#""hi, you""#);
}

/// The browser keeps a handle on the installed filesystem so the page
/// can save or delete files while a REPL session is live: changes made
/// through the handle must be visible to the program, and must not be
/// echoed back to the observer (the page already has them).
#[test]
fn a_shared_filesystem_can_change_between_runs() {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&changes);
    let fs = Rc::new(
        MemoryFileSystem::new().with_observer(Box::new(move |c| log.borrow_mut().push(c.clone()))),
    );
    set_file_system(Some(Box::new(Rc::clone(&fs))));
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    let exists = |env| {
        format!(
            "{:?}",
            eval_source(r#"(file-exists? "a.txt")"#, env).unwrap()
        )
    };
    assert_eq!(exists(env.clone()), "#f");
    fs.insert("a.txt", "from the page");
    assert_eq!(exists(env.clone()), "#t");
    assert!(fs.discard("a.txt"));
    assert_eq!(exists(env), "#f");
    assert!(changes.borrow().is_empty(), "{:?}", changes.borrow());
}

/// `None` must put the real disk back, so an embedder can install a
/// filesystem for one run and restore the default afterwards.
#[test]
fn removing_the_filesystem_restores_the_disk() {
    install_memory(&[]);
    assert_eq!(
        format!("{:?}", run(r#"(file-exists? "Cargo.toml")"#).unwrap()),
        "#f"
    );
    set_file_system(None);
    assert_eq!(
        format!("{:?}", run(r#"(file-exists? "Cargo.toml")"#).unwrap()),
        "#t"
    );
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// Install an in-memory filesystem holding `files`, and return the list
/// its observer appends every change to.
fn install_memory(files: &[(&str, &str)]) -> Rc<RefCell<Vec<FileChange>>> {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&changes);
    let fs =
        MemoryFileSystem::new().with_observer(Box::new(move |c| log.borrow_mut().push(c.clone())));
    for (path, contents) in files {
        fs.insert(path, *contents);
    }
    set_file_system(Some(Box::new(fs)));
    changes
}

/// Evaluate `source` in a fresh environment with the base library.
fn run(source: &str) -> Result<Value, EvalError> {
    let env = Env::new_global();
    install_base(&env).expect("install_base");
    eval_source(source, env)
}
