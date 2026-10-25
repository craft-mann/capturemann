//! Raw events in, `cursor.jsonl` lines out. Single-owner state machine, driven by the writer
//! thread in production and called directly in tests (no timing involved).

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use cm_core::{MediaTime, PauseGap, SessionClock};

use crate::image::ImageStore;
use crate::schema::{Availability, CursorEvent, CursorHeader};
use crate::source::{RawEvent, RawKind};
use crate::target::CursorTarget;

/// Minimum spacing between written moves: 1/120 s = 1000 ticks.
pub(crate) const MOVE_INTERVAL_TICKS: i64 = cm_core::TICKS_PER_SECOND / 120;

/// What travels from the recorder and the source to the writer thread.
pub(crate) enum Msg {
    Raw(RawEvent),
    Pause(MediaTime),
    Resume(MediaTime),
    Stop(MediaTime),
}

/// Live counters, shared with the recorder for `stats()`.
#[derive(Debug, Default)]
pub(crate) struct Counters {
    pub delivered: AtomicU64,
    pub written: AtomicU64,
    pub dropped: AtomicU64,
    pub coalesced: AtomicU64,
    pub paused_discarded: AtomicU64,
    pub before_origin: AtomicU64,
    pub images: AtomicU64,
}

fn bump(c: &AtomicU64) {
    c.fetch_add(1, Ordering::Relaxed);
}

pub(crate) struct Pipeline<W: Write> {
    clock: SessionClock,
    target: CursorTarget,
    clicks: bool,
    w: W,
    images: ImageStore,
    counters: Arc<Counters>,
    gap_from: Option<MediaTime>,
    gaps: Vec<PauseGap>,
    last_t: i64,
    /// Last written move position and time.
    last_move: Option<(i32, i32)>,
    last_move_t: i64,
    pending: Option<CursorEvent>,
    /// Latest known state, tracked even while paused so resume can re-anchor.
    last_global: Option<(f64, f64)>,
    current_image: Option<CursorEvent>,
    pub(crate) io_error: Option<String>,
}

impl Pipeline<std::io::BufWriter<std::fs::File>> {
    /// `fdatasync` of the log: a power loss keeps what was flushed, not just what the OS cached
    /// (R3-12). Called every couple of seconds from the writer thread, never on the event path.
    pub(crate) fn sync(&mut self) {
        if self.io_error.is_none() {
            if let Err(e) = self.w.get_ref().sync_data() {
                self.io_error = Some(e.to_string());
            }
        }
    }
}

impl<W: Write> Pipeline<W> {
    pub(crate) fn new(
        clock: SessionClock,
        header: &CursorHeader,
        w: W,
        images: ImageStore,
        counters: Arc<Counters>,
    ) -> Self {
        let mut p = Pipeline {
            clock,
            target: header.target,
            clicks: header.clicks == Availability::Available,
            w,
            images,
            counters,
            gap_from: None,
            gaps: Vec::new(),
            last_t: 0,
            last_move: None,
            last_move_t: i64::MIN / 2,
            pending: None,
            last_global: None,
            current_image: None,
            io_error: None,
        };
        match serde_json::to_string(header) {
            Ok(line) => p.write_line(&line),
            Err(e) => p.io_error = Some(format!("header: {e}")),
        }
        p.flush();
        p
    }

    pub(crate) fn gaps(&self) -> &[PauseGap] {
        &self.gaps
    }

    pub(crate) fn image_count(&self) -> usize {
        self.images.len()
    }

    fn write_line(&mut self, line: &str) {
        if self.io_error.is_some() {
            return;
        }
        if let Err(e) = writeln!(self.w, "{line}") {
            self.io_error = Some(e.to_string());
        }
    }

    pub(crate) fn flush(&mut self) {
        if self.io_error.is_none() {
            if let Err(e) = self.w.flush() {
                self.io_error = Some(e.to_string());
            }
        }
    }

    fn in_gap(&self, t: MediaTime) -> bool {
        self.gap_from.is_some_and(|f| t >= f) || self.gaps.iter().any(|g| t >= g.from && t < g.to)
    }

    fn emit(&mut self, mut ev: CursorEvent) {
        if self.io_error.is_some() {
            bump(&self.counters.dropped);
            return;
        }
        // Lines stay non-decreasing even if a polled image is stamped after a later tap event.
        let t = ev.t().max(self.last_t);
        ev.set_t(t);
        self.last_t = t;
        match serde_json::to_string(&ev) {
            Ok(line) => {
                self.write_line(&line);
                if self.io_error.is_some() {
                    bump(&self.counters.dropped);
                } else {
                    bump(&self.counters.written);
                }
            }
            Err(_) => bump(&self.counters.dropped),
        }
    }

    fn emit_move(&mut self, ev: CursorEvent) {
        if let CursorEvent::Move { t, x, y, .. } = ev {
            self.last_move = Some((x, y));
            self.last_move_t = t;
        }
        self.emit(ev);
    }

    /// Writes a held move, if it changes the position.
    fn flush_pending(&mut self) {
        if let Some(ev) = self.pending.take() {
            if let CursorEvent::Move { x, y, .. } = ev {
                if self.last_move != Some((x, y)) {
                    self.emit_move(ev);
                    return;
                }
            }
            bump(&self.counters.coalesced);
        }
    }

    /// Called when the writer thread has been idle for a move interval.
    pub(crate) fn on_idle(&mut self) {
        self.flush_pending();
    }

    pub(crate) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    fn map(&self, gx: f64, gy: f64) -> (i32, i32, bool) {
        self.target.map(gx, gy)
    }

    pub(crate) fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Raw(raw) => self.handle_raw(raw),
            Msg::Pause(at) => {
                if let Some(CursorEvent::Move { t, .. }) = &self.pending {
                    if MediaTime::from_ticks(*t) >= at {
                        self.pending = None;
                    }
                }
                self.flush_pending();
                self.gap_from.get_or_insert(at);
            }
            Msg::Resume(at) => {
                if let Some(from) = self.gap_from.take() {
                    self.gaps.push(PauseGap { from, to: at });
                    // Re-anchor: where the cursor is and what it looks like when we come back.
                    if let Some((gx, gy)) = self.last_global {
                        let (x, y, outside) = self.map(gx, gy);
                        self.emit_move(CursorEvent::Move {
                            t: at.ticks(),
                            x,
                            y,
                            outside,
                        });
                    }
                    if let Some(mut img) = self.current_image.clone() {
                        img.set_t(at.ticks());
                        self.emit(img);
                    }
                }
            }
            Msg::Stop(at) => {
                if let Some(CursorEvent::Move { t, .. }) = &self.pending {
                    if MediaTime::from_ticks(*t) > at {
                        self.pending = None;
                    }
                }
                self.flush_pending();
                self.flush();
            }
        }
    }

    fn handle_raw(&mut self, raw: RawEvent) {
        bump(&self.counters.delivered);
        let mt = self.clock.from_host_ns(raw.host_ns);
        let t = mt.ticks();
        // State that must survive gaps and pre-origin stamps.
        match &raw.kind {
            RawKind::Move { gx, gy } | RawKind::Button { gx, gy, .. } => {
                self.last_global = Some((*gx, *gy));
            }
            _ => {}
        }
        let image_event = if let RawKind::Image(img) = &raw.kind {
            match self.images.add(img) {
                Ok(id) => {
                    bump(&self.counters.images);
                    Some(CursorEvent::Cursor {
                        t,
                        id,
                        hotspot: img.hotspot,
                        scale: img.scale,
                    })
                }
                Err(e) => {
                    self.io_error.get_or_insert(e.to_string());
                    None
                }
            }
        } else {
            None
        };
        // A re-announcement of the image already in effect writes nothing.
        let image_event = match image_event {
            Some(ev) => {
                let same = matches!((&ev, &self.current_image),
                    (CursorEvent::Cursor { id, .. }, Some(CursorEvent::Cursor { id: cur, .. })) if id == cur);
                self.current_image = Some(ev.clone());
                (!same).then_some(ev)
            }
            None => None,
        };
        if t < 0 {
            bump(&self.counters.before_origin);
            return;
        }
        if self.in_gap(mt) {
            bump(&self.counters.paused_discarded);
            return;
        }
        match raw.kind {
            RawKind::Move { gx, gy } => {
                let (x, y, outside) = self.map(gx, gy);
                let ev = CursorEvent::Move { t, x, y, outside };
                if t - self.last_move_t >= MOVE_INTERVAL_TICKS {
                    if self.pending.take().is_some() {
                        bump(&self.counters.coalesced);
                    }
                    if self.last_move != Some((x, y)) {
                        self.emit_move(ev);
                    }
                } else {
                    if self.pending.is_some() {
                        bump(&self.counters.coalesced);
                    }
                    self.pending = Some(ev);
                }
            }
            RawKind::Button {
                gx,
                gy,
                button,
                down,
                mods,
            } => {
                if !self.clicks {
                    return;
                }
                self.flush_pending();
                let (x, y, outside) = self.map(gx, gy);
                self.emit(if down {
                    CursorEvent::Down {
                        t,
                        button,
                        x,
                        y,
                        mods,
                        outside,
                    }
                } else {
                    CursorEvent::Up {
                        t,
                        button,
                        x,
                        y,
                        mods,
                        outside,
                    }
                });
            }
            RawKind::Scroll { dx, dy } => {
                if !self.clicks {
                    return;
                }
                self.flush_pending();
                self.emit(CursorEvent::Scroll {
                    t,
                    dx: dx.signum(),
                    dy: dy.signum(),
                });
            }
            RawKind::Image(_) => {
                if let Some(ev) = image_event {
                    self.flush_pending();
                    self.emit(ev);
                }
            }
        }
    }
}
