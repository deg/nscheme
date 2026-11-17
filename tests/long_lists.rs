//! Freeing long lists (bead nscheme-9x7).
//!
//! Rust's generated drop for a cons cell freed the `cdr` from inside the
//! cell's own drop, one stack frame per element, so a long list crashed
//! the process with a stack overflow when it was freed, not when it was
//! built: about 10,000 elements in a browser, about 1,000,000 natively.
//! `impl Drop for Pair` now walks the spine in a loop.
//!
//! The tests run on a thread with a small stack, so a recursive drop
//! would overflow at a size that builds quickly. A stack overflow aborts
//! the whole test binary, which is why these tests have a file of their
//! own.

use std::thread;

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::eval_source;
use nscheme::value::Value;

/// 256K of stack: a recursive drop of 200,000 pairs needs megabytes.
const SMALL_STACK: usize = 256 * 1024;

/// Run `f` on a thread with [`SMALL_STACK`], returning its result.
fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("thread panicked")
}

/// A list built directly from Rust and dropped: the bare `Drop` impl,
/// with no evaluator involved.
#[test]
fn dropping_a_long_list_does_not_overflow_the_stack() {
    on_small_stack(|| {
        let list = Value::list_from((0..200_000).map(Value::Int));
        assert_eq!(list.list_length(), Some(200_000));
        drop(list);
    });
}

/// A long tail that someone else still holds must survive the head being
/// freed: the walk stops at the first shared pair.
#[test]
fn freeing_a_list_keeps_a_shared_tail_alive() {
    on_small_stack(|| {
        let list = Value::list_from((0..200_000).map(Value::Int));
        let Value::Pair(head) = &list else {
            panic!("not a pair")
        };
        let tail = head.borrow().cdr.clone();
        drop(list);
        assert_eq!(tail.list_length(), Some(199_999));
    });
}

/// The case the browser demo hit: a Scheme program builds a long list,
/// and the evaluator frees it once the program is done with it.
#[test]
fn a_scheme_program_can_build_and_drop_a_long_list() {
    let v = on_small_stack(|| {
        let env = Env::new_global();
        install_base(&env).expect("install_base");
        let v = eval_source("(length (make-list 200000 0))", env).expect("eval");
        format!("{v:?}")
    });
    assert_eq!(v, "200000");
}
