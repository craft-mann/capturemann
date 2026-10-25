//! Command line: `cm-overlay <region|window|display> [options]`.

use std::path::PathBuf;

use cm_ipc::Endpoint;

use crate::error::OverlayError;
use crate::picker::Mode;

pub const USAGE: &str = "usage: cm-overlay <region|window|display> [--socket PATH] \
[--confirm-on-release] [--no-magnifier] [--no-hint] [--pick] [--exit-after-first-frame]

Shows a frozen, dimmed copy of every display and lets you pick a region (drag, then Enter),
a window (click) or a display (click). Prints the capture as JSON on stdout.
Esc cancels (exit 3, nothing saved). Exit codes follow the shared error table.

  --socket PATH         daemon socket path (Unix) or pipe name (Windows); also CAPTUREMANN_SOCKET
  --confirm-on-release  region mode: capture when the mouse is released instead of on Enter
  --no-magnifier        hide the magnifier (CAP-3)
  --no-hint             hide the key hints
  --pick                only choose: print the chosen target as JSON (`{\"picked\": ...}`) and take no
                        screenshot (the desktop app uses it to pick what to record)
  --exit-after-first-frame
                        benchmark: print the phase timings (ms since process start) as JSON
                        and exit once every display has painted its first frame";

#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    pub mode: Mode,
    pub endpoint: Endpoint,
    pub confirm_on_release: bool,
    pub magnifier: bool,
    pub hint: bool,
    /// Print the chosen target instead of capturing it (REC-1: the recording start sheet).
    pub pick: bool,
    pub exit_after_first_frame: bool,
}

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum Parsed {
    Run(Args),
    Help,
}

fn endpoint_from(s: &str) -> Endpoint {
    if cfg!(windows) {
        Endpoint::Pipe(s.to_owned())
    } else {
        Endpoint::Path(PathBuf::from(s))
    }
}

/// Parses `args` (without the program name). `env_socket` is `CAPTUREMANN_SOCKET`.
pub fn parse<I: IntoIterator<Item = String>>(
    args: I,
    env_socket: Option<String>,
) -> Result<Parsed, OverlayError> {
    let mut mode = None;
    let mut socket = env_socket.filter(|s| !s.is_empty());
    let (mut confirm_on_release, mut magnifier, mut hint) = (false, true, true);
    let mut exit_after_first_frame = false;
    let mut pick = false;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => return Ok(Parsed::Help),
            "--socket" => {
                socket = Some(
                    it.next()
                        .ok_or_else(|| OverlayError::invalid_input("--socket needs a value"))?,
                );
            }
            "--confirm-on-release" => confirm_on_release = true,
            "--no-magnifier" => magnifier = false,
            "--no-hint" => hint = false,
            "--pick" => pick = true,
            "--exit-after-first-frame" => exit_after_first_frame = true,
            "region" | "window" | "display" => {
                if mode.is_some() {
                    return Err(OverlayError::invalid_input("only one mode may be given"));
                }
                mode = Some(match a.as_str() {
                    "region" => Mode::Region,
                    "window" => Mode::Window,
                    _ => Mode::Display,
                });
            }
            other => {
                return Err(OverlayError::invalid_input(format!(
                    "unknown argument `{other}`"
                )))
            }
        }
    }
    let mode = mode.ok_or_else(|| {
        OverlayError::invalid_input("a mode is required: region, window or display")
    })?;
    Ok(Parsed::Run(Args {
        mode,
        endpoint: socket
            .as_deref()
            .map_or_else(Endpoint::default_for_platform, endpoint_from),
        confirm_on_release,
        magnifier,
        hint,
        pick,
        exit_after_first_frame,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(a: &[&str]) -> Result<Parsed, OverlayError> {
        parse(a.iter().map(|s| (*s).to_owned()), None)
    }

    #[test]
    fn parses_modes_and_flags() {
        let Ok(Parsed::Run(a)) = p(&["region", "--confirm-on-release", "--no-magnifier"]) else {
            panic!("expected Run");
        };
        assert_eq!(a.mode, Mode::Region);
        assert!(a.confirm_on_release && !a.magnifier && a.hint && !a.exit_after_first_frame);
        let Ok(Parsed::Run(b)) = p(&["region", "--exit-after-first-frame"]) else {
            panic!("expected Run");
        };
        assert!(b.exit_after_first_frame);
    }

    #[test]
    fn pick_flag() {
        let Ok(Parsed::Run(a)) = p(&["region", "--pick"]) else {
            panic!("expected Run");
        };
        assert!(a.pick);
        let Ok(Parsed::Run(b)) = p(&["region"]) else {
            panic!("expected Run");
        };
        assert!(!b.pick);
    }

    #[test]
    fn bad_input_is_invalid_input_exit_2() {
        for bad in [&[][..], &["region", "window"], &["--socket"], &["circle"]] {
            let e = p(bad).unwrap_err();
            assert_eq!(e.exit, 2, "{bad:?}");
        }
        assert_eq!(p(&["--help"]).unwrap(), Parsed::Help);
    }

    #[test]
    fn socket_comes_from_the_flag_or_the_environment() {
        let Ok(Parsed::Run(a)) = parse(["display".to_owned()], Some("/tmp/x.sock".to_owned()))
        else {
            panic!("expected Run");
        };
        assert_eq!(a.endpoint, endpoint_from("/tmp/x.sock"));
        let Ok(Parsed::Run(b)) = parse(
            [
                "display".to_owned(),
                "--socket".to_owned(),
                "/tmp/y.sock".to_owned(),
            ],
            Some("/tmp/x.sock".to_owned()),
        ) else {
            panic!("expected Run");
        };
        assert_eq!(b.endpoint, endpoint_from("/tmp/y.sock"));
    }
}
