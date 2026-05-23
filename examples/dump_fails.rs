use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::{eval, eval_source};
use nscheme::parse::parse_program;
use nscheme::value::{Symbol, Value};

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
            for (i, d) in datums.into_iter().enumerate() {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = eval(d, env.clone());
                }));
                let _ = i;
            }
            let failures = env.lookup(&Symbol::intern("$failures")).unwrap();
            // print the list reversed (so older first)
            let mut items: Vec<Value> = Vec::new();
            let mut cur = failures;
            while let Value::Pair(p) = cur {
                let pair = p.borrow();
                items.push(pair.car.clone());
                cur = pair.cdr.clone();
            }
            items.reverse();
            for (i, f) in items.iter().enumerate() {
                println!("{}: {}", i, f);
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
