//! Build script of the vendored `sherpa-onnx-sys` (CaptureMann). The upstream script downloaded a
//! prebuilt archive at build time and unpacked it; this one never touches the network and
//! unpacks nothing. It links the static sherpa-onnx and ONNX Runtime libraries found in the
//! directory named by `SHERPA_ONNX_LIB_DIR` (in this repository: `packaging/sherpa/out/lib`,
//! made by `make sherpa` on macOS and Linux and by `packaging/sherpa/fetch-windows.ps1` on Windows, which
//! `.cargo/config.toml` points the variable at).

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
             (packaging/sherpa/fetch-macos.sh, fetch-linux.sh or, on Windows, fetch-windows.ps1; \
             pinned and checksummed), or point \
             SHERPA_ONNX_LIB_DIR at the `lib` folder of a sherpa-onnx v2.0.1 static-lib archive. \
             To build without the speaker engine, use `--no-default-features`.",
        );
    };
    let dir = PathBuf::from(dir);
    // MSVC static libraries are `name.lib`; the Unix ones are `libname.a`.
    let windows = env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows");
    let file_name = |l: &str| {
        if windows {
            format!("{l}.lib")
        } else {
            format!("lib{l}.a")
        }
    };
    let fetch_hint = if windows {
        "Run packaging\\sherpa\\fetch-windows.ps1 (-Crt md for a plain cargo build, the default mt for \
         the +crt-static package build)"
    } else {
        "Run `make sherpa`"
    };
    if !dir.is_dir() {
        fail(&format!(
            "SHERPA_ONNX_LIB_DIR is not a directory: {}. {fetch_hint} to create it.",
            dir.display()
        ));
    }
    let missing: Vec<String> = STATIC_LIBS
        .iter()
        .filter(|l| !dir.join(file_name(l)).is_file())
        .map(|l| file_name(l))
        .collect();
    if !missing.is_empty() {
        fail(&format!(
            "{} does not hold the sherpa-onnx static libraries (missing: {}). {fetch_hint}, \
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
        "windows" => {
            // The MSVC runtime comes in through the static libraries' own directives; these are the
            // system libraries ONNX Runtime may reference (unresolved symbols at link time say if
            // one more is needed; not yet linked on a Windows machine).
            for l in ["advapi32", "shlwapi", "ole32"] {
                println!("cargo:rustc-link-lib=dylib={l}");
            }
        }
        _ => {}
    }
}

fn fail(message: &str) -> ! {
    // One clear message for the person building, then stop.
    eprintln!("\nerror: sherpa-onnx-sys: {message}\n");
    std::process::exit(1)
}
