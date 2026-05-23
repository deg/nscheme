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
            let mut counts: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
            for (i, d) in datums.into_iter().enumerate() {
                if let Err(e) = eval(d, env.clone()) {
                    let msg = format!("{e}");
                    counts.entry(msg).or_default().push(i);
                }
            }
            let mut v: Vec<_> = counts.into_iter().collect();
            v.sort_by_key(|(_, ids)| std::cmp::Reverse(ids.len()));
            for (m, ids) in v {
                println!("[{} times] {} -- e.g. #{:?}", ids.len(), m, &ids[..ids.len().min(5)]);
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
