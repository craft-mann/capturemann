//! Window thumbnails for the picker (R19) and, later, the Record sheet's window list (R20).
//!
//! A thumbnail is one capture of the window (the internal `window_thumbnail` action, about
//! 300 ms each), so they are requested lazily for the cards that are on screen, at most
//! [`MAX_IN_FLIGHT`] at a time, decoded off the UI thread and kept for the life of the [`Thumbs`]
//! model. A failed one stays failed (the card keeps its neutral tile) until the model is dropped.
//!
//! The scheduling is [`ThumbQueue`], pure and tested with a fake loader; [`Thumbs`] is the GPUI
//! model around it.
//!
//! Contract for other surfaces: `Thumbs::new(backend)`, `state(window_id) -> ThumbState`,
//! `request(&[window_id], cx)` with the ids that are visible now, `release(cx)` when done.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use base64::Engine;
use gpui::{Context, RenderImage};
use image::Frame;
use serde_json::json;

use crate::backend::Backend;
use crate::poster::decode_poster;

/// How many captures run at once: each one is a capture round trip through the daemon.
pub const MAX_IN_FLIGHT: usize = 3;
/// Longer edge of the requested thumbnail (a card's image is about 280 px wide, 2x for Retina).
pub const THUMB_MAX_PX: u32 = 560;

/// What a card draws.
#[derive(Clone)]
pub enum ThumbState {
    /// Not requested yet, queued, or being captured: a neutral placeholder tile.
    Loading,
    Ready(Arc<RenderImage>),
    /// The capture or the decode failed: the same neutral tile, no retry.
    Failed,
}

// ---------------------------------------------------------------------------------------------
// The queue

#[derive(Debug, Clone, PartialEq, Eq)]
enum Slot<T> {
    Queued,
    InFlight,
    Ready(T),
    Failed,
}

/// Scheduling and cache of the thumbnail requests, without any I/O. `T` is the decoded image.
#[derive(Debug)]
pub struct ThumbQueue<T> {
    slots: HashMap<u32, Slot<T>>,
    /// Ids waiting to start, in priority order (front first).
    queue: VecDeque<u32>,
    in_flight: usize,
    limit: usize,
}

/// What the cache holds for one id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Peek<'a, T> {
    Loading,
    Ready(&'a T),
    Failed,
}

impl<T> ThumbQueue<T> {
    pub fn new(limit: usize) -> Self {
        ThumbQueue {
            slots: HashMap::new(),
            queue: VecDeque::new(),
            in_flight: 0,
            limit: limit.max(1),
        }
    }

    pub fn peek(&self, id: u32) -> Peek<'_, T> {
        match self.slots.get(&id) {
            Some(Slot::Ready(t)) => Peek::Ready(t),
            Some(Slot::Failed) => Peek::Failed,
            Some(Slot::Queued) | Some(Slot::InFlight) | None => Peek::Loading,
        }
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight
    }

    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// The ids that are wanted now, front of the list first (a repeated id counts once). What
    /// is already cached, queued or running is not asked for again; what was queued but is not
    /// wanted any more (scrolled away, filtered out) is dropped from the queue; what is wanted
    /// goes to the front in the given order. Returns the ids to start right now.
    pub fn request(&mut self, wanted: &[u32]) -> Vec<u32> {
        let mut order: Vec<u32> = Vec::with_capacity(wanted.len());
        for &id in wanted {
            if !order.contains(&id) {
                order.push(id);
            }
        }
        // Forget queued work nobody wants now.
        let dropped: Vec<u32> = self
            .queue
            .iter()
            .copied()
            .filter(|id| !order.contains(id))
            .collect();
        for id in dropped {
            self.slots.remove(&id);
        }
        self.queue.clear();
        for id in order {
            match self.slots.get(&id) {
                None | Some(Slot::Queued) => {
                    self.slots.insert(id, Slot::Queued);
                    self.queue.push_back(id);
                }
                Some(Slot::InFlight) | Some(Slot::Ready(_)) | Some(Slot::Failed) => {}
            }
        }
        self.pump()
    }

    /// A capture ended (`Some(image)` or `None` for a failure). Returns the next ids to start.
    pub fn finish(&mut self, id: u32, image: Option<T>) -> Vec<u32> {
        if matches!(self.slots.get(&id), Some(Slot::InFlight)) {
            self.in_flight = self.in_flight.saturating_sub(1);
            self.slots.insert(
                id,
                match image {
                    Some(t) => Slot::Ready(t),
                    None => Slot::Failed,
                },
            );
        }
        self.pump()
    }

    fn pump(&mut self) -> Vec<u32> {
        let mut start = Vec::new();
        while self.in_flight < self.limit {
            let Some(id) = self.queue.pop_front() else {
                break;
            };
            self.slots.insert(id, Slot::InFlight);
            self.in_flight += 1;
            start.push(id);
        }
        start
    }

    /// Takes every decoded image out (the picker closed) and forgets everything.
    pub fn drain_ready(&mut self) -> Vec<T> {
        self.queue.clear();
        self.in_flight = 0;
        self.slots
            .drain()
            .filter_map(|(_, s)| match s {
                Slot::Ready(t) => Some(t),
                _ => None,
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------------------------
// The model

/// Fetches the PNG bytes of one window thumbnail: `(window_id, max_px)`.
pub type PngFuture = Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send>>;
pub type Loader = Arc<dyn Fn(u32, u32) -> PngFuture + Send + Sync>;

/// The loader that asks the daemon's internal `window_thumbnail` action.
pub fn daemon_loader(backend: Arc<Backend>) -> Loader {
    Arc::new(move |window_id, max_px| {
        let backend = backend.clone();
        Box::pin(async move {
            let daemon = backend
                .daemon
                .clone()
                .ok_or_else(|| "no daemon".to_owned())?;
            let rt = backend.rt.clone();
            rt.spawn(async move {
                let v = Backend::call(
                    &daemon,
                    "window_thumbnail",
                    json!({ "window_id": window_id, "max_px": max_px }),
                )
                .await?;
                let b64 = v
                    .get("png_base64")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| "window_thumbnail returned no image".to_owned())?;
                base64::engine::general_purpose::STANDARD
                    .decode(b64)
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| e.to_string())?
        })
    })
}

/// The GPUI model: one per open picker (or sheet list).
pub struct Thumbs {
    queue: ThumbQueue<Arc<RenderImage>>,
    loader: Loader,
}

impl Thumbs {
    pub fn new(backend: Arc<Backend>) -> Self {
        Self::with_loader(daemon_loader(backend))
    }

    pub fn with_loader(loader: Loader) -> Self {
        Thumbs {
            queue: ThumbQueue::new(MAX_IN_FLIGHT),
            loader,
        }
    }

    pub fn state(&self, window_id: u32) -> ThumbState {
        match self.queue.peek(window_id) {
            Peek::Loading => ThumbState::Loading,
            Peek::Ready(i) => ThumbState::Ready(i.clone()),
            Peek::Failed => ThumbState::Failed,
        }
    }

    /// Asks for the thumbnails of the windows that are visible now (front of the list first).
    /// Cheap to call on every paint: known ids are skipped.
    pub fn request(&mut self, window_ids: &[u32], cx: &mut Context<Self>) {
        let start = self.queue.request(window_ids);
        self.start(start, cx);
    }

    fn start(&mut self, ids: Vec<u32>, cx: &mut Context<Self>) {
        for id in ids {
            let loader = self.loader.clone();
            cx.spawn(async move |this, cx| {
                let png = loader(id, THUMB_MAX_PX).await;
                let image = match png {
                    Ok(bytes) => {
                        cx.background_executor()
                            .spawn(async move { decode_png(&bytes) })
                            .await
                    }
                    Err(_) => None,
                };
                let _ = this.update(cx, |this, cx| {
                    let next = this.queue.finish(id, image);
                    this.start(next, cx);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// Frees the decoded images (the picker closed).
    pub fn release(&mut self, cx: &mut Context<Self>) {
        for image in self.queue.drain_ready() {
            cx.drop_image(image, None);
        }
    }
}

/// A PNG to the BGRA image GPUI draws, at most [`THUMB_MAX_PX`] wide.
fn decode_png(bytes: &[u8]) -> Option<Arc<RenderImage>> {
    let img = decode_poster(bytes, THUMB_MAX_PX).ok()?;
    Some(Arc::new(RenderImage::new([Frame::new(img)])))
}

#[cfg(test)]
mod tests {
    use super::*;

    type Q = ThumbQueue<&'static str>;

    #[test]
    fn at_most_the_limit_start_at_once_and_the_rest_wait_in_order() {
        let mut q = Q::new(3);
        assert_eq!(q.request(&[1, 2, 3, 4, 5]), vec![1, 2, 3]);
        assert_eq!((q.in_flight(), q.queued()), (3, 2));
        // One finishes: exactly one more starts, the next in line.
        assert_eq!(q.finish(2, Some("b")), vec![4]);
        assert_eq!((q.in_flight(), q.queued()), (3, 1));
        assert_eq!(q.finish(1, Some("a")), vec![5]);
        assert_eq!(q.finish(3, None), Vec::<u32>::new());
        assert_eq!(q.in_flight(), 2);
    }

    #[test]
    fn states_follow_the_life_of_a_request() {
        let mut q = Q::new(1);
        assert_eq!(q.peek(7), Peek::Loading); // never requested
        q.request(&[7, 8]);
        assert_eq!(q.peek(7), Peek::Loading); // running
        assert_eq!(q.peek(8), Peek::Loading); // queued
        q.finish(7, Some("seven"));
        assert_eq!(q.peek(7), Peek::Ready(&"seven"));
        q.finish(8, None);
        assert_eq!(q.peek(8), Peek::Failed);
    }

    #[test]
    fn known_ids_are_not_asked_for_again() {
        let mut q = Q::new(3);
        assert_eq!(q.request(&[1, 2]), vec![1, 2]);
        // Repainting asks again while both run: nothing new starts.
        assert_eq!(q.request(&[1, 2]), Vec::<u32>::new());
        q.finish(1, Some("a"));
        q.finish(2, None);
        // Cached and failed ones stay as they are: no retry, no second capture.
        assert_eq!(q.request(&[1, 2]), Vec::<u32>::new());
        assert_eq!(q.peek(1), Peek::Ready(&"a"));
        assert_eq!(q.peek(2), Peek::Failed);
    }

    #[test]
    fn a_repeated_id_in_one_request_counts_once() {
        let mut q = Q::new(3);
        assert_eq!(q.request(&[4, 4, 4]), vec![4]);
        assert_eq!(q.in_flight(), 1);
    }

    #[test]
    fn queued_work_that_is_no_longer_wanted_is_dropped() {
        let mut q = Q::new(1);
        assert_eq!(q.request(&[1, 2, 3, 4]), vec![1]);
        // The list scrolled: only 4 and 5 are visible. 2 and 3 never start; 1 still runs.
        assert_eq!(q.request(&[4, 5]), Vec::<u32>::new());
        assert_eq!(q.queued(), 2);
        assert_eq!(q.finish(1, Some("a")), vec![4]);
        assert_eq!(q.finish(4, Some("d")), vec![5]);
        assert_eq!(q.peek(2), Peek::Loading);
        // 2 can be asked for later again.
        q.finish(5, None);
        assert_eq!(q.request(&[2]), vec![2]);
    }

    #[test]
    fn the_newest_request_goes_to_the_front() {
        let mut q = Q::new(1);
        q.request(&[1, 2, 3]);
        q.request(&[3, 2]); // 3 now first in line
        assert_eq!(q.finish(1, Some("a")), vec![3]);
        assert_eq!(q.finish(3, Some("c")), vec![2]);
    }

    #[test]
    fn a_running_capture_that_is_no_longer_wanted_still_lands_in_the_cache() {
        let mut q = Q::new(2);
        q.request(&[1, 2]);
        q.request(&[9]); // 1 and 2 scrolled away while running
        assert_eq!(q.in_flight(), 2);
        q.finish(1, Some("a"));
        assert_eq!(q.peek(1), Peek::Ready(&"a"));
        // A late finish for an id that was never started changes nothing.
        let before = q.in_flight();
        assert!(q.finish(77, Some("x")).len() <= 1);
        assert_eq!(q.peek(77), Peek::Loading);
        assert!(q.in_flight() <= before + 1);
    }

    #[test]
    fn a_zero_limit_still_runs_one() {
        let mut q = Q::new(0);
        assert_eq!(q.request(&[1, 2]), vec![1]);
    }

    #[test]
    fn draining_empties_everything() {
        let mut q = Q::new(2);
        q.request(&[1, 2, 3]);
        q.finish(1, Some("a"));
        let ready = q.drain_ready();
        assert_eq!(ready, vec!["a"]);
        assert_eq!((q.in_flight(), q.queued()), (0, 0));
        assert_eq!(q.peek(1), Peek::Loading);
    }

    /// A fake loader scripted per id, run to completion the way the model drives the queue:
    /// every finished capture starts the next queued one.
    #[test]
    fn a_fake_loader_never_has_more_than_the_limit_in_flight() {
        let loader = |id: u32| -> Option<&'static str> {
            match id {
                3 | 6 => None,
                _ => Some("ok"),
            }
        };
        let mut q = Q::new(MAX_IN_FLIGHT);
        let mut running: VecDeque<u32> = q.request(&[1, 2, 3, 4, 5, 6, 7, 8]).into();
        let mut peak = running.len();
        let mut order = Vec::new();
        while let Some(id) = running.pop_front() {
            order.push(id);
            running.extend(q.finish(id, loader(id)));
            peak = peak.max(running.len());
        }
        assert_eq!(peak, MAX_IN_FLIGHT);
        assert_eq!(order, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        for id in 1..=8 {
            match id {
                3 | 6 => assert_eq!(q.peek(id), Peek::Failed),
                _ => assert_eq!(q.peek(id), Peek::Ready(&"ok")),
            }
        }
        assert_eq!(q.in_flight(), 0);
    }

    #[test]
    fn decode_png_makes_a_bgra_image() {
        let img = image::RgbaImage::from_pixel(8, 4, image::Rgba([10, 20, 30, 255]));
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let decoded = decode_png(&png).expect("a PNG decodes");
        let bytes = decoded.as_bytes(0).expect("one frame");
        // BGRA: red and blue swapped.
        assert_eq!(&bytes[..4], &[30, 20, 10, 255]);
        assert!(decode_png(b"not a png").is_none());
    }
}
