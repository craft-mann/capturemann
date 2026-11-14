//! Boards (BRD-1): a board is an ordinary screenshot capture (`kind: "screenshot"`) whose
//! `original.png` is the finished picture, so a 0.3.3 app shows it as a picture. The manifest
//! adds `source: "board"` and `board { layout, options, sources[] }`; the sources are the
//! flattened copies that went into the picture (their ids and titles only: the pixels are in
//! `original.png`, so deleting a source breaks nothing, BRD-1.AC3).

use std::path::{Path, PathBuf};

use cm_core::CmError;
use serde_json::{json, Value};
use time::OffsetDateTime;

use crate::bundle::{ShotBundle, MANIFEST_FILE};
use crate::util::{io_err, json_err, new_id, write_atomic};

/// Longest edge of a board's poster in physical px. Boards are wide or tall (a strip is 1641 x 364),
/// so the 512 px default left a few dozen pixels of height that the Library card then stretched.
pub const BOARD_POSTER_EDGE: u32 = 1280;

/// A fresh capture id for a board (`shot_`, like any screenshot).
pub fn mint_id(now: OffsetDateTime) -> String {
    new_id("shot", now)
}

/// Builds the `.cmshot` of a board inside `stage` and returns its folder and a thumbnail warning.
/// `board` is the JSON written under `manifest.board` (layout, options, sources). The folder is
/// complete, ready for [`crate::import::install_as`].
pub fn build_board_bundle(
    stage: &Path,
    id: &str,
    title: &str,
    png: &[u8],
    board: &Value,
    now: OffsetDateTime,
) -> Result<(PathBuf, Option<String>), CmError> {
    let bundle = ShotBundle::create(stage, title, png, now)?;
    let dir = bundle.dir().to_path_buf();
    let thumb_warning = crate::thumbs::write_poster(&bundle, BOARD_POSTER_EDGE)
        .err()
        .map(|e| e.to_string());
    let mpath = dir.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&mpath).map_err(|e| io_err("cannot read", &mpath, &e))?;
    let mut m: Value = serde_json::from_str(&text).map_err(|e| json_err("manifest.json", &e))?;
    m["id"] = json!(id);
    m["source"] = json!("board");
    m["board"] = board.clone();
    let out = serde_json::to_string_pretty(&m).map_err(|e| json_err("manifest.json", &e))?;
    write_atomic(&mpath, out.as_bytes())?;
    Ok((dir, thumb_warning))
}

/// The layout name of a board bundle (`pair`, `grid`, `strip`, `free`), for the Library badge.
/// `None` for any other capture. Reads the manifest only.
pub fn board_layout(bundle_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(bundle_dir.join(MANIFEST_FILE)).ok()?;
    let m: Value = serde_json::from_str(&text).ok()?;
    if m.get("source").and_then(Value::as_str) != Some("board") {
        return None;
    }
    m["board"]["layout"]["kind"].as_str().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(8, 6, image::Rgba([1, 2, 3, 255]));
        let mut v = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut v), image::ImageFormat::Png)
            .unwrap();
        v
    }

    #[test]
    fn a_board_bundle_is_a_screenshot_with_board_data() {
        let tmp = tempfile::tempdir().unwrap();
        let now = OffsetDateTime::now_utc();
        let board = json!({"layout": {"kind": "pair"}, "sources": [{"capture_id": "a"}]});
        let (dir, _) =
            build_board_bundle(tmp.path(), "shot_X", "My board", &png(), &board, now).unwrap();
        assert_eq!(board_layout(&dir).as_deref(), Some("pair"));
        // Opens as an ordinary screenshot with no ops, and keeps the board data on save.
        let mut b = ShotBundle::open(&dir).unwrap();
        assert_eq!(b.manifest().id, "shot_X");
        assert!(b.ops().is_empty());
        b.save().unwrap();
        assert_eq!(board_layout(&dir).as_deref(), Some("pair"));
    }
}
