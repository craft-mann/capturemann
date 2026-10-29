//! Phase timers and memory readings (PERF-1): where the time of a long run goes, kept in memory
//! for the diagnostics report and printed by the benchmark harness.
//!
//! A *run* is a named piece of work (`transcribe`, `speakers`, `report`). [`begin`] starts a fresh
//! record of it, [`span`] adds the time of one phase (`decode`, `whisper`, ...) when the guard drops
//! (a phase can be entered many times; the record sums the time and counts the entries) and
//! [`finish`] stamps the wall time and the process's peak memory. Only the last record of each run
//! name is kept, nothing is written to disk and nothing leaves the computer. A span costs two
//! clock reads and one short lock, so it is only put around work that takes milliseconds or more.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

/// The time of one phase of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct Phase {
    pub name: String,
    pub secs: f64,
    pub count: u32,
}

/// The record of one run.
#[derive(Debug, Clone, PartialEq)]
pub struct RunRecord {
    pub run: String,
    /// Wall time from [`begin`] to [`finish`]; `None` while the run is going.
    pub wall_secs: Option<f64>,
    /// Peak resident memory of the whole process at [`finish`] (it only ever grows).
    pub peak_rss_bytes: Option<u64>,
    pub phases: Vec<Phase>,
    started: Option<Instant>,
}

static RUNS: Mutex<BTreeMap<String, RunRecord>> = Mutex::new(BTreeMap::new());

fn runs() -> std::sync::MutexGuard<'static, BTreeMap<String, RunRecord>> {
    RUNS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Starts a fresh record of `run`, dropping the previous one.
pub fn begin(run: &str) {
    runs().insert(
        run.to_owned(),
        RunRecord {
            run: run.to_owned(),
            wall_secs: None,
            peak_rss_bytes: None,
            phases: Vec::new(),
            started: Some(Instant::now()),
        },
    );
}

/// Stamps the wall time and the peak memory on `run` and logs one line at debug level.
pub fn finish(run: &str) {
    let peak = peak_rss_bytes();
    let mut g = runs();
    let Some(r) = g.get_mut(run) else {
        return;
    };
    r.wall_secs = r.started.map(|s| s.elapsed().as_secs_f64());
    r.peak_rss_bytes = peak;
    log::debug!("perf: {}", summary_line(r));
}

/// A timer for one phase of `run`; the time is added when it drops.
#[must_use = "the phase is timed until this guard drops"]
pub struct Span {
    run: &'static str,
    phase: &'static str,
    start: Instant,
}

/// Times `phase` of `run` until the returned guard drops.
pub fn span(run: &'static str, phase: &'static str) -> Span {
    Span {
        run,
        phase,
        start: Instant::now(),
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        let secs = self.start.elapsed().as_secs_f64();
        let mut g = runs();
        let r = g.entry(self.run.to_owned()).or_insert_with(|| RunRecord {
            run: self.run.to_owned(),
            wall_secs: None,
            peak_rss_bytes: None,
            phases: Vec::new(),
            started: None,
        });
        match r.phases.iter_mut().find(|p| p.name == self.phase) {
            Some(p) => {
                p.secs += secs;
                p.count += 1;
            }
            None => r.phases.push(Phase {
                name: self.phase.to_owned(),
                secs,
                count: 1,
            }),
        }
    }
}

/// The last record of every run, by name.
pub fn records() -> Vec<RunRecord> {
    runs().values().cloned().collect()
}

/// Forgets every record (tests, and the harness between configurations).
pub fn clear() {
    runs().clear();
}

/// One line: `transcribe: 312.4 s wall, peak 2.1 GB; whisper 280.0 s (19), decode 9.1 s (1)`.
pub fn summary_line(r: &RunRecord) -> String {
    let mut s = format!("{}:", r.run);
    match r.wall_secs {
        Some(w) => s.push_str(&format!(" {w:.1} s wall")),
        None => s.push_str(" running"),
    }
    if let Some(b) = r.peak_rss_bytes {
        s.push_str(&format!(", peak {:.1} GB", b as f64 / 1e9));
    }
    let parts: Vec<String> = r
        .phases
        .iter()
        .map(|p| format!("{} {:.1} s ({})", p.name, p.secs, p.count))
        .collect();
    if !parts.is_empty() {
        s.push_str("; ");
        s.push_str(&parts.join(", "));
    }
    s
}

/// The peak resident memory of this process so far, in bytes (`None` where it cannot be read).
pub fn peak_rss_bytes() -> Option<u64> {
    #[cfg(unix)]
    {
        // SAFETY: `getrusage` fills the zeroed struct we hand it and nothing else.
        let ru = unsafe {
            let mut ru: libc::rusage = std::mem::zeroed();
            if libc::getrusage(libc::RUSAGE_SELF, &mut ru) != 0 {
                return None;
            }
            ru
        };
        let max = u64::try_from(ru.ru_maxrss).ok()?;
        // macOS reports bytes, Linux kilobytes.
        Some(if cfg!(target_os = "macos") {
            max
        } else {
            max * 1024
        })
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// How many performance cores an Apple Silicon Mac has (its efficiency cores slow a run that
/// waits for its slowest thread). `None` on any other computer, where nothing is known about the
/// cores beyond their count.
pub fn performance_cores() -> Option<usize> {
    #[cfg(target_os = "macos")]
    {
        let mut n: libc::c_int = 0;
        let mut len = std::mem::size_of::<libc::c_int>();
        // SAFETY: the name is a NUL-terminated literal and `n`/`len` are valid for `len` bytes.
        let ok = unsafe {
            libc::sysctlbyname(
                c"hw.perflevel0.logicalcpu".as_ptr(),
                (&raw mut n).cast(),
                &raw mut len,
                std::ptr::null_mut(),
                0,
            ) == 0
        };
        if ok && n > 0 {
            return usize::try_from(n).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // The records are process-wide, so each test uses run names of its own.
    #[test]
    fn performance_cores_never_exceeds_the_logical_cores() {
        if let Some(n) = performance_cores() {
            assert!(n >= 1 && n <= std::thread::available_parallelism().unwrap().get());
        }
    }

    #[test]
    fn spans_add_up_per_phase_and_count_entries() {
        begin("t_spans");
        for _ in 0..3 {
            let _s = span("t_spans", "work");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        {
            let _s = span("t_spans", "other");
        }
        finish("t_spans");
        let r = records().into_iter().find(|r| r.run == "t_spans").unwrap();
        let work = r.phases.iter().find(|p| p.name == "work").unwrap();
        assert_eq!(work.count, 3);
        assert!(work.secs >= 0.012, "{}", work.secs);
        assert_eq!(r.phases.len(), 2);
        assert!(r.wall_secs.unwrap() >= work.secs);
        assert!(summary_line(&r).starts_with("t_spans: "));
    }

    #[test]
    fn begin_drops_the_previous_record() {
        begin("t_again");
        {
            let _s = span("t_again", "a");
        }
        begin("t_again");
        let r = records().into_iter().find(|r| r.run == "t_again").unwrap();
        assert!(r.phases.is_empty());
        assert!(r.wall_secs.is_none());
        assert!(summary_line(&r).contains("running"));
    }

    #[cfg(unix)]
    #[test]
    fn the_peak_memory_is_a_plausible_number() {
        let b = peak_rss_bytes().unwrap();
        assert!((1_000_000..1_000_000_000_000).contains(&b), "{b}");
    }
}
