use nscheme::parse::parse_program;
fn main() {
    let s = std::fs::read_to_string("tests/r7rs-corpus/chibi-r7rs-tests.scm").unwrap();
    let d = parse_program(&s).unwrap();
    for i in [118, 147, 206, 209, 210] {
        println!("=== #{} ===", i);
        println!("{}", d[i]);
        println!();
    }
}
