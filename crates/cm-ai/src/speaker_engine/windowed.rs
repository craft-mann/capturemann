//! Speaker finding on long audio in windows (M2b): the memory of one engine run grows with the
//! length of the audio and the library ignores a stop until the run ends, so a recording longer
//! than [`Tuning::long_secs`] is cut into windows of about [`Tuning::window_secs`] at quiet spots,
//! each window is run on its own (with the automatic count), and the speakers of the windows are
//! linked by their voice prints: one print per speaker and window (the mean of the prints of that
//! speaker's longest turns), then agglomerative linking by cosine distance. Two speakers of one
//! window are never linked to each other. With a known count the linking goes on until that many
//! remain; otherwise it stops at [`Tuning::link_distance`].
//!
//! A stop is noticed between windows and inside a window's run at the engine's own pace (a window
//! takes seconds, not the minutes of a whole recording). If the engine cannot compute voice prints
//! ([`TurnEngine::embed_all`] answers `None`), the caller runs the whole audio as before.

use cm_core::CmError;

use super::{fold_small_speakers, tidy_turns, ClusterParams, SpeakerModels, Turn, TurnEngine};
use crate::speakers::DiarizeHints;
use crate::transcribe::cancelled;

/// How long audio is run whole, how long a window is, and how close two prints must be to be one
/// speaker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tuning {
    /// Audio up to this long (seconds) is run in one piece, as before.
    pub long_secs: f64,
    /// The length of a window (seconds); the cut moves up to [`CUT_SEARCH_SECS`] to a quiet spot.
    pub window_secs: f64,
    /// Cosine distance (1 - similarity) under which two speakers of different windows are one.
    pub link_distance: f32,
    /// When an automatic run finds more speakers than the maximum: run the whole audio again with
    /// exactly the maximum (the 0.2.6 way), instead of giving the extra speakers to the nearest
    /// kept ones. Only for the measurement of the two.
    pub second_run_for_max: bool,
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning {
            long_secs: 1200.0,
            window_secs: 600.0,
            link_distance: 0.5,
            second_run_for_max: false,
        }
    }
}

/// How far a window boundary may move to find a quiet spot (seconds).
const CUT_SEARCH_SECS: f64 = 10.0;
/// Most turns of a speaker whose prints are averaged, and the longest stretch of one turn used.
const PRINT_TURNS: usize = 6;
const PRINT_MAX_SECS: f64 = 8.0;
const RATE: usize = 16_000;

/// Window boundaries in samples: 0, the cuts, the end.
pub fn window_cuts(samples: &[f32], window_secs: f64) -> Vec<usize> {
    let win = (window_secs * RATE as f64) as usize;
    let search = (CUT_SEARCH_SECS * RATE as f64) as usize;
    let frame = RATE / 10;
    let mut cuts = vec![0];
    let mut at = 0usize;
    while samples.len() - at > win + win / 2 {
        let target = at + win;
        let (lo, hi) = (target - search, (target + search).min(samples.len()));
        let mut best = (f32::MAX, target);
        let mut i = lo;
        while i + frame <= hi {
            let e = samples[i..i + frame].iter().map(|s| s * s).sum::<f32>();
            if e < best.0 {
                best = (e, i + frame / 2);
            }
            i += frame;
        }
        cuts.push(best.1);
        at = best.1;
    }
    cuts.push(samples.len());
    cuts
}

struct Item {
    window: usize,
    local: i32,
    print: Vec<f32>,
    secs: f64,
}

fn unit(mut v: Vec<f32>) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        v.iter_mut().for_each(|x| *x /= n);
    }
    v
}

fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        return 1.0;
    }
    1.0 - dot / (na * nb)
}

/// Runs `samples` in windows. `Ok(None)` when the engine cannot compute prints.
pub fn run_windowed(
    engine: &dyn TurnEngine,
    models: &SpeakerModels,
    samples: &[f32],
    hints: &DiarizeHints,
    tuning: Tuning,
    auto_threshold: f32,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<Option<Vec<Turn>>, CmError> {
    let cuts = window_cuts(samples, tuning.window_secs);
    let windows = cuts.len() - 1;
    let auto = ClusterParams {
        num_clusters: None,
        threshold: auto_threshold,
    };
    let mut per_window: Vec<Vec<Turn>> = Vec::with_capacity(windows);
    let mut items: Vec<Item> = Vec::new();
    for w in 0..windows {
        let audio = &samples[cuts[w]..cuts[w + 1]];
        let (lo, hi) = (w as f32 / windows as f32, (w + 1) as f32 / windows as f32);
        if !progress(lo) {
            return Err(cancelled());
        }
        let turns = fold_small_speakers(engine.run(models, audio, auto, &mut |f| {
            progress(lo + (hi - lo) * 0.9 * f)
        })?);
        // One print per speaker of this window.
        let mut ids: Vec<i32> = turns.iter().map(|t| t.speaker).collect();
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            let mut own: Vec<&Turn> = turns.iter().filter(|t| t.speaker == id).collect();
            let secs: f64 = own.iter().map(|t| t.end - t.start).sum();
            own.sort_by(|a, b| (b.end - b.start).total_cmp(&(a.end - a.start)));
            let clips: Vec<&[f32]> = own
                .iter()
                .take(PRINT_TURNS)
                .map(|t| {
                    let a = (t.start * RATE as f64) as usize;
                    let b =
                        ((t.start + (t.end - t.start).min(PRINT_MAX_SECS)) * RATE as f64) as usize;
                    &audio[a.min(audio.len())..b.min(audio.len())]
                })
                .filter(|c| c.len() >= RATE / 2)
                .collect();
            if clips.is_empty() {
                continue;
            }
            let Some(prints) = engine.embed_all(models, &clips)? else {
                return Ok(None);
            };
            let mut mean = vec![0.0f32; prints.first().map_or(0, Vec::len)];
            for p in &prints {
                for (m, x) in mean.iter_mut().zip(unit(p.clone())) {
                    *m += x;
                }
            }
            items.push(Item {
                window: w,
                local: id,
                print: unit(mean),
                secs,
            });
        }
        per_window.push(turns);
        if !progress(hi) {
            return Err(cancelled());
        }
    }
    if items.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let global = link(&items, hints, tuning.link_distance);
    let mut out = Vec::new();
    for (w, turns) in per_window.into_iter().enumerate() {
        let offset = cuts[w] as f64 / RATE as f64;
        for t in turns {
            let Some(i) = items
                .iter()
                .position(|it| it.window == w && it.local == t.speaker)
            else {
                continue;
            };
            out.push(Turn {
                start: t.start + offset,
                end: t.end + offset,
                speaker: global[i],
            });
        }
    }
    Ok(Some(tidy_turns(out)))
}

/// Agglomerative linking of the items: the global speaker number of each.
fn link(items: &[Item], hints: &DiarizeHints, threshold: f32) -> Vec<i32> {
    // Clusters of item indexes with a duration-weighted mean print.
    let mut clusters: Vec<(Vec<usize>, Vec<f32>)> = items
        .iter()
        .enumerate()
        .map(|(i, it)| (vec![i], it.print.clone()))
        .collect();
    let target = hints.num_speakers.map(|n| n as usize);
    let max = hints.max_speakers.unwrap_or(super::AUTO_MAX_SPEAKERS) as usize;
    let min = hints.min_speakers.unwrap_or(1) as usize;
    loop {
        let must = match target {
            Some(n) => clusters.len() > n,
            None => clusters.len() > max,
        };
        let floor = target.unwrap_or(min).max(1);
        if clusters.len() <= floor && !must {
            break;
        }
        // The closest pair that shares no window; if every pair shares one and a merge is still
        // required, the closest pair of all.
        let conflict = |a: &Vec<usize>, b: &Vec<usize>| {
            a.iter()
                .any(|&i| b.iter().any(|&j| items[i].window == items[j].window))
        };
        let mut best: Option<(f32, usize, usize)> = None;
        let mut best_any: Option<(f32, usize, usize)> = None;
        for i in 0..clusters.len() {
            for j in i + 1..clusters.len() {
                let d = cosine_distance(&clusters[i].1, &clusters[j].1);
                if best_any.is_none_or(|b| d < b.0) {
                    best_any = Some((d, i, j));
                }
                if !conflict(&clusters[i].0, &clusters[j].0) && best.is_none_or(|b| d < b.0) {
                    best = Some((d, i, j));
                }
            }
        }
        let pick = match (best, must) {
            (Some(b), true) => Some(b),
            (Some(b), false) if b.0 < threshold && target.is_none() => Some(b),
            (None, true) => best_any,
            _ => None,
        };
        let Some((_, i, j)) = pick else { break };
        let (b_items, b_print) = clusters.remove(j);
        let (a_items, a_print) = &mut clusters[i];
        let wa: f64 = a_items.iter().map(|&k| items[k].secs).sum();
        let wb: f64 = b_items.iter().map(|&k| items[k].secs).sum();
        let merged: Vec<f32> = a_print
            .iter()
            .zip(&b_print)
            .map(|(x, y)| ((f64::from(*x) * wa + f64::from(*y) * wb) / (wa + wb).max(1e-9)) as f32)
            .collect();
        a_items.extend(b_items);
        *a_print = unit(merged);
    }
    // Number the clusters by the speech they hold, largest first.
    clusters.sort_by(|a, b| {
        let t = |c: &Vec<usize>| c.iter().map(|&k| items[k].secs).sum::<f64>();
        t(&b.0).total_cmp(&t(&a.0))
    });
    let mut out = vec![0; items.len()];
    for (g, (members, _)) in clusters.iter().enumerate() {
        for &m in members {
            out[m] = i32::try_from(g).unwrap_or(0);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI32, Ordering};

    use super::*;
    use crate::speaker_engine::run_with_hints_tuned;

    const RATE: usize = 16_000;

    /// Audio whose sample value says who speaks (0.1 per speaker, 0.0 is a pause): 20 s blocks,
    /// speaker 1, 2, 3 in turn with 1 s pauses.
    fn audio(blocks: usize) -> Vec<f32> {
        let mut v = Vec::new();
        for b in 0..blocks {
            v.extend(std::iter::repeat_n(0.1 * ((b % 3) + 1) as f32, 19 * RATE));
            v.extend(std::iter::repeat_n(0.0, RATE));
        }
        v
    }

    /// An engine that hears the speakers from the sample values but numbers them differently in
    /// every run (as clustering of separate windows does), and whose prints are one-hot.
    struct Permuting {
        runs: AtomicI32,
        prints: bool,
    }

    impl TurnEngine for Permuting {
        fn run(
            &self,
            _: &SpeakerModels,
            samples: &[f32],
            _: ClusterParams,
            progress: &mut dyn FnMut(f32) -> bool,
        ) -> Result<Vec<Turn>, CmError> {
            if !progress(0.5) {
                return Err(cancelled());
            }
            let shift = self.runs.fetch_add(1, Ordering::SeqCst);
            let mut out = Vec::new();
            let mut start = None::<(usize, i32)>;
            for (i, s) in samples
                .iter()
                .enumerate()
                .chain(std::iter::once((samples.len(), &0.0)))
            {
                let who = (*s * 10.0).round() as i32;
                match (start, who) {
                    (Some((a, w)), x) if x != w => {
                        out.push(Turn {
                            start: a as f64 / RATE as f64,
                            end: i as f64 / RATE as f64,
                            speaker: (w + shift) % 3,
                        });
                        start = (x > 0).then_some((i, x));
                    }
                    (None, x) if x > 0 => start = Some((i, x)),
                    _ => {}
                }
            }
            Ok(out)
        }

        fn embed_all(
            &self,
            _: &SpeakerModels,
            clips: &[&[f32]],
        ) -> Result<Option<Vec<Vec<f32>>>, CmError> {
            if !self.prints {
                return Ok(None);
            }
            Ok(Some(
                clips
                    .iter()
                    .map(|c| {
                        let who = (c[c.len() / 2] * 10.0).round() as usize;
                        let mut p = vec![0.0; 4];
                        p[who.min(3)] = 1.0;
                        p
                    })
                    .collect(),
            ))
        }
    }

    fn models() -> SpeakerModels {
        SpeakerModels {
            segmentation: "s".into(),
            embedding: "e".into(),
        }
    }

    fn small() -> Tuning {
        Tuning {
            long_secs: 100.0,
            window_secs: 100.0,
            link_distance: 0.5,
            ..Tuning::default()
        }
    }

    /// The same speaker keeps one number across windows although every window numbered them
    /// differently, and the turns cover the file.
    #[test]
    fn speakers_are_linked_across_windows() {
        let x = audio(18); // 360 s, windows of about 100 s
        let e = Permuting {
            runs: AtomicI32::new(0),
            prints: true,
        };
        let turns = run_with_hints_tuned(
            &e,
            &models(),
            &x,
            &DiarizeHints::default(),
            small(),
            &mut |_| true,
        )
        .unwrap();
        assert!(e.runs.load(Ordering::SeqCst) >= 3, "windows were run");
        let mut ids: Vec<i32> = turns.iter().map(|t| t.speaker).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 3, "{turns:?}");
        // Block b is spoken by speaker (b % 3): all blocks of one speaker share a number.
        let who_of = |b: usize| {
            let at = (b as f64 * 20.0) + 5.0;
            turns
                .iter()
                .find(|t| t.start <= at && t.end >= at)
                .unwrap()
                .speaker
        };
        for b in 0..15 {
            assert_eq!(who_of(b), who_of(b + 3), "block {b}");
        }
        assert_ne!(who_of(0), who_of(1));
    }

    /// A known count is the number of speakers left whatever the distance.
    #[test]
    fn a_known_count_is_kept() {
        let x = audio(18);
        let e = Permuting {
            runs: AtomicI32::new(0),
            prints: true,
        };
        let hints = DiarizeHints {
            num_speakers: Some(2),
            ..DiarizeHints::default()
        };
        let turns =
            run_with_hints_tuned(&e, &models(), &x, &hints, small(), &mut |_| true).unwrap();
        let mut ids: Vec<i32> = turns.iter().map(|t| t.speaker).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 2, "{ids:?}");
    }

    /// Stop between windows: the run ends at once with the cancelled answer.
    #[test]
    fn cancel_stops_between_windows() {
        let x = audio(18);
        let e = Permuting {
            runs: AtomicI32::new(0),
            prints: true,
        };
        let mut calls = 0;
        let r = run_with_hints_tuned(
            &e,
            &models(),
            &x,
            &DiarizeHints::default(),
            small(),
            &mut |_| {
                calls += 1;
                calls < 4
            },
        );
        assert!(crate::transcribe::is_cancelled(&r.unwrap_err()));
        assert!(
            e.runs.load(Ordering::SeqCst) <= 2,
            "it did not go on through every window"
        );
    }

    /// An engine without prints, and short audio, are run whole as before.
    #[test]
    fn without_prints_or_with_short_audio_the_whole_audio_is_one_run() {
        let x = audio(18);
        let e = Permuting {
            runs: AtomicI32::new(0),
            prints: false,
        };
        run_with_hints_tuned(
            &e,
            &models(),
            &x,
            &DiarizeHints::default(),
            small(),
            &mut |_| true,
        )
        .unwrap();
        // The window run asked once, found no prints, then the whole file was run once more.
        assert!(e.runs.load(Ordering::SeqCst) >= 1);
        let e = Permuting {
            runs: AtomicI32::new(0),
            prints: true,
        };
        run_with_hints_tuned(
            &e,
            &models(),
            &audio(3),
            &DiarizeHints::default(),
            small(),
            &mut |_| true,
        )
        .unwrap();
        assert_eq!(e.runs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cuts_fall_in_quiet_spots_and_cover_the_audio() {
        let x = audio(18);
        let cuts = window_cuts(&x, 100.0);
        assert_eq!(*cuts.first().unwrap(), 0);
        assert_eq!(*cuts.last().unwrap(), x.len());
        for c in &cuts[1..cuts.len() - 1] {
            assert!(
                x[*c].abs() < 1e-6,
                "cut inside a speaker's talk at {}",
                *c as f64 / RATE as f64
            );
        }
    }
}
