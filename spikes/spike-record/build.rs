//! macOS: let the binary find the Swift runtime at launch.
//!
//! The `screencapturekit` / `apple-cf` crates link a Swift bridge, and their
//! build scripts emit `-rpath /usr/lib/swift`, but Cargo applies a dependency's
//! `rustc-link-arg` only to that dependency's own targets. Without this, dyld
//! fails at startup with "Library not loaded: @rpath/libswift_Concurrency.dylib".

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    // The OS copy of the Swift runtime (in the dyld shared cache since macOS 12).
    println!("cargo:rustc-link-arg-bins=-Wl,-rpath,/usr/lib/swift");

    // Command Line Tools only (no Xcode): the Swift back-deployment shims
    // (libswiftCompatibility56.a, ...Packs.a) live in <CLT>/usr/lib/swift/macosx. The
    // dependencies' build scripts only search the Xcode toolchain path, which does not
    // exist on a CLT-only machine, so the link fails with
    // "__swift_FORCE_LOAD_$_swiftCompatibility56 not found". Add the CLT path as well.
    if let Ok(out) = Command::new("xcode-select").arg("-p").output() {
        if out.status.success() {
            let dev = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let lib = format!("{dev}/usr/lib/swift/macosx");
            if std::path::Path::new(&lib).exists() {
                println!("cargo:rustc-link-search=native={lib}");
            }
        }
    }

    // Fallback: the active Xcode toolchain's copy, if Xcode is installed.
    if let Ok(out) = Command::new("xcode-select").arg("-p").output() {
        if out.status.success() {
            let dev = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let lib = format!("{dev}/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx");
            if std::path::Path::new(&lib).exists() {
                println!("cargo:rustc-link-arg-bins=-Wl,-rpath,{lib}");
            }
        }
    }
}
