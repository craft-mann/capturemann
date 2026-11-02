// Embeds the app icon in cm-desktop.exe on Windows (see app.rc). Nothing to do elsewhere.
fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/brand/app.ico");
    // The build script runs on the host: the resource compiler is only there on Windows.
    #[cfg(windows)]
    {
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            embed_resource::compile("app.rc", embed_resource::NONE)
                .manifest_required()
                .expect("could not embed the app icon (needs the Windows SDK resource compiler)");
        }
    }
}
