//! The library watcher: notices bundles that change from OUTSIDE the daemon and keeps the index
//! (and every client) in step (LIB-1 "Edge cases", LIB-2).
//!
//! A bundle copied in from another Mac, restored from a backup, renamed or deleted in Finder
//! shows up without a manual "Rescan library": the watcher indexes it incrementally
//! ([`cm_library::Library::peek_dir`] then `upsert`, or `remove`; never a full rescan) and
//! publishes `capture.created`, `capture.revised` (with `renamed: true` for a folder rename) or
//! `capture.deleted`, each with `by: "watcher"` and `external: true`, so every client refreshes
//! through the events it already handles.
//!
//! # Mechanism: polling, on purpose
//!
//! Every [`WatchOptions::poll`] (2 s) the top level of the library folder is scanned: for each
//! `*.cmrec` / `*.cmshot` folder, its own modification time, the size and modification time of
//! `manifest.json`, and whether `recording.lock` exists. A directory listing and a few stats
//! per bundle; nothing is read. The measured cost is in `tests/library_watch.rs`
//! (`scan_cost_of_a_big_library`, which prints it).
//!
//! A file-notification crate (`notify`: FSEvents, ReadDirectoryChanges) was considered and not
//! used: it is not in the dependency tree, adding it needs the network, and its licence (CC0-1.0)
//! would need a reviewed `deny.toml` allowance. Polling needs neither, behaves the same on a
//! network share or an external drive (where FSEvents is unreliable), cannot overflow an event
//! queue during a big copy and has no platform code to leave untested on Windows. The price is a
//! latency of up to `poll + debounce` (about 3 s). Swapping in `notify` later only replaces the
//! trigger of a scan; the rest (settling, own-write suppression) stays.
//!
//! # Rules
//!
//! * **Settling.** A change is acted on only after the bundle's signature (folder mtime,
//!   manifest mtime and size, lock) has stayed the same for [`WatchOptions::debounce`] (1 s), so
//!   a bundle copied file by file is indexed once, after it settles, and only when its
//!   `manifest.json` parses. A manifest that stays unreadable for a further
//!   [`WatchOptions::broken_grace`] (10 s) is indexed as a `broken` row, exactly as
//!   `rescan_library` does.
//! * **Unfinished recordings.** A bundle with `recording.lock` is never indexed; it is picked up
//!   after the lock goes away (recovery or the end of the recording).
//! * **The daemon's own writes.** The watcher listens to the daemon's own `capture.*` events; a
//!   change to a capture that the daemon announced itself within the 5 s before the change
//!   settled is still indexed (idempotent) but not announced again. `exports/` and other
//!   subfolders do not change a bundle's own folder mtime; atomic temp files (`*.tmp`,
//!   `*.tmp-<pid>`) are not bundles. No loop is possible: the watcher's events are tagged
//!   `by: "watcher"` and ignored by itself.
//! * **Duplicates.** A copied bundle whose capture id is already indexed at another folder that
//!   still exists is ignored (the `rescan` rule). If the other folder is gone it is a rename.
//! * **An unavailable folder** (unplugged drive, `read_dir` fails) pauses the watcher: nothing is
//!   removed from the index, one line is logged, and it resumes by diffing against what it knew
//!   before. A changed library root starts over with a silent baseline (the root is read from
//!   the library at every scan; today the daemon's library cannot be re-pointed in process, a
//!   new `library.path` takes effect at the next start).
//! * The first scan is a silent baseline: what is already on disk at start is the startup
//!   rescan's business.
//!
//! The driver ([`Daemon::start_library_watcher`](crate::Daemon::start_library_watcher)) holds no
//! reference to the daemon, so the task ends with the last clone of it (or at
//! [`Daemon::stop_library_watcher`](crate::Daemon::stop_library_watcher)).

use cm_library::bundle::MANIFEST_FILE;
use cm_library::index::CaptureRecord;
use cm_library::manifest::CaptureKind;
use cm_library::rec::LOCK_FILE;
use cm_library::Library;
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::sync::broadcast::error::TryRecvError;

use crate::daemon::lock_library;
use crate::events::{self, EventBus};

/// The `by` of the watcher's events.
pub const WATCHER_ACTOR: &str = "watcher";

/// How long before a change settled an event of the daemon itself still explains it.
const OWN_WINDOW_MS: u64 = 5_000;

/// Timing of the watcher.
#[derive(Debug, Clone, Copy)]
pub struct WatchOptions {
    /// Time between scans.
    pub poll: Duration,
    /// How long a bundle's signature must stay unchanged before it is acted on.
    pub debounce: Duration,
    /// How much longer an unreadable manifest is waited for before the bundle is indexed as
    /// `broken`.
    pub broken_grace: Duration,
}

impl Default for WatchOptions {
    fn default() -> Self {
        WatchOptions {
            poll: Duration::from_secs(2),
            debounce: Duration::from_secs(1),
            broken_grace: Duration::from_secs(10),
        }
    }
}

/// What a bundle folder looks like from the outside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sig {
    dir_mtime: Option<SystemTime>,
    manifest: Option<(SystemTime, u64)>,
    locked: bool,
}

/// A change the watcher applied to the index (and announces).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Created {
        id: String,
        title: String,
        broken: bool,
    },
    Revised {
        id: String,
        title: String,
        renamed: bool,
    },
    Deleted {
        id: String,
        title: String,
    },
}

impl Change {
    pub fn capture_id(&self) -> &str {
        match self {
            Change::Created { id, .. }
            | Change::Revised { id, .. }
            | Change::Deleted { id, .. } => id,
        }
    }
}

/// The result of one [`WatchState::tick`].
#[derive(Debug, Default)]
pub struct Tick {
    /// Changes applied to the index this tick that are announced (own-write ones are applied
    /// but not listed).
    pub changes: Vec<Change>,
    /// Whether the library folder could be read.
    pub available: bool,
    /// An index problem (the change is retried next tick).
    pub error: Option<String>,
}

#[derive(Debug)]
struct Pending {
    /// `None`: the folder is gone.
    sig: Option<Sig>,
    since_ms: u64,
}

/// The watcher's memory. Driven by [`WatchState::tick`] with a caller-supplied clock, so tests
/// never sleep.
#[derive(Debug)]
pub struct WatchState {
    opts: WatchOptions,
    root: Option<PathBuf>,
    baseline_done: bool,
    known: BTreeMap<String, Sig>,
    pending: HashMap<String, Pending>,
    own: HashMap<String, u64>,
}

/// Lists the bundle folders directly under `root` with their signatures. `Err` when the folder
/// cannot be read (unplugged drive).
pub fn scan(root: &Path) -> std::io::Result<BTreeMap<String, Sig>> {
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(root)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Some(ext) = Path::new(&name).extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if CaptureKind::from_extension(ext).is_none() {
            continue;
        }
        let path = entry.path();
        let is_dir = match entry.file_type() {
            Ok(t) if t.is_symlink() => path.is_dir(),
            Ok(t) => t.is_dir(),
            Err(_) => false,
        };
        if !is_dir {
            continue;
        }
        let manifest = std::fs::metadata(path.join(MANIFEST_FILE))
            .ok()
            .and_then(|m| Some((m.modified().ok()?, m.len())));
        out.insert(
            name,
            Sig {
                dir_mtime: entry.metadata().ok().and_then(|m| m.modified().ok()),
                manifest,
                locked: path.join(LOCK_FILE).exists(),
            },
        );
    }
    Ok(out)
}

fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

impl WatchState {
    pub fn new(opts: WatchOptions) -> Self {
        WatchState {
            opts,
            root: None,
            baseline_done: false,
            known: BTreeMap::new(),
            pending: HashMap::new(),
            own: HashMap::new(),
        }
    }

    /// The daemon announced a change to `capture_id` itself at `now_ms`.
    pub fn note_own(&mut self, capture_id: &str, now_ms: u64) {
        self.own.insert(capture_id.to_owned(), now_ms);
        if self.own.len() > 512 {
            self.own
                .retain(|_, t| now_ms.saturating_sub(*t) < OWN_WINDOW_MS * 4);
        }
    }

    fn is_own(&self, id: &str, since_ms: u64) -> bool {
        self.own
            .get(id)
            .is_some_and(|t| *t + OWN_WINDOW_MS >= since_ms)
    }

    /// One scan and, for the bundles that settled, the index updates. `now_ms` is any
    /// monotonic millisecond clock.
    pub fn tick(&mut self, library: &Mutex<Library>, now_ms: u64) -> Tick {
        let root = lock_library(library).root().to_owned();
        if self.root.as_deref() != Some(root.as_path()) {
            self.root = Some(root.clone());
            self.baseline_done = false;
            self.known.clear();
            self.pending.clear();
        }
        let Ok(cur) = scan(&root) else {
            return Tick::default();
        };
        let mut tick = Tick {
            available: true,
            ..Tick::default()
        };
        if !self.baseline_done {
            self.known = cur;
            self.baseline_done = true;
            return tick;
        }
        self.observe(&cur, now_ms);
        let debounce = ms(self.opts.debounce);
        let mut ready: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, p)| now_ms.saturating_sub(p.since_ms) >= debounce)
            .map(|(n, _)| n.clone())
            .collect();
        if ready.is_empty() {
            return tick;
        }
        ready.sort();
        let mut lib = lock_library(library);
        // Additions and changes first, so a rename repoints the row before the old name's
        // removal looks for it.
        let (adds, removes): (Vec<_>, Vec<_>) = ready
            .into_iter()
            .partition(|n| self.pending[n].sig.is_some());
        for name in adds {
            if let Err(e) = self.apply_add(&mut lib, &root, &cur, &name, now_ms, &mut tick) {
                tick.error = Some(format!("{name}: {e}"));
            }
        }
        for name in removes {
            if let Err(e) = self.apply_remove(&mut lib, &name, &mut tick) {
                tick.error = Some(format!("{name}: {e}"));
            }
        }
        tick
    }

    /// Updates `pending` from the new scan.
    fn observe(&mut self, cur: &BTreeMap<String, Sig>, now_ms: u64) {
        for (name, sig) in cur {
            if self.known.get(name) == Some(sig) {
                self.pending.remove(name);
            } else if self
                .pending
                .get(name)
                .is_none_or(|p| p.sig.as_ref() != Some(sig))
            {
                self.pending.insert(
                    name.clone(),
                    Pending {
                        sig: Some(sig.clone()),
                        since_ms: now_ms,
                    },
                );
            }
        }
        // A folder that came and went before anybody acted on it just evaporates.
        let known = &self.known;
        self.pending.retain(|name, p| {
            cur.contains_key(name) || (known.contains_key(name) && p.sig.is_none())
        });
        for name in self.known.keys() {
            if !cur.contains_key(name) && !self.pending.contains_key(name) {
                self.pending.insert(
                    name.clone(),
                    Pending {
                        sig: None,
                        since_ms: now_ms,
                    },
                );
            }
        }
    }

    fn apply_add(
        &mut self,
        lib: &mut Library,
        root: &Path,
        cur: &BTreeMap<String, Sig>,
        name: &str,
        now_ms: u64,
        tick: &mut Tick,
    ) -> Result<(), cm_core::CmError> {
        let Some((sig, since_ms)) = self
            .pending
            .get(name)
            .and_then(|p| Some((p.sig.clone()?, p.since_ms)))
        else {
            return Ok(());
        };
        if sig.locked {
            // An unfinished recording: not indexed until the lock goes.
            self.settle(name, sig);
            return Ok(());
        }
        let dir = root.join(name);
        let rec: CaptureRecord = match lib.peek_dir(&dir) {
            Ok(r) => r,
            Err(_) => {
                let grace = ms(self.opts.debounce + self.opts.broken_grace);
                if now_ms.saturating_sub(since_ms) < grace {
                    // Probably still being copied: look again next tick.
                    return Ok(());
                }
                lib.peek_dir_or_broken(&dir)
            }
        };
        let prev = self.known.get(name).cloned();
        let by_id = lib.get(&rec.id).ok();
        let change = match by_id {
            Some(old) if old.path != name => {
                if cur.contains_key(&old.path) {
                    // A second copy of an indexed capture: the first one stays (the rescan rule).
                    self.settle(name, sig);
                    return Ok(());
                }
                lib.upsert(&rec)?;
                Some(Change::Revised {
                    id: rec.id.clone(),
                    title: rec.title.clone(),
                    renamed: true,
                })
            }
            Some(old) => {
                lib.upsert(&rec)?;
                let manifest_changed = prev.is_some_and(|p| p.manifest != sig.manifest);
                (manifest_changed || old != rec).then(|| Change::Revised {
                    id: rec.id.clone(),
                    title: rec.title.clone(),
                    renamed: false,
                })
            }
            None => {
                // A new id. Another row on this folder (a broken one, or a manifest whose id was
                // edited) is replaced.
                if let Some(old) = lib.get_by_path(name)?.filter(|o| o.id != rec.id) {
                    lib.remove(&old.id)?;
                    if !old.broken {
                        self.emit(
                            Change::Deleted {
                                id: old.id,
                                title: old.title,
                            },
                            since_ms,
                            tick,
                        );
                    }
                }
                lib.upsert(&rec)?;
                Some(Change::Created {
                    id: rec.id.clone(),
                    title: rec.title.clone(),
                    broken: rec.broken,
                })
            }
        };
        if let Some(c) = change {
            self.emit(c, since_ms, tick);
        }
        self.settle(name, sig);
        Ok(())
    }

    fn settle(&mut self, name: &str, sig: Sig) {
        self.known.insert(name.to_owned(), sig);
        self.pending.remove(name);
    }

    fn apply_remove(
        &mut self,
        lib: &mut Library,
        name: &str,
        tick: &mut Tick,
    ) -> Result<(), cm_core::CmError> {
        let since_ms = self.pending.get(name).map_or(0, |p| p.since_ms);
        if let Some(row) = lib.get_by_path(name)? {
            lib.remove(&row.id)?;
            self.emit(
                Change::Deleted {
                    id: row.id,
                    title: row.title,
                },
                since_ms,
                tick,
            );
        }
        self.known.remove(name);
        self.pending.remove(name);
        Ok(())
    }

    fn emit(&self, c: Change, since_ms: u64, tick: &mut Tick) {
        if !self.is_own(c.capture_id(), since_ms) {
            tick.changes.push(c);
        }
    }
}

/// Stops the watcher task when dropped.
pub(crate) struct WatcherGuard(tokio::task::JoinHandle<()>);

impl Drop for WatcherGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Publishes one change on the bus.
pub fn publish(bus: &EventBus, change: &Change) {
    let (name, data) = match change {
        Change::Created { id, title, broken } => (
            events::CAPTURE_CREATED,
            json!({ "capture_id": id, "title": title, "broken": broken,
                    "by": WATCHER_ACTOR, "external": true }),
        ),
        Change::Revised { id, title, renamed } => (
            events::CAPTURE_REVISED,
            json!({ "capture_id": id, "title": title, "renamed": renamed,
                    "by": WATCHER_ACTOR, "external": true }),
        ),
        Change::Deleted { id, title } => (
            events::CAPTURE_DELETED,
            json!({ "capture_id": id, "title": title,
                    "by": WATCHER_ACTOR, "external": true }),
        ),
    };
    bus.publish(name, data);
}

/// Starts the polling task. Needs a tokio runtime.
pub(crate) fn spawn(
    library: Arc<Mutex<Library>>,
    bus: EventBus,
    opts: WatchOptions,
) -> WatcherGuard {
    let mut rx = bus.subscribe();
    let handle = tokio::spawn(async move {
        let start = tokio::time::Instant::now();
        let mut state = WatchState::new(opts);
        let mut last_error: Option<String> = None;
        let mut was_available = true;
        loop {
            let now_ms = ms(start.elapsed());
            // The daemon's own capture events explain the changes they cause.
            loop {
                match rx.try_recv() {
                    Ok(ev) => {
                        if ev.name.starts_with("capture.")
                            && ev.data.get("by").and_then(|b| b.as_str()) != Some(WATCHER_ACTOR)
                        {
                            if let Some(id) = ev.data.get("capture_id").and_then(|v| v.as_str()) {
                                state.note_own(id, now_ms);
                            }
                        }
                    }
                    Err(TryRecvError::Lagged(_)) => {}
                    Err(TryRecvError::Empty | TryRecvError::Closed) => break,
                }
            }
            let lib = library.clone();
            let joined = tokio::task::spawn_blocking(move || {
                let t = state.tick(&lib, now_ms);
                (state, t)
            })
            .await;
            let Ok((st, tick)) = joined else {
                return;
            };
            state = st;
            if tick.available != was_available {
                if tick.available {
                    eprintln!("cm-daemon: the library folder is reachable again");
                } else {
                    eprintln!("cm-daemon: the library folder is unavailable; watching paused");
                }
                was_available = tick.available;
                let path = lock_library(&library).root().display().to_string();
                bus.publish(
                    if tick.available {
                        events::LIBRARY_ONLINE
                    } else {
                        events::LIBRARY_OFFLINE
                    },
                    json!({ "path": path }),
                );
            }
            if tick.error != last_error {
                if let Some(e) = &tick.error {
                    eprintln!("cm-daemon: library watcher could not index: {e}");
                }
                last_error = tick.error.clone();
            }
            for c in &tick.changes {
                publish(&bus, c);
            }
            tokio::time::sleep(opts.poll).await;
        }
    });
    WatcherGuard(handle)
}
