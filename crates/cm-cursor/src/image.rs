//! Cursor images: content-addressed ids, PNG encoding and the `cursors/index.json` index.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use cm_core::CmError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::schema::INDEX_FILE;

/// A cursor image as the OS reports it: straight (non-premultiplied) RGBA8, top-left origin.
#[derive(Debug, Clone, PartialEq)]
pub struct CursorImage {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes, RGBA, not premultiplied.
    pub rgba: Vec<u8>,
    /// Hotspot in px of this image.
    pub hotspot: [i32; 2],
    /// Image px per logical point (2.0 for a Retina cursor).
    pub scale: f64,
}

impl CursorImage {
    /// Stable id: `c_` + 12 hex digits of SHA-256 over a domain tag, the dimensions, the scale
    /// (rounded to 1/1000), the hotspot and the pixel bytes. Identical across runs and machines.
    pub fn id(&self) -> String {
        let mut h = Sha256::new();
        h.update(b"cm-cursor-image-v1");
        h.update(self.width.to_le_bytes());
        h.update(self.height.to_le_bytes());
        let milli = (self.scale * 1000.0)
            .round()
            .clamp(0.0, f64::from(u32::MAX)) as u32;
        h.update(milli.to_le_bytes());
        h.update(self.hotspot[0].to_le_bytes());
        h.update(self.hotspot[1].to_le_bytes());
        h.update(&self.rgba);
        let digest = h.finalize();
        let mut id = String::from("c_");
        for b in &digest[..6] {
            id.push_str(&format!("{b:02x}"));
        }
        id
    }

    /// Whether the buffer length matches the dimensions.
    pub fn is_valid(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.rgba.len() as u64 == u64::from(self.width) * u64::from(self.height) * 4
    }

    /// Encodes as an RGBA8 PNG (deterministic for equal pixels).
    pub fn to_png(&self) -> Result<Vec<u8>, CmError> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(Cursor::new(&mut out), self.width, self.height);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc
                .write_header()
                .map_err(|e| CmError::internal(format!("png header: {e}")))?;
            w.write_image_data(&self.rgba)
                .map_err(|e| CmError::internal(format!("png data: {e}")))?;
        }
        Ok(out)
    }

    /// Decodes any PNG into straight RGBA8 (used by platform extractors that get PNG from AppKit).
    pub fn from_png(png_bytes: &[u8], hotspot: [i32; 2], scale: f64) -> Result<Self, CmError> {
        let mut dec = png::Decoder::new(Cursor::new(png_bytes));
        dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = dec
            .read_info()
            .map_err(|e| CmError::internal(format!("png decode: {e}")))?;
        let mut buf = vec![0u8; reader.output_buffer_size().unwrap_or(0)];
        let info = reader
            .next_frame(&mut buf)
            .map_err(|e| CmError::internal(format!("png decode: {e}")))?;
        let px = &buf[..info.buffer_size()];
        let rgba: Vec<u8> = match info.color_type {
            png::ColorType::Rgba => px.to_vec(),
            png::ColorType::Rgb => px
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|c| [c[0], c[1], c[2], 255])
                .collect(),
            png::ColorType::GrayscaleAlpha => px
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|c| [c[0], c[0], c[0], c[1]])
                .collect(),
            png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            png::ColorType::Indexed => {
                return Err(CmError::internal("png decode: unexpanded palette"))
            }
        };
        Ok(CursorImage {
            width: info.width,
            height: info.height,
            rgba,
            hotspot,
            scale,
        })
    }
}

/// One entry of `cursors/index.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageEntry {
    /// File name inside `cursors/` (`<id>.png`).
    pub file: String,
    /// PNG size in px.
    pub size: [u32; 2],
    pub hotspot: [i32; 2],
    /// PNG px per logical point.
    pub scale: f64,
}

/// `cursors/index.json`: id to [`ImageEntry`]. Small; rewritten atomically when an image is added.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ImageIndex {
    pub images: BTreeMap<String, ImageEntry>,
}

impl ImageIndex {
    /// Reads `<bundle>/cursors/index.json`.
    pub fn read(cursors_dir: &Path) -> Result<Self, CmError> {
        let p = cursors_dir.join(INDEX_FILE);
        let bytes = fs::read(&p)
            .map_err(|e| CmError::not_found(format!("cursor image index {}: {e}", p.display())))?;
        serde_json::from_slice(&bytes)
            .map_err(|e| CmError::invalid_input(format!("cursor image index: {e}")))
    }

    pub fn get(&self, id: &str) -> Option<&ImageEntry> {
        self.images.get(id)
    }
}

/// Writes images into `cursors/`, deduplicated by id, and keeps the index current.
#[derive(Debug)]
pub(crate) struct ImageStore {
    dir: PathBuf,
    index: ImageIndex,
}

impl ImageStore {
    pub(crate) fn new(dir: PathBuf) -> Self {
        ImageStore {
            dir,
            index: ImageIndex::default(),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.index.images.len()
    }

    /// Stores `img` if new and returns its id.
    pub(crate) fn add(&mut self, img: &CursorImage) -> Result<String, CmError> {
        let id = img.id();
        if self.index.images.contains_key(&id) {
            return Ok(id);
        }
        let file = format!("{id}.png");
        atomic_write(&self.dir.join(&file), &img.to_png()?)?;
        self.index.images.insert(
            id.clone(),
            ImageEntry {
                file,
                size: [img.width, img.height],
                hotspot: img.hotspot,
                scale: img.scale,
            },
        );
        let json = serde_json::to_vec_pretty(&self.index)
            .map_err(|e| CmError::internal(format!("index: {e}")))?;
        atomic_write(&self.dir.join(INDEX_FILE), &json)?;
        Ok(id)
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), CmError> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)
        .and_then(|()| fs::rename(&tmp, path))
        .map_err(|e| crate::io_err(&e, path))
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::CursorImage;

    pub(crate) fn sample(seed: u8) -> CursorImage {
        let mut rgba = Vec::new();
        for i in 0..(4 * 6) {
            rgba.extend_from_slice(&[
                seed,
                i as u8,
                255 - i as u8,
                if i % 5 == 0 { 0 } else { 255 },
            ]);
        }
        CursorImage {
            width: 4,
            height: 6,
            rgba,
            hotspot: [1, 2],
            scale: 2.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::sample;
    use super::*;

    #[test]
    fn id_is_stable_across_runs() {
        // Locked value: if this changes, ids written by older versions no longer dedupe.
        assert_eq!(sample(7).id(), sample(7).id());
        assert_eq!(sample(7).id(), "c_dd2ed3cebdfd");
    }

    #[test]
    fn id_depends_on_pixels_hotspot_and_scale() {
        let a = sample(7);
        assert_ne!(a.id(), sample(8).id());
        let mut b = a.clone();
        b.hotspot = [2, 2];
        assert_ne!(a.id(), b.id());
        let mut c = a.clone();
        c.scale = 1.0;
        assert_ne!(a.id(), c.id());
        assert!(a.id().starts_with("c_") && a.id().len() == 14);
    }

    #[test]
    fn png_round_trips_pixels() {
        let a = sample(3);
        let png = a.to_png().unwrap();
        assert_eq!(png, a.to_png().unwrap(), "encoding is deterministic");
        let b = CursorImage::from_png(&png, a.hotspot, a.scale).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.id(), b.id());
    }

    #[test]
    fn store_dedupes_and_indexes() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = ImageStore::new(dir.path().to_path_buf());
        let a = s.add(&sample(1)).unwrap();
        let again = s.add(&sample(1)).unwrap();
        let b = s.add(&sample(2)).unwrap();
        assert_eq!(a, again);
        assert_ne!(a, b);
        assert_eq!(s.len(), 2);
        let idx = ImageIndex::read(dir.path()).unwrap();
        assert_eq!(idx.images.len(), 2);
        assert_eq!(idx.get(&a).unwrap().size, [4, 6]);
        assert!(dir.path().join(format!("{a}.png")).exists());
        assert!(!dir.path().join("index.tmp").exists());
    }
}
