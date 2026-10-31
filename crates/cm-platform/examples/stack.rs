//! Prints the Linux stack report of this system (read-only):
//! `cargo run -p cm-platform --example stack`.

fn main() {
    match cm_platform::stack::probe() {
        None => println!("not a Linux system: no stack report"),
        Some(r) => {
            for l in r.fact_lines() {
                println!("{l}");
            }
            println!(
                "screenshots ok: {}, recording stack ok: {}",
                r.screenshots_ok, r.recording_stack_ok
            );
            for g in &r.gaps {
                println!("[{:?}] {}", g.severity, g.sentence);
            }
        }
    }
}
