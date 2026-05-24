use nscheme::parse::parse_program;
fn main() {
    let s = std::fs::read_to_string("tests/r7rs-corpus/chibi-r7rs-tests.scm").unwrap();
    let d = parse_program(&s).unwrap();
    println!("=== #116 ===\n{}\n", d[116]);
    println!("=== #118 ===\n{}\n", d[118]);
}
