//! Prints the grammar as one standard EBNF document.
fn main() {
    print!("{}", oracle_ir_grammar::engine::export());
}
