use nscheme::parse::parse_program;
fn main() {
    let s = std::fs::read_to_string("tests/r7rs-corpus/chibi-r7rs-tests.scm").unwrap();
    let d = parse_program(&s).unwrap();
    for i in [944, 945, 946, 1056, 1072, 1094, 1120, 1122] {
        println!("=== #{} ===", i);
        println!("{}", d[i]);
        println!();
    }
}
