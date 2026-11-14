//! The recording session of a Click Guide and the handlers of `start_guide`, `stop_guide`,
//! `get_guide` and `set_guide` (GDE-1, GDE-2, GDE-4).
//!
//! ```text
//! OS click ──> GuideClicks ──> sink (filters, takes the frame on mouse-down) ──> worker thread
//!                                                       (OCR title, badge and ring, proposals)
//! ```
//!
//! The sink runs on the listener's helper thread and does only what must happen at the moment of
//! the click: filter it (paused, outside the target, inside the CaptureMann HUD), tell a second
//! click of a double click from a new step, and take the frame. Everything slow is the worker's.
//! One worker owns the guide folder while it records, so HUD controls (note step, undo last step,
//! stop) are queued on its channel and applied in the order the user pressed them.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use cm_capture::{CaptureRequest, CaptureTarget, Capturer, Frame};
use cm_core::{Actor, CmError};
use cm_library::guide::{ClickPoint, GuideBundle};
use cm_render::ocr::{suggest_redactions, SuggestOptions, TextLine, TextRecognizer};
use image::RgbaImage;
use serde_json::{json, Value};

use super::clicks::{GuideClicks, LocalClick};
use super::titles::draft_title;
use super::{guide_json, NoteArgs, SetArgs};
use crate::action::ScreenshotTarget;
use crate::clock::Clock;
use crate::daemon::{blocking, lock_library, Inner};
use crate::error::DaemonError;
use crate::events::{self, EventBus};

/// A second mouse-down on the same spot within this time is a double click.
const DOUBLE_CLICK_NS: u64 = 400_000_000;
/// ... and within this many physical px.
const DOUBLE_CLICK_PX: f64 = 6.0;
/// How long a HUD control waits for the worker (a step in the queue may be reading its text).
const REPLY_TIMEOUT: Duration = Duration::from_secs(60);

type Reply = mpsc::SyncSender<Result<Value, CmError>>;

enum Job {
    Step {
        frame: Frame,
        click: ClickPoint,
    },
    /// The same spot was clicked again: the newest step becomes a double click.
    Double,
    Note(NoteArgs, Reply),
    RemoveLast(Reply),
    Stop(Reply),
}

struct Active {
    guide_id: String,
    dir: PathBuf,
    paused: Arc<AtomicBool>,
    /// `stop_guide` during the countdown.
    cancelled: Arc<AtomicBool>,
    listening: Arc<AtomicBool>,
    tx: mpsc::Sender<Job>,
    join: Option<JoinHandle<()>>,
    steps: Arc<AtomicUsize>,
}

/// The one running Click Guide, the click source and the HUD exclusion rectangles.
pub(crate) struct GuideRuntime {
    pub(crate) clicks: Arc<dyn GuideClicks>,
    active: Mutex<Option<Active>>,
    /// Rectangles of CaptureMann's own HUD windows in the OS's global space `[x, y, w, h]`: a
    /// click inside one is never a step (GDE-1.AC2).
    exclusions: Arc<Mutex<Vec<[f64; 4]>>>,
}

impl GuideRuntime {
    pub(crate) fn new(clicks: Arc<dyn GuideClicks>) -> Self {
        GuideRuntime {
            clicks,
            active: Mutex::new(None),
            exclusions: Arc::default(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Active>> {
        self.active.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn set_exclusions(&self, rects: Vec<[f64; 4]>) {
        *self
            .exclusions
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = rects;
    }

    /// The id of the guide being recorded, if any.
    pub(crate) fn active_id(&self) -> Option<String> {
        self.lock().as_ref().map(|a| a.guide_id.clone())
    }
}

struct SinkState {
    paused: Arc<AtomicBool>,
    exclusions: Arc<Mutex<Vec<[f64; 4]>>>,
    capturer: Arc<Capturer>,
    target: CaptureTarget,
    tx: mpsc::Sender<Job>,
    events: EventBus,
    guide_id: String,
    /// `(host_ns, x, y, button)` of the newest step's click, and whether it is a double already.
    last: Option<(u64, f64, f64, String, bool)>,
}

impl SinkState {
    fn excluded(&self, gx: f64, gy: f64) -> bool {
        self.exclusions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|r| gx >= r[0] && gx < r[0] + r[2] && gy >= r[1] && gy < r[1] + r[3])
    }

    fn on_click(&mut self, c: LocalClick) {
        if self.paused.load(Ordering::Relaxed) || c.outside || self.excluded(c.gx, c.gy) {
            return;
        }
        if let Some((t, x, y, button, double)) = &mut self.last {
            let near = (c.x - *x).hypot(c.y - *y) <= DOUBLE_CLICK_PX;
            if !*double
                && *button == c.button
                && near
                && c.host_ns.saturating_sub(*t) <= DOUBLE_CLICK_NS
            {
                *double = true;
                let _ = self.tx.send(Job::Double);
                return;
            }
        }
        // The frame is taken now, on mouse-down: the step shows what was clicked, not the result.
        let frame = match self
            .capturer
            .capture(CaptureRequest::new(self.target), None)
        {
            Ok(Some(cap)) => cap.frame,
            Ok(None) => return,
            Err(e) => {
                self.events.publish(
                    "guide.error",
                    json!({ "guide_id": self.guide_id, "message": e.to_string() }),
                );
                return;
            }
        };
        self.last = Some((c.host_ns, c.x, c.y, c.button.clone(), false));
        let _ = self.tx.send(Job::Step {
            frame,
            click: ClickPoint {
                x: c.x,
                y: c.y,
                button: c.button,
                count: 1,
            },
        });
    }
}

struct Worker {
    guide: GuideBundle,
    recognizer: Arc<dyn TextRecognizer>,
    events: EventBus,
    clock: Arc<dyn Clock>,
    by: Actor,
    steps: Arc<AtomicUsize>,
    guide_id: String,
    /// The text lines and click of the newest step (a double click rewrites its title).
    last: Option<(Vec<TextLine>, ClickPoint)>,
}

impl Worker {
    fn step(&mut self, frame: Frame, click: ClickPoint) -> Result<(), CmError> {
        let png = frame.to_png()?;
        let size = frame.size();
        let lines = match RgbaImage::from_raw(size.width, size.height, frame.into_rgba()) {
            Some(img) if self.recognizer.is_available() => {
                self.recognizer.recognize(&img).unwrap_or_default()
            }
            _ => Vec::new(),
        };
        let title = draft_title(&lines, (click.x, click.y), &click.button, 1);
        let step =
            self.guide
                .add_click_step(&png, click.clone(), &title, &self.by, self.clock.now())?;
        // Auto-redact proposes; nothing is blurred here (GDE-1.AC4).
        let proposals = !suggest_redactions(
            &lines,
            (size.width, size.height),
            &SuggestOptions::default(),
        )
        .is_empty();
        if proposals {
            self.guide.set_has_proposals(&step.id, true)?;
        }
        self.last = Some((lines, click));
        let n = self.steps.fetch_add(1, Ordering::SeqCst) + 1;
        self.events.publish(
            "guide.step_added",
            json!({ "guide_id": self.guide_id, "step_id": step.id, "count": n,
                    "title": step.title, "has_proposals": proposals }),
        );
        Ok(())
    }

    fn double(&mut self) -> Result<(), CmError> {
        let Some((lines, click)) = &self.last else {
            return Ok(());
        };
        let title = draft_title(lines, (click.x, click.y), &click.button, 2);
        if let Some(id) = self.guide.amend_last_click(2, &title)? {
            self.events.publish(
                "guide.step_updated",
                json!({ "guide_id": self.guide_id, "step_id": id, "title": title }),
            );
        }
        Ok(())
    }

    fn run(mut self, rx: mpsc::Receiver<Job>) {
        for job in rx {
            match job {
                Job::Step { frame, click } => {
                    if let Err(e) = self.step(frame, click) {
                        self.events.publish(
                            "guide.error",
                            json!({ "guide_id": self.guide_id, "message": e.to_string() }),
                        );
                    }
                }
                Job::Double => {
                    let _ = self.double();
                }
                Job::Note(n, reply) => {
                    let r = self
                        .guide
                        .add_note_step(&n.title, &n.note)
                        .map(|s| json!({ "step_id": s.id }));
                    if r.is_ok() {
                        self.steps.fetch_add(1, Ordering::SeqCst);
                    }
                    let _ = reply.send(r);
                }
                Job::RemoveLast(reply) => {
                    let r = self.guide.remove_last_step().map(|s| {
                        if s.is_some() {
                            self.steps.fetch_sub(1, Ordering::SeqCst);
                        }
                        self.last = None;
                        json!({ "removed": s.map(|s| s.id) })
                    });
                    let _ = reply.send(r);
                }
                Job::Stop(reply) => {
                    let _ = reply.send(Ok(json!({})));
                    break;
                }
            }
        }
    }
}

async fn ask(
    tx: &mpsc::Sender<Job>,
    make: impl FnOnce(Reply) -> Job,
) -> Result<Value, DaemonError> {
    let (rtx, rrx) = mpsc::sync_channel(1);
    tx.send(make(rtx))
        .map_err(|_| CmError::internal("the guide recorder has stopped"))?;
    blocking(move || {
        rrx.recv_timeout(REPLY_TIMEOUT)
            .map_err(|_| CmError::internal("the guide recorder did not answer"))?
    })
    .await
}

impl Inner {
    /// The folder of the guide `id` (`not_found` or `invalid_input` for another kind).
    fn guide_dir(&self, id: &str) -> Result<PathBuf, CmError> {
        let lib = lock_library(&self.library);
        let rec = lib.get(id)?;
        if rec.kind != cm_library::manifest::CaptureKind::Guide {
            return Err(CmError::invalid_field("guide_id", "not a Click Guide"));
        }
        Ok(lib.bundle_path(&rec))
    }

    /// `guide_id` of the call, or the guide that is being recorded.
    fn guide_id_or_active(&self, id: Option<&str>) -> Result<String, CmError> {
        match id {
            Some(i) => Ok(i.to_owned()),
            None => self.guide.active_id().ok_or_else(|| {
                CmError::invalid_field("guide_id", "is required when no guide is recording")
            }),
        }
    }

    pub(crate) async fn start_guide(
        &self,
        actor: &Actor,
        target: ScreenshotTarget,
        title: Option<String>,
        countdown_s: f64,
    ) -> Result<Value, DaemonError> {
        let capture_target = target.to_capture_target();
        let capturer = self.capturer.clone();
        let targets = blocking({
            let c = capturer.clone();
            move || c.list_targets_with(true)
        })
        .await?;
        let now = self.clock.now();
        let (library, t) = (self.library.clone(), title.clone());
        let target_json = serde_json::to_value(target).unwrap_or(Value::Null);
        let (guide_id, dir) = {
            // Reserve the slot first, so two starts cannot both create a guide.
            let mut slot = self.guide.lock();
            if slot.is_some() {
                return Err(CmError::busy("a Click Guide is already recording").into());
            }
            let (guide, id, dir) = {
                let mut lib = lock_library(&library);
                let title = match t {
                    Some(t) if !t.trim().is_empty() => t,
                    _ => cm_library::index::default_title("Click Guide", now)?,
                };
                let root = lib.root().to_owned();
                let g = GuideBundle::create(&root, &title, target_json, now)?;
                let dir = g.dir().to_owned();
                let rec = lib.index_dir(&dir)?;
                (g, rec.id, dir)
            };
            let (tx, rx) = mpsc::channel();
            let steps = Arc::new(AtomicUsize::new(0));
            let worker = Worker {
                guide,
                recognizer: self.recognizer.clone(),
                events: self.events.clone(),
                clock: self.clock.clone(),
                by: actor.clone(),
                steps: steps.clone(),
                guide_id: id.clone(),
                last: None,
            };
            let join = std::thread::spawn(move || worker.run(rx));
            *slot = Some(Active {
                guide_id: id.clone(),
                dir: dir.clone(),
                paused: Arc::new(AtomicBool::new(false)),
                cancelled: Arc::new(AtomicBool::new(false)),
                listening: Arc::new(AtomicBool::new(false)),
                tx,
                join: Some(join),
                steps,
            });
            (id, dir)
        };
        self.events.publish(
            events::CAPTURE_CREATED,
            json!({ "capture_id": guide_id, "by": actor }),
        );
        let _ = &dir;

        // The countdown (REC-4): the user arranges the screen; `stop_guide` cancels it.
        let cancelled = self
            .guide
            .lock()
            .as_ref()
            .map(|a| a.cancelled.clone())
            .unwrap_or_default();
        let mut left = countdown_s;
        while left > 0.0 && !cancelled.load(Ordering::SeqCst) {
            self.events.publish(
                "guide.countdown",
                json!({ "guide_id": guide_id, "remaining_s": left.ceil() }),
            );
            let step = left.min(1.0);
            tokio::time::sleep(Duration::from_secs_f64(step)).await;
            left -= step;
        }
        if cancelled.load(Ordering::SeqCst) {
            return Ok(json!({ "guide_id": guide_id, "state": "cancelled", "step_count": 0 }));
        }

        // Start listening. A missing permission or a session without click events fails here,
        // before anything is recorded (GDE-1.AC5, AC6), and the empty guide is removed.
        let started = {
            let slot = self.guide.lock();
            slot.as_ref()
                .map(|a| (a.paused.clone(), a.tx.clone(), a.listening.clone()))
        };
        let Some((paused, tx, listening)) = started else {
            return Ok(json!({ "guide_id": guide_id, "state": "cancelled", "step_count": 0 }));
        };
        let mut sink = SinkState {
            paused,
            exclusions: self.guide.exclusions.clone(),
            capturer,
            target: capture_target,
            tx,
            events: self.events.clone(),
            guide_id: guide_id.clone(),
            last: None,
        };
        let res = self.guide.clicks.start(
            &capture_target,
            &targets,
            Box::new(move |c| sink.on_click(c)),
        );
        if let Err(e) = res {
            self.discard_active_guide().await;
            return Err(e.into());
        }
        listening.store(true, Ordering::SeqCst);
        self.events.publish(
            "guide.started",
            json!({ "guide_id": guide_id, "by": actor }),
        );
        Ok(json!({ "guide_id": guide_id, "state": "recording", "step_count": 0 }))
    }

    /// Stops the worker and removes the guide folder (a failed start, or no step was taken).
    async fn discard_active_guide(&self) {
        let a = self.guide.lock().take();
        let Some(mut a) = a else { return };
        self.guide.clicks.stop();
        let (rtx, rrx) = mpsc::sync_channel(1);
        let _ = a.tx.send(Job::Stop(rtx));
        let join = a.join.take();
        let (library, id, dir) = (self.library.clone(), a.guide_id.clone(), a.dir.clone());
        let _ = blocking(move || {
            let _ = rrx.recv_timeout(REPLY_TIMEOUT);
            if let Some(j) = join {
                let _ = j.join();
            }
            let mut lib = lock_library(&library);
            let _ = lib.remove(&id);
            let _ = std::fs::remove_dir_all(&dir);
            Ok(())
        })
        .await;
    }

    pub(crate) async fn stop_guide(
        &self,
        actor: &Actor,
        id: Option<String>,
    ) -> Result<Value, DaemonError> {
        let a = {
            let mut slot = self.guide.lock();
            match (&*slot, &id) {
                (None, _) => {
                    return Err(CmError::invalid_input("no Click Guide is recording").into())
                }
                (Some(a), Some(i)) if &a.guide_id != i => {
                    return Err(CmError::invalid_input(format!(
                        "guide `{i}` is not the one recording ({})",
                        a.guide_id
                    ))
                    .into())
                }
                _ => {}
            }
            slot.take()
        };
        let Some(mut a) = a else {
            return Err(CmError::invalid_input("no Click Guide is recording").into());
        };
        a.cancelled.store(true, Ordering::SeqCst);
        // No click reaches the sink after this returns.
        self.guide.clicks.stop();
        let (rtx, rrx) = mpsc::sync_channel(1);
        let _ = a.tx.send(Job::Stop(rtx));
        let join = a.join.take();
        let (library, dir, gid) = (self.library.clone(), a.dir.clone(), a.guide_id.clone());
        let out = blocking(move || {
            let _ = rrx.recv_timeout(REPLY_TIMEOUT);
            if let Some(j) = join {
                let _ = j.join();
            }
            let g = GuideBundle::open(&dir)?;
            if g.steps().is_empty() {
                // Nothing was clicked: an empty guide is not kept.
                let mut lib = lock_library(&library);
                let _ = lib.remove(&gid);
                let _ = std::fs::remove_dir_all(&dir);
                return Ok(
                    json!({ "guide_id": gid, "state": "discarded", "step_count": 0,
                                  "steps": [], "discarded": true }),
                );
            }
            lock_library(&library).index_dir(&dir)?;
            Ok(guide_json(&gid, &g, "stopped"))
        })
        .await?;
        self.events.publish(
            "guide.stopped",
            json!({ "guide_id": a.guide_id, "by": actor, "step_count": out["step_count"] }),
        );
        Ok(out)
    }

    pub(crate) async fn get_guide(&self, id: Option<String>) -> Result<Value, DaemonError> {
        let id = self.guide_id_or_active(id.as_deref())?;
        let _serial = self.capture_locks.lock(&id).await;
        let state = {
            let slot = self.guide.lock();
            slot.as_ref()
                .filter(|a| a.guide_id == id)
                .map(|a| {
                    if !a.listening.load(Ordering::SeqCst) {
                        "countdown"
                    } else if a.paused.load(Ordering::SeqCst) {
                        "paused"
                    } else {
                        "recording"
                    }
                })
                .unwrap_or("stopped")
        };
        let dir = self.guide_dir(&id)?;
        let gid = id.clone();
        let mut v = blocking(move || {
            let g = GuideBundle::open(&dir)?;
            Ok(guide_json(&gid, &g, state))
        })
        .await?;
        // While recording the worker may be ahead of the file.
        if state != "stopped" {
            if let Some(n) = self
                .guide
                .lock()
                .as_ref()
                .map(|a| a.steps.load(Ordering::SeqCst))
            {
                v["recorded_steps"] = json!(n);
            }
        }
        Ok(v)
    }

    pub(crate) async fn set_guide(&self, actor: &Actor, a: SetArgs) -> Result<Value, DaemonError> {
        let id = self.guide_id_or_active(a.guide_id.as_deref())?;
        let active = self
            .guide
            .lock()
            .as_ref()
            .map(|x| (x.guide_id.clone(), x.paused.clone(), x.tx.clone()));
        let recording_this = active.as_ref().is_some_and(|(g, _, _)| *g == id);
        let edits_steps = a.title.is_some()
            || a.intro.is_some()
            || a.numbering.is_some()
            || a.steps.is_some()
            || a.focus_crop.is_some()
            || !a.retake.is_empty();
        if recording_this {
            if edits_steps {
                return Err(CmError::busy(
                    "this guide is recording: stop it before editing its steps, or use \
                     `paused`, `note_step` and `remove_last`",
                )
                .into());
            }
            let (_, paused, tx) = active.expect("checked");
            if let Some(p) = a.paused {
                paused.store(p, Ordering::SeqCst);
                self.events.publish(
                    if p { "guide.paused" } else { "guide.resumed" },
                    json!({ "guide_id": id }),
                );
            }
            if let Some(n) = a.note_step {
                ask(&tx, |r| Job::Note(n, r)).await?;
            }
            if a.remove_last {
                ask(&tx, Job::RemoveLast).await?;
            }
            return self.get_guide(Some(id)).await;
        }
        if a.paused.is_some() || a.remove_last {
            return Err(CmError::invalid_input(
                "`paused` and `remove_last` only apply to a guide that is recording",
            )
            .into());
        }
        let _serial = self.capture_locks.lock(&id).await;
        let dir = self.guide_dir(&id)?;
        let (by, now) = (actor.clone(), self.clock.now());
        let capturer = self.capturer.clone();
        let (library, events, gid) = (self.library.clone(), self.events.clone(), id.clone());
        let v = blocking(move || {
            let mut g = GuideBundle::open(&dir)?;
            if let Some(t) = &a.title {
                g.set_title(t, &by, now)?;
            }
            if let Some(t) = &a.intro {
                g.set_intro(t, &by, now)?;
            }
            if let Some(n) = a.numbering {
                g.set_numbering(n, &by, now)?;
            }
            if let Some(steps) = &a.steps {
                g.edit_steps(steps, &by, now)?;
            }
            if let Some(n) = &a.note_step {
                g.insert_note_step(None, &n.title, &n.note, &by, now)?;
            }
            if let Some(on) = a.focus_crop {
                for s in &a.crop_steps {
                    g.set_focus_crop(s, on, &by, now)?;
                }
            }
            if !a.retake.is_empty() {
                let target = ScreenshotTarget::parse(&g.doc().target)?;
                for s in &a.retake {
                    let cap = capturer
                        .capture(CaptureRequest::new(target.to_capture_target()), None)?
                        .ok_or_else(|| CmError::invalid_input("capture was cancelled"))?;
                    g.retake_step(s, &cap.frame.to_png()?, &by, now)?;
                }
            }
            lock_library(&library).index_dir(&dir)?;
            events.publish(
                events::CAPTURE_REVISED,
                json!({ "capture_id": gid, "by": by }),
            );
            Ok(guide_json(&gid, &g, "stopped"))
        })
        .await?;
        Ok(v)
    }

    /// `undo` and `redo` on a guide id (GDE-2: every change is one step of the guide's history).
    pub(crate) async fn undo_redo_guide(
        &self,
        actor: &Actor,
        id: &str,
        undo: bool,
    ) -> Result<Value, DaemonError> {
        if self.guide.active_id().as_deref() == Some(id) {
            return Err(CmError::busy("this guide is recording").into());
        }
        let dir = self.guide_dir(id)?;
        let (by, now, library) = (actor.clone(), self.clock.now(), self.library.clone());
        let gid = id.to_owned();
        let out = blocking(move || {
            let mut g = GuideBundle::open(&dir)?;
            let name = if undo {
                g.undo(&by, now)?
            } else {
                g.redo(&by, now)?
            };
            lock_library(&library).index_dir(&dir)?;
            Ok(json!({
                "capture_id": gid,
                "changed": name.is_some(),
                "undone": name,
                "can_undo": g.can_undo(),
                "can_redo": g.can_redo(),
                "guide": guide_json(&gid, &g, "stopped"),
            }))
        })
        .await?;
        self.events.publish(
            events::CAPTURE_REVISED,
            json!({ "capture_id": id, "by": actor }),
        );
        Ok(out)
    }
}
