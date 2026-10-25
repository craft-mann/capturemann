//! The translators' and CI tool: `cargo run -p cm-i18n -- <command>`.
//!
//! * `extract`   regenerate `locales/capturemann.pot` from the `tr!` calls in the sources
//! * `check`     verify the template is fresh and every `.po` is sound (exit 1 on a problem)
//! * `coverage`  per-language percentages
//! * `unwrapped` list the UI literals not wrapped yet (`--count` prints only the number)
//! * `baseline`  write the current unwrapped count to `locales/unwrapped-baseline.txt`

use cm_i18n::extract;
use std::process::ExitCode;

fn main() -> ExitCode {
    let root = extract::workspace_root();
    let cmd = std::env::args().nth(1).unwrap_or_default();
    match cmd.as_str() {
        "extract" => {
            let scan = extract::scan_workspace(&root);
            for p in &scan.problems {
                eprintln!("{p}");
            }
            if !scan.problems.is_empty() {
                return ExitCode::FAILURE;
            }
            let text = extract::render_pot(&scan.messages);
            if let Err(e) = std::fs::write(root.join(extract::POT_FILE), text) {
                eprintln!("cannot write {}: {e}", extract::POT_FILE);
                return ExitCode::FAILURE;
            }
            println!("{}: {} messages", extract::POT_FILE, scan.messages.len());
            ExitCode::SUCCESS
        }
        "check" => {
            let problems = extract::check_all(&root);
            for p in &problems {
                eprintln!("{p}");
            }
            if problems.is_empty() {
                println!("ok");
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        "coverage" => {
            let scan = extract::scan_workspace(&root);
            match extract::load_catalogues(&root) {
                Ok(cats) => {
                    print!("{}", extract::coverage_report(&scan.messages, &cats));
                    ExitCode::SUCCESS
                }
                Err(p) => {
                    eprintln!("{p}");
                    ExitCode::FAILURE
                }
            }
        }
        "unwrapped" => {
            let hits = extract::count_unwrapped(&root);
            if std::env::args().any(|a| a == "--count") {
                println!("{}", hits.len());
            } else {
                for h in &hits {
                    println!("{h}");
                }
                println!("{} unwrapped", hits.len());
            }
            ExitCode::SUCCESS
        }
        "baseline" => {
            let n = extract::count_unwrapped(&root).len();
            match std::fs::write(root.join(extract::BASELINE_FILE), extract::baseline_text(n)) {
                Ok(()) => {
                    println!("{}: {n}", extract::BASELINE_FILE);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("cannot write {}: {e}", extract::BASELINE_FILE);
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("usage: cargo run -p cm-i18n -- extract | check | coverage | unwrapped [--count] | baseline");
            ExitCode::FAILURE
        }
    }
}
