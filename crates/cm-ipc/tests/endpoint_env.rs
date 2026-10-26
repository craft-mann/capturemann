//! Endpoint resolution through the process environment. One test function, because the
//! environment is process-wide (this file is its own test binary, so it cannot race the unit
//! tests). Only values under a temp dir are used; no real daemon socket is touched.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use cm_ipc::{Endpoint, MAX_SOCKET_PATH_BYTES};

fn restore(name: &str, old: Option<std::ffi::OsString>) {
    match old {
        Some(v) => std::env::set_var(name, v),
        None => std::env::remove_var(name),
    }
}

/// Endpoint rules (specs/02-agent-interface.md): `CAPTUREMANN_SOCKET` wins when non-empty, an
/// empty or unset value gives the platform default, and the default follows `HOME`.
#[test]
fn socket_variable_and_home_resolution_table() {
    let old_sock = std::env::var_os("CAPTUREMANN_SOCKET");
    let old_home = std::env::var_os("HOME");
    let old_xdg = std::env::var_os("XDG_RUNTIME_DIR");
    let tmp = tempfile::tempdir().unwrap();
    // Short on purpose: the macOS default must fit a socket address (nothing is created here).
    let home = PathBuf::from(format!("/tmp/cmipc-{}", std::process::id()));
    std::env::set_var("HOME", &home);
    std::env::remove_var("XDG_RUNTIME_DIR");

    let explicit = tmp.path().join("custom.sock");
    std::env::set_var("CAPTUREMANN_SOCKET", &explicit);
    assert_eq!(Endpoint::from_env_or_default(), Endpoint::Path(explicit));

    // A relative path is kept as given.
    std::env::set_var("CAPTUREMANN_SOCKET", "rel/daemon.sock");
    assert_eq!(
        Endpoint::from_env_or_default(),
        Endpoint::Path(PathBuf::from("rel/daemon.sock"))
    );

    // Empty and unset are the default.
    let default = Endpoint::default_for_platform();
    std::env::set_var("CAPTUREMANN_SOCKET", "");
    assert_eq!(Endpoint::from_env_or_default(), default);
    std::env::remove_var("CAPTUREMANN_SOCKET");
    assert_eq!(Endpoint::from_env_or_default(), default);

    // The default fits a socket address.
    let Endpoint::Path(p) = &default else {
        panic!("path expected")
    };
    assert!(p.ends_with("daemon.sock"), "{}", p.display());
    assert!(p.as_os_str().len() <= MAX_SOCKET_PATH_BYTES);
    #[cfg(target_os = "macos")]
    assert_eq!(
        p,
        &home.join("Library/Application Support/CaptureMann/daemon.sock")
    );

    // A too-long HOME falls back to the short per-user directory.
    std::env::set_var("HOME", tmp.path().join("u".repeat(150)));
    let Endpoint::Path(p) = Endpoint::default_for_platform() else {
        panic!("path expected")
    };
    assert!(
        p.to_string_lossy().starts_with("/tmp/cm-"),
        "{}",
        p.display()
    );
    assert!(p.as_os_str().len() <= MAX_SOCKET_PATH_BYTES);

    // Without HOME the default goes to the system temp dir.
    std::env::remove_var("HOME");
    let Endpoint::Path(p) = Endpoint::default_for_platform() else {
        panic!("path expected")
    };
    assert!(p.ends_with("daemon.sock"));

    restore("CAPTUREMANN_SOCKET", old_sock);
    restore("HOME", old_home);
    restore("XDG_RUNTIME_DIR", old_xdg);
}
