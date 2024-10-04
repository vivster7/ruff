use std::path::Path;

fn main() {
    println!("Running symbol_finder...");

    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        println!("Usage: cargo run --bin symbol_finder -- scripts/add_rule.py ");
        return;
    }

    let path = Path::new(&args[1]);
    ruff_linter::process::process_path(path);
}
