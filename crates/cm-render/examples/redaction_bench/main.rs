//! The Auto-redact benchmark (RED-1): builds a synthetic US screenshot set with exact ground truth
//! and measures the detector on it. Headless; needs the OS text recognizer only for `run`.
//!
//! ```text
//! cargo run -p cm-render --example redaction_bench --release -- gen --out <dir> [--seed N] [--count N]
//! cargo run -p cm-render --example redaction_bench --release -- run --set <dir> [--report <file.md>] [--split dev|held_out|all] [--detector-only]
//! ```
//!
//! `gen/` writes a set (see `schema.rs`), `harness/` reads one. All data is synthetic.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod gen;
mod harness;
mod schema;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("gen") => gen::main(&args[1..]),
        Some("run") => harness::main(&args[1..]),
        _ => {
            eprintln!("usage: redaction_bench gen|run ... (see the file header)");
            2
        }
    };
    std::process::exit(code);
}
