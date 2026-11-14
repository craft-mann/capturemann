//! Offline check of the stitcher on frames written by `long_shot_probe --dump <dir>`: pushes
//! every PNG of the directory (sorted by name) and prints what each push did, then writes the
//! stitched image. No screen, no input.
//!
//! ```text
//! long_shot_stitch <frames dir> <out.png>
//! ```

use cm_capture::long_shot::{Direction, Limits, Stitcher};
use cm_capture::Frame;

fn load(path: &std::path::Path) -> Frame {
    let file = std::fs::File::open(path).expect("open frame");
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().expect("png header");
    let mut buf = vec![0; reader.output_buffer_size().expect("png size")];
    let info = reader.next_frame(&mut buf).expect("png data");
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => panic!("unsupported colour type {other:?}"),
    };
    Frame::new(info.width, info.height, rgba).expect("frame")
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("frames dir");
    let out = args.next().expect("out png");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "png"))
        .collect();
    files.sort();
    let mut stitcher = Stitcher::new(Direction::Vertical, Limits::default());
    for f in &files {
        let frame = load(f);
        let outcome = stitcher.push(&frame, None);
        println!(
            "{} {}x{} -> {outcome:?} (len {}, bands {:?})",
            f.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
            frame.width(),
            frame.height(),
            stitcher.len_px(),
            stitcher.bands()
        );
    }
    if files.len() >= 2 && std::env::var_os("LS_ANALYZE").is_some() {
        analyze(
            &load(&files[files.len() - 2]),
            &load(&files[files.len() - 1]),
        );
    }
    let image = stitcher.finish().expect("stitched");
    std::fs::write(&out, image.frame.to_png().expect("png")).expect("write");
    println!(
        "{} frames, image {}x{}",
        image.frames,
        image.frame.width(),
        image.frame.height()
    );
}

/// Diagnosis: the shift with the fewest differing rows, and which columns differ there.
fn analyze(a: &Frame, b: &Frame) {
    let (w, h) = (a.width() as usize, a.height() as usize);
    let (top, bot) = (125usize, h - 12);
    let row = |f: &Frame, y: usize| f.rgba()[y * w * 4..(y + 1) * w * 4].to_vec();
    let mut best = (usize::MAX, 0usize);
    for s in 100..(bot - top - 20) {
        let mut bad = 0;
        for y in top..(bot - s) {
            if row(a, y + s) != row(b, y) {
                bad += 1;
            }
        }
        if bad < best.0 {
            best = (bad, s);
        }
    }
    if let Some(forced) = std::env::var("LS_SHIFT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        let mut bad = Vec::new();
        for y in top..(bot - forced) {
            if row(a, y + forced) != row(b, y) {
                bad.push(y);
            }
        }
        for x in [3usize, 6, 7, 8, 40, 300, 900] {
            let y = 200usize;
            println!(
                "x {x}: f001 {:?}  f005 {:?}",
                a.pixel(x as u32, (y + forced) as u32),
                b.pixel(x as u32, y as u32)
            );
        }
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for &y in &bad {
            match runs.last_mut() {
                Some(r) if r.1 + 1 == y => r.1 = y,
                _ => runs.push((y, y)),
            }
        }
        println!("runs of differing rows: {:?}", &runs[..runs.len().min(30)]);
        let mut xs = std::collections::BTreeSet::new();
        for &y in bad.iter().take(60) {
            let (ra, rb) = (row(a, y + forced), row(b, y));
            for x in 0..w {
                if ra[x * 4..x * 4 + 4] != rb[x * 4..x * 4 + 4] {
                    xs.insert(x);
                }
            }
        }
        let xs: Vec<_> = xs.into_iter().collect();
        println!(
            "columns differing in the first 60 bad rows: {} {:?}",
            xs.len(),
            &xs[..xs.len().min(40)]
        );
        println!(
            "shift {forced}: {} differing rows, first {:?}",
            bad.len(),
            &bad[..bad.len().min(20)]
        );
    }
    let head: Vec<usize> = (0..top).filter(|&y| row(a, y) != row(b, y)).collect();
    let foot: Vec<usize> = (bot..h).filter(|&y| row(a, y) != row(b, y)).collect();
    println!("header rows differing: {head:?}; footer rows differing: {foot:?}");
    for y in (120..125).chain(1085..1089) {
        let s = 774;
        let (ra, rb) = (row(a, y + s), row(b, y));
        let cols: Vec<usize> = (0..w)
            .filter(|&x| ra[x * 4..x * 4 + 4] != rb[x * 4..x * 4 + 4])
            .collect();
        println!(
            "corner row {y} at shift {s}: {} differing columns, first {:?}",
            cols.len(),
            &cols[..cols.len().min(8)]
        );
    }
    for s in 774..=774 {
        let bad = (top..(bot - s))
            .filter(|&y| row(a, y + s) != row(b, y))
            .count();
        // Differing only left of the scrollbar and right of the left border.
        let bad_core = (top..(bot - s))
            .filter(|&y| row(a, y + s)[40 * 4..1590 * 4] != row(b, y)[40 * 4..1590 * 4])
            .count();
        println!("shift {s}: {bad} differing rows, {bad_core} in the core columns 12..1600");
    }
    println!(
        "best shift {} with {} differing rows of {}",
        best.1,
        best.0,
        bot - top - best.1
    );
    let mut cols = std::collections::BTreeSet::new();
    for y in top..(bot - best.1) {
        let (ra, rb) = (row(a, y + best.1), row(b, y));
        for x in 0..w {
            if ra[x * 4..x * 4 + 4] != rb[x * 4..x * 4 + 4] {
                cols.insert(x);
            }
        }
    }
    let v: Vec<_> = cols.iter().copied().collect();
    println!(
        "{} differing columns; first {:?} last {:?}",
        v.len(),
        &v[..v.len().min(12)],
        &v[v.len().saturating_sub(12)..]
    );
}
