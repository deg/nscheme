use nscheme::builtins::install_base;
use nscheme::env::Env;
use nscheme::eval::eval_source;
fn main() {
    let env = Env::new_global();
    install_base(&env).unwrap();
    let prog = r#"
(display "= ") (display (= 9007199254740992.0 9007199254740993)) (newline)
(display "9007199254740992.0 = ") (write 9007199254740992.0) (newline)
(display "9007199254740993 = ") (write 9007199254740993) (newline)
"#;
    eval_source(prog, env).unwrap();
}
