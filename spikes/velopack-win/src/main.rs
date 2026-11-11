// GUI-subsystem scratch exe (like cm-desktop): no console. Velopack startup first, auto-apply off.
#![windows_subsystem = "windows"]
include!("common.rs");

fn main() {
    velopack_startup(false);
    let args: Vec<String> = std::env::args().skip(1).collect();
    app_main(&args);
}
