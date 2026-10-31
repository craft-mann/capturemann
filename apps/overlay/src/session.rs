//! Talking to the daemon: freezing every display, then committing the user's choice.
//!
//! Flow (CAP-1): `list_targets { windows: false }` (displays only: window enumeration is slow
//! and not needed to freeze), one `freeze_display { display_id }` per display (a frame in
//! the daemon's short-lived cache: no bundle, no `capture.created`, nothing in the library),
//! the picker runs, then on confirm `capture_screenshot { target: { frame_id, rect? } }`
//! crops the frozen frame (display and region modes) or `capture_screenshot` takes the window.
//! Finally `release_frame` frees the frames. [`stream`] is the flow the binary uses: it reports
//! each step as a [`SessionEvent`] so the windows can open while the frames are still arriving,
//! and loads the window list (hover highlight, click to capture a window) on a second
//! connection after the freezes are under way. [`freeze_all`] is the blocking form.
//!
//! Because the daemon crops the very frame the user looked at, a region is the frozen pixels
//! by construction (CAP-1.AC2): there is no second capture to differ. The returned PNG is
//! still compared with the same crop of the overlay's copy and reported as `pixel_identical`.
//!
//! The pixels come as a raw RGBA8 file in the daemon's private temp directory
//! (`transport: "file"`), which is far cheaper than a PNG round trip; see `cm-daemon`'s
//! `frames` module for the measurements.

use std::sync::Arc;

use base64::Engine as _;
use cm_core::{ErrorCode, PhysicalRect};
use cm_ipc::Endpoint;
use serde_json::{json, Value};

use crate::client::Client;
use crate::error::OverlayError;
use crate::paint::Frame;
use crate::picker::{Mode, Outcome};
use crate::targets::{DisplayInfo, Targets, WindowInfo};
use crate::trace::Tracer;

/// A frozen display: the daemon's frame of it, loaded.
#[derive(Clone)]
pub struct Frozen {
    pub display: DisplayInfo,
    /// The daemon's `frame_id` (the key of `capture_screenshot`'s `target.frame_id`).
    pub frame_id: String,
    /// Shared with the window that shows it.
    pub frame: Arc<Frame>,
}

/// Progress of [`stream`], in the order it happens (windows may come after the frames).
#[derive(Clone)]
pub enum SessionEvent {
    /// The displays, before any frame exists: windows can be opened now.
    Displays(Vec<DisplayInfo>),
    /// One display is frozen and its frame is loaded.
    Frozen(Frozen),
    /// The window list (empty when it could not be loaded: windows are then simply not
    /// highlighted or pickable).
    Windows(Vec<WindowInfo>),
    /// The session failed; no more events follow.
    Failed(OverlayError),
}

/// Where [`stream`] reports to; called from runtime threads.
pub type Emit = Arc<dyn Fn(SessionEvent) + Send + Sync>;

/// Every `frame_id` the daemon handed out, noted the moment `freeze_display` answers. A freeze
/// still decoding its file when the overlay ends has no [`Frozen`] yet, but its frame is in the
/// daemon's cache (33 MB raw at 4K) and must still be released (R4-20).
pub type IssuedFrames = Arc<std::sync::Mutex<Vec<String>>>;

/// The user's confirmed choice on one display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    pub display_id: u32,
    pub outcome: Outcome,
}

fn decode_png_rgba(b64: &str) -> Result<(u32, u32, Vec<u8>), OverlayError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| OverlayError::internal(format!("bad image data from the daemon: {e}")))?;
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .map_err(|e| OverlayError::internal(format!("cannot decode the daemon's PNG: {e}")))?
        .to_rgba8();
    Ok((img.width(), img.height(), img.into_raw()))
}

/// `freeze_display` params: the raw-file transport.
pub fn freeze_params(display_id: u32) -> Value {
    json!({ "display_id": display_id, "transport": "file" })
}

/// `capture_screenshot` params for the whole frozen display (CAP-1.AC1).
pub fn frame_display_params(frame_id: &str) -> Value {
    json!({ "target": { "frame_id": frame_id } })
}

/// `capture_screenshot` params for a physical region relative to the frozen display
/// (CAP-1.AC2): the daemon crops the frame, it does not capture again.
pub fn region_params(frame_id: &str, rect: PhysicalRect) -> Value {
    json!({ "target": { "frame_id": frame_id, "rect": [
        rect.origin.x, rect.origin.y, rect.size.width, rect.size.height
    ] } })
}

/// `capture_screenshot` params for a window.
pub fn window_params(window_id: u32) -> Value {
    json!({ "target": { "window_id": window_id } })
}

/// Connects, lists targets and freezes every display. Fails before any window opens, so a
/// missing permission shows no overlay (CAP-1 edge cases).
pub async fn freeze_all(endpoint: &Endpoint) -> Result<(Targets, Vec<Frozen>), OverlayError> {
    let mut client = Client::connect(endpoint).await?;
    let targets = parse_targets(client.call("list_targets", json!({})).await?)?;
    if targets.displays.is_empty() {
        return Err(OverlayError::new(ErrorCode::NotFound, "no displays found"));
    }
    let mut tasks = tokio::task::JoinSet::new();
    for (i, d) in targets.displays.iter().cloned().enumerate() {
        let endpoint = endpoint.clone();
        tasks.spawn(async move { (i, freeze_one(&endpoint, d, None, None).await) });
    }
    let mut slots: Vec<Option<Frozen>> = targets.displays.iter().map(|_| None).collect();
    let mut first_err: Option<OverlayError> = None;
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok((i, Ok(f))) => slots[i] = Some(f),
            Ok((_, Err(e))) => keep_most_useful(&mut first_err, e),
            Err(e) => first_err = Some(OverlayError::internal(format!("freeze task failed: {e}"))),
        }
    }
    if let Some(e) = first_err {
        return Err(e);
    }
    let frozen = slots.into_iter().flatten().collect();
    Ok((targets, frozen))
}

async fn freeze_one(
    endpoint: &Endpoint,
    display: DisplayInfo,
    tracer: Option<&Tracer>,
    issued: Option<&IssuedFrames>,
) -> Result<Frozen, OverlayError> {
    let mut client = Client::connect(endpoint).await?;
    let result = client
        .call("freeze_display", freeze_params(display.id))
        .await?;
    if let Some(t) = tracer {
        t.mark("freeze_returned", json!({ "display_id": display.id }));
    }
    let frame_id = result["frame_id"]
        .as_str()
        .ok_or_else(|| OverlayError::internal("freeze_display returned no frame_id"))?
        .to_owned();
    if let Some(issued) = issued {
        issued
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(frame_id.clone());
    }
    let (w, h) = (
        result["width"].as_u64().unwrap_or(0),
        result["height"].as_u64().unwrap_or(0),
    );
    if (w, h)
        != (
            u64::from(display.bounds.size.width),
            u64::from(display.bounds.size.height),
        )
    {
        return Err(OverlayError::internal(format!(
            "display {} is {}x{} px but its frame is {w}x{h} px; \
             the overlay cannot map the window 1:1",
            display.id, display.bounds.size.width, display.bounds.size.height
        )));
    }
    let path = result["path"]
        .as_str()
        .ok_or_else(|| OverlayError::internal("freeze_display returned no path"))?
        .to_owned();
    let (w, h) = (display.bounds.size.width, display.bounds.size.height);
    let frame = tokio::task::spawn_blocking(move || {
        let rgba = std::fs::read(&path).map_err(|e| {
            OverlayError::internal(format!("cannot read the daemon's frame file `{path}`: {e}"))
        })?;
        Frame::from_rgba(w, h, &rgba)
            .ok_or_else(|| OverlayError::internal("the daemon's frame file has the wrong size"))
    })
    .await
    .map_err(|e| OverlayError::internal(format!("frame task failed: {e}")))??;
    if let Some(t) = tracer {
        t.mark("frame_decoded", json!({ "display_id": display.id }));
    }
    Ok(Frozen {
        display,
        frame_id,
        frame: Arc::new(frame),
    })
}

fn parse_targets(v: Value) -> Result<Targets, OverlayError> {
    serde_json::from_value(v)
        .map_err(|e| OverlayError::internal(format!("unexpected list_targets result: {e}")))
}

/// `list_targets` params for the displays only: no window enumeration (CAP-1.AC1).
pub fn displays_only_params() -> Value {
    json!({ "windows": false })
}

/// Connects, lists the displays and freezes them all in parallel, reporting every step to
/// `emit` (see [`SessionEvent`]); returns the frozen displays. Errors are returned, not
/// emitted: the caller decides how to tell the window layer.
///
/// The window list is requested on its own connection once the freezes are under way, and its
/// event may arrive after this function returns.
pub async fn stream(
    endpoint: &Endpoint,
    tracer: &Tracer,
    emit: &Emit,
) -> Result<Vec<Frozen>, OverlayError> {
    stream_tracked(endpoint, tracer, emit, &IssuedFrames::default()).await
}

/// [`stream`], noting every frame id in `issued` as soon as the daemon returns it, so the caller
/// can release frames of freezes that are still in flight when it gives up waiting.
pub async fn stream_tracked(
    endpoint: &Endpoint,
    tracer: &Tracer,
    emit: &Emit,
    issued: &IssuedFrames,
) -> Result<Vec<Frozen>, OverlayError> {
    let mut client = Client::connect(endpoint).await?;
    tracer.mark("connected", Value::Null);
    let displays = match client.call("list_targets", displays_only_params()).await {
        Ok(v) => parse_targets(v)?.displays,
        // A daemon from before `windows: false` rejects the parameter: ask for everything.
        Err(e) if e.code == "invalid_input" => {
            parse_targets(client.call("list_targets", json!({})).await?)?.displays
        }
        Err(e) => return Err(e),
    };
    tracer.mark("displays_listed", json!({ "displays": displays.len() }));
    if displays.is_empty() {
        return Err(OverlayError::new(ErrorCode::NotFound, "no displays found"));
    }
    emit(SessionEvent::Displays(displays.clone()));

    let mut tasks = tokio::task::JoinSet::new();
    for (i, d) in displays.iter().cloned().enumerate() {
        let (endpoint, tracer, emit, issued) = (
            endpoint.clone(),
            tracer.clone(),
            emit.clone(),
            issued.clone(),
        );
        tasks.spawn(async move {
            let r = freeze_one(&endpoint, d, Some(&tracer), Some(&issued)).await;
            if let Ok(f) = &r {
                emit(SessionEvent::Frozen(f.clone()));
            }
            (i, r)
        });
    }
    {
        let (endpoint, tracer, emit) = (endpoint.clone(), tracer.clone(), emit.clone());
        tokio::spawn(async move {
            let windows = match load_windows(&endpoint).await {
                Ok(w) => w,
                Err(e) => {
                    tracer.mark("windows_failed", json!({ "error": e.message }));
                    Vec::new()
                }
            };
            tracer.mark("windows_listed", json!({ "windows": windows.len() }));
            emit(SessionEvent::Windows(windows));
        });
    }
    let mut slots: Vec<Option<Frozen>> = displays.iter().map(|_| None).collect();
    let mut first_err: Option<OverlayError> = None;
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok((i, Ok(f))) => slots[i] = Some(f),
            Ok((_, Err(e))) => keep_most_useful(&mut first_err, e),
            Err(e) => first_err = Some(OverlayError::internal(format!("freeze task failed: {e}"))),
        }
    }
    if let Some(e) = first_err {
        return Err(e);
    }
    tracer.mark("frozen", json!({ "displays": displays.len() }));
    Ok(slots.into_iter().flatten().collect())
}

async fn load_windows(endpoint: &Endpoint) -> Result<Vec<WindowInfo>, OverlayError> {
    let mut client = Client::connect(endpoint).await?;
    Ok(parse_targets(client.call("list_targets", json!({})).await?)?.windows)
}

/// A permission error is the most useful one to report.
fn keep_most_useful(slot: &mut Option<OverlayError>, e: OverlayError) {
    if slot.as_ref().is_none_or(|p| p.code != "permission_missing") {
        *slot = Some(e);
    }
}

/// Frees the daemon's frames (best effort: they also expire on their own).
pub async fn release_all(endpoint: &Endpoint, frozen: &[Frozen]) {
    let ids: Vec<String> = frozen.iter().map(|f| f.frame_id.clone()).collect();
    release_ids(endpoint, &ids).await;
}

/// Frees the frames with these ids.
pub async fn release_ids(endpoint: &Endpoint, ids: &[String]) {
    if ids.is_empty() {
        return;
    }
    let Ok(mut client) = Client::connect(endpoint).await else {
        return;
    };
    for id in ids {
        let _ = client
            .call("release_frame", json!({ "frame_id": id }))
            .await;
    }
}

/// The ids to release at the end: every frame the daemon issued plus every one that was loaded
/// (a loaded frame was issued too; this only dedupes).
pub fn ids_to_release(issued: &[String], frozen: &[Frozen]) -> Vec<String> {
    let mut all: Vec<String> = issued.to_vec();
    for f in frozen {
        if !all.contains(&f.frame_id) {
            all.push(f.frame_id.clone());
        }
    }
    all
}

/// True when the PNG (base64) holds exactly the pixels of `rect` in the frozen frame.
pub fn png_matches_crop(frame: &Frame, rect: PhysicalRect, b64: &str) -> bool {
    let Some(expected) = frame.crop_rgba(rect) else {
        return false;
    };
    match decode_png_rgba(b64) {
        Ok((w, h, rgba)) => w == rect.size.width && h == rect.size.height && rgba == expected,
        Err(_) => false,
    }
}

/// `capture_screenshot` params for the desktop's own picker (`target: { pick: true }`).
pub fn pick_params() -> Value {
    json!({ "target": { "pick": true } })
}

/// True on a Wayland session, where this program may not draw above other programs nor read
/// their geometry: `XDG_SESSION_TYPE` decides when it names a server, otherwise
/// `WAYLAND_DISPLAY` (the same rule as `cm-capture`'s session detection; the overlay does not
/// link that crate).
pub fn is_wayland_session(xdg_session_type: Option<&str>, wayland_display: Option<&str>) -> bool {
    match xdg_session_type {
        Some("wayland") => true,
        Some("x11") => false,
        _ => wayland_display.is_some_and(|d| !d.is_empty()),
    }
}

/// Window mode on a Wayland session: the compositor has no window list to highlight, so the
/// desktop's own picker (through the daemon's `pick` target) chooses the window and the
/// capture is stored by the daemon. Esc in that picker is `user_declined` (exit 3).
pub async fn pick_with_system_picker(
    endpoint: &Endpoint,
    mode: Mode,
) -> Result<Value, OverlayError> {
    let mut client = Client::connect(endpoint).await?;
    let r = client.call("capture_screenshot", pick_params()).await?;
    Ok(summarize(mode, 0, &r, None, None))
}

/// The JSON printed on stdout: the daemon's capture fields without image bytes.
pub fn summarize(
    mode: Mode,
    display_id: u32,
    result: &Value,
    rect: Option<PhysicalRect>,
    pixel_identical: Option<bool>,
) -> Value {
    let mode_name = match mode {
        Mode::Region => "region",
        Mode::Window => "window",
        Mode::Display => "display",
    };
    let mut out = json!({
        "mode": mode_name,
        "capture_id": result["capture_id"],
        "path": result["path"],
        "kind": result["kind"],
        "width": result["width"],
        "height": result["height"],
        "display_id": display_id,
        "warnings": result["warnings"],
    });
    if let Some(r) = rect {
        out["rect"] = json!([r.origin.x, r.origin.y, r.size.width, r.size.height]);
    }
    if let Some(same) = pixel_identical {
        out["pixel_identical"] = json!(same);
    }
    out
}

/// `--pick`: the chosen target as JSON, with no capture made (the recording start sheet turns it
/// into `start_recording`'s `target`). Regions are physical px relative to the display, exactly
/// like `capture_screenshot`'s `rect`.
pub fn pick_json(mode: Mode, choice: Choice) -> Value {
    let mode_name = match mode {
        Mode::Region => "region",
        Mode::Window => "window",
        Mode::Display => "display",
    };
    let mut picked = json!({ "mode": mode_name, "display_id": choice.display_id });
    match choice.outcome {
        Outcome::Display => {}
        Outcome::Window { window_id } => picked["window_id"] = json!(window_id),
        Outcome::Region { rect } => {
            picked["rect"] = json!([
                rect.origin.x,
                rect.origin.y,
                rect.size.width,
                rect.size.height
            ]);
        }
    }
    json!({ "picked": picked })
}

/// Turns the user's choice into a capture and returns the JSON to print.
pub async fn commit(
    endpoint: &Endpoint,
    mode: Mode,
    choice: Choice,
    frozen: &[Frozen],
) -> Result<Value, OverlayError> {
    let source = frozen
        .iter()
        .find(|f| f.display.id == choice.display_id)
        .ok_or_else(|| OverlayError::internal("the chosen display is not frozen"))?;
    match choice.outcome {
        // The whole frozen frame is the display capture (CAP-1.AC1).
        Outcome::Display => {
            let mut client = Client::connect(endpoint).await?;
            let r = client
                .call("capture_screenshot", frame_display_params(&source.frame_id))
                .await?;
            Ok(summarize(mode, choice.display_id, &r, None, None))
        }
        Outcome::Window { window_id } => {
            let mut client = Client::connect(endpoint).await?;
            let r = client
                .call("capture_screenshot", window_params(window_id))
                .await?;
            Ok(summarize(mode, choice.display_id, &r, None, None))
        }
        Outcome::Region { rect } => {
            let mut client = Client::connect(endpoint).await?;
            let r = client
                .call("capture_screenshot", region_params(&source.frame_id, rect))
                .await?;
            let same = r["image"]["data"]
                .as_str()
                .is_some_and(|b64| png_matches_crop(&source.frame, rect, b64));
            Ok(summarize(
                mode,
                choice.display_id,
                &r,
                Some(rect),
                Some(same),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::{PhysicalPoint, PhysicalSize};

    /// CAP-1 on Linux: Wayland is recognised by the session type, then by the display variable.
    #[test]
    fn wayland_is_told_by_the_session_type_then_the_display() {
        assert!(is_wayland_session(Some("wayland"), None));
        assert!(is_wayland_session(None, Some("wayland-0")));
        assert!(is_wayland_session(Some("tty"), Some("wayland-0")));
        // An X11 session with a leftover WAYLAND_DISPLAY is X11.
        assert!(!is_wayland_session(Some("x11"), Some("wayland-0")));
        assert!(!is_wayland_session(None, None));
        assert!(!is_wayland_session(None, Some("")));
        assert_eq!(pick_params()["target"]["pick"], true);
    }

    /// R4-20: a frame whose freeze was still decoding when the overlay ended is released too:
    /// the issued ids are what counts, the loaded ones only add nothing new.
    #[test]
    fn r4_20_issued_frames_are_all_released() {
        let issued = vec!["fr_1".to_owned(), "fr_2".to_owned()];
        assert_eq!(ids_to_release(&issued, &[]), issued);
        assert!(ids_to_release(&[], &[]).is_empty());
        let shared = IssuedFrames::default();
        shared.lock().unwrap().push("fr_9".into());
        let copy = shared.clone();
        assert_eq!(copy.lock().unwrap().as_slice(), ["fr_9"]);
    }

    fn rect(x: i32, y: i32, w: u32, h: u32) -> PhysicalRect {
        PhysicalRect {
            origin: PhysicalPoint { x, y },
            size: PhysicalSize {
                width: w,
                height: h,
            },
        }
    }

    fn png_b64(w: u32, h: u32, rgba: Vec<u8>) -> String {
        let img = image::RgbaImage::from_raw(w, h, rgba).unwrap();
        let mut bytes = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn gradient_frame(w: u32, h: u32) -> (Frame, Vec<u8>) {
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&[x as u8, y as u8, (x ^ y) as u8, 255]);
            }
        }
        (Frame::from_rgba(w, h, &rgba).unwrap(), rgba)
    }

    /// CAP-1.AC2: the daemon is asked for the physical rect relative to the display, as
    /// `[x, y, w, h]`.
    #[test]
    fn cap1_ac2_region_params_are_physical_xywh() {
        let p = region_params("frame_1", rect(2207, 669, 420, 649));
        assert_eq!(
            p,
            json!({"target": {"frame_id": "frame_1", "rect": [2207, 669, 420, 649]}})
        );
    }

    #[test]
    fn freeze_and_window_params() {
        // The freeze makes no bundle: it is `freeze_display`, not `capture_screenshot`.
        assert_eq!(
            freeze_params(1),
            json!({"display_id": 1, "transport": "file"})
        );
        assert_eq!(
            frame_display_params("f"),
            json!({"target": {"frame_id": "f"}})
        );
        assert_eq!(window_params(9), json!({"target": {"window_id": 9}}));
    }

    /// CAP-1.AC2: a returned PNG equal to the frozen crop is pixel-identical; one differing
    /// pixel (or size) is not.
    #[test]
    fn cap1_ac2_pixel_identity_check() {
        let (frame, rgba) = gradient_frame(64, 48);
        let r = rect(10, 5, 20, 12);
        let crop = frame.crop_rgba(r).unwrap();
        assert!(png_matches_crop(&frame, r, &png_b64(20, 12, crop.clone())));
        let mut off = crop.clone();
        off[0] ^= 1;
        assert!(!png_matches_crop(&frame, r, &png_b64(20, 12, off)));
        let wrong_size = frame.crop_rgba(rect(10, 5, 21, 12)).unwrap();
        assert!(!png_matches_crop(&frame, r, &png_b64(21, 12, wrong_size)));
        assert!(!png_matches_crop(&frame, r, "not base64!"));
        // The frame itself decodes to the original bytes.
        let (w, h, back) = decode_png_rgba(&png_b64(64, 48, rgba.clone())).unwrap();
        assert_eq!((w, h, back), (64, 48, rgba));
    }

    /// `--pick` prints the target and nothing else (no capture id, nothing saved).
    #[test]
    fn pick_json_names_the_target() {
        let c = |outcome| Choice {
            display_id: 2,
            outcome,
        };
        assert_eq!(
            pick_json(Mode::Display, c(Outcome::Display)),
            json!({"picked": {"mode": "display", "display_id": 2}})
        );
        assert_eq!(
            pick_json(Mode::Window, c(Outcome::Window { window_id: 7 })),
            json!({"picked": {"mode": "window", "display_id": 2, "window_id": 7}})
        );
        assert_eq!(
            pick_json(
                Mode::Region,
                c(Outcome::Region {
                    rect: rect(10, 20, 300, 200)
                })
            ),
            json!({"picked": {"mode": "region", "display_id": 2, "rect": [10, 20, 300, 200]}})
        );
    }

    #[test]
    fn summary_has_the_capture_id_and_no_image() {
        let r = json!({"capture_id": "abc", "path": "/x/original.png", "kind": "region",
            "width": 420, "height": 649, "warnings": [], "image": {"data": "AAAA"}});
        let s = summarize(
            Mode::Region,
            1,
            &r,
            Some(rect(2207, 669, 420, 649)),
            Some(true),
        );
        assert_eq!(s["capture_id"], "abc");
        assert_eq!(s["rect"], json!([2207, 669, 420, 649]));
        assert_eq!(s["pixel_identical"], true);
        assert!(s.get("image").is_none());
    }
}
