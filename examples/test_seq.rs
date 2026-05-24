use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::eval_source;
fn main() {
    let env = Env::new_global();
    install_base(&env).unwrap();
    for s in [
        "-1.7976931348623157e+308",
        "4.940656458412465e-324",
        "9.881312916824931e-324",
        "1.48219693752374e-323",
        "1.976262583364986e-323",
        "2.470328229206233e-323",
        "2.420921664622108e-322",
        "2.420921664622108e-320",
        "1.4489974452386991",
        "0.14285714285714282",
        "1.7976931348623157e+308",
    ] {
        let prog = format!(r#"(let* ((n (string->number {:?})) (out (open-output-string))) (write n out) (display (get-output-string out)) (newline))"#, s);
        let _ = eval_source(&prog, env.clone());
    }
}
