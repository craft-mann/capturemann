//! `get_diagnostics` (DIAG-1): one block of text a user can paste into an issue. It says what runs
//! where (app, system, permissions, models, engines), the few settings that change behaviour, how
//! big the library is, how the last recording was really encoded and the last warnings and errors
//! from the log file. Nothing here reads audio, transcripts, titles or file names of recordings, and
//! nothing is sent anywhere: the text is only returned to the caller.
//!
//! The finished text goes through one last redaction pass (home folder to `~`, secrets scrubbed), on
//! top of the redaction the log lines already had when they were written.

use std::fmt::Write as _;
use std::path::Path;

use cm_core::logfile::{recent_lines, redact_home, LogLine};
use cm_core::CmError;
use serde_json::{json, Map, Value};

use crate::daemon::{blocking, lock_library, Inner};
use crate::DaemonError;

/// How many warning and error lines the report carries.
pub const LOG_LINES: usize = 200;

/// Setting keys (by prefix) that change how the app behaves and are safe to print. Everything
/// else, and every key `validate_setting_key` refuses (secrets), is left out.
const SETTING_PREFIXES: &[&str] = &["recording.", "capture.", "ai.", "appearance.language"];
/// Settings under an allowed prefix that are not printed (large or per-machine layout data).
const SETTING_SKIP: &[&str] = &["recording.hud_positions", "recording.fps60_selftest"];

/// One titled group of lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    pub lines: Vec<String>,
}

impl Section {
    fn new(title: &str, lines: Vec<String>) -> Section {
        Section {
            title: title.to_owned(),
            lines,
        }
    }
}

/// The report as text: `## Title` then one `- line` per line.
pub fn render_text(sections: &[Section]) -> String {
    let mut out = String::new();
    for s in sections {
        let _ = writeln!(out, "## {}", s.title);
        if s.lines.is_empty() {
            out.push_str("- (none)\n");
        }
        for l in &s.lines {
            let _ = writeln!(out, "- {l}");
        }
        out.push('\n');
    }
    out.trim_end().to_owned() + "\n"
}

/// The printable settings: allowed prefixes, no secrets, long values cut.
pub fn setting_lines(all: &Map<String, Value>) -> Vec<String> {
    let mut keys: Vec<&String> = all
        .keys()
        .filter(|k| SETTING_PREFIXES.iter().any(|p| k.starts_with(p)))
        .filter(|k| !SETTING_SKIP.contains(&k.as_str()))
        .filter(|k| crate::engines::validate_setting_key(k).is_ok())
        .collect();
    keys.sort();
    keys.into_iter()
        .map(|k| {
            let mut v = all[k].to_string();
            if v.chars().count() > 120 {
                v = v.chars().take(120).collect::<String>() + "...";
            }
            format!("{k} = {v}")
        })
        .collect()
}

/// How the last recording was really encoded, from its manifest's screen track (the recording's
/// title and file names are not printed).
pub fn last_recording_lines(manifest: &cm_library::manifest::RecManifest) -> Vec<String> {
    let mut out = Vec::new();
    let Some(screen) = manifest.tracks.get("screen") else {
        return vec!["the last recording has no screen track".to_owned()];
    };
    let d = screen.get("details").unwrap_or(screen);
    let pick = |k: &str| d.get(k).filter(|v| !v.is_null()).map(ToString::to_string);
    let mut parts = Vec::new();
    for k in [
        "codec",
        "size",
        "fps",
        "bitrate_bps",
        "quality",
        "late_pct",
        "stalls",
        "stopped_early",
    ] {
        if let Some(v) = pick(k) {
            parts.push(format!("{k} {v}"));
        }
    }
    out.push(format!(
        "{} recorded {}: {}",
        manifest.id,
        manifest.created.date(),
        parts.join(", ")
    ));
    if let Some(w) = manifest.extra.get("warnings") {
        out.push(format!("warnings {w}"));
    }
    out
}

/// The last run of each kind in this session (`transcribe`, `speakers`, ...): wall time, peak memory
/// and the time per phase. Nothing about the content of the audio.
pub fn run_lines() -> Vec<String> {
    cm_core::perf::records()
        .iter()
        .filter(|r| r.wall_secs.is_some())
        .map(cm_core::perf::summary_line)
        .collect()
}

/// Log lines as report lines.
pub fn log_report_lines(lines: &[LogLine]) -> Vec<String> {
    lines
        .iter()
        .map(|l| format!("{} {} {} {}", l.time, l.process, l.level, l.text))
        .collect()
}

#[cfg(target_os = "macos")]
fn sysctl(name: &str) -> Option<String> {
    let out = std::process::Command::new("sysctl")
        .args(["-n", name])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// The machine: OS, chip, memory, cores.
pub fn system_lines(stack: Option<&cm_platform::stack::Report>) -> Vec<String> {
    let mut out = Vec::new();
    out.push(format!(
        "{} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    #[cfg(target_os = "macos")]
    {
        if let Some(v) = sysctl("kern.osproductversion") {
            out.push(format!("macOS {v}"));
        }
        if let Some(c) = sysctl("machdep.cpu.brand_string") {
            out.push(format!("chip {c}"));
        }
        if let Some(m) = sysctl("hw.memsize").and_then(|s| s.parse::<u64>().ok()) {
            out.push(format!("memory {} GB", m / 1_000_000_000));
        }
        if let Some(n) = sysctl("hw.ncpu") {
            out.push(format!("cores {n}"));
        }
    }
    if let Ok(n) = std::thread::available_parallelism() {
        out.push(format!("threads available {n}"));
    }
    // Linux only (None elsewhere): the desktop stack the screenshot and recording paths depend on.
    if let Some(stack) = stack {
        out.extend(stack.fact_lines());
    }
    out.push(match cm_auth::backend_from_env() {
        cm_auth::Backend::Keyring => "secrets in the system credential store".to_owned(),
        cm_auth::Backend::File(_) => "secrets in a file (owner-only, not encrypted)".to_owned(),
    });
    out
}

fn permission_lines() -> Vec<String> {
    // Linux has no permission prompts to report: the portal, PipeWire and the encoder are.
    if let Some(lines) = crate::recording::sys::linux::diagnostic_permission_lines() {
        return lines;
    }
    vec![
        format!("microphone {:?}", cm_audio::microphone_permission()),
        format!(
            "screen recording {:?}",
            cm_audio::screen_recording_permission()
        ),
        format!(
            "camera {:?}",
            cm_camera::permission::check_camera_permission()
        ),
    ]
}

fn app_lines() -> Vec<String> {
    let exe = std::env::current_exe().ok();
    let in_bundle = exe
        .as_deref()
        .is_some_and(|p| p.to_string_lossy().contains(".app/Contents/"));
    vec![
        format!("CaptureMann {}", env!("CARGO_PKG_VERSION")),
        format!(
            "started from {}",
            if in_bundle {
                "an app bundle"
            } else {
                "a plain binary"
            }
        ),
        format!("config folder {}", crate::config::config_dir().display()),
        format!("log folder {}", crate::logs::log_dir().display()),
    ]
}

fn engine_lines() -> Vec<String> {
    let mut out = Vec::new();
    match cm_media::tools::locate() {
        Ok(t) => out.push(format!("ffmpeg at {}", t.ffmpeg.display())),
        Err(e) => out.push(format!("ffmpeg not found: {e}")),
    }
    match cm_ai::text_model::engine::locate_helper(
        &cm_ai::text_model::engine::SearchPaths::from_env(),
    ) {
        Ok(p) => out.push(format!("llama-server at {}", p.display())),
        Err(e) => out.push(format!("llama-server not found: {e}")),
    }
    out.push(cm_render::ocr::describe_system());
    out.push(format!(
        "speaker engine built in: {}",
        cfg!(feature = "sherpa")
    ));
    out
}

impl Inner {
    /// `get_diagnostics`: `{ text, sections: [{ title, lines }] }`.
    pub(crate) async fn get_diagnostics(&self) -> Result<Value, DaemonError> {
        // Linux only; asks the session bus a few read-only questions, so off the async threads.
        let stack = tokio::task::spawn_blocking(cm_platform::stack::probe)
            .await
            .ok()
            .flatten();
        let mut sections = vec![
            Section::new("App", app_lines()),
            Section::new("System", system_lines(stack.as_ref())),
            Section::new("Permissions", permission_lines()),
        ];
        let gaps = stack.map(|r| r.gap_lines()).unwrap_or_default();
        if !gaps.is_empty() {
            sections.push(Section::new("Missing on this system", gaps));
        }

        // Models and engines.
        let svc = self.ai.models.clone();
        // A section that cannot be read says so; the report is for broken setups too.
        let mut lines: Vec<String> = match blocking(move || svc.list()).await {
            Ok(models) => {
                let mut l: Vec<String> = models
                    .models
                    .iter()
                    .filter(|m| m.installed)
                    .map(|m| {
                        format!(
                            "speech model {}{} ({} MB)",
                            m.id,
                            if m.active { " (active)" } else { "" },
                            m.size_mb
                        )
                    })
                    .collect();
                if l.is_empty() {
                    l.push("no speech model installed".to_owned());
                }
                l
            }
            Err(e) => vec![format!("speech models unavailable: {e}")],
        };
        if let Ok(t) = self.get_text_model().await {
            lines.push(format!(
                "text model: provider {}, model {}, engine {}",
                t["provider"], t["embedded_model"], t["engine"]
            ));
        }
        lines.extend(engine_lines());
        sections.push(Section::new("Models and engines", lines));

        // Settings.
        let all = self.settings.all().map_err(DaemonError::from)?;
        sections.push(Section::new("Settings", setting_lines(&all)));

        // Library and the last recording.
        let (library, last) = {
            let lib = lock_library(&self.library);
            let list = lib
                .list(&cm_library::index::ListQuery::default())
                .map_err(DaemonError::from)?;
            let bytes: u64 = list.iter().map(|r| r.size_bytes).sum();
            let broken = list.iter().filter(|r| r.broken).count();
            let free = self.free_space.free_bytes(lib.root());
            let lines = vec![
                format!("{} captures, {} MB", list.len(), bytes / 1_000_000),
                format!("{broken} unreadable"),
                match free {
                    Some(f) => format!("free space on the library volume {} GB", f / 1_000_000_000),
                    None => "free space unknown".to_owned(),
                },
            ];
            let last = list
                .iter()
                .find(|r| r.kind == cm_library::manifest::CaptureKind::Recording && !r.broken)
                .map(|r| lib.bundle_path(r));
            (lines, last)
        };
        sections.push(Section::new("Library", library));
        let last_lines = match last {
            Some(dir) => last_recording_of(&dir),
            None => vec!["no recording yet".to_owned()],
        };
        sections.push(Section::new("Last recording", last_lines));

        // Where the time of the last runs of this session went (PERF-1); empty after a restart.
        sections.push(Section::new("Last runs", run_lines()));

        // The log.
        let dir = crate::logs::log_dir();
        let lines = blocking(move || Ok(recent_lines(&dir, "WARN", LOG_LINES))).await?;
        sections.push(Section::new(
            "Warnings and errors (newest last)",
            log_report_lines(&lines),
        ));

        let text = final_pass(&render_text(&sections));
        let sections: Vec<Value> = sections
            .iter()
            .map(|s| {
                json!({
                    "title": s.title,
                    "lines": s.lines.iter().map(|l| final_pass(l)).collect::<Vec<_>>(),
                })
            })
            .collect();
        Ok(json!({ "text": text, "sections": sections }))
    }
}

fn last_recording_of(dir: &Path) -> Vec<String> {
    match cm_library::rec::read_manifest(dir) {
        Ok(m) => last_recording_lines(&m),
        Err(CmError::NotFound { .. }) => {
            vec!["the last recording's manifest is missing".to_owned()]
        }
        Err(e) => vec![format!("the last recording's manifest cannot be read: {e}")],
    }
}

/// The last redaction pass over finished text.
fn final_pass(s: &str) -> String {
    let home = cm_platform::dirs::home_dir()
        .and_then(|h| h.to_str().map(str::to_owned))
        .filter(|h| h.len() > 1);
    redact_home(&cm_jobs::scrub_secrets(s), home.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_has_a_heading_per_section_and_a_dash_per_line() {
        let t = render_text(&[
            Section::new("A", vec!["one".into(), "two".into()]),
            Section::new("B", vec![]),
        ]);
        assert_eq!(t, "## A\n- one\n- two\n\n## B\n- (none)\n");
    }

    #[test]
    fn only_allowed_non_secret_settings_are_printed() {
        let all = serde_json::from_value::<Map<String, Value>>(json!({
            "recording.fps": 30,
            "recording.hud_positions": {"a": 1},
            "recording.fps60_selftest": {"v": 1},
            "ai.languages": "vi,en",
            "ai.text.api_key": "sk-123",
            "youtube.refresh_token": "abc",
            "library.path": "/Users/x/Movies",
            "appearance.language": "vi",
            "ai.long": "z".repeat(500),
        }))
        .unwrap();
        let lines = setting_lines(&all);
        let text = lines.join("\n");
        assert!(text.contains("recording.fps = 30"));
        assert!(text.contains("ai.languages = \"vi,en\""));
        assert!(text.contains("appearance.language"));
        for banned in [
            "hud_positions",
            "selftest",
            "api_key",
            "refresh",
            "library.path",
        ] {
            assert!(!text.contains(banned), "{banned}: {text}");
        }
        assert!(lines.iter().all(|l| l.len() < 200));
    }

    #[test]
    fn the_final_pass_hides_a_token_and_the_home_folder() {
        let out = final_pass("Authorization: Bearer abc.def.ghi at /nowhere");
        assert!(!out.contains("abc.def.ghi"), "{out}");
    }

    #[test]
    fn log_lines_keep_time_process_level_and_text() {
        let l = LogLine {
            time: "2026-10-29T10:00:00Z".into(),
            process: "desktop".into(),
            level: "WARN".into(),
            text: "cm_encode: x".into(),
        };
        assert_eq!(
            log_report_lines(&[l]),
            ["2026-10-29T10:00:00Z desktop WARN cm_encode: x"]
        );
    }
}
