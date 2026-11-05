//! `cm-regress`: the release regression suite. It drives the SHIPPED package (the programs of the
//! installed folder or the unpacked zip) as a black box and writes `results.json` and a Markdown
//! report. It links nothing from the product, so it cannot hide a product bug. How to run it and how
//! to add a case: `docs/development/regression.md`.

#![cfg_attr(test, allow(clippy::unwrap_used))]

mod cases;
mod ctx;
mod fixtures;
mod hygiene;
mod mcp;
mod media;
mod model;
mod proc;
mod report;
mod runner;
mod spec;
mod sys;
mod util;

use ctx::Options;
use model::{RunEnv, Tier};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

const USAGE: &str = "usage: cm-regress [--tier smoke|standard|full|hardware] [--only <id|area>,...] [--allow screen,sound,mic,camera,second_monitor,models,ffmpeg_full,network]
                  [--package-dir <dir>] [--out <dir>] [--keep] [--list] [--dev]
                  [--installer <setup.exe>] [--zip <file>] [--commit <sha>] [--whisper-model <ggml file>] [--whisper-model-id <id>]
                  [--speech <audio file>] [--speaker-models <dir>]
                  [--soak-minutes <n>] [--timeout-scale <f>] [--verdict <text>] [--who <name>]

Runs the regression cases against the programs in --package-dir (default: the per-user install folder, else the
folder of cm-regress). Every case uses its own temporary library, config folder and endpoint. Writes results.json
and report-<tier>.md to --out. Exit code 0 when no case failed, 1 when one did, 2 on a usage error.";

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        tier: Tier::Smoke,
        only: Vec::new(),
        allow: BTreeSet::new(),
        package_dir: PathBuf::new(),
        out_dir: PathBuf::from("cm-regress-out"),
        keep: false,
        list: false,
        dev: false,
        installer: None,
        zip: None,
        commit: None,
        whisper_model: None,
        whisper_model_id: "small".to_owned(),
        speech_audio: None,
        speaker_models: None,
        soak_minutes: 0,
        timeout_scale: 1.0,
        verdict: "PARTIAL".to_owned(),
        who: "cm-regress (automated)".to_owned(),
        started: SystemTime::now(),
    };
    let mut package: Option<PathBuf> = None;
    let mut i = 0;
    let value = |i: &mut usize, flag: &str| -> Result<String, String> {
        *i += 1;
        args.get(*i)
            .cloned()
            .ok_or_else(|| format!("{flag} needs a value"))
    };
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--tier" => {
                let v = value(&mut i, a)?;
                o.tier = Tier::parse(&v).ok_or_else(|| format!("unknown tier `{v}`"))?;
            }
            "--only" => {
                o.only = value(&mut i, a)?
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            }
            "--allow" => {
                for w in value(&mut i, a)?.split(',').filter(|s| !s.is_empty()) {
                    let known = [
                        "screen",
                        "sound",
                        "mic",
                        "camera",
                        "second_monitor",
                        "models",
                        "ffmpeg_full",
                        "network",
                    ];
                    if !known.contains(&w) {
                        return Err(format!(
                            "unknown --allow word `{w}` (known: {})",
                            known.join(", ")
                        ));
                    }
                    o.allow.insert(w.to_owned());
                }
            }
            "--package-dir" => package = Some(PathBuf::from(value(&mut i, a)?)),
            "--out" => o.out_dir = PathBuf::from(value(&mut i, a)?),
            "--keep" => o.keep = true,
            "--list" => o.list = true,
            "--dev" => o.dev = true,
            "--installer" => o.installer = Some(PathBuf::from(value(&mut i, a)?)),
            "--zip" => o.zip = Some(PathBuf::from(value(&mut i, a)?)),
            "--commit" => o.commit = Some(value(&mut i, a)?),
            "--whisper-model" => {
                o.whisper_model = Some(PathBuf::from(value(&mut i, a)?));
                o.allow.insert("models".to_owned());
            }
            "--whisper-model-id" => o.whisper_model_id = value(&mut i, a)?,
            "--speech" => o.speech_audio = Some(PathBuf::from(value(&mut i, a)?)),
            "--speaker-models" => {
                o.speaker_models = Some(PathBuf::from(value(&mut i, a)?));
                o.allow.insert("models".to_owned());
            }
            "--soak-minutes" => {
                o.soak_minutes = value(&mut i, a)?
                    .parse()
                    .map_err(|_| "--soak-minutes needs a whole number".to_owned())?
            }
            "--timeout-scale" => {
                o.timeout_scale = value(&mut i, a)?
                    .parse()
                    .map_err(|_| "--timeout-scale needs a number".to_owned())?;
                if !(o.timeout_scale > 0.0 && o.timeout_scale <= 20.0) {
                    return Err("--timeout-scale must be between 0 and 20".to_owned());
                }
            }
            "--verdict" => o.verdict = value(&mut i, a)?,
            "--who" => o.who = value(&mut i, a)?,
            "--help" | "-h" => return Err(String::new()),
            other => return Err(format!("unknown argument `{other}`")),
        }
        i += 1;
    }
    o.package_dir = match package {
        Some(p) => p,
        None => default_package_dir()
            .ok_or("no --package-dir given and no installed CaptureMann found")?,
    };
    Ok(o)
}

fn default_package_dir() -> Option<PathBuf> {
    let has = |d: &PathBuf| d.join(sys::exe_name("capturemann")).is_file();
    if let Some(d) = sys::default_install_dir().filter(has) {
        return Some(d);
    }
    let own = std::env::current_exe().ok()?.parent()?.to_path_buf();
    if has(&own) {
        return Some(own);
    }
    None
}

fn command_output(program: &str, args: &[&str]) -> String {
    let mut c = Command::new(program);
    c.args(args);
    sys::hide_console(&mut c);
    c.output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn os_and_gpu() -> (String, String) {
    if sys::PLATFORM == "windows" {
        let os = command_output(
            "powershell",
            &["-NoProfile", "-Command", "(Get-CimInstance Win32_OperatingSystem | ForEach-Object { $_.Caption + ' ' + $_.Version + ' build ' + $_.BuildNumber })"],
        );
        let gpu = command_output(
            "powershell",
            &["-NoProfile", "-Command", "(Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name + ' (driver ' + $_.DriverVersion + ')' }) -join '; '"],
        );
        (os, gpu)
    } else {
        (command_output("uname", &["-srm"]), "n/a".to_owned())
    }
}

fn run_info(o: &Options, flags: String, started: SystemTime) -> report::RunInfo {
    let (os, gpu) = os_and_gpu();
    let pkg = ctx::Pkg {
        dir: o.package_dir.clone(),
    };
    let version = command_output(&pkg.exe("capturemann").to_string_lossy(), &["--version"]);
    let ffmpeg_exe = if pkg.has("ffmpeg") {
        pkg.exe("ffmpeg")
    } else {
        PathBuf::from("ffmpeg")
    };
    let ffmpeg = command_output(&ffmpeg_exe.to_string_lossy(), &["-hide_banner", "-version"])
        .lines()
        .next()
        .unwrap_or("not found")
        .to_owned();
    let mut hashes = Vec::new();
    for base in [
        "capturemann",
        "cm-mcp",
        "cm-daemon",
        "cm-desktop",
        "cm-overlay",
        "ffmpeg",
        "ffprobe",
        "llama-server",
    ] {
        if pkg.has(base) {
            if let Ok(h) = util::sha256_file(&pkg.exe(base)) {
                hashes.push((sys::exe_name(base), h));
            }
        }
    }
    for extra in [&o.zip, &o.installer].into_iter().flatten() {
        if let Ok(h) = util::sha256_file(extra) {
            hashes.push((
                extra
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                h,
            ));
        }
    }
    let commit = o
        .commit
        .clone()
        .or_else(|| std::env::var("CM_REGRESS_COMMIT").ok())
        .or_else(|| {
            let t = std::fs::read_to_string(o.package_dir.join("BUILD.txt")).ok()?;
            t.lines()
                .find_map(|l| l.strip_prefix("commit:"))
                .map(|s| s.trim().to_owned())
        })
        .unwrap_or_default();
    report::RunInfo {
        started: util::utc_iso(started),
        finished: String::new(),
        who: o.who.clone(),
        tier: o.tier.name().to_owned(),
        flags,
        platform: sys::PLATFORM.to_owned(),
        os,
        gpu,
        commit,
        version: version.trim_start_matches("capturemann").trim().to_owned(),
        package_dir: o.package_dir.display().to_string(),
        hashes,
        ffmpeg,
        verdict: o.verdict.clone(),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = match parse_args(&args) {
        Ok(o) => o,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("cm-regress: {e}");
            }
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    sys::make_dpi_aware();
    let all = cases::all();
    let has_desktop = sys::has_desktop();
    let env = RunEnv {
        tier: opts.tier,
        only_selected: !opts.only.is_empty(),
        allow: opts.allow.clone(),
        has_desktop,
        platform: sys::PLATFORM,
        have_installer: opts.installer.is_some(),
        have_models: opts.whisper_model.is_some() || opts.speaker_models.is_some(),
    };
    let selected = runner::select(&all, &opts.only, opts.tier);
    if opts.list {
        println!(
            "{:<34} {:<9} {:<9} {:<9} title",
            "id", "tier", "area", "platforms"
        );
        for c in &selected {
            let plats: Vec<&str> = c
                .platforms
                .iter()
                .map(|p| match p {
                    model::Platform::Windows => "win",
                    model::Platform::MacOs => "mac",
                    model::Platform::Linux => "linux",
                })
                .collect();
            let needs: Vec<&str> = c.needs.iter().map(|n| n.name()).collect();
            let why = model::not_runnable(c, &env)
                .map(|r| format!("  [skip: {r}]"))
                .unwrap_or_default();
            println!(
                "{:<34} {:<9} {:<9} {:<9} {} (needs: {}){}",
                c.id,
                c.tier.name(),
                c.area,
                plats.join(","),
                c.title,
                needs.join(","),
                why
            );
        }
        println!("{} cases", selected.len());
        return ExitCode::SUCCESS;
    }
    if !opts
        .package_dir
        .join(sys::exe_name("capturemann"))
        .is_file()
    {
        eprintln!(
            "cm-regress: {} has no {}",
            opts.package_dir.display(),
            sys::exe_name("capturemann")
        );
        return ExitCode::from(2);
    }
    if let Err(e) = std::fs::create_dir_all(&opts.out_dir) {
        eprintln!("cm-regress: cannot create {}: {e}", opts.out_dir.display());
        return ExitCode::from(2);
    }
    let started = SystemTime::now();
    let mut info = run_info(&opts, args.join(" "), started);
    println!(
        "cm-regress: {} {} on {}",
        info.platform, info.version, info.os
    );
    println!("package: {}", info.package_dir);
    println!(
        "tier {} ({} cases selected), desktop session: {}",
        opts.tier.name(),
        selected.len(),
        has_desktop
    );

    let opts = Arc::new(opts);
    let mut runner = runner::Runner::new(opts.clone(), env);
    let mut results: Vec<report::CaseResult> = Vec::new();
    let json_path = opts.out_dir.join("results.json");
    for case in selected {
        let r = runner.run_case(case);
        println!(
            "{:<4} {:<34} {:>6.1}s  {}",
            r.status,
            r.id,
            r.duration_ms as f64 / 1000.0,
            util::cut(&util::one_line(&r.message), 160)
        );
        results.push(r);
        info.finished = util::utc_iso(SystemTime::now());
        let _ = std::fs::write(
            &json_path,
            serde_json::to_vec_pretty(&report::results_json(&info, &results)).unwrap_or_default(),
        );
    }
    runner.finish();
    info.finished = util::utc_iso(SystemTime::now());
    let _ = std::fs::write(
        &json_path,
        serde_json::to_vec_pretty(&report::results_json(&info, &results)).unwrap_or_default(),
    );
    let md_path = opts.out_dir.join(format!("report-{}.md", opts.tier.name()));
    let _ = std::fs::write(&md_path, report::markdown(&info, &results));
    let fails = results.iter().filter(|r| r.status == "FAIL").count();
    let passes = results.iter().filter(|r| r.status == "PASS").count();
    let skips = results.iter().filter(|r| r.status == "SKIP").count();
    println!(
        "\n{passes} passed, {fails} failed, {skips} skipped; {}",
        md_path.display()
    );
    // A short pause so a slow process table settles before the caller checks for strays.
    std::thread::sleep(Duration::from_millis(100));
    if fails > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn flags_parse() {
        let o = parse_args(&a("--package-dir /x --tier full --only mcp,rec.screen --allow screen,sound --keep --soak-minutes 5 --timeout-scale 2.5")).unwrap();
        assert_eq!(o.tier, Tier::Full);
        assert_eq!(o.only, ["mcp", "rec.screen"]);
        assert!(o.allow.contains("screen") && o.allow.contains("sound") && o.keep);
        assert_eq!((o.soak_minutes, o.timeout_scale), (5, 2.5));
        assert_eq!(o.verdict, "PARTIAL");
    }

    #[test]
    fn bad_flags_are_refused() {
        assert!(parse_args(&a("--package-dir /x --tier nope")).is_err());
        assert!(parse_args(&a("--package-dir /x --allow teleport")).is_err());
        assert!(parse_args(&a("--package-dir /x --bogus")).is_err());
        assert!(parse_args(&a("--package-dir")).is_err());
        assert!(parse_args(&a("--package-dir /x --timeout-scale 0")).is_err());
    }

    #[test]
    fn a_whisper_model_implies_models_allowed() {
        let o = parse_args(&a("--package-dir /x --whisper-model /m/ggml-small.bin")).unwrap();
        assert!(o.allow.contains("models") && o.whisper_model.is_some());
    }
}
