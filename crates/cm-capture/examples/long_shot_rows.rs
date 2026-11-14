//! The row check of CAP-6.AC1 on a stitched image of `tests/fixtures/long-shot/long-page.html`:
//! every row has a blue bar whose width follows `40 + (i * 37) % 260` CSS px, so a duplicated or
//! missing row breaks the sequence. Prints one JSON line and exits 1 when the sequence is wrong.
//!
//! ```text
//! long_shot_rows <stitched.png>
//! ```

fn main() {
    let path = std::env::args().nth(1).expect("png path");
    let file = std::fs::File::open(&path).expect("open");
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().expect("png header");
    let mut buf = vec![0; reader.output_buffer_size().expect("png size")];
    let info = reader.next_frame(&mut buf).expect("png data");
    let (w, h) = (info.width as usize, info.height as usize);
    let bpp = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => panic!("unsupported colour type {other:?}"),
    };
    // A bar pixel is blue: b high, r low.
    let is_bar = |x: usize, y: usize| {
        let p = &buf[(y * w + x) * bpp..];
        p[2] > 170 && p[0] < 110
    };
    // The longest run of bar pixels in line `y` right of the number (text has blue fringes).
    let run_at = |y: usize| {
        let mut x = 150;
        let mut best = (0usize, 0usize);
        while x < w {
            if is_bar(x, y) {
                let start = x;
                while x < w && is_bar(x, y) {
                    x += 1;
                }
                if x - start > best.1 {
                    best = (start, x - start);
                }
            } else {
                x += 1;
            }
        }
        best
    };
    // The browser's own bars (profile button, icons) are above the page: start under them.
    let (y_start, y_end) = (125, h.saturating_sub(10));
    let mut bars: Vec<(usize, usize, usize)> = Vec::new(); // (top y, left x, widest run)
    let mut y = y_start;
    while y < y_end {
        let (x, len) = run_at(y);
        if len >= 30 {
            let top = y;
            let (mut left, mut widest) = (x, len);
            while y < y_end && run_at(y).1 >= 30 {
                let (x, len) = run_at(y);
                if len > widest {
                    widest = len;
                    left = x;
                }
                y += 1;
            }
            bars.push((top, left, widest));
        } else {
            y += 1;
        }
    }
    // Scale from the first bar: width(1) = 77 CSS px.
    let scale = bars
        .first()
        .map_or(1.0, |&(_, _, bw)| bw as f64 / 77.0)
        .max(0.1);
    let mut bad = Vec::new();
    for (i, &(top, _, bw)) in bars.iter().enumerate() {
        let n = i + 1;
        let want = (40 + (n * 37) % 260) as f64 * scale;
        if (bw as f64 - want).abs() > 3.0 {
            bad.push(serde_json::json!({"row": n, "y": top, "width": bw, "want": want.round()}));
        }
    }
    // Rows sit on a regular grid: the distance between bars must be constant.
    let pitch: Vec<usize> = bars.windows(2).map(|p| p[1].0 - p[0].0).collect();
    let (pmin, pmax) = (
        pitch.iter().copied().min().unwrap_or(0),
        pitch.iter().copied().max().unwrap_or(0),
    );
    let ok = bad.is_empty() && pmax - pmin <= 1 && !bars.is_empty();
    println!(
        "{}",
        serde_json::json!({
            "image_px": [w, h], "rows_found": bars.len(), "scale": scale,
            "pitch_min": pmin, "pitch_max": pmax, "wrong_rows": bad.len(),
            "first_wrong": bad.first(), "ok": ok,
        })
    );
    if !ok {
        std::process::exit(1);
    }
}
