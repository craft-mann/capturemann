// Console scratch exe (like capturemann.exe) WITH Velopack startup first. For timing.
include!("../common.rs");

fn main() {
    if std::env::var("VS_NOVELO").is_err() {
        velopack_startup(false);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|s| s.as_str()) == Some("--version") {
        println!("vcli {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    app_main(&args);
}
