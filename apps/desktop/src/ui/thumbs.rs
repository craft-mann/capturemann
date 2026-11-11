//! Poster cache for the library grid. Posters are located through the daemon (`get_capture`
//! returns the bundle folder), decoded off the UI thread, and kept in a bounded cache so idle
//! memory stays low (NFR-5).

use gpui::{Context, RenderImage};
use image::Frame;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use crate::backend::Backend;
use crate::poster::{decode_poster, MAX_POSTER_WIDTH};

static DECODE_LOCK: Mutex<()> = Mutex::new(());

/// Decoded posters kept at once (about 0.7 MB each at 512 px).
const CACHE_CAP: usize = 60;

enum Entry {
    Pending,
    /// No poster file (broken bundle, recording without a poster yet).
    Missing,
    Ready {
        image: Arc<RenderImage>,
        used: u64,
    },
}

pub enum Thumb {
    Loading,
    Missing,
    Ready(Arc<RenderImage>),
}

pub struct ThumbCache {
    backend: Arc<Backend>,
    entries: HashMap<String, Entry>,
    /// Posters cut to a card's shape, by capture id and aspect (see [`ThumbCache::get_cover`]).
    crops: HashMap<(String, u32), Arc<RenderImage>>,
    tick: u64,
}

/// The middle of `image` cut to the width-over-height `aspect`, or `None` when it already has that
/// shape (within a pixel) or cannot be read. The card draws it with `ObjectFit::Fill` and its own
/// corner radius: GPUI's `Cover` fit rounds the corners of the oversize image, not of the visible
/// part, so a cropped poster poked out of the rounded border of its frame.
fn cover_crop(image: &RenderImage, aspect: f32) -> Option<Arc<RenderImage>> {
    let size = image.size(0);
    let (w, h) = (u32::from(size.width), u32::from(size.height));
    let bytes = image.as_bytes(0)?;
    if w == 0 || h == 0 || bytes.len() != (w as usize) * (h as usize) * 4 || aspect <= 0.0 {
        return None;
    }
    let (cw, ch) = if (w as f32) / (h as f32) > aspect {
        (((h as f32) * aspect).round().clamp(1.0, w as f32) as u32, h)
    } else {
        (w, ((w as f32) / aspect).round().clamp(1.0, h as f32) as u32)
    };
    if cw.abs_diff(w) <= 1 && ch.abs_diff(h) <= 1 {
        return None;
    }
    let (x0, y0) = ((w - cw) / 2, (h - ch) / 2);
    let mut out = Vec::with_capacity((cw as usize) * (ch as usize) * 4);
    for row in y0..y0 + ch {
        let start = ((row as usize) * (w as usize) + x0 as usize) * 4;
        out.extend_from_slice(&bytes[start..start + (cw as usize) * 4]);
    }
    // The bytes keep the channel order GPUI stores them in; only the buffer type is `RgbaImage`.
    let cropped = image::RgbaImage::from_raw(cw, ch, out)?;
    Some(Arc::new(RenderImage::new([Frame::new(cropped)])))
}

/// Drops everything cached for capture `id` and returns the decoded images to release.
fn forget_poster(
    entries: &mut HashMap<String, Entry>,
    crops: &mut HashMap<(String, u32), Arc<RenderImage>>,
    id: &str,
) -> Vec<Arc<RenderImage>> {
    let mut out = Vec::new();
    if let Some(Entry::Ready { image, .. }) = entries.remove(id) {
        out.push(image);
    }
    let cut: Vec<(String, u32)> = crops.keys().filter(|(k, _)| k == id).cloned().collect();
    out.extend(cut.into_iter().filter_map(|key| crops.remove(&key)));
    out
}

impl ThumbCache {
    pub fn new(backend: Arc<Backend>) -> Self {
        ThumbCache {
            backend,
            entries: HashMap::new(),
            crops: HashMap::new(),
            tick: 0,
        }
    }

    /// Like [`ThumbCache::get`], with the poster cut to the width-over-height `aspect` of the frame
    /// that shows it, so the frame can draw it unscaled-to-fit and round its corners exactly.
    pub fn get_cover(&mut self, id: &str, aspect: f32, cx: &mut Context<Self>) -> Thumb {
        match self.get(id, cx) {
            Thumb::Ready(image) => {
                let key = (id.to_owned(), (aspect * 1000.0).round() as u32);
                if let Some(c) = self.crops.get(&key) {
                    return Thumb::Ready(c.clone());
                }
                match cover_crop(&image, aspect) {
                    Some(c) => {
                        self.crops.insert(key, c.clone());
                        Thumb::Ready(c)
                    }
                    None => Thumb::Ready(image),
                }
            }
            other => other,
        }
    }

    /// Returns the poster state and starts loading it on first request.
    pub fn get(&mut self, id: &str, cx: &mut Context<Self>) -> Thumb {
        self.tick += 1;
        let tick = self.tick;
        match self.entries.get_mut(id) {
            Some(Entry::Ready { image, used }) => {
                *used = tick;
                return Thumb::Ready(image.clone());
            }
            Some(Entry::Pending) => return Thumb::Loading,
            Some(Entry::Missing) => return Thumb::Missing,
            None => {}
        }
        let Some(daemon) = self.backend.daemon.clone() else {
            return Thumb::Missing;
        };
        self.entries.insert(id.to_owned(), Entry::Pending);
        let id = id.to_owned();
        let rt = self.backend.rt.clone();
        // Recordings have no poster in their bundle (and `get_capture` only opens screenshots):
        // the first frame is made once, by FFmpeg or Quick Look, and cached in the library.
        let recording_root = id
            .starts_with("rec_")
            .then(|| self.backend.library_root.clone());
        cx.spawn(async move |this, cx| {
            if let Some(root) = recording_root {
                let rec_id = id.clone();
                let image = cx
                    .background_executor()
                    .spawn(async move {
                        let root = root?;
                        let dir = crate::recording::files::find_bundle(&root, &rec_id)?;
                        // Making the poster may run a tool for seconds: no lock held for that.
                        // Only the decode is serialized (NFR-5).
                        let (path, note) =
                            crate::ui::recording::details::poster_file(&root, &rec_id, &dir);
                        let _one = DECODE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
                        crate::ui::recording::details::decode_poster_file(&path, note).0
                    })
                    .await;
                this.update(cx, |this, cx| {
                    match image {
                        Some(img) => {
                            let tick = this.tick;
                            this.entries.insert(
                                id,
                                Entry::Ready {
                                    image: Arc::new(RenderImage::new([Frame::new(img)])),
                                    used: tick,
                                },
                            );
                            this.evict(cx);
                        }
                        None => {
                            this.entries.insert(id, Entry::Missing);
                        }
                    }
                    cx.notify();
                })
                .ok();
                return;
            }
            let dir = {
                let (daemon, id) = (daemon.clone(), id.clone());
                rt.spawn(async move { Backend::capture_dir(&daemon, &id).await })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r)
            };
            let image = match dir {
                Ok(dir) => {
                    let path = dir.join("thumbs").join("poster.jpg");
                    cx.background_executor()
                        .spawn(async move {
                            // One decode at a time: the allocator keeps what a decode
                            // freed, so serial decodes reuse one set of buffers (NFR-5).
                            let _one = DECODE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
                            let bytes = std::fs::read(&path).map_err(|e| {
                                crate::recording::files::PosterError::Os(e.to_string())
                            })?;
                            decode_poster(&bytes, MAX_POSTER_WIDTH)
                        })
                        .await
                        .ok()
                }
                Err(_) => None,
            };
            this.update(cx, |this, cx| {
                match image {
                    Some(img) => {
                        let tick = this.tick;
                        this.entries.insert(
                            id,
                            Entry::Ready {
                                image: Arc::new(RenderImage::new([Frame::new(img)])),
                                used: tick,
                            },
                        );
                        this.evict(cx);
                    }
                    None => {
                        this.entries.insert(id, Entry::Missing);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        Thumb::Loading
    }

    /// Forget a poster (the capture changed) so it reloads.
    pub fn invalidate(&mut self, id: &str, cx: &mut Context<Self>) {
        for image in forget_poster(&mut self.entries, &mut self.crops, id) {
            cx.drop_image(image, None);
        }
        cx.notify();
    }

    /// Forgets the posters that failed to load (the file may exist by now) and keeps every decoded
    /// one: a library reload must not make the visible grid re-decode and flicker (R1-7).
    pub fn invalidate_missing(&mut self, cx: &mut Context<Self>) {
        self.entries.retain(|_, e| !matches!(e, Entry::Missing));
        cx.notify();
    }

    pub fn invalidate_all(&mut self, cx: &mut Context<Self>) {
        for (_, e) in self.entries.drain() {
            if let Entry::Ready { image, .. } = e {
                cx.drop_image(image, None);
            }
        }
        for (_, c) in self.crops.drain() {
            cx.drop_image(c, None);
        }
        cx.notify();
    }

    fn evict(&mut self, cx: &mut Context<Self>) {
        let ready = self
            .entries
            .values()
            .filter(|e| matches!(e, Entry::Ready { .. }))
            .count();
        if ready <= CACHE_CAP {
            return;
        }
        let mut by_use: Vec<(String, u64)> = self
            .entries
            .iter()
            .filter_map(|(k, e)| match e {
                Entry::Ready { used, .. } => Some((k.clone(), *used)),
                _ => None,
            })
            .collect();
        by_use.sort_by_key(|(_, u)| *u);
        for (k, _) in by_use.into_iter().take(ready - CACHE_CAP) {
            if let Some(Entry::Ready { image, .. }) = self.entries.remove(&k) {
                cx.drop_image(image, None);
            }
            let cut: Vec<(String, u32)> = self
                .crops
                .keys()
                .filter(|(id, _)| *id == k)
                .cloned()
                .collect();
            for key in cut {
                if let Some(c) = self.crops.remove(&key) {
                    cx.drop_image(c, None);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(w: u32, h: u32) -> RenderImage {
        let mut img = image::RgbaImage::new(w, h);
        for (x, _, p) in img.enumerate_pixels_mut() {
            p.0 = [(x % 251) as u8, 0, 0, 255];
        }
        RenderImage::new([Frame::new(img)])
    }

    #[test]
    fn a_poster_is_cut_to_the_middle_in_the_frames_shape() {
        let wide = render(200, 100);
        let sq = cover_crop(&wide, 1.0).unwrap();
        assert_eq!(
            (u32::from(sq.size(0).width), u32::from(sq.size(0).height)),
            (100, 100)
        );
        // The middle: the first column of the cut is column 50 of the source.
        assert_eq!(sq.as_bytes(0).unwrap()[0], 50);
        let tall = render(100, 200);
        let sq = cover_crop(&tall, 1.0).unwrap();
        assert_eq!(
            (u32::from(sq.size(0).width), u32::from(sq.size(0).height)),
            (100, 100)
        );
    }

    /// The thumbnail of an edited screenshot (17-redaction AC "poster follows ops"): a changed
    /// capture must lose its card-shaped cut as well as its decoded poster, or the grid keeps
    /// showing the pre-redaction pixels.
    #[test]
    fn invalidating_a_capture_also_drops_its_card_cuts() {
        let mut entries = HashMap::new();
        let mut crops = HashMap::new();
        entries.insert(
            "a".to_owned(),
            Entry::Ready {
                image: Arc::new(render(200, 100)),
                used: 1,
            },
        );
        crops.insert(
            ("a".to_owned(), 1000),
            cover_crop(&render(200, 100), 1.0).unwrap(),
        );
        crops.insert(
            ("b".to_owned(), 1000),
            cover_crop(&render(200, 100), 1.0).unwrap(),
        );
        let freed = forget_poster(&mut entries, &mut crops, "a");
        assert!(entries.is_empty());
        assert!(
            !crops.contains_key(&("a".to_owned(), 1000)),
            "the cut of the edited capture survived"
        );
        assert!(
            crops.contains_key(&("b".to_owned(), 1000)),
            "other captures keep theirs"
        );
        assert_eq!(freed.len(), 2);
    }

    #[test]
    fn a_poster_of_the_frames_shape_is_used_as_it_is() {
        assert!(cover_crop(&render(160, 100), 1.6).is_none());
        assert!(cover_crop(&render(161, 100), 1.6).is_none());
        assert!(cover_crop(&render(160, 100), 0.0).is_none());
    }
}
