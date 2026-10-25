//! Frozen display frames: the no-bundle freeze used by the overlay's picker (CAP-1).
//!
//! `freeze_display` captures a display once and keeps the pixels in a short-lived cache.
//! `capture_screenshot { target: { frame_id, rect? } }` then crops that frame (physical px)
//! and creates the bundle from it, so the saved region is the frame the user looked at by
//! construction, with no second capture (CAP-1.AC2). Nothing here touches the library and no
//! `capture.created` event is published until a bundle is made.
//!
//! # Lifetime
//!
//! A frame is kept until it was idle (not frozen, cropped or read) for [`DEFAULT_FRAME_TTL`],
//! `release_frame` is called, or the cache is over its limits ([`MAX_FRAMES`] frames,
//! [`MAX_FRAME_BYTES`] bytes), in which case the least recently used frames go first. Eviction
//! runs on every action, so an idle daemon holds nothing for long. Time comes from the
//! daemon's [`crate::Clock`], so tests do not sleep.
//!
//! # Pixels on the wire
//!
//! `transport: "file"` (default) writes the frame as raw RGBA8 (`width * 4` bytes per row, no
//! header) to a file in a directory owned by the daemon (mode 0700, file 0600, removed with
//! the frame and when the daemon exits) and returns its `path`; the client reads or maps it.
//! `transport: "inline"` returns the same bytes base64-encoded in `data`, for a client that
//! cannot read the daemon's temp directory. The params are [`crate::action::FreezeDisplayParams`].
//!
//! Measured on a 3024x1964 (23.7 MB) Retina display, release build, real xcap backend, local
//! Unix socket, median of 4 runs, client time to pixels in memory:
//!
//! | path | ms |
//! |---|---|
//! | `capture_screenshot` (PNG, bundle, base64) + client PNG decode (the old freeze) | ~285 |
//! | `freeze_display` `file` + client `read` | ~37 |
//! | `freeze_display` `inline` (31.6 MB of base64) + client decode | ~100 |
//!
//! Most of the ~37 ms is the screen grab itself; writing and reading the file add 2-4 ms
//! because the file sits in the page cache. Base64 inflates the payload by a third and costs
//! an encode and a decode of 24 MB, hence `file` is the default and `inline` the fallback.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use cm_capture::Frame;
use cm_core::{CmError, ScaleFactor};
use time::OffsetDateTime;

/// How long an unused frame is kept. The picker may sit open while the user decides, and
/// every crop of a frame renews it.
pub const DEFAULT_FRAME_TTL: Duration = Duration::from_secs(120);
/// At most this many frames are cached (one per display of a picker session, with room).
pub const MAX_FRAMES: usize = 8;
/// At most this many pixel bytes are cached (a 6K display frame is about 54 MB).
pub const MAX_FRAME_BYTES: usize = 1 << 30;

/// A cached frame and where it came from.
#[derive(Clone, Debug)]
pub(crate) struct CachedFrame {
    pub frame: Arc<Frame>,
    pub display_id: u32,
    pub scale: ScaleFactor,
}

struct Entry {
    cached: CachedFrame,
    last_used: OffsetDateTime,
    file: Option<PathBuf>,
}

#[derive(Default)]
struct State {
    next: u64,
    entries: HashMap<String, Entry>,
    dir: Option<PathBuf>,
}

pub(crate) struct FrameCache {
    ttl: Duration,
    state: Mutex<State>,
    swept_stale: std::sync::Once,
}

fn bytes_of(e: &Entry) -> usize {
    e.cached.frame.rgba().len()
}

fn remove_file(p: &Option<PathBuf>) {
    if let Some(p) = p {
        let _ = std::fs::remove_file(p);
    }
}

impl FrameCache {
    pub(crate) fn new(ttl: Duration) -> Self {
        FrameCache {
            ttl,
            state: Mutex::new(State::default()),
            swept_stale: std::sync::Once::new(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Drops frames idle for longer than the TTL.
    pub(crate) fn evict_expired(&self, now: OffsetDateTime) {
        let ttl = time::Duration::try_from(self.ttl).unwrap_or(time::Duration::MAX);
        let mut st = self.lock();
        if st.entries.is_empty() {
            return;
        }
        st.entries.retain(|_, e| {
            let keep = now - e.last_used <= ttl;
            if !keep {
                remove_file(&e.file);
            }
            keep
        });
    }

    /// Stores a frame; returns its id and drops least-recently-used frames over the limits.
    pub(crate) fn insert(&self, cached: CachedFrame, now: OffsetDateTime) -> String {
        let mut st = self.lock();
        st.next += 1;
        let id = format!(
            "frame_{}_{:08x}",
            st.next,
            now.unix_timestamp_nanos() as u32
        );
        st.entries.insert(
            id.clone(),
            Entry {
                cached,
                last_used: now,
                file: None,
            },
        );
        loop {
            let total: usize = st.entries.values().map(bytes_of).sum();
            if st.entries.len() <= MAX_FRAMES && total <= MAX_FRAME_BYTES {
                break;
            }
            let oldest = st
                .entries
                .iter()
                .filter(|(k, _)| **k != id)
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => {
                    if let Some(e) = st.entries.remove(&k) {
                        remove_file(&e.file);
                    }
                }
                None => break,
            }
        }
        id
    }

    /// The frame for `id`, renewing its lifetime. `not_found` if unknown or expired.
    pub(crate) fn get(&self, id: &str, now: OffsetDateTime) -> Result<CachedFrame, CmError> {
        let mut st = self.lock();
        match st.entries.get_mut(id) {
            Some(e) => {
                e.last_used = now;
                Ok(e.cached.clone())
            }
            None => Err(CmError::not_found(format!(
                "frame `{id}` is unknown or expired (frames are kept for {} s after last use); \
                 freeze the display again",
                self.ttl.as_secs()
            ))),
        }
    }

    /// Frees a frame; `false` if it was already gone.
    pub(crate) fn release(&self, id: &str) -> bool {
        match self.lock().entries.remove(id) {
            Some(e) => {
                remove_file(&e.file);
                true
            }
            None => false,
        }
    }

    pub(crate) fn ttl(&self) -> Duration {
        self.ttl
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.lock().entries.len()
    }

    /// Writes the raw RGBA8 pixels of `id` to a file in the daemon's frame directory and
    /// returns its path. The file lives as long as the frame.
    pub(crate) fn write_file(&self, id: &str) -> Result<PathBuf, CmError> {
        let (frame, dir) = {
            let st = self.lock();
            let frame = match st.entries.get(id) {
                Some(e) => e.cached.frame.clone(),
                None => return Err(CmError::not_found(format!("frame `{id}` is gone"))),
            };
            (frame, st.dir.clone())
        };
        let dir = match dir {
            Some(d) if d.is_dir() => d,
            _ => {
                let d = self.create_dir()?;
                self.lock().dir = Some(d.clone());
                d
            }
        };
        let path = dir.join(format!("{id}.rgba"));
        write_private(&path, frame.rgba())
            .map_err(|e| CmError::internal(format!("cannot write `{}`: {e}", path.display())))?;
        let mut st = self.lock();
        match st.entries.get_mut(id) {
            Some(e) => e.file = Some(path.clone()),
            None => {
                // Released while writing.
                let _ = std::fs::remove_file(&path);
                return Err(CmError::not_found(format!("frame `{id}` is gone")));
            }
        }
        Ok(path)
    }

    fn create_dir(&self) -> Result<PathBuf, CmError> {
        let root = std::env::temp_dir();
        self.swept_stale.call_once(|| sweep_stale(&root));
        let nanos = OffsetDateTime::now_utc().unix_timestamp_nanos();
        let dir = root.join(format!("{DIR_PREFIX}{}-{nanos:x}", std::process::id()));
        create_private_dir(&dir)
            .map_err(|e| CmError::internal(format!("cannot create `{}`: {e}", dir.display())))?;
        Ok(dir)
    }
}

impl Drop for FrameCache {
    fn drop(&mut self) {
        let st = self.state.get_mut().unwrap_or_else(PoisonError::into_inner);
        if let Some(d) = &st.dir {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

const DIR_PREFIX: &str = "capturemann-frames-";

/// Removes frame directories left by daemons that crashed (untouched for over an hour; a
/// live daemon's directory is renewed by every frame it writes and recreated when missing).
fn sweep_stale(root: &std::path::Path) {
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    for entry in rd.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(DIR_PREFIX) {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > Duration::from_secs(3600));
        if old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(unix)]
fn create_private_dir(p: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().mode(0o700).create(p)
}

#[cfg(not(unix))]
fn create_private_dir(p: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir(p)
}

fn write_private(p: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(p)?;
    f.write_all(bytes)?;
    f.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn frame() -> CachedFrame {
        CachedFrame {
            frame: Arc::new(Frame::new(2, 2, vec![7; 16]).unwrap()),
            display_id: 1,
            scale: ScaleFactor::ONE,
        }
    }

    #[test]
    fn expires_after_the_idle_ttl_and_use_renews() {
        let t0 = datetime!(2026-01-01 00:00 UTC);
        let c = FrameCache::new(Duration::from_secs(10));
        let id = c.insert(frame(), t0);
        let at = |s| t0 + time::Duration::seconds(s);
        c.evict_expired(at(9));
        assert!(c.get(&id, at(9)).is_ok(), "renewed at 9 s");
        c.evict_expired(at(18));
        assert!(c.get(&id, at(18)).is_ok(), "9 s idle is within the TTL");
        c.evict_expired(at(30));
        assert_eq!(c.len(), 0);
        let e = c.get(&id, at(30)).unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::NotFound);
    }

    #[test]
    fn release_and_limits() {
        let t0 = datetime!(2026-01-01 00:00 UTC);
        let c = FrameCache::new(DEFAULT_FRAME_TTL);
        let first = c.insert(frame(), t0);
        for i in 1..=MAX_FRAMES as i64 {
            c.insert(frame(), t0 + time::Duration::seconds(i));
        }
        assert_eq!(c.len(), MAX_FRAMES);
        assert!(
            c.get(&first, t0).is_err(),
            "the least recently used one went"
        );
        let any = c.insert(frame(), t0 + time::Duration::seconds(99));
        assert!(c.release(&any));
        assert!(!c.release(&any));
    }

    #[test]
    fn file_is_raw_rgba_private_and_removed_with_the_frame() {
        let t0 = datetime!(2026-01-01 00:00 UTC);
        let c = FrameCache::new(DEFAULT_FRAME_TTL);
        let id = c.insert(frame(), t0);
        let p = c.write_file(&id).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), vec![7u8; 16]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode();
            assert_eq!(mode(&p) & 0o077, 0);
            assert_eq!(mode(p.parent().unwrap()) & 0o077, 0);
        }
        assert!(c.release(&id));
        assert!(!p.exists());
        let id2 = c.insert(frame(), t0);
        let p2 = c.write_file(&id2).unwrap();
        let dir = p2.parent().unwrap().to_path_buf();
        drop(c);
        assert!(!dir.exists(), "the directory goes with the cache");
    }
}
