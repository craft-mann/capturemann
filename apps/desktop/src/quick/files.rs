//! The file side of a preset run (QC-4.AC7): the folder `Save to` names, and the picture that is
//! written there. Pure functions; the run itself is in `ui::quick_flow`.

use std::path::{Path, PathBuf};

use cm_core::preset::{expand_save_to, PresetImageFormat};

/// The folder of a preset's `Save to`: `{date}` becomes the local date (`2026-11-18`) and a
/// leading `~` the home folder. A relative path is taken as relative to the home folder, so a
/// run never writes into whatever folder the app happened to start in.
pub fn resolve_dir(template: &str, date: time::Date, home: Option<&Path>) -> PathBuf {
    let expanded = expand_save_to(template.trim(), date);
    let path = Path::new(&expanded);
    if let Some(rest) = expanded.strip_prefix('~') {
        let rest = rest.trim_start_matches(['/', '\\']);
        return match home {
            Some(h) if rest.is_empty() => h.to_path_buf(),
            Some(h) => h.join(rest),
            None => PathBuf::from(rest),
        };
    }
    if path.is_absolute() {
        return path.to_path_buf();
    }
    match home {
        Some(h) => h.join(path),
        None => path.to_path_buf(),
    }
}

/// The picture a preset writes: the PNG of the capture in the preset's format and scale. Scale 2
/// keeps the capture's own pixels; scale 1 writes half the width and height. Returns the bytes
/// and the file extension.
pub fn render_copy(
    png: &[u8],
    format: Option<PresetImageFormat>,
    scale: Option<u8>,
) -> Result<(Vec<u8>, &'static str), String> {
    let half = scale == Some(1);
    let jpeg = format == Some(PresetImageFormat::Jpeg);
    if !half && !jpeg {
        return Ok((png.to_vec(), "png"));
    }
    let mut img = image::load_from_memory(png).map_err(|e| e.to_string())?;
    if half {
        let (w, h) = (
            img.width().div_ceil(2).max(1),
            img.height().div_ceil(2).max(1),
        );
        img = img.resize_exact(w, h, image::imageops::FilterType::Lanczos3);
    }
    let mut out = Vec::new();
    if jpeg {
        let rgb = img.to_rgb8();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92)
            .encode_image(&rgb)
            .map_err(|e| e.to_string())?;
        Ok((out, "jpg"))
    } else {
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok((out, "png"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date() -> time::Date {
        time::Date::from_calendar_date(2026, time::Month::November, 18).unwrap()
    }

    /// QC-4.AC7: `~/Bugs/{date}` is the home folder, `Bugs`, and the local date.
    #[test]
    fn qc4_ac7_the_folder_has_the_local_date() {
        let home = Path::new("/home/ann");
        assert_eq!(
            resolve_dir("~/Bugs/{date}", date(), Some(home)),
            Path::new("/home/ann/Bugs/2026-11-18")
        );
        assert_eq!(
            resolve_dir("/srv/shots/{date}/a", date(), Some(home)),
            Path::new("/srv/shots/2026-11-18/a")
        );
        assert_eq!(
            resolve_dir("Shots", date(), Some(home)),
            Path::new("/home/ann/Shots")
        );
        assert_eq!(resolve_dir("~", date(), Some(home)), home);
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 20, 30, 255]));
        let mut out = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn the_copy_follows_the_format_and_scale() {
        let p = png(40, 20);
        let (same, ext) = render_copy(&p, Some(PresetImageFormat::Png), Some(2)).unwrap();
        assert_eq!((same, ext), (p.clone(), "png"));
        let (half, ext) = render_copy(&p, None, Some(1)).unwrap();
        assert_eq!(ext, "png");
        let img = image::load_from_memory(&half).unwrap();
        assert_eq!((img.width(), img.height()), (20, 10));
        let (jpg, ext) = render_copy(&p, Some(PresetImageFormat::Jpeg), None).unwrap();
        assert_eq!(ext, "jpg");
        assert_eq!(&jpg[..2], &[0xFF, 0xD8]);
    }
}
