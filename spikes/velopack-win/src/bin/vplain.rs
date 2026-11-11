// Console scratch exe WITHOUT Velopack: the timing control.
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("vplain {}", env!("CARGO_PKG_VERSION"));
    }
}
