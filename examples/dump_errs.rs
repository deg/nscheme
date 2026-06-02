//! Group every top-level eval error from the chibi conformance suite
//! by error message, showing how many corpus datums hit each error
//! and a few example indices. Useful for picking the biggest bucket
//! to fix next: `cargo run --example dump_errs`.

use std::collections::HashMap;

use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{eval, eval_source};
use nscheme::parse::parse_program;

fn main() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let env = Env::new_global();
            install_base(&env).unwrap();
            let shim = std::fs::read_to_string("tests/r7rs-corpus/chibi-test-shim.scm").unwrap();
            eval_source(&shim, env.clone()).unwrap();
            let corpus = std::fs::read_to_string("tests/r7rs-corpus/chibi-r7rs-tests.scm").unwrap();
            let datums = parse_program(&corpus).unwrap();
            // Bucket errors by their formatted message; record the
            // first few datum indices that hit each bucket so the
            // caller can `bd-`grep into the corpus.
            let mut counts: HashMap<String, Vec<usize>> = HashMap::new();
            for (i, d) in datums.into_iter().enumerate() {
                if let Err(e) = eval(d, env.clone()) {
                    counts.entry(format!("{e}")).or_default().push(i);
                }
            }
            let mut by_count: Vec<_> = counts.into_iter().collect();
            by_count.sort_by_key(|(_, ids)| std::cmp::Reverse(ids.len()));
            for (msg, ids) in by_count {
                let n = ids.len();
                let sample = &ids[..n.min(5)];
                println!("[{n} times] {msg} -- e.g. #{sample:?}");
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
