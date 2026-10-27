//! Build script of the vendored `sherpa-onnx-sys` (CaptureMann). The upstream script downloaded a
//! prebuilt archive at build time and unpacked it; this one never touches the network and
//! unpacks nothing. It links the static sherpa-onnx and ONNX Runtime libraries found in the
//! directory named by `SHERPA_ONNX_LIB_DIR` (in this repository: `packaging/sherpa/out/lib`,
//! made by `make sherpa`, which `.cargo/config.toml` points the variable at).

use std::env;
use std::path::PathBuf;

/// The static libraries of the `sherpa-onnx-v2.0.1-*-static-lib` release archives, in link order.
const STATIC_LIBS: &[&str] = &[
    "sherpa-onnx-c-api",
    "sherpa-onnx-core",
    "kaldi-decoder-core",
    "sherpa-onnx-kaldifst-core",
    "sherpa-onnx-fstfar",
    "sherpa-onnx-fst",
    "kaldi-native-fbank-core",
    "kissfft-float",
    "onnxruntime",
    "ssentencepiece_core",
    "utf8proc",
];

fn main() {
    println!("cargo:rerun-if-env-changed=SHERPA_ONNX_LIB_DIR");
    println!("cargo:rerun-if-changed=build.rs");

    let Some(dir) = env::var_os("SHERPA_ONNX_LIB_DIR").filter(|v| !v.is_empty()) else {
        fail(
            "SHERPA_ONNX_LIB_DIR is not set. The speaker engine (sherpa-onnx) links prebuilt \
             static libraries and this build never downloads them. Run `make sherpa` once \
             (packaging/sherpa/fetch-macos.sh, pinned and checksummed), or point \
             SHERPA_ONNX_LIB_DIR at the `lib` folder of a sherpa-onnx v2.0.1 static-lib archive. \
             To build without the speaker engine, use `--no-default-features`.",
        );
    };
    let dir = PathBuf::from(dir);
    if !dir.is_dir() {
        fail(&format!(
            "SHERPA_ONNX_LIB_DIR is not a directory: {}. Run `make sherpa` to create it.",
            dir.display()
        ));
    }
    let missing: Vec<String> = STATIC_LIBS
        .iter()
        .filter(|l| !dir.join(format!("lib{l}.a")).is_file())
        .map(|l| format!("lib{l}.a"))
        .collect();
    if !missing.is_empty() {
        fail(&format!(
            "{} does not hold the sherpa-onnx static libraries (missing: {}). Run `make sherpa`, \
             or use the `lib` folder of the v2.0.1 static-lib archive for this platform.",
            dir.display(),
            missing.join(", ")
        ));
    }
    println!("cargo:rerun-if-changed={}", dir.display());

    println!("cargo:rustc-link-search=native={}", dir.display());
    for lib in STATIC_LIBS {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    match env::var("CARGO_CFG_TARGET_OS").unwrap_or_default().as_str() {
        "linux" => {
            for l in ["stdc++", "m", "pthread", "dl"] {
                println!("cargo:rustc-link-lib=dylib={l}");
            }
        }
        "macos" => {
            println!("cargo:rustc-link-lib=dylib=c++");
            println!("cargo:rustc-link-lib=framework=Foundation");
        }
        _ => {}
    }
}

fn fail(message: &str) -> ! {
    // One clear message for the person building, then stop.
    eprintln!("\nerror: sherpa-onnx-sys: {message}\n");
    std::process::exit(1)
}
