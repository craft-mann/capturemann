//! Text recognition through the system `tesseract` program (Apache-2.0), used where the OS has
//! no recognizer of its own (Linux; also any other platform that has it installed).
//!
//! Tesseract runs as a **helper process**, like `ffmpeg` and `llama-server`: it is never linked
//! into CaptureMann, so its licence and its C++ dependencies stay outside the shipped binary.
//! The image goes in on stdin as a PNG and the words come back as TSV on stdout; nothing
//! touches the disk or the network. The language data (`eng.traineddata`) comes with the
//! distribution's package (`tesseract-ocr` on Debian and Ubuntu, `tesseract` on Fedora and
//! Arch), so the app downloads no model for it.
//!
//! General preparation, not tuned to any picture: grayscale, an up-scaling step because
//! tesseract reads text best at about 30 px per line and UI text is 9 to 14 px, and (when most of
//! the picture is dark) inversion to dark-on-light. Tesseract's own line numbers are ignored:
//! words are joined into lines by geometry in [`group_words`], so a phone number whose
//! groups tesseract split stays one line for the detector.

use super::{TextLine, TextRecognizer, TextWord};
use cm_core::{CmError, Rc};
use image::imageops::FilterType;
use image::{GrayImage, RgbaImage};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// `CM_TESSERACT`: a `tesseract` executable, or a folder holding it.
pub const ENV_TESSERACT: &str = "CM_TESSERACT";
/// `CM_TESSDATA`: a folder with the `.traineddata` files (passed as `--tessdata-dir`).
pub const ENV_TESSDATA: &str = "CM_TESSDATA";

/// How long one recognition may take before the helper is killed.
const TIMEOUT: Duration = Duration::from_secs(180);
/// Upper bound for the up-scaled picture (pixels), to bound memory on 5K screens.
const MAX_SCALED_PIXELS: u64 = 40_000_000;

/// How the picture is prepared and read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TesseractOptions {
    /// Up-scaling factor (1 = none). Reduced automatically to stay under the pixel bound.
    pub scale: u32,
    /// Tesseract page segmentation mode (11 = sparse text, 3 = automatic, 6 = one block).
    pub psm: u32,
    /// Invert pictures whose mean brightness is below this (0..=255); 0 never inverts.
    pub invert_below: u8,
}

impl Default for TesseractOptions {
    fn default() -> Self {
        TesseractOptions {
            scale: 2,
            psm: 11,
            invert_below: 110,
        }
    }
}

impl TesseractOptions {
    /// The defaults, with `CM_TESSERACT_SCALE` (1 to 4) and `CM_TESSERACT_PSM` (1 to 13) applied
    /// when set. Meant for measuring on the benchmark, not for everyday use.
    pub fn from_env() -> Self {
        let mut o = Self::default();
        let num = |k: &str| {
            std::env::var(k)
                .ok()
                .and_then(|v| v.trim().parse::<u32>().ok())
        };
        if let Some(v) = num("CM_TESSERACT_SCALE").filter(|v| (1..=4).contains(v)) {
            o.scale = v;
        }
        if let Some(v) = num("CM_TESSERACT_PSM").filter(|v| (1..=13).contains(v)) {
            o.psm = v;
        }
        o
    }
}

/// Where each lookup step looks; injectable for tests.
#[derive(Debug, Clone, Default)]
pub struct SearchPaths {
    pub env: Option<OsString>,
    pub exe_dir: Option<PathBuf>,
    pub path_var: Option<OsString>,
}

impl SearchPaths {
    pub fn from_env() -> SearchPaths {
        SearchPaths {
            env: std::env::var_os(ENV_TESSERACT).filter(|v| !v.is_empty()),
            exe_dir: std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(Path::to_path_buf)),
            path_var: std::env::var_os("PATH"),
        }
    }
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "tesseract.exe"
    } else {
        "tesseract"
    }
}

/// Finds `tesseract`: `CM_TESSERACT` (a file or a folder; a wrong value is an error, not
/// skipped), next to the executable, then `PATH`.
///
/// # Errors
/// `not_implemented` with what to install.
pub fn locate(paths: &SearchPaths) -> Result<PathBuf, CmError> {
    let name = exe_name();
    if let Some(v) = &paths.env {
        let p = PathBuf::from(v);
        let candidate = if p.is_dir() { p.join(name) } else { p };
        return if candidate.is_file() {
            Ok(candidate)
        } else {
            Err(CmError::not_implemented(format!(
                "{ENV_TESSERACT} points to `{}`, which is not a file; fix or unset it",
                candidate.display()
            )))
        };
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    dirs.extend(paths.exe_dir.clone());
    if let Some(p) = &paths.path_var {
        dirs.extend(std::env::split_paths(p));
    }
    dirs.iter()
        .map(|d| d.join(name))
        .find(|p| p.is_file())
        .ok_or_else(|| CmError::not_implemented(install_hint()))
}

/// What to do when the engine is missing (shown by the CLI, MCP and diagnostics).
pub fn install_hint() -> String {
    format!(
        "text recognition needs the `tesseract` program (Apache-2.0) with English data: install \
         it (Debian and Ubuntu: `sudo apt install tesseract-ocr`; Fedora: `sudo dnf install \
         tesseract`; Arch: `sudo pacman -S tesseract tesseract-data-eng`) or point {ENV_TESSERACT} \
         to the executable"
    )
}

/// The recognizer: finds `tesseract` when asked and remembers a working one.
#[derive(Debug, Default)]
pub struct TesseractRecognizer {
    options: TesseractOptions,
    paths: SearchPaths,
    use_env_paths: bool,
    ready: Mutex<Option<PathBuf>>,
}

impl TesseractRecognizer {
    /// Looks at `CM_TESSERACT`, the executable's folder and `PATH` when asked.
    pub fn system() -> Self {
        TesseractRecognizer {
            use_env_paths: true,
            options: TesseractOptions::from_env(),
            ..Self::default()
        }
    }

    /// Replaces the preparation options.
    pub fn with_options(mut self, options: TesseractOptions) -> Self {
        self.options = options;
        self
    }

    /// A recognizer that uses exactly these lookup paths (tests).
    pub fn with_paths(paths: SearchPaths) -> Self {
        TesseractRecognizer {
            paths,
            ..Self::default()
        }
    }

    fn search(&self) -> SearchPaths {
        if self.use_env_paths {
            SearchPaths::from_env()
        } else {
            self.paths.clone()
        }
    }

    /// The working executable, or why there is none. A hit is remembered; a miss is looked up
    /// again next time (the user may install the program while the app runs).
    ///
    /// # Errors
    /// `not_implemented` saying what is missing and how to get it.
    pub fn engine(&self) -> Result<PathBuf, CmError> {
        if let Some(p) = self.ready.lock().ok().and_then(|g| g.clone()) {
            if p.is_file() {
                return Ok(p);
            }
        }
        let exe = locate(&self.search())?;
        let langs = list_langs(&exe)?;
        if !langs.iter().any(|l| l == "eng") {
            return Err(CmError::not_implemented(format!(
                "`tesseract` at {} has no English data (`eng`); install it (Debian and Ubuntu: \
                 `sudo apt install tesseract-ocr-eng`) or set {ENV_TESSDATA} to a folder with \
                 eng.traineddata",
                exe.display()
            )));
        }
        if let Ok(mut g) = self.ready.lock() {
            *g = Some(exe.clone());
        }
        Ok(exe)
    }
}

fn tessdata_args(cmd: &mut Command) {
    if let Some(d) = std::env::var_os(ENV_TESSDATA).filter(|v| !v.is_empty()) {
        cmd.arg("--tessdata-dir").arg(d);
    }
}

/// The installed language codes (`tesseract --list-langs`).
fn list_langs(exe: &Path) -> Result<Vec<String>, CmError> {
    let mut cmd = Command::new(exe);
    tessdata_args(&mut cmd);
    let out = cmd
        .arg("--list-langs")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| CmError::not_implemented(format!("cannot run `{}`: {e}", exe.display())))?;
    // Older versions print the list on stderr.
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.contains(' ') && !l.contains(':') && !l.contains('/'))
        .map(str::to_owned)
        .collect())
}

/// The version line for diagnostics, for example `tesseract 5.3.4`.
pub fn version(exe: &Path) -> Option<String> {
    let out = Command::new(exe)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    text.lines().next().map(|l| l.trim().to_owned())
}

impl TextRecognizer for TesseractRecognizer {
    fn name(&self) -> &'static str {
        "tesseract"
    }

    fn is_available(&self) -> bool {
        self.engine().is_ok()
    }

    fn install_hint(&self) -> Option<String> {
        self.engine().err().map(|e| e.to_string())
    }

    fn recognize(&self, image: &RgbaImage) -> Result<Vec<TextLine>, CmError> {
        let exe = self.engine()?;
        let (png, scale) = prepare(image, &self.options)?;
        let tsv = run(&exe, &png, self.options.psm)?;
        let words = parse_tsv(&tsv, f64::from(scale));
        Ok(group_words(words))
    }
}

/// Grayscale, up-scale and (for mostly dark pictures) invert; returns the PNG and the factor
/// actually used.
fn prepare(image: &RgbaImage, o: &TesseractOptions) -> Result<(Vec<u8>, u32), CmError> {
    use image::{DynamicImage, ExtendedColorType, ImageEncoder};
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return Err(CmError::invalid_input("the picture is empty"));
    }
    let mut gray: GrayImage = DynamicImage::ImageRgba8(image.clone()).into_luma8();
    let sum: u64 = gray.as_raw().iter().map(|&v| u64::from(v)).sum();
    let mean = sum / u64::from(w) / u64::from(h);
    if o.invert_below > 0 && mean < u64::from(o.invert_below) {
        image::imageops::invert(&mut gray);
    }
    let mut scale = o.scale.max(1);
    while scale > 1 && u64::from(w) * u64::from(h) * u64::from(scale * scale) > MAX_SCALED_PIXELS {
        scale -= 1;
    }
    if scale > 1 {
        gray = image::imageops::resize(&gray, w * scale, h * scale, FilterType::CatmullRom);
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            gray.as_raw(),
            gray.width(),
            gray.height(),
            ExtendedColorType::L8,
        )
        .map_err(|e| CmError::internal(format!("png encoding failed: {e}")))?;
    Ok((png, scale))
}

fn drain<R: Read + Send + 'static>(r: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut b = Vec::new();
        if let Some(mut r) = r {
            let _ = r.read_to_end(&mut b);
        }
        String::from_utf8_lossy(&b).into_owned()
    })
}

fn run(exe: &Path, png: &[u8], psm: u32) -> Result<String, CmError> {
    let mut cmd = Command::new(exe);
    tessdata_args(&mut cmd);
    cmd.args([
        "stdin",
        "stdout",
        "-l",
        "eng",
        "--psm",
        &psm.to_string(),
        "tsv",
    ])
    // One thread: tesseract's OpenMP threads slow it down on small pictures.
    .env("OMP_THREAD_LIMIT", "1")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| CmError::internal(format!("cannot run tesseract: {e}")))?;
    let stdin = child.stdin.take();
    let data = png.to_vec();
    let writer = std::thread::spawn(move || {
        if let Some(mut s) = stdin {
            let _ = s.write_all(&data);
        }
    });
    let out_t = drain(child.stdout.take());
    let err_t = drain(child.stderr.take());
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CmError::internal("tesseract took too long and was stopped"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            Err(e) => return Err(CmError::internal(format!("tesseract: {e}"))),
        }
    };
    let _ = writer.join();
    let out = out_t.join().unwrap_or_default();
    let err = err_t.join().unwrap_or_default();
    if !status.success() {
        let first = err.lines().next().unwrap_or("no message");
        return Err(CmError::internal(format!(
            "tesseract failed ({status}): {first}"
        )));
    }
    Ok(out)
}

/// One word from the TSV with its confidence (0..=1).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Word {
    pub text: String,
    pub rect: Rc,
    pub conf: f32,
}

/// Reads tesseract's TSV (`level page block par line word left top width height conf text`),
/// keeps word rows with text and divides the boxes by the up-scaling factor.
pub(crate) fn parse_tsv(tsv: &str, scale: f64) -> Vec<Word> {
    let mut out = Vec::new();
    for row in tsv.lines().skip(1) {
        let f: Vec<&str> = row.splitn(12, '\t').collect();
        if f.len() < 12 || f[0] != "5" {
            continue;
        }
        let text = f[11].trim();
        if text.is_empty() {
            continue;
        }
        let num = |i: usize| f[i].trim().parse::<f64>().ok();
        let (Some(x), Some(y), Some(w), Some(h), Some(conf)) =
            (num(6), num(7), num(8), num(9), num(10))
        else {
            continue;
        };
        if w <= 0.0 || h <= 0.0 || conf < 0.0 {
            continue;
        }
        out.push(Word {
            text: text.to_owned(),
            rect: Rc::new(x / scale, y / scale, w / scale, h / scale),
            conf: (conf / 100.0).clamp(0.0, 1.0) as f32,
        });
    }
    out
}

/// Joins words into lines by geometry: a word continues a line when it overlaps the line
/// vertically by at least half of the smaller height, starts to the right of the line's end
/// (with a little tolerance) and is closer than 1.5 times the text height. Table columns that
/// are further apart stay separate lines. Lines come out in reading order.
pub(crate) fn group_words(mut words: Vec<Word>) -> Vec<TextLine> {
    words.sort_by(|a, b| a.rect.x.total_cmp(&b.rect.x));
    let mut lines: Vec<Vec<Word>> = Vec::new();
    for w in words {
        let mut best: Option<(usize, f64)> = None;
        for (i, l) in lines.iter().enumerate() {
            let last = &l[l.len() - 1].rect;
            let top = l.iter().map(|x| x.rect.y).fold(f64::MAX, f64::min);
            let bottom = l
                .iter()
                .map(|x| x.rect.y + x.rect.h)
                .fold(f64::MIN, f64::max);
            let overlap = (w.rect.y + w.rect.h).min(bottom) - w.rect.y.max(top);
            let min_h = w.rect.h.min(bottom - top);
            let gap = w.rect.x - (last.x + last.w);
            let h = w.rect.h.max(last.h);
            let fits = overlap >= 0.5 * min_h && gap >= -0.3 * h && gap <= 1.5 * h;
            if fits && best.is_none_or(|(_, o)| overlap > o) {
                best = Some((i, overlap));
            }
        }
        match best {
            Some((i, _)) => lines[i].push(w),
            None => lines.push(vec![w]),
        }
    }
    let mut out: Vec<TextLine> = lines.into_iter().map(line_of).collect();
    out.sort_by(|a, b| {
        let key = |l: &TextLine| (l.rect.y + l.rect.h / 2.0, l.rect.x);
        let (ka, kb) = (key(a), key(b));
        // Rows within half a line height count as the same row, left to right.
        if (ka.0 - kb.0).abs() < 0.5 * a.rect.h.min(b.rect.h) {
            ka.1.total_cmp(&kb.1)
        } else {
            ka.0.total_cmp(&kb.0)
        }
    });
    out
}

fn line_of(words: Vec<Word>) -> TextLine {
    let x0 = words.iter().map(|w| w.rect.x).fold(f64::MAX, f64::min);
    let y0 = words.iter().map(|w| w.rect.y).fold(f64::MAX, f64::min);
    let x1 = words
        .iter()
        .map(|w| w.rect.x + w.rect.w)
        .fold(f64::MIN, f64::max);
    let y1 = words
        .iter()
        .map(|w| w.rect.y + w.rect.h)
        .fold(f64::MIN, f64::max);
    let conf = words.iter().map(|w| w.conf).sum::<f32>() / words.len() as f32;
    let text = words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    TextLine {
        text,
        rect: Rc::new(x0, y0, x1 - x0, y1 - y0),
        confidence: conf,
        words: words
            .into_iter()
            .map(|w| TextWord {
                text: w.text,
                rect: w.rect,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(word: &str, x: u32, y: u32, w: u32, h: u32, conf: i32) -> String {
        format!("5\t1\t1\t1\t1\t1\t{x}\t{y}\t{w}\t{h}\t{conf}\t{word}")
    }

    #[test]
    fn tsv_keeps_words_and_scales_boxes() {
        let tsv = format!(
            "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
             1\t1\t0\t0\t0\t0\t0\t0\t800\t600\t-1\t\n{}\n{}\n5\t1\t1\t1\t1\t3\t5\t5\t5\t5\t-1\t \n",
            row("hello", 40, 20, 100, 30, 91),
            row("a@b.co", 200, 20, 120, 30, 80),
        );
        let w = parse_tsv(&tsv, 2.0);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].text, "hello");
        assert_eq!(w[0].rect, Rc::new(20.0, 10.0, 50.0, 15.0));
        assert!((w[1].conf - 0.8).abs() < 1e-6);
    }

    #[test]
    fn words_join_into_lines_by_geometry() {
        let mk = |t: &str, x: f64, y: f64, w: f64| Word {
            text: t.into(),
            rect: Rc::new(x, y, w, 12.0),
            conf: 0.9,
        };
        let lines = group_words(vec![
            mk("555", 60.0, 10.0, 24.0),
            mk("Call", 10.0, 10.0, 30.0),
            mk("0132", 90.0, 11.0, 30.0),
            mk("Second", 10.0, 40.0, 50.0),
            // A far column on the first row stays its own line.
            mk("Right", 400.0, 10.0, 40.0),
            mk("415", 44.0, 10.0, 12.0),
        ]);
        let texts: Vec<_> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["Call 415 555 0132", "Right", "Second"]);
        assert_eq!(lines[0].words.len(), 4);
    }

    #[test]
    fn locate_order_and_errors() {
        let dir = std::env::temp_dir().join(format!("cm-tess-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join(exe_name());
        std::fs::write(&exe, b"").unwrap();
        // The variable wins, as a file or a folder.
        for v in [exe.clone(), dir.clone()] {
            let p = SearchPaths {
                env: Some(v.into_os_string()),
                ..SearchPaths::default()
            };
            assert_eq!(locate(&p).unwrap(), exe);
        }
        // A wrong variable is named, not skipped.
        let bad = SearchPaths {
            env: Some(dir.join("nope").into_os_string()),
            path_var: Some(dir.clone().into_os_string()),
            ..SearchPaths::default()
        };
        assert!(locate(&bad)
            .unwrap_err()
            .to_string()
            .contains(ENV_TESSERACT));
        // PATH is the last step; nothing found says what to install.
        let on_path = SearchPaths {
            path_var: Some(dir.clone().into_os_string()),
            ..SearchPaths::default()
        };
        assert_eq!(locate(&on_path).unwrap(), exe);
        let none = SearchPaths::default();
        assert!(locate(&none)
            .unwrap_err()
            .to_string()
            .contains("tesseract-ocr"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_engine_is_unavailable_and_not_implemented() {
        let r = TesseractRecognizer::with_paths(SearchPaths::default());
        assert!(!r.is_available());
        assert!(r
            .install_hint()
            .unwrap()
            .contains("apt install tesseract-ocr"));
        let img = RgbaImage::from_pixel(8, 8, image::Rgba([255, 255, 255, 255]));
        let e = r.recognize(&img).unwrap_err();
        assert!(e.to_string().contains("tesseract"), "{e}");
    }

    #[test]
    fn dark_pictures_are_inverted_and_scaled() {
        let dark = RgbaImage::from_pixel(10, 6, image::Rgba([10, 10, 10, 255]));
        let (png, s) = prepare(&dark, &TesseractOptions::default()).unwrap();
        assert_eq!(s, 2);
        let back = image::load_from_memory(&png).unwrap().to_luma8();
        assert_eq!(back.dimensions(), (20, 12));
        assert!(back.as_raw().iter().all(|&v| v > 200));
        let light = RgbaImage::from_pixel(10, 6, image::Rgba([240, 240, 240, 255]));
        let (png, _) = prepare(&light, &TesseractOptions::default()).unwrap();
        let back = image::load_from_memory(&png).unwrap().to_luma8();
        assert!(back.as_raw().iter().all(|&v| v > 200));
    }
}
