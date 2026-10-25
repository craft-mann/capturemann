//! "Latest frame" slot shared by the capture callback (producer) and the
//! constant-frame-rate writer (consumer). The producer overwrites; the
//! consumer copies whatever is newest at each tick.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

#[derive(Default)]
pub struct FrameSink {
    slot: Mutex<Slot>,
    received: AtomicU64,
}

#[derive(Default)]
struct Slot {
    buf: Vec<u8>,
    w: u32,
    h: u32,
    seq: u64,
}

impl FrameSink {
    /// Copy a BGRA frame in, dropping per-row padding (`stride` bytes per row).
    pub fn push(&self, data: &[u8], w: u32, h: u32, stride: usize) {
        let row = w as usize * 4;
        if stride < row || data.len() < stride * (h as usize - 1) + row {
            return; // malformed frame
        }
        let mut s = self.slot.lock().unwrap();
        s.buf.resize(row * h as usize, 0);
        if stride == row {
            s.buf.copy_from_slice(&data[..row * h as usize]);
        } else {
            for y in 0..h as usize {
                s.buf[y * row..(y + 1) * row].copy_from_slice(&data[y * stride..y * stride + row]);
            }
        }
        s.w = w;
        s.h = h;
        s.seq += 1;
        drop(s);
        self.received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn size(&self) -> Option<(u32, u32)> {
        let s = self.slot.lock().unwrap();
        (s.seq > 0).then_some((s.w, s.h))
    }

    /// Copy the newest frame into `out` if it has the expected size; returns its sequence number.
    pub fn copy_latest(&self, out: &mut [u8], w: u32, h: u32) -> Option<u64> {
        let s = self.slot.lock().unwrap();
        if s.seq == 0 || s.w != w || s.h != h || s.buf.len() != out.len() {
            return None;
        }
        out.copy_from_slice(&s.buf);
        Some(s.seq)
    }

    pub fn received(&self) -> u64 {
        self.received.load(Ordering::Relaxed)
    }
}
