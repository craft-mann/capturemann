//! S4 spike: cursor telemetry.
//!
//! Records cursor moves, button down/up (+ modifier flags only, never keys),
//! scroll direction and cursor images (+ hotspot), each stamped with the OS
//! monotonic capture time converted to MediaTime ticks (120,000/s) against a
//! session origin. Output: `cursor.jsonl` and `cursors/<id>.png`.
//!
//! macOS: listen-only CGEventTap + NSCursor image.
//! Windows: WH_MOUSE_LL hook + GetCursorInfo (UNTESTED, written blind).
//!
//! Usage:
//!   spike-cursor record     [--secs N] [--out DIR]   (default 10 s, Ctrl-C stops)
//!   spike-cursor selfcheck  [--out DIR] [--left-clicks]
//!
//! `selfcheck` synthesizes events (CGEventPost) and measures event timestamp
//! error vs. the time just before posting, callback latency, jitter,
//! monotonicity and dropped clicks. It MOVES YOUR REAL CURSOR. Clicks use the
//! middle button by default so nothing gets activated; --left-clicks adds
//! left clicks in the top-right menu bar corner area.

use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    io::{BufWriter, Write},
    path::PathBuf,
    sync::mpsc,
    thread,
    time::Duration,
};

use anyhow::Result;

pub const TICKS_PER_SEC: i64 = 120_000;

pub fn ns_to_ticks(ns: u64, origin_ns: u64) -> i64 {
    let d = ns as i128 - origin_ns as i128;
    (d * TICKS_PER_SEC as i128 / 1_000_000_000) as i64
}

pub fn mods_json(m: u8) -> String {
    let mut v = vec![];
    if m & 1 != 0 { v.push("\"shift\""); }
    if m & 2 != 0 { v.push("\"ctrl\""); }
    if m & 4 != 0 { v.push("\"alt\""); }
    if m & 8 != 0 { v.push("\"cmd\""); }
    format!("[{}]", v.join(","))
}

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
use mac as platform;
#[cfg(windows)]
mod win;
#[cfg(windows)]
use win as platform;

#[cfg(not(any(target_os = "macos", windows)))]
compile_error!("spike-cursor supports macOS and Windows only");

pub enum Rec {
    Line(String),
    Done,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("record");
    let flag = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let out = PathBuf::from(flag("--out").unwrap_or_else(|| format!("spike-out/cursor/{cmd}")));
    let secs: f64 = flag("--secs").and_then(|s| s.parse().ok()).unwrap_or(10.0);
    fs::create_dir_all(out.join("cursors"))?;
    match cmd {
        "record" => platform::record(&out, secs),
        "selfcheck" => platform::selfcheck(&out, args.iter().any(|a| a == "--left-clicks")),
        _ => {
            eprintln!("usage: spike-cursor record [--secs N] [--out DIR] | selfcheck [--out DIR] [--left-clicks]");
            Ok(())
        }
    }
}

/// Spawn the writer thread; returns a sender of lines and a join handle.
pub fn spawn_writer(path: PathBuf) -> (mpsc::Sender<Rec>, thread::JoinHandle<u64>) {
    let (tx, rx) = mpsc::channel::<Rec>();
    let h = thread::spawn(move || {
        let mut w = BufWriter::new(fs::File::create(path).expect("create cursor.jsonl"));
        let mut n = 0u64;
        let mut last_flush = std::time::Instant::now();
        loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(Rec::Line(l)) => {
                    let _ = writeln!(w, "{l}");
                    n += 1;
                }
                Ok(Rec::Done) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if last_flush.elapsed() > Duration::from_secs(1) {
                let _ = w.flush(); // append-only, crash-safe-ish (REC-8)
                last_flush = std::time::Instant::now();
            }
        }
        let _ = w.flush();
        n
    });
    (tx, h)
}

pub fn hash_id(bytes: &[u8]) -> String {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    format!("c_{:08x}", h.finish() as u32)
}
