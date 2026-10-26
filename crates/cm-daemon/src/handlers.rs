//! The engine-backed actions: the code between the bus and `cm-capture`, `cm-library` and
//! `cm-render`.
//!
//! Undo semantics: `annotate`, `undo` and `redo` are one bundle revision each (one undo
//! step, however many ops). Creating a capture and changing settings are not undo steps (a
//! new bundle has no history yet; settings are not capture-scoped).

use base64::Engine as _;
use cm_capture::{crop_display_frame, delay_from_secs, Capture, CaptureRequest, CaptureTarget};
use cm_core::{Actor, CmError, PhysicalPoint, PhysicalRect, PhysicalSize};
use cm_library::index::{CaptureRecord, ListQuery};
use cm_library::manifest::CaptureKind;
use cm_library::thumbs::{write_poster, DEFAULT_POSTER_EDGE};
use cm_library::{Library, ShotBundle};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use time::format_description::well_known::Rfc3339;

use crate::action::{
    AnnotateParams, CaptureFromFrameParams, CaptureRefParams, CaptureScreenshotParams,
    FrameTransport, FreezeDisplayParams, GetCaptureParams, GetSettingsParams, ListLibraryParams,
    ReleaseFrameParams, SetSettingsParams, SuggestRedactionsParams, WindowThumbnailParams,
};
use crate::daemon::{blocking, lock_library, settings_result, Inner};
use crate::engines::{check_setting_writes, validate_setting_key};
use crate::error::DaemonError;
use crate::events;
use crate::frames::CachedFrame;

/// Largest `list_library` cursor (an offset) accepted (R1-15).
const MAX_CURSOR: usize = i32::MAX as usize;

/// Largest `freeze_display` frame an agent may take with `transport: "inline"` (R2-6).
pub const MAX_INLINE_FRAME_BYTES: usize = 8_000_000;

fn image_json(png: &[u8]) -> Value {
    json!({
        "mime_type": "image/png",
        "data": base64::engine::general_purpose::STANDARD.encode(png),
    })
}

fn render_png(bundle: &ShotBundle) -> Result<Vec<u8>, CmError> {
    let base = image::open(bundle.original_path())
        .map_err(|e| CmError::internal(format!("cannot read original.png: {e}")))?
        .to_rgba8();
    let out = cm_render::render_annotations(&base, bundle.ops())?;
    cm_render::encode_png(&out)
}

fn read_file(p: &std::path::Path) -> Result<Vec<u8>, CmError> {
    std::fs::read(p).map_err(|e| {
        let msg = format!("cannot read `{}`: {e}", p.display());
        if e.kind() == std::io::ErrorKind::NotFound {
            CmError::not_found(msg)
        } else {
            CmError::internal(msg)
        }
    })
}

fn record_json(r: &CaptureRecord) -> Value {
    json!({
        "id": r.id,
        "title": r.title,
        "kind": r.kind.as_str(),
        "created": r.created.format(&Rfc3339).unwrap_or_default(),
        "duration": r.duration,
        "width": r.width,
        "height": r.height,
        "size_bytes": r.size_bytes,
        "broken": r.broken,
    })
}

/// Writes the poster after the library lock was released (decoding and scaling the image is
/// slow and must not block other actions), then refreshes the index row so its `size_bytes`
/// includes the poster. Returns the warning when the poster could not be written.
fn write_poster_unlocked(
    library: &Mutex<Library>,
    bundle: &ShotBundle,
    what: &str,
) -> Option<String> {
    let warning = write_poster(bundle, DEFAULT_POSTER_EDGE)
        .err()
        .map(|e| format!("Thumbnail not {what}: {e}"));
    // The row already exists; a failure here only leaves a slightly stale size.
    let _ = lock_library(library).index_shot(bundle);
    warning
}

impl Inner {
    pub(crate) async fn list_targets(&self, windows: bool) -> Result<Value, DaemonError> {
        let capturer = self.capturer.clone();
        let targets = blocking(move || capturer.list_targets_with(windows)).await?;
        serde_json::to_value(targets).map_err(|e| DaemonError::Internal(e.to_string()))
    }

    pub(crate) async fn capture_screenshot(
        &self,
        actor: &Actor,
        p: CaptureScreenshotParams,
    ) -> Result<Value, DaemonError> {
        let delay = delay_from_secs(p.delay_s.unwrap_or(0.0))?;
        let request = CaptureRequest {
            target: p.target.to_capture_target(),
            delay,
        };
        let capturer = self.capturer.clone();
        let (copy, title) = (p.copy_to_clipboard, p.title);
        // R1-11 / CAP-5.AC2: the delay can be cancelled. The guard cancels the capture when
        // this future is dropped (the connection closed) and stays registered under the
        // `request_id` for `Daemon::cancel_delayed_capture`.
        let guard = self.delayed.register(p.request_id.clone());
        let token = guard.token();
        let out = self
            .store_capture(actor, copy, title, None, move || {
                capturer
                    .capture(request, Some(&token))?
                    .ok_or_else(|| CmError::invalid_input("capture was cancelled"))
            })
            .await;
        drop(guard);
        out
    }

    /// `capture_screenshot { target: { frame_id, rect? } }`: crops the frozen frame, never
    /// captures again (CAP-1.AC2).
    pub(crate) async fn capture_from_frame(
        &self,
        actor: &Actor,
        p: CaptureFromFrameParams,
    ) -> Result<Value, DaemonError> {
        let cached = self.frames.get(&p.frame_id, self.clock.now())?;
        let rect = p.rect.map(|r| PhysicalRect {
            origin: PhysicalPoint {
                x: r[0] as i32,
                y: r[1] as i32,
            },
            size: PhysicalSize {
                width: r[2] as u32,
                height: r[3] as u32,
            },
        });
        let frame_id = p.frame_id;
        self.store_capture(
            actor,
            p.copy_to_clipboard,
            p.title,
            Some(frame_id),
            move || crop_display_frame(&cached.frame, cached.display_id, cached.scale, rect),
        )
        .await
    }

    /// Takes a finished capture, makes the bundle, optionally copies, announces it.
    async fn store_capture(
        &self,
        actor: &Actor,
        copy: bool,
        title: Option<String>,
        frame_id: Option<String>,
        take: impl FnOnce() -> Result<Capture, CmError> + Send + 'static,
    ) -> Result<Value, DaemonError> {
        let (library, clipboard) = (self.library.clone(), self.clipboard.clone());
        let now = self.clock.now();
        let out = blocking(move || {
            let cap = take()?;
            let png = cap.frame.to_png()?;
            let bundle = lock_library(&library).create_screenshot_without_poster(
                &png,
                title.as_deref(),
                now,
            )?;
            let mut warnings: Vec<String> = bundle.warnings().to_vec();
            warnings.extend(write_poster_unlocked(&library, &bundle, "created"));
            let copied = if copy {
                match clipboard.copy_png(&png) {
                    Ok(()) => true,
                    Err(e) => {
                        warnings.push(format!("Not copied to the clipboard: {e}"));
                        false
                    }
                }
            } else {
                false
            };
            let size = cap.frame.size();
            let mut out = json!({
                "capture_id": bundle.manifest().id,
                "path": bundle.original_path(),
                "kind": cap.kind,
                "width": size.width,
                "height": size.height,
                "copied_to_clipboard": copied,
                "warnings": warnings,
                "image": image_json(&png),
            });
            if let Some(id) = frame_id {
                out["frame_id"] = json!(id);
            }
            Ok(out)
        })
        .await?;
        self.events.publish(
            events::CAPTURE_CREATED,
            json!({ "capture_id": out["capture_id"], "by": actor }),
        );
        Ok(out)
    }

    /// `freeze_display`: one capture of the display into the frame cache. No bundle, no
    /// event, nothing in the library.
    pub(crate) async fn freeze_display(
        &self,
        actor: &Actor,
        p: FreezeDisplayParams,
    ) -> Result<Value, DaemonError> {
        let capturer = self.capturer.clone();
        let display_id = p.display_id;
        let cap = blocking(move || {
            capturer
                .capture(
                    CaptureRequest::new(CaptureTarget::Display { display_id }),
                    None,
                )?
                .ok_or_else(|| CmError::invalid_input("capture was cancelled"))
        })
        .await?;
        let frame = Arc::new(cap.frame);
        let (width, height) = (frame.width(), frame.height());
        // R2-6: a socket client cannot make the daemon queue tens of MB of base64 in memory;
        // big frames go through the file transport.
        if p.transport == FrameTransport::Inline
            && actor.is_agent()
            && frame.rgba().len() > MAX_INLINE_FRAME_BYTES
        {
            return Err(CmError::invalid_field(
                "transport",
                format!(
                    "`inline` is limited to {} MB of pixels ({}x{} is {} MB); use `file`",
                    MAX_INLINE_FRAME_BYTES / 1_000_000,
                    width,
                    height,
                    frame.rgba().len() / 1_000_000
                ),
            )
            .into());
        }
        let scale = cap.scale;
        let id = self.frames.insert(
            CachedFrame {
                frame: frame.clone(),
                display_id,
                scale,
            },
            self.clock.now(),
        );
        let mut out = json!({
            "frame_id": id,
            "display_id": display_id,
            "width": width,
            "height": height,
            "scale": scale.get(),
            "format": "rgba8",
            "stride": u64::from(width) * 4,
            "bytes": frame.rgba().len(),
            "ttl_s": self.frames.ttl().as_secs(),
        });
        let sent = match p.transport {
            FrameTransport::File => {
                let (this, id) = (self.frames.clone(), id.clone());
                blocking(move || this.write_file(&id)).await.map(|path| {
                    out["transport"] = json!("file");
                    out["path"] = json!(path);
                })
            }
            FrameTransport::Inline => {
                let f = frame.clone();
                blocking(move || Ok(base64::engine::general_purpose::STANDARD.encode(f.rgba())))
                    .await
                    .map(|data| {
                        out["transport"] = json!("inline");
                        out["data"] = json!(data);
                    })
            }
        };
        if let Err(e) = sent {
            self.frames.release(&id);
            return Err(e);
        }
        Ok(out)
    }

    /// `window_thumbnail`: one window capture, downscaled, as a PNG. Nothing is cached or stored.
    pub(crate) async fn window_thumbnail(
        &self,
        p: WindowThumbnailParams,
    ) -> Result<Value, DaemonError> {
        let capturer = self.capturer.clone();
        blocking(move || {
            let frame = capturer.window_thumbnail(p.window_id, p.max_px)?;
            let png = frame.to_png()?;
            Ok(json!({
                "window_id": p.window_id,
                "width": frame.width(),
                "height": frame.height(),
                "png_base64": base64::engine::general_purpose::STANDARD.encode(png),
            }))
        })
        .await
    }

    pub(crate) fn release_frame(&self, p: &ReleaseFrameParams) -> Result<Value, DaemonError> {
        Ok(json!({ "released": self.frames.release(&p.frame_id) }))
    }

    pub(crate) async fn annotate(
        &self,
        actor: &Actor,
        p: AnnotateParams,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&p.capture_id).await;
        let (library, now, by) = (self.library.clone(), self.clock.now(), actor.clone());
        let clipboard = self.clipboard.clone();
        let id = p.capture_id.clone();
        let copy = p.copy_to_clipboard;
        let return_image = p.return_image;
        let (rev, png, copied, warnings) = blocking(move || {
            let mut bundle = lock_library(&library).open_screenshot(&id)?;
            // One batch, one revision; validation happens before anything changes.
            let rev = if p.ops.is_empty() && p.replace {
                // "Revert to original": clear every op as one undo step (ANN-3).
                if bundle.ops().is_empty() {
                    return Err(CmError::invalid_field(
                        "ops",
                        "there are no annotations to clear",
                    ));
                }
                bundle.revert_to_original(by, now)?
            } else {
                bundle.annotate(p.ops, p.replace, by, now)?
            };
            let png = render_png(&bundle)?;
            bundle.write_render(&png)?;
            lock_library(&library).save_shot_without_poster(&mut bundle)?;
            write_poster_unlocked(&library, &bundle, "updated");
            // R1-1: the clipboard gets the RENDERED image (annotations and redactions applied),
            // after the revision is saved; never `original.png`.
            let mut warnings: Vec<String> = Vec::new();
            let copied = copy
                && match clipboard.copy_png(&png) {
                    Ok(()) => true,
                    Err(e) => {
                        warnings.push(format!("Not copied to the clipboard: {e}"));
                        false
                    }
                };
            Ok((rev, png, copied, warnings))
        })
        .await?;
        self.events.publish(
            events::CAPTURE_REVISED,
            json!({ "capture_id": p.capture_id, "revision": rev.rev, "by": rev.by }),
        );
        Ok(json!({
            "capture_id": p.capture_id,
            "revision": rev.rev,
            "by": rev.by,
            "copied_to_clipboard": copied,
            "warnings": warnings,
            "image": return_image.then(|| image_json(&png)),
        }))
    }

    pub(crate) async fn undo_redo(
        &self,
        actor: &Actor,
        p: CaptureRefParams,
        undo: bool,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&p.capture_id).await;
        // A recording's undo steps are its edits (EDT-1 to EDT-3), not annotation ops.
        let (library, id) = (self.library.clone(), p.capture_id.clone());
        let is_recording =
            blocking(move || Ok(lock_library(&library).get(&id)?.kind.is_recording())).await?;
        if is_recording {
            return self.undo_redo_recording(actor, &p.capture_id, undo).await;
        }
        let (library, now, by) = (self.library.clone(), self.clock.now(), actor.clone());
        let id = p.capture_id.clone();
        let (rev, png, can_undo, can_redo) = blocking(move || {
            let mut bundle = lock_library(&library).open_screenshot(&id)?;
            let rev = if undo {
                bundle.undo(by, now)
            } else {
                bundle.redo(by, now)?
            };
            let Some(rev) = rev else {
                return Ok((None, None, bundle.can_undo(), bundle.can_redo()));
            };
            let png = render_png(&bundle)?;
            bundle.write_render(&png)?;
            lock_library(&library).save_shot_without_poster(&mut bundle)?;
            write_poster_unlocked(&library, &bundle, "updated");
            Ok((Some(rev), Some(png), bundle.can_undo(), bundle.can_redo()))
        })
        .await?;
        if let Some(r) = &rev {
            self.events.publish(
                events::CAPTURE_REVISED,
                json!({ "capture_id": p.capture_id, "revision": r.rev, "by": r.by }),
            );
        }
        // Nothing to undo/redo is not an error: `revision` is null and `changed` false.
        Ok(json!({
            "capture_id": p.capture_id,
            "revision": rev.as_ref().map(|r| r.rev),
            "changed": rev.is_some(),
            "can_undo": can_undo,
            "can_redo": can_redo,
            "image": png.as_deref().map(image_json),
        }))
    }

    pub(crate) async fn get_capture(&self, p: GetCaptureParams) -> Result<Value, DaemonError> {
        // R2-14: reads wait for an annotate/undo on the same capture to finish writing, so the
        // image and the ops come from one revision.
        let _serial = self.capture_locks.lock(&p.capture_id).await;
        let library = self.library.clone();
        let out = blocking(move || {
            let bundle = lock_library(&library).open_screenshot(&p.capture_id)?;
            let m = bundle.manifest();
            let latest = m.latest_rev();
            if let Some(r) = p.revision {
                if r != latest {
                    return Err(CmError::invalid_field(
                        "revision",
                        format!("only the current revision ({latest}) can be read"),
                    ));
                }
            }
            let image = if !p.include_image {
                Value::Null
            } else if bundle.ops().is_empty() {
                image_json(&read_file(&bundle.original_path())?)
            } else if m.render.is_some() && !bundle.render_is_stale() {
                image_json(&read_file(&bundle.render_path())?)
            } else {
                image_json(&render_png(&bundle)?)
            };
            Ok(json!({
                "capture_id": m.id,
                "title": m.title,
                "kind": m.kind.as_str(),
                "created": m.created.format(&Rfc3339).unwrap_or_default(),
                "path": bundle.dir(),
                "original": m.original,
                "tracks": [{ "kind": "image", "file": m.original.file, "size": m.original.size }],
                "uploads": cm_library::uploads_of(bundle.dir()),
                "publishes": cm_library::publishes_of(bundle.dir()),
                "ops": bundle.ops(),
                "revision": latest,
                "revisions": bundle.revisions(),
                "can_undo": bundle.can_undo(),
                "can_redo": bundle.can_redo(),
                "warnings": bundle.warnings(),
                "image": image,
            }))
        })
        .await?;
        Ok(out)
    }

    /// `suggest_redactions` (ANN-2.AC4): runs the text recognizer over `original.png` (never the
    /// render, so an earlier blur cannot hide a secret from detection) and proposes blur
    /// rectangles in original-image pixels. Nothing is applied and nothing is written; the
    /// previews are masked so the result never repeats a secret.
    pub(crate) async fn suggest_redactions(
        &self,
        p: SuggestRedactionsParams,
    ) -> Result<Value, DaemonError> {
        use cm_render::ocr::{RedactionKind, SuggestOptions};
        let recognizer = self.recognizer.clone();
        if !recognizer.is_available() {
            return Err(CmError::not_implemented(
                "text recognition is not available on this machine, so redactions cannot be \
                 suggested; blur regions yourself with `annotate`",
            )
            .into());
        }
        let library = self.library.clone();
        let out = blocking(move || {
            let path = lock_library(&library)
                .open_screenshot(&p.capture_id)?
                .original_path();
            let image = image::open(&path)
                .map_err(|e| CmError::internal(format!("cannot read original.png: {e}")))?
                .to_rgba8();
            let lines = recognizer.recognize(&image)?;
            let kinds = p
                .kinds
                .iter()
                .filter_map(|k| RedactionKind::parse(k))
                .collect();
            let found = cm_render::ocr::suggest_redactions(
                &lines,
                image.dimensions(),
                &SuggestOptions { kinds },
            );
            let rects: Vec<Value> = found
                .iter()
                .map(|s| {
                    json!({
                        "rect": [s.rect.x.round(), s.rect.y.round(), s.rect.w.round(), s.rect.h.round()],
                        "kind": s.kind.as_str(),
                        "confidence": (f64::from(s.confidence) * 100.0).round() / 100.0,
                        "text_preview": s.text_preview,
                    })
                })
                .collect();
            Ok(json!({
                "capture_id": p.capture_id,
                "recognizer": recognizer.name(),
                "image_size": [image.width(), image.height()],
                "rects": rects,
                "applied": false,
            }))
        })
        .await?;
        Ok(out)
    }

    pub(crate) async fn list_library(&self, p: ListLibraryParams) -> Result<Value, DaemonError> {
        let offset = match &p.cursor {
            None => 0,
            Some(c) => c
                .parse::<usize>()
                .ok()
                // R1-15: an absurd cursor must not overflow `offset + limit` (or SQLite's i64
                // OFFSET) downstream.
                .filter(|n| *n <= MAX_CURSOR)
                .ok_or_else(|| CmError::invalid_field("cursor", "not a cursor from this daemon"))?,
        };
        let library = self.library.clone();
        blocking(move || {
            let q = ListQuery {
                kind: p.kind.as_deref().and_then(CaptureKind::parse),
                status: p
                    .status
                    .as_deref()
                    .and_then(cm_library::index::StatusFilter::parse),
                // One extra row tells whether another page exists.
                limit: Some(p.limit + 1),
                offset,
                ..ListQuery::default()
            };
            let lib = lock_library(&library);
            let searching = p.query.as_deref().is_some_and(|t| !t.trim().is_empty());
            let mut hits: Vec<(CaptureRecord, Option<cm_library::index::TranscriptMatch>)> =
                match p.query.as_deref().map(str::trim) {
                    Some(text) if !text.is_empty() => lib
                        .search(text, &q)?
                        .into_iter()
                        .map(|h| (h.record, h.transcript))
                        .collect(),
                    _ => lib.list(&q)?.into_iter().map(|r| (r, None)).collect(),
                };
            let more = hits.len() > p.limit;
            hits.truncate(p.limit);
            let captures: Vec<Value> = hits
                .iter()
                .map(|(r, t)| {
                    let mut v = record_json(r);
                    let dir = lib.bundle_path(r);
                    v["uploads"] = Value::Array(cm_library::uploads_of(&dir));
                    v["publishes"] = Value::Array(cm_library::publishes_of(&dir));
                    // Transcript matches (search only): the snippet around the hit and where in
                    // the recording it starts.
                    if searching {
                        v["transcript"] = match t {
                            Some(m) => json!({
                                "snippet": m.snippet,
                                "start_s": m.start.as_secs_f64(),
                            }),
                            None => Value::Null,
                        };
                    }
                    v
                })
                .collect();
            Ok(json!({
                "captures": captures,
                "next_cursor": more.then(|| offset.saturating_add(p.limit).to_string()),
            }))
        })
        .await
    }

    pub(crate) fn get_settings(&self, p: GetSettingsParams) -> Result<Value, DaemonError> {
        let all = self.settings.all()?;
        let Some(keys) = p.keys else {
            return Ok(settings_result(
                all.into_iter()
                    .filter(|(k, _)| validate_setting_key(k).is_ok())
                    .collect(),
            ));
        };
        let mut out = serde_json::Map::new();
        for k in keys {
            validate_setting_key(&k).map_err(|e| match e {
                CmError::InvalidInput { message } => CmError::InvalidInput {
                    message: message.replacen("values", "keys", 1),
                },
                other => other,
            })?;
            let v = all
                .get(&k)
                .ok_or_else(|| CmError::not_found(format!("setting `{k}` is not set")))?;
            out.insert(k, v.clone());
        }
        Ok(settings_result(out))
    }

    pub(crate) async fn set_settings(
        &self,
        actor: &Actor,
        p: SetSettingsParams,
    ) -> Result<Value, DaemonError> {
        // The self-test verdict is written by the self-test only, whoever asks.
        if p.values
            .contains_key(crate::recording::selftest::SETTING_KEY)
        {
            return Err(CmError::invalid_field(
                "values",
                format!(
                    "`{}` is written by the 60 fps self-test only; run `run_fps_selftest`",
                    crate::recording::selftest::SETTING_KEY
                ),
            )
            .into());
        }
        // R2-3: key/value validation for everyone; the app-only deny list for agents.
        check_setting_writes(actor.is_agent(), &p.values)?;
        // The store may write a file: keep that I/O off the async workers.
        let (store, values) = (self.settings.clone(), p.values);
        let effective = blocking(move || store.set(&values)).await?;
        Ok(settings_result(
            effective
                .into_iter()
                .filter(|(k, _)| validate_setting_key(k).is_ok())
                .collect(),
        ))
    }
}
