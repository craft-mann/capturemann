//! Offline diagnosis of one refused pair of frames (see `docs/development/long-shot-debug.md`):
//! for every shift, how many of the rows that overlap are not equal, over the columns between the
//! trims. Prints the best shifts, so that a pair that "does not line up" shows whether a shift
//! exists that nearly does and which rows spoil it.
//!
//! ```text
//! long_shot_pair <last.png> <next.png> [header rows] [footer rows] [left trim] [right trim]
//! ```

use cm_capture::Frame;

fn load(path: &str) -> Frame {
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
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (a, b) = (load(&args[0]), load(&args[1]));
    let num = |i: usize, d: usize| args.get(i).and_then(|v| v.parse().ok()).unwrap_or(d);
    let (header, footer, left, right) = (num(2, 0), num(3, 0), num(4, 0), num(5, 0));
    let (w, h) = (a.width() as usize, a.height() as usize);
    let cols = left * 4..(w - right) * 4;
    let row = |f: &Frame, y: usize| f.rgba()[y * w * 4..(y + 1) * w * 4][cols.clone()].to_vec();
    let rows_a: Vec<_> = (0..h).map(|y| row(&a, y)).collect();
    let rows_b: Vec<_> = (0..h).map(|y| row(&b, y)).collect();
    let tol: u8 = std::env::var("LS_TOL").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let same = |x: &[u8], y: &[u8]| x.iter().zip(y).all(|(p, q)| p.abs_diff(*q) <= tol);
    let flat = |r: &[u8]| r.chunks(4).all(|p| p == &r[..4]);
    let cw = h - header - footer;
    let mut res: Vec<(usize, usize, usize)> = Vec::new();
    for s in 1..cw.saturating_sub(8) {
        let overlap = cw - s;
        let (mut bad, mut ink_bad, mut ink_ok) = (0, 0, 0);
        for i in 0..overlap {
            let (ia, ib) = (header + s + i, header + i);
            let inked = !flat(&rows_a[ia]) || !flat(&rows_b[ib]);
            if same(&rows_a[ia], &rows_b[ib]) {
                if inked {
                    ink_ok += 1;
                }
            } else {
                bad += 1;
                if inked {
                    ink_bad += 1;
                }
            }
        }
        res.push((s, bad * 1000 / overlap.max(1), ink_ok * 1000 / (ink_ok + ink_bad).max(1)));
    }
    res.sort_by_key(|r| r.1);
    println!("best shifts (shift, differing rows per mille of the overlap, inked rows that match per mille):");
    for r in res.iter().take(8) {
        println!("  {r:?}");
    }
    if let Some(&(s, _, _)) = res.first() {
        let overlap = cw - s;
        let bad: Vec<usize> = (0..overlap)
            .filter(|&i| !same(&rows_a[header + s + i], &rows_b[header + i]))
            .map(|i| header + i)
            .collect();
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for &y in &bad {
            match runs.last_mut() {
                Some(r) if r.1 + 1 == y => r.1 = y,
                _ => runs.push((y, y)),
            }
        }
        println!(
            "at shift {s}: rows of the next frame that differ, as runs: {:?}",
            &runs[..runs.len().min(30)]
        );
    }
}
