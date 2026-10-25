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
    tick: u64,
}

impl ThumbCache {
    pub fn new(backend: Arc<Backend>) -> Self {
        ThumbCache {
            backend,
            entries: HashMap::new(),
            tick: 0,
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
        if let Some(Entry::Ready { image, .. }) = self.entries.remove(id) {
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
        }
    }
}
