//! The text rules for how Windows windows and displays are named in the picker. Pure functions, so they are
//! tested on every platform; the calls that read the names from Windows are in `win_window.rs` and
//! `win_display.rs`.

/// Whether a name is a file path or an executable name and not something to show a person
/// ("C:\Program Files\WindowsApps\...\Notepad.exe").
pub fn looks_like_path(name: &str) -> bool {
    let n = name.trim();
    n.contains('\\')
        || n.contains('/')
        || n.as_bytes().get(1) == Some(&b':')
        || n.to_ascii_lowercase().ends_with(".exe")
}

/// The file name of a path without its folder and `.exe`: `C:\x\Notepad.exe` gives `Notepad`.
pub fn exe_stem(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    if file.to_ascii_lowercase().ends_with(".exe") {
        file[..file.len() - 4].to_string()
    } else {
        file.to_string()
    }
}

/// Whether the program is the frame that Windows runs for every packaged (UWP) window. The window belongs to
/// this process; the app's own process owns a child window inside it.
pub fn is_frame_host(exe_path: &str) -> bool {
    exe_path
        .rsplit(['\\', '/'])
        .next()
        .is_some_and(|f| f.eq_ignore_ascii_case("ApplicationFrameHost.exe"))
}

/// Whether a description is a generic "... Host" name of a program that hosts another one (a console
/// host, a frame host): its last word is `Host` and it has at least one more word.
fn is_host_description(d: &str) -> bool {
    let mut words = d.split_whitespace().rev();
    words.next().is_some_and(|w| w.eq_ignore_ascii_case("host")) && words.next().is_some()
}

/// The name the picker shows for a window's app: the first of the program's own descriptions
/// (`FileDescription`, `ProductName`) that is text a person can read, else the program's file name, else
/// the window title. Never a path.
///
/// A program that only hosts the app often describes itself as the host ("Windows Terminal Host" for the
/// terminal's `OpenConsole.exe`) while its product name is the app's ("Windows Terminal"). When the first
/// description ends in "Host" and a later readable description differs from it, the later one is used.
pub fn choose_app_name(descriptions: &[&str], exe_path: &str, title: &str) -> String {
    let readable: Vec<&str> = descriptions
        .iter()
        .map(|d| d.trim())
        .filter(|d| !d.is_empty() && !looks_like_path(d))
        .collect();
    if let Some(first) = readable.first() {
        if is_host_description(first) {
            if let Some(other) = readable
                .iter()
                .skip(1)
                .find(|d| !d.eq_ignore_ascii_case(first) && !is_host_description(d))
            {
                return (*other).to_string();
            }
        }
        return (*first).to_string();
    }
    let stem = exe_stem(exe_path);
    if !stem.trim().is_empty() {
        return stem;
    }
    let t = title.trim();
    if looks_like_path(t) {
        String::new()
    } else {
        t.to_string()
    }
}

/// The name of a display: the monitor's own name when Windows has one, else "Display N" with N from the
/// device name (`\\.\DISPLAY2` is "Display 2"), else the display's place in the list.
pub fn display_label(friendly: Option<&str>, device: &str, index: usize) -> String {
    if let Some(f) = friendly.map(str::trim).filter(|f| !f.is_empty()) {
        return f.to_string();
    }
    let digits: String = device
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    match digits.parse::<u32>() {
        Ok(n) if n > 0 => format!("Display {n}"),
        _ => format!("Display {}", index + 1),
    }
}

/// A rectangle `(left, top, right, bottom)` as `(x, y, width, height)`; an empty or inverted one gives `None`.
pub fn rect_to_xywh(left: i32, top: i32, right: i32, bottom: i32) -> Option<(i32, i32, u32, u32)> {
    if right <= left || bottom <= top {
        return None;
    }
    Some((left, top, (right - left) as u32, (bottom - top) as u32))
}

/// Where the visible frame sits inside the window rectangle: `(x, y, width, height)` in the rectangle's own
/// pixels. A frame that is empty or not inside the rectangle gives the whole rectangle. Both rectangles are
/// `(left, top, right, bottom)` in the same (physical) pixels.
pub fn crop_box(outer: (i32, i32, i32, i32), frame: (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    let inside = frame.2 > frame.0
        && frame.3 > frame.1
        && frame.0 >= outer.0
        && frame.1 >= outer.1
        && frame.2 <= outer.2
        && frame.3 <= outer.3;
    if inside {
        (
            frame.0 - outer.0,
            frame.1 - outer.1,
            frame.2 - frame.0,
            frame.3 - frame.1,
        )
    } else {
        (0, 0, outer.2 - outer.0, outer.3 - outer.1)
    }
}

/// The size a DPI-unaware window paints itself at when `PrintWindow` is given a bitmap of its physical size
/// `(ow, oh)`: Windows virtualises such a window, so it draws at `96 / dpi` of that size into the top-left
/// corner and leaves the rest of the bitmap black. `dpi` is the monitor's effective DPI (96 = 100 %); 96 or
/// less gives the whole bitmap.
pub fn unaware_painted_size(ow: u32, oh: u32, dpi: u32) -> (u32, u32) {
    if dpi <= 96 {
        return (ow, oh);
    }
    let scale =
        |v: u32| ((v as u64 * 96 + dpi as u64 / 2) / dpi as u64).clamp(1, v.max(1) as u64) as u32;
    (scale(ow), scale(oh))
}

/// Scales the top-left `sw` x `sh` part of a 4-bytes-per-pixel picture (`stride_w` pixels per row) up to
/// `dw` x `dh` with bilinear sampling. `None` when a size is zero or the buffer is too short.
pub fn upscale_top_left(
    src: &[u8],
    stride_w: usize,
    sw: usize,
    sh: usize,
    dw: usize,
    dh: usize,
) -> Option<Vec<u8>> {
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 || sw > stride_w || src.len() < stride_w * sh * 4 {
        return None;
    }
    let mut out = vec![0u8; dw * dh * 4];
    // Pixel centres: destination centre (i + 0.5) maps to source (i + 0.5) * s / d - 0.5, in 1/256ths.
    let axis = |d: usize, s: usize| -> Vec<(usize, usize, u32)> {
        (0..d)
            .map(|i| {
                let pos = (((2 * i + 1) * s * 256) as i64 / (2 * d) as i64 - 128).max(0);
                let i0 = ((pos >> 8) as usize).min(s - 1);
                let i1 = (i0 + 1).min(s - 1);
                (i0, i1, (pos & 255) as u32)
            })
            .collect()
    };
    let (xs, ys) = (axis(dw, sw), axis(dh, sh));
    for (y, &(y0, y1, fy)) in ys.iter().enumerate() {
        for (x, &(x0, x1, fx)) in xs.iter().enumerate() {
            let at = |xx: usize, yy: usize, c: usize| src[(yy * stride_w + xx) * 4 + c] as u32;
            for c in 0..4 {
                let top = at(x0, y0, c) * (256 - fx) + at(x1, y0, c) * fx;
                let bot = at(x0, y1, c) * (256 - fx) + at(x1, y1, c) * fx;
                out[(y * dw + x) * 4 + c] = ((top * (256 - fy) + bot * fy + 32768) >> 16) as u8;
            }
        }
    }
    Some(out)
}

/// The rectangle a window screenshot covers, `(x, y, width, height)` in virtual-screen px: the visible frame
/// inside the window rectangle, or the whole rectangle (see [`crop_box`]). The picker lists this and `capture`
/// saves this, so the two cannot differ. `None` for an empty rectangle.
pub fn visible_xywh(
    outer: (i32, i32, i32, i32),
    frame: (i32, i32, i32, i32),
) -> Option<(i32, i32, u32, u32)> {
    let (x, y, w, h) = crop_box(outer, frame);
    rect_to_xywh(outer.0 + x, outer.1 + y, outer.0 + x + w, outer.1 + y + h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unaware_window_paints_at_the_unscaled_size() {
        // 150 % display: a 1426x721 physical window paints 951x481 in its corner.
        assert_eq!(unaware_painted_size(1426, 721, 144), (951, 481));
        assert_eq!(unaware_painted_size(1000, 500, 96), (1000, 500));
        assert_eq!(unaware_painted_size(1000, 500, 0), (1000, 500));
        assert_eq!(unaware_painted_size(1, 1, 288), (1, 1));
    }

    #[test]
    fn the_painted_corner_fills_the_whole_picture() {
        // A 2x2 corner of a 4x4 buffer (the rest black) scaled to 4x4 has no black left.
        let mut src = vec![0u8; 4 * 4 * 4];
        for y in 0..2 {
            for x in 0..2 {
                src[(y * 4 + x) * 4..][..4].copy_from_slice(&[200, 100, 50, 255]);
            }
        }
        let out = upscale_top_left(&src, 4, 2, 2, 4, 4).unwrap();
        assert_eq!(out.len(), 64);
        assert!(out.chunks(4).all(|p| p == [200, 100, 50, 255]));
        assert!(upscale_top_left(&src, 4, 0, 2, 4, 4).is_none());
        assert!(upscale_top_left(&src[..8], 4, 2, 2, 4, 4).is_none());
    }

    #[test]
    fn paths_and_executable_names_are_not_app_names() {
        assert!(looks_like_path(
            r"C:\Program Files\WindowsApps\x\Notepad.exe"
        ));
        assert!(looks_like_path("Notepad.exe"));
        assert!(looks_like_path("/usr/bin/x"));
        assert!(!looks_like_path("Windows Terminal"));
        assert!(!looks_like_path("Notepad"));
    }

    #[test]
    fn the_stem_is_the_file_name_without_exe() {
        assert_eq!(exe_stem(r"C:\x\Notepad.exe"), "Notepad");
        assert_eq!(exe_stem(r"C:\x\Notepad.EXE"), "Notepad");
        assert_eq!(exe_stem("calc"), "calc");
        assert_eq!(exe_stem(""), "");
    }

    #[test]
    fn the_frame_host_is_found_by_file_name() {
        assert!(is_frame_host(
            r"C:\Windows\System32\ApplicationFrameHost.exe"
        ));
        assert!(is_frame_host(
            r"c:\windows\system32\applicationframehost.EXE"
        ));
        assert!(!is_frame_host(r"C:\Windows\notepad.exe"));
    }

    #[test]
    fn the_app_name_is_a_description_then_the_file_name_then_the_title() {
        let exe = r"C:\Program Files\WindowsApps\p\Notepad\Notepad.exe";
        assert_eq!(
            choose_app_name(&["Notepad", ""], exe, "a - Notepad"),
            "Notepad"
        );
        // A description that is a path is skipped.
        assert_eq!(
            choose_app_name(&[r"C:\Program Files\Wi", " "], exe, "t"),
            "Notepad"
        );
        // No description and no program: the title, unless it is a path.
        assert_eq!(choose_app_name(&[], "", " Settings "), "Settings");
        assert_eq!(choose_app_name(&[], "", r"C:\x\y.exe"), "");
    }

    #[test]
    fn a_host_description_gives_way_to_the_product_name() {
        let exe = r"C:\Program Files\WindowsApps\t\OpenConsole.exe";
        // FileDescription first, ProductName second, as the lookup passes them.
        assert_eq!(
            choose_app_name(&["Windows Terminal Host", "Windows Terminal"], exe, "x"),
            "Windows Terminal"
        );
        // Both are hosts, or the product name is the same: the first stays.
        assert_eq!(
            choose_app_name(&["Console Window Host", "Microsoft Windows Host"], exe, "x"),
            "Console Window Host"
        );
        assert_eq!(
            choose_app_name(
                &["Windows Terminal Host", "windows terminal host"],
                exe,
                "x"
            ),
            "Windows Terminal Host"
        );
        // A host with no other readable name keeps its own; a one-word "Host" is not a host suffix.
        assert_eq!(
            choose_app_name(&["Windows Terminal Host", r"C:\x.exe"], exe, "x"),
            "Windows Terminal Host"
        );
        assert_eq!(choose_app_name(&["Host", "Other"], exe, "x"), "Host");
        // A description that does not end in Host is not touched.
        assert_eq!(
            choose_app_name(&["Notepad", "Microsoft Notepad"], exe, "x"),
            "Notepad"
        );
    }

    #[test]
    fn a_display_without_a_name_is_called_display_n() {
        assert_eq!(
            display_label(Some("DELL U2720Q"), r"\\.\DISPLAY1", 0),
            "DELL U2720Q"
        );
        assert_eq!(display_label(None, r"\\.\DISPLAY2", 0), "Display 2");
        assert_eq!(display_label(Some("  "), r"\\.\DISPLAY12", 0), "Display 12");
        assert_eq!(display_label(None, "", 2), "Display 3");
        assert_eq!(display_label(None, r"\\.\DISPLAY0", 1), "Display 2");
    }

    #[test]
    fn a_rectangle_gives_the_same_size_everywhere() {
        // The size is right minus left: the listing and the saved picture both use this, at 150 % scale too.
        assert_eq!(
            rect_to_xywh(111, 104, 1537, 824),
            Some((111, 104, 1426, 720))
        );
        assert_eq!(rect_to_xywh(0, 0, 0, 10), None);
        assert_eq!(rect_to_xywh(5, 5, 4, 9), None);
    }

    #[test]
    fn the_crop_is_the_visible_frame_inside_the_window_rectangle() {
        // The runner's Notepad: rectangle (104,104)-(872,623), frame (111,104)-(865,616).
        assert_eq!(
            crop_box((104, 104, 872, 623), (111, 104, 865, 616)),
            (7, 0, 754, 512)
        );
    }

    #[test]
    fn a_frame_that_does_not_fit_gives_the_whole_rectangle() {
        assert_eq!(crop_box((10, 10, 110, 60), (0, 0, 0, 0)), (0, 0, 100, 50));
        assert_eq!(
            crop_box((10, 10, 110, 60), (5, 10, 110, 60)),
            (0, 0, 100, 50)
        );
    }

    /// Mixed-DPI layouts (virtual-screen physical px): the listed rectangle is the rectangle `capture` cuts, also
    /// where the frame pokes 1 px out of the window rectangle (the old listing then said 1426x720 or 1427x721
    /// and the saved picture was the whole rectangle) and on monitors at negative coordinates.
    #[test]
    fn the_listed_rectangle_is_the_saved_rectangle_on_every_layout() {
        // (outer, frame, listed)
        let cases = [
            // 100 % laptop panel at 0,0 next to a 150 % monitor at 1920,0: a window on the second monitor.
            (
                (2000, 100, 3500, 900),
                (2011, 100, 3489, 892),
                (2011, 100, 1478, 792),
            ),
            // 200 % primary, a 100 % monitor to the LEFT at -1920,0: negative coordinates.
            (
                (-1900, 50, -900, 650),
                (-1893, 50, -907, 643),
                (-1893, 50, 986, 593),
            ),
            // A monitor ABOVE the primary (y < 0).
            (
                (100, -1000, 900, -400),
                (107, -1000, 893, -407),
                (107, -1000, 786, 593),
            ),
            // The frame pokes 1 px outside the rectangle (rounding at 150 %): the whole rectangle, as saved.
            (
                (1920, 0, 2720, 600),
                (1919, 0, 2720, 600),
                (1920, 0, 800, 600),
            ),
            // A window straddling two monitors: the same rule, nothing clipped to a monitor.
            (
                (1800, 100, 2200, 500),
                (1807, 100, 2193, 493),
                (1807, 100, 386, 393),
            ),
            // No frame reported.
            ((10, 10, 110, 60), (0, 0, 0, 0), (10, 10, 100, 50)),
        ];
        for (outer, frame, want) in cases {
            assert_eq!(
                visible_xywh(outer, frame),
                Some(want),
                "{outer:?} {frame:?}"
            );
        }
        assert_eq!(visible_xywh((5, 5, 5, 9), (0, 0, 0, 0)), None);
    }
}
