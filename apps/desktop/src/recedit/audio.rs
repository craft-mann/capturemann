//! The editor's sound: the decoded audio tracks are mixed for the current edit list on a worker
//! thread (`cm_media::decode::AudioBank::render`, the exporter's mix) and played through
//! `cm_audio::playback::PreviewAudio`. The mix is on the output timeline, so the device position
//! is the output time and the pacer can use it as the master clock. An edit that changes what is
//! heard (cuts, speed, trim, gains, mutes, splits) starts a new mix; the old one keeps playing
//! until the new one is ready. A background change does not remix.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use cm_audio::mix::MixedAudio;
use cm_audio::playback::PreviewAudio;
use cm_core::{AudioMix, EditList, MediaTime, PauseGap, Span, SpeedSpan};
use cm_i18n::tr;
use cm_media::decode::AudioBank;

/// The parts of an edit list the sound depends on.
#[derive(Debug, Clone, PartialEq)]
pub struct MixKey {
    start: MediaTime,
    end: MediaTime,
    base_speed: f64,
    cuts: Vec<Span>,
    splits: Vec<MediaTime>,
    speeds: Vec<SpeedSpan>,
    audio: AudioMix,
}

impl MixKey {
    pub fn of(l: &EditList) -> MixKey {
        MixKey {
            start: l.start,
            end: l.end,
            base_speed: l.base_speed,
            cuts: l.cuts.clone(),
            splits: l.splits.clone(),
            speeds: l.speeds.clone(),
            audio: l.audio.clone(),
        }
    }
}

type Slot = Arc<Mutex<Option<(u64, Result<Arc<MixedAudio>, String>)>>>;

pub struct AudioEngine {
    out: PreviewAudio,
    bank: Arc<AudioBank>,
    gaps: Vec<PauseGap>,
    generation: Arc<AtomicU64>,
    slot: Slot,
    wanted: Option<MixKey>,
    /// A mix is being rendered.
    mixing: bool,
    /// Why there is no sound, when the mix failed.
    pub problem: Option<String>,
}

impl AudioEngine {
    /// Opens the output device for `bank`. Fails without a device or without audio tracks.
    pub fn open(bank: AudioBank, gaps: Vec<PauseGap>) -> Result<AudioEngine, String> {
        if bank.is_empty() {
            return Err(tr!("The recording has no audio"));
        }
        let out = PreviewAudio::open().map_err(|e| e.to_string())?;
        Ok(AudioEngine {
            out,
            bank: Arc::new(bank),
            gaps,
            generation: Arc::new(AtomicU64::new(0)),
            slot: Arc::new(Mutex::new(None)),
            wanted: None,
            mixing: false,
            problem: None,
        })
    }

    /// Makes sure the sound matches `list`: starts a new mix when what is heard changed.
    pub fn sync(&mut self, list: &EditList) {
        let key = MixKey::of(list);
        if self.wanted.as_ref() == Some(&key) {
            return;
        }
        self.wanted = Some(key);
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.mixing = true;
        let (bank, gaps, list) = (self.bank.clone(), self.gaps.clone(), list.clone());
        let (slot, current) = (self.slot.clone(), self.generation.clone());
        let _ = std::thread::Builder::new()
            .name("recedit-audio-mix".into())
            .spawn(move || {
                let r = bank
                    .render(&list, &gaps)
                    .map(Arc::new)
                    .map_err(|e| e.to_string());
                // A newer edit already started another mix: this one is stale.
                if current.load(Ordering::SeqCst) == generation {
                    *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some((generation, r));
                }
            });
    }

    /// Picks up a finished mix. Returns true when the sound changed.
    pub fn poll(&mut self) -> bool {
        let Ok(mut slot) = self.slot.try_lock() else {
            return false;
        };
        let Some((generation, result)) = slot.take() else {
            return false;
        };
        if generation != self.generation.load(Ordering::SeqCst) {
            return false;
        }
        self.mixing = false;
        match result {
            Ok(mix) => {
                self.out.set_mix(mix);
                self.problem = None;
                true
            }
            Err(e) => {
                self.problem = Some(e);
                false
            }
        }
    }

    /// The mix on the device is the one for the current edit list, so it can be played without
    /// a jump (false while a newer mix is being rendered).
    pub fn is_current(&self) -> bool {
        self.out.has_mix() && !self.mixing
    }

    pub fn is_playing(&self) -> bool {
        self.out.is_playing()
    }

    pub fn play_from(&self, secs: f64) {
        self.out.play_from(secs);
    }

    pub fn pause(&self) {
        self.out.pause();
    }

    /// Output seconds played, while the device is running; `None` otherwise (the pacer then
    /// uses the wall clock).
    pub fn position(&self) -> Option<f64> {
        if self.out.is_playing() {
            self.out.position()
        } else {
            None
        }
    }

    pub fn device_name(&self) -> &str {
        self.out.device_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::EditContext;

    fn list() -> EditList {
        EditList::full(&EditContext {
            end: MediaTime::from_secs(60),
            live_at: None,
            gaps: &[],
        })
    }

    /// What is heard decides the remix: cuts, speed and gains do; the background does not.
    #[test]
    fn the_mix_key_ignores_the_background_only() {
        let base = list();
        let key = MixKey::of(&base);
        let cut = base
            .cut(&[Span::new(
                MediaTime::from_secs(10),
                MediaTime::from_secs(15),
            )])
            .unwrap();
        assert_ne!(MixKey::of(&cut), key);
        let mut louder = base.clone();
        louder.audio.mic_gain_db = 3.0;
        assert_ne!(MixKey::of(&louder), key);
        let mut muted = base.clone();
        muted.audio.system_mute = true;
        assert_ne!(MixKey::of(&muted), key);
        let sped = base
            .set_speed(
                Span::new(MediaTime::from_secs(0), MediaTime::from_secs(5)),
                2.0,
                false,
            )
            .unwrap();
        assert_ne!(MixKey::of(&sped), key);
        let mut bg = base.clone();
        bg.background.padding_pct = 8.0;
        assert_eq!(MixKey::of(&bg), key, "a background change does not remix");
    }

    #[test]
    fn no_audio_tracks_means_no_engine() {
        assert!(AudioEngine::open(AudioBank::default(), Vec::new()).is_err());
    }
}
