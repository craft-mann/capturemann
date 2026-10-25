//! macOS backend: listen-only CGEventTap + NSCursor images.
use std::{
    collections::HashMap,
    ffi::c_void,
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSCursor};
use objc2_foundation::NSDictionary;

use crate::{hash_id, mods_json, ns_to_ticks, spawn_writer, Rec};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct CGPoint {
    x: f64,
    y: f64,
}

type Ptr = *mut c_void;
type TapCb = extern "C" fn(Ptr, u32, Ptr, Ptr) -> Ptr;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(tap: u32, place: u32, opts: u32, mask: u64, cb: TapCb, user: Ptr) -> Ptr;
    fn CGEventTapEnable(tap: Ptr, enable: bool);
    fn CGEventGetLocation(ev: Ptr) -> CGPoint;
    fn CGEventGetTimestamp(ev: Ptr) -> u64;
    fn CGEventGetFlags(ev: Ptr) -> u64;
    fn CGEventGetIntegerValueField(ev: Ptr, field: u32) -> i64;
    fn CGEventCreate(src: Ptr) -> Ptr;
    fn CGEventCreateMouseEvent(src: Ptr, ty: u32, pt: CGPoint, button: u32) -> Ptr;
    fn CGEventPost(tap: u32, ev: Ptr);
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayCopyDisplayMode(d: u32) -> Ptr;
    fn CGDisplayModeGetPixelWidth(m: Ptr) -> usize;
    fn CGDisplayModeGetWidth(m: Ptr) -> usize;
    fn CGPreflightPostEventAccess() -> bool;
    fn CGDisplayBounds(d: u32) -> CGRect;
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFMachPortCreateRunLoopSource(a: Ptr, port: Ptr, order: isize) -> Ptr;
    fn CFRunLoopGetCurrent() -> Ptr;
    fn CFRunLoopAddSource(rl: Ptr, src: Ptr, mode: Ptr);
    fn CFRunLoopRun();
    fn CFRunLoopStop(rl: Ptr);
    fn CFRelease(p: Ptr);
    static kCFRunLoopCommonModes: Ptr;
}
extern "C" {
    fn mach_absolute_time() -> u64;
    fn mach_timebase_info(info: *mut [u32; 2]) -> i32;
    fn getrusage(who: i32, ru: *mut [i64; 18]) -> i32;
}

const MOVED: u32 = 5;
const SCROLL: u32 = 22;
const DISABLED_TIMEOUT: u32 = 0xFFFF_FFFE;
const DISABLED_USER: u32 = 0xFFFF_FFFF;

fn mask() -> u64 {
    // moves, drags, buttons (left, right, other), scroll. No key events: keystrokes are never seen.
    [1u32, 2, 3, 4, 5, 6, 7, 25, 26, 27, 22].iter().fold(0, |m, t| m | 1u64 << t)
}

struct Clock {
    numer: u64,
    denom: u64,
    origin_ns: u64,
    /// CGEventGetTimestamp is in mach ticks (true) or ns (false); detected at start.
    ev_in_ticks: AtomicBool,
    unit_known: AtomicBool,
}
impl Clock {
    fn new() -> Clock {
        let mut tb = [0u32; 2];
        unsafe { mach_timebase_info(&mut tb) };
        let mut c = Clock { numer: tb[0] as u64, denom: tb[1] as u64, origin_ns: 0, ev_in_ticks: AtomicBool::new(false), unit_known: AtomicBool::new(false) };
        c.origin_ns = c.now_ns();
        c
    }
    fn ticks_to_ns(&self, t: u64) -> u64 {
        (t as u128 * self.numer as u128 / self.denom as u128) as u64
    }
    fn now_ns(&self) -> u64 {
        self.ticks_to_ns(unsafe { mach_absolute_time() })
    }
    /// CGEventGetTimestamp: detect on first event whether it is mach ticks or ns (whichever is closer to now).
    fn ev_ns(&self, ts: u64) -> u64 {
        if !self.unit_known.swap(true, Ordering::Relaxed) {
            let t = unsafe { mach_absolute_time() };
            let d_ticks = t.abs_diff(ts);
            let d_ns = self.ticks_to_ns(t).abs_diff(ts);
            self.ev_in_ticks.store(d_ticks < d_ns, Ordering::Relaxed);
            eprintln!("event timestamp unit detected: {} (timebase {}/{})", if d_ticks < d_ns { "mach ticks" } else { "ns" }, self.numer, self.denom);
        }
        if self.ev_in_ticks.load(Ordering::Relaxed) { self.ticks_to_ns(ts) } else { ts }
    }
}

#[derive(Clone, Debug)]
struct Obs {
    kind: &'static str, // move, down, up, scroll
    button: &'static str,
    ev_ns: u64,
    cb_ns: u64,
    x: f64,
    #[allow(dead_code)]
    y: f64,
}

struct Ctx {
    clk: Clock,
    scale: f64,
    tx: mpsc::Sender<Rec>,
    obs: Mutex<Vec<Obs>>,
    tap: AtomicU64,
    timeouts: AtomicU64,
    last_ev_ns: AtomicU64,
    non_monotonic: AtomicU64,
}

fn button_name(n: i64) -> &'static str {
    match n {
        0 => "left",
        1 => "right",
        2 => "middle",
        3 => "back",
        4 => "forward",
        _ => "other",
    }
}

extern "C" fn tap_cb(_proxy: Ptr, ty: u32, ev: Ptr, user: Ptr) -> Ptr {
    let ctx = unsafe { &*(user as *const Ctx) };
    let cb_ns = ctx.clk.now_ns();
    if ty == DISABLED_TIMEOUT || ty == DISABLED_USER {
        ctx.timeouts.fetch_add(1, Ordering::Relaxed);
        unsafe { CGEventTapEnable(ctx.tap.load(Ordering::Relaxed) as Ptr, true) };
        return ev;
    }
    let ev_ns = ctx.clk.ev_ns(unsafe { CGEventGetTimestamp(ev) });
    let prev = ctx.last_ev_ns.swap(ev_ns, Ordering::Relaxed);
    if ev_ns < prev {
        ctx.non_monotonic.fetch_add(1, Ordering::Relaxed);
    }
    let p = unsafe { CGEventGetLocation(ev) };
    let t = ns_to_ticks(ev_ns, ctx.clk.origin_ns);
    // Positions: points -> physical px of the main display (spike simplification).
    let (x, y) = (p.x * ctx.scale, p.y * ctx.scale);
    let (kind, button, line) = match ty {
        1 | 3 | 25 | 2 | 4 | 26 => {
            let down = matches!(ty, 1 | 3 | 25);
            let b = button_name(unsafe { CGEventGetIntegerValueField(ev, 3) });
            let f = unsafe { CGEventGetFlags(ev) };
            let mut m = 0u8;
            if f & 0x0002_0000 != 0 { m |= 1 }
            if f & 0x0004_0000 != 0 { m |= 2 }
            if f & 0x0008_0000 != 0 { m |= 4 }
            if f & 0x0010_0000 != 0 { m |= 8 }
            let k = if down { "down" } else { "up" };
            (k, b, format!(r#"{{"t":{t},"type":"{k}","button":"{b}","x":{x:.0},"y":{y:.0},"mods":{}}}"#, mods_json(m)))
        }
        MOVED | 6 | 7 | 27 => ("move", "", format!(r#"{{"t":{t},"type":"move","x":{x:.0},"y":{y:.0}}}"#)),
        SCROLL => {
            // direction only, no magnitudes: sign of the point-delta fields (96 = Y, 97 = X)
            let dy = unsafe { CGEventGetIntegerValueField(ev, 96) }.signum();
            let dx = unsafe { CGEventGetIntegerValueField(ev, 97) }.signum();
            ("scroll", "", format!(r#"{{"t":{t},"type":"scroll","dx":{dx},"dy":{dy}}}"#))
        }
        _ => return ev,
    };
    let _ = ctx.tx.send(Rec::Line(line));
    ctx.obs.lock().unwrap().push(Obs { kind, button, ev_ns, cb_ns, x: p.x, y: p.y });
    ev
}

fn scale() -> f64 {
    unsafe {
        // CGDisplayPixelsWide returns the logical size on HiDPI; the mode's pixel width is physical.
        let m = CGDisplayCopyDisplayMode(CGMainDisplayID());
        let s = CGDisplayModeGetPixelWidth(m) as f64 / CGDisplayModeGetWidth(m) as f64;
        CFRelease(m);
        s
    }
}

fn cpu_secs() -> f64 {
    let mut ru = [0i64; 18];
    unsafe { getrusage(0, &mut ru) };
    // ru_utime {sec, usec(i32 padded)}, ru_stime
    (ru[0] + ru[2]) as f64 + (ru[1] + ru[3]) as f64 / 1e6
}

/// Extract the current system cursor as PNG; returns (png, hotspot_pts, size_pts, pixel_size).
#[allow(deprecated)]
fn current_cursor_png() -> Option<(usize, Vec<u8>, (f64, f64), (f64, f64), (usize, usize))> {
    let c = NSCursor::currentSystemCursor()?;
    let ptr = objc2::rc::Retained::as_ptr(&c) as usize;
    let img = c.image();
    let hot = c.hotSpot();
    let size = img.size();
    let tiff = img.TIFFRepresentation()?;
    let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
    let (pw, ph) = (rep.pixelsWide() as usize, rep.pixelsHigh() as usize);
    let png = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }?;
    Some((ptr, png.to_vec(), (hot.x, hot.y), (size.width, size.height), (pw, ph)))
}

/// Polls the cursor shape; emits a `cursor` line + PNG when the image changes.
#[allow(deprecated)]
fn cursor_poller(ctx: Arc<Ctx>, out: std::path::PathBuf, stop: Arc<AtomicBool>, stats: Arc<Mutex<Vec<String>>>) {
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut last_tid = String::new();
    let mut last_id = String::new();
    while !stop.load(Ordering::Relaxed) {
        let t0 = ctx.clk.now_ns();
        // Identify by hash of the TIFF bytes (cheap); encode PNG only for new images.
        if let Some(c) = NSCursor::currentSystemCursor() {
            let hot = c.hotSpot();
            let img = c.image();
            let key = img.TIFFRepresentation().map(|t| { let b = t.to_vec(); (hash_id(&b), b.len()) });
            if let Some((tid, _)) = key {
                let tid = format!("{tid}@{:.0},{:.0}", hot.x, hot.y);
                if tid != last_tid {
                    last_tid = tid;
                    if let Some((_, png, hot, size, px)) = current_cursor_png() {
                        let id = hash_id(&png);
                        if id != last_id {
                            if seen.insert(id.clone(), ()).is_none() {
                                let _ = fs::write(out.join("cursors").join(format!("{id}.png")), &png);
                            }
                            let sc = px.0 as f64 / size.0.max(1.0);
                            let t = ns_to_ticks(t0, ctx.clk.origin_ns);
                            let line = format!(
                                r#"{{"t":{t},"type":"cursor","id":"{id}","hotspot":[{:.0},{:.0}],"scale":{sc:.2},"size_px":[{},{}]}}"#,
                                hot.0 * sc, hot.1 * sc, px.0, px.1
                            );
                            stats.lock().unwrap().push(format!("{id}: {}x{} px, {:.0}x{:.0} pt, scale {sc:.2}, hotspot px ({:.0},{:.0}), {} B png", px.0, px.1, size.0, size.1, hot.0 * sc, hot.1 * sc, png.len()));
                            let _ = ctx.tx.send(Rec::Line(line));
                            last_id = id;
                        }
                    }
                }
            }
        }
        // 30 Hz while the mouse is active, 4 Hz when idle (keeps idle CPU negligible).
        let idle = ctx.clk.now_ns().saturating_sub(ctx.last_ev_ns.load(Ordering::Relaxed)) > 300_000_000;
        thread::sleep(Duration::from_millis(if idle { 250 } else { 33 }));
    }
}

/// Fallback when the tap is denied: poll position at 120 Hz, record only changes.
fn poll_positions(ctx: Arc<Ctx>, stop: Arc<AtomicBool>) {
    let mut last = (f64::NAN, f64::NAN);
    while !stop.load(Ordering::Relaxed) {
        unsafe {
            let e = CGEventCreate(std::ptr::null_mut());
            let p = CGEventGetLocation(e);
            CFRelease(e);
            if (p.x, p.y) != last {
                last = (p.x, p.y);
                let t = ns_to_ticks(ctx.clk.now_ns(), ctx.clk.origin_ns);
                let _ = ctx.tx.send(Rec::Line(format!(r#"{{"t":{t},"type":"move","x":{:.0},"y":{:.0}}}"#, p.x * ctx.scale, p.y * ctx.scale)));
            }
        }
        thread::sleep(Duration::from_micros(8333));
    }
}

struct Session {
    ctx: Arc<Ctx>,
    stop: Arc<AtomicBool>,
    writer: Option<thread::JoinHandle<u64>>,
    cursors: Arc<Mutex<Vec<String>>>,
    helpers: Vec<thread::JoinHandle<()>>,
    tap_ok: bool,
}

/// Build the context, detect the event-timestamp unit, install the tap on this thread's run loop.
fn start(out: &Path, request_permission: bool) -> Session {
    let (tx, rx_w) = {
        let (tx, h) = spawn_writer(out.join("cursor.jsonl"));
        (tx, h)
    };
    let clk = Clock::new();
    let sc = scale();
    let ctx = Arc::new(Ctx {
        clk,
        scale: sc,
        tx,
        obs: Mutex::new(vec![]),
        tap: AtomicU64::new(0),
        timeouts: AtomicU64::new(0),
        last_ev_ns: AtomicU64::new(0),
        non_monotonic: AtomicU64::new(0),
    });
    let _ = ctx.tx.send(Rec::Line(format!(
        r#"{{"type":"session","origin":"mach_absolute_time","ticks_per_sec":120000,"display_scale":{sc}}}"#
    )));
    let granted = unsafe { CGPreflightListenEventAccess() };
    if !granted && request_permission {
        unsafe { CGRequestListenEventAccess() };
    }
    // Unsafe-but-fine: ctx lives for the whole run (leaked clone as user pointer).
    let user = Arc::into_raw(ctx.clone()) as Ptr;
    let tap = unsafe { CGEventTapCreate(1 /*session*/, 0 /*head*/, 1 /*listen only*/, mask(), tap_cb, user) };
    let stop = Arc::new(AtomicBool::new(false));
    let cursors = Arc::new(Mutex::new(vec![]));
    let mut helpers = vec![];
    let tap_ok = !tap.is_null();
    if tap_ok {
        ctx.tap.store(tap as u64, Ordering::Relaxed);
        unsafe {
            let src = CFMachPortCreateRunLoopSource(std::ptr::null_mut(), tap, 0);
            CFRunLoopAddSource(CFRunLoopGetCurrent(), src, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);
        }
    } else {
        eprintln!(
            "permission_missing: cannot create the event tap. Grant Input Monitoring (System Settings > Privacy & Security > Input Monitoring) \
             to this terminal/app and re-run. Falling back to position polling; clicks/scroll are unavailable."
        );
        let _ = ctx.tx.send(Rec::Line(r#"{"type":"status","cursor.clicks":"unavailable","reason":"permission_missing"}"#.into()));
        let (c, s) = (ctx.clone(), stop.clone());
        helpers.push(thread::spawn(move || poll_positions(c, s)));
    }
    let (c, s, o, st) = (ctx.clone(), stop.clone(), out.to_path_buf(), cursors.clone());
    helpers.push(thread::spawn(move || cursor_poller(c, o, s, st)));
    Session { ctx, stop, writer: Some(rx_w), cursors, helpers, tap_ok }
}

fn finish(mut s: Session) -> u64 {
    finish_ref(&mut s)
}

pub fn record(out: &Path, secs: f64) -> Result<()> {
    let cpu0 = cpu_secs();
    let wall0 = Instant::now();
    let s = start(out, true);
    let rl = unsafe { CFRunLoopGetCurrent() } as usize;
    let _ = ctrlc::set_handler(move || unsafe { CFRunLoopStop(rl as Ptr) });
    thread::spawn(move || {
        thread::sleep(Duration::from_secs_f64(secs));
        unsafe { CFRunLoopStop(rl as Ptr) };
    });
    println!("recording cursor for {secs} s (tap: {}) ...", if s.tap_ok { "ok" } else { "permission_missing" });
    unsafe { CFRunLoopRun() };
    let cpu = cpu_secs() - cpu0;
    let wall = wall0.elapsed().as_secs_f64();
    let obs = s.ctx.obs.lock().unwrap().len();
    let cursors = s.cursors.lock().unwrap().clone();
    let (to, nm) = (s.ctx.timeouts.load(Ordering::Relaxed), s.ctx.non_monotonic.load(Ordering::Relaxed));
    let o = s.ctx.obs.lock().unwrap().clone();
    let mut dl: Vec<f64> = o.iter().map(|o| (o.cb_ns as f64 - o.ev_ns as f64) / 1e6).collect();
    stats("tap delivery: callback time minus event timestamp", &mut dl);
    let mut iv: Vec<f64> = o.windows(2).filter(|w| w[0].kind == "move" && w[1].kind == "move").map(|w| (w[1].ev_ns as f64 - w[0].ev_ns as f64) / 1e6).collect();
    stats("move-to-move interval", &mut iv);
    let n = finish(s);
    println!("{n} lines written ({obs} tap events), cursor images: {}", cursors.len());
    for c in cursors { println!("  {c}"); }
    println!("tap timeouts {to}, non-monotonic ts {nm}, CPU {:.3} s over {wall:.1} s = {:.2}%", cpu, cpu / wall * 100.0);
    Ok(())
}

fn post(ty: u32, p: CGPoint, button: u32) {
    unsafe {
        let e = CGEventCreateMouseEvent(std::ptr::null_mut(), ty, p, button);
        CGEventPost(0, e);
        CFRelease(e);
    }
}

fn stats(name: &str, v: &mut Vec<f64>) {
    if v.is_empty() {
        println!("{name}: no samples");
        return;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    let p = |q: f64| v[((v.len() - 1) as f64 * q) as usize];
    println!("{name}: n={} min {:.3} mean {:.3} p50 {:.3} p95 {:.3} p99 {:.3} max {:.3} ms", v.len(), v[0], mean, p(0.5), p(0.95), p(0.99), v[v.len() - 1]);
}

pub fn selfcheck(out: &Path, left_clicks: bool) -> Result<()> {
    let cpu0 = cpu_secs();
    let wall0 = Instant::now();
    let mut s = start(out, false);
    if !s.tap_ok {
        println!("RESULT: permission_missing (no event tap). Cannot measure; grant Input Monitoring to the terminal and re-run.");
        finish(s);
        return Ok(());
    }
    if !unsafe { CGPreflightPostEventAccess() } {
        println!("post_permission_missing: CGEventPost is silently dropped without Accessibility (post access). Synthetic self-check cannot inject events; grant Accessibility to the terminal and re-run, or use `record` with real input.");
    }
    // Detect CGEventGetTimestamp unit by posting one probe move and comparing.
    // (done inside the run: posting thread does it first, see below)
    let rl = unsafe { CFRunLoopGetCurrent() } as usize;
    let sc = s.ctx.scale;
    let _ = sc;
    let ctx = s.ctx.clone();
    // Posted record: (kind, button, x, post_ns)
    let poster = thread::spawn(move || {
        let (w, h) = unsafe { let d = CGMainDisplayID(); (CGDisplayBounds(d).w, CGDisplayBounds(d).h) };
        let _ = h;
        let mut posted: Vec<(&'static str, &'static str, f64, u64)> = vec![];
        thread::sleep(Duration::from_millis(300)); // let the tap settle
        // Phase 1: 400 moves at a nominal 120 Hz along a line near the top-right of the screen.
        let y = 300.0;
        let start = Instant::now();
        for i in 0..400u32 {
            let target = start + Duration::from_secs_f64(i as f64 / 120.0);
            while Instant::now() < target { std::hint::spin_loop(); }
            let p = CGPoint { x: w - 600.0 + i as f64, y };
            let ns = ctx.clk.now_ns();
            post(MOVED, p, 0);
            posted.push(("move", "", p.x, ns));
        }
        // Phase 2: 60 middle-button clicks (down/up 15 ms apart), plus optional left clicks in the menu bar.
        let mut clicks: Vec<(u32, u32, &'static str, CGPoint)> = vec![(25, 26, "middle", CGPoint { x: w - 300.0, y: 300.0 })];
        if left_clicks {
            clicks.push((1, 2, "left", CGPoint { x: w - 300.0, y: 300.0 }));
        }
        for round in 0..30 {
            for (d, u, name, pt) in &clicks {
                let _ = round;
                post(MOVED, *pt, 0);
                thread::sleep(Duration::from_millis(5));
                let ns = ctx.clk.now_ns();
                post(*d, *pt, if *name == "left" { 0 } else { 2 });
                posted.push(("down", name, pt.x, ns));
                thread::sleep(Duration::from_millis(15));
                let ns = ctx.clk.now_ns();
                post(*u, *pt, if *name == "left" { 0 } else { 2 });
                posted.push(("up", name, pt.x, ns));
                thread::sleep(Duration::from_millis(30));
            }
        }
        thread::sleep(Duration::from_millis(500));
        unsafe { CFRunLoopStop(rl as Ptr) };
        posted
    });
    // Unit detection happens after the run, from the data: choose the interpretation (ticks vs ns)
    // by re-reading raw timestamps. To keep the callback simple we ask for both via a probe here.
    unsafe { CFRunLoopRun() };
    let posted = poster.join().unwrap();
    let cpu = cpu_secs() - cpu0;
    let wall = wall0.elapsed().as_secs_f64();
    let obs = s.ctx.obs.lock().unwrap().clone();
    s.ctx.obs.lock().unwrap().clear();
    let nm = s.ctx.non_monotonic.load(Ordering::Relaxed);
    let to = s.ctx.timeouts.load(Ordering::Relaxed);

    // Match posted -> observed. Moves by x (unique per step, +-0.5), clicks by order per (kind, button).
    let mut err = vec![]; // event ts - post time
    let mut lat = vec![]; // callback time - post time
    let mut mv_seen = 0;
    let mut mv_intervals = vec![];
    let mut last_mv: Option<u64> = None;
    for (kind, _b, x, post_ns) in posted.iter().filter(|p| p.0 == "move") {
        let _ = kind;
        if let Some(o) = obs.iter().find(|o| o.kind == "move" && (o.x - x).abs() < 0.5) {
            mv_seen += 1;
            err.push((o.ev_ns as f64 - *post_ns as f64) / 1e6);
            lat.push((o.cb_ns as f64 - *post_ns as f64) / 1e6);
            if let Some(l) = last_mv { mv_intervals.push((o.ev_ns as f64 - l as f64) / 1e6); }
            last_mv = Some(o.ev_ns);
        }
    }
    let mv_posted = posted.iter().filter(|p| p.0 == "move").count();
    let mut click_posted = (0, 0);
    let mut click_seen = (0, 0);
    let mut cerr = vec![];
    for kind in ["down", "up"] {
        let p: Vec<_> = posted.iter().filter(|p| p.0 == kind).collect();
        let o: Vec<_> = obs.iter().filter(|o| o.kind == kind).collect();
        if kind == "down" { click_posted.0 = p.len(); click_seen.0 = o.len() } else { click_posted.1 = p.len(); click_seen.1 = o.len() }
        for (pp, oo) in p.iter().zip(o.iter()) {
            cerr.push((oo.ev_ns as f64 - pp.3 as f64) / 1e6);
            lat.push((oo.cb_ns as f64 - pp.3 as f64) / 1e6);
            if oo.button != pp.1 { println!("button mismatch: posted {} got {}", pp.1, oo.button); }
        }
    }
    let n = finish_ref(&mut s);
    println!("--- selfcheck (events synthesized with CGEventPost at kCGHIDEventTap) ---");
    println!("lines written: {n}");
    println!("moves: posted {mv_posted}, seen {mv_seen} (move coalescing is legal; clicks are not)");
    println!("clicks: down posted {} seen {}, up posted {} seen {}", click_posted.0, click_seen.0, click_posted.1, click_seen.1);
    stats("move  event-ts minus post time (accuracy)", &mut err.clone());
    stats("click event-ts minus post time (accuracy)", &mut cerr.clone());
    stats("callback minus post time (event->record latency)", &mut lat.clone());
    let mut jit: Vec<f64> = mv_intervals.iter().map(|d| (d - 1000.0 / 120.0).abs()).collect();
    stats("move interval jitter vs 8.333 ms nominal (abs)", &mut jit);
    println!("non-monotonic event timestamps: {nm}; tap timeouts/disables: {to}");
    println!("CPU {:.3} s over {wall:.1} s wall = {:.2}% (includes the synthesizing thread)", cpu, cpu / wall * 100.0);
    for c in s.cursors.lock().unwrap().iter() { println!("cursor image: {c}"); }
    Ok(())
}

fn finish_ref(s: &mut Session) -> u64 {
    s.stop.store(true, Ordering::Relaxed);
    for h in s.helpers.drain(..) {
        let _ = h.join();
    }
    let _ = s.ctx.tx.send(Rec::Done);
    s.writer.take().and_then(|w| w.join().ok()).unwrap_or(0)
}
