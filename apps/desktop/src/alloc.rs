//! Global allocator that hands large blocks straight back to the OS (NFR-5).
//!
//! macOS `malloc` keeps freed large blocks (about 1 MB and up) in a cache and counts them as
//! resident for a long time: one 3024x1964 capture (a 24 MB frame, its PNG and the copies the
//! editor makes) left ~65 MB of dirty "freed" pages in an idle process, and the release build
//! never gave them back (`malloc_zone_pressure_relief` does not touch that cache). Large
//! blocks therefore come from `mmap` and go back with `munmap`, so the memory the app holds
//! while idle is the memory it is using. Everything smaller goes to the system allocator.

#![cfg(target_os = "macos")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::ptr;

/// Blocks of at least this many bytes bypass `malloc`.
const DIRECT_MIN: usize = 1 << 20;
/// `mmap` returns page-aligned memory; bigger alignments go to the system allocator.
const MAX_ALIGN: usize = 4096;

pub struct ReturnLargeToOs;

fn direct(layout: Layout) -> bool {
    layout.size() >= DIRECT_MIN && layout.align() <= MAX_ALIGN
}

// SAFETY: `alloc` returns either a fresh `mmap` region (page aligned, at least `size` bytes,
// zeroed) or defers to `System`; `dealloc` picks the same path from the same layout, so a
// block is always released by the allocator that produced it. `realloc` uses the default
// alloc + copy + dealloc when a block starts or ends up in the direct path.
unsafe impl GlobalAlloc for ReturnLargeToOs {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if direct(layout) {
            // SAFETY: anonymous private mapping, no file descriptor involved.
            let p = unsafe {
                libc::mmap(
                    ptr::null_mut(),
                    layout.size(),
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_PRIVATE | libc::MAP_ANON,
                    -1,
                    0,
                )
            };
            if p == libc::MAP_FAILED {
                ptr::null_mut()
            } else {
                p.cast()
            }
        } else {
            // SAFETY: forwarded unchanged.
            unsafe { System.alloc(layout) }
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if direct(layout) {
            // Fresh anonymous pages are already zero.
            // SAFETY: same contract as `alloc`.
            unsafe { self.alloc(layout) }
        } else {
            // SAFETY: forwarded unchanged.
            unsafe { System.alloc_zeroed(layout) }
        }
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        if direct(layout) {
            // SAFETY: `p` came from `mmap` with this size (see `alloc`).
            unsafe { libc::munmap(p.cast(), layout.size()) };
        } else {
            // SAFETY: `p` came from `System` with this layout.
            unsafe { System.dealloc(p, layout) }
        }
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `new_size` is valid for this alignment.
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        if !direct(layout) && !direct(new_layout) {
            // SAFETY: forwarded unchanged.
            return unsafe { System.realloc(p, layout, new_size) };
        }
        // SAFETY: `new_layout` is valid; `p` holds `layout.size()` initialized-or-not bytes
        // that are copied, then released by the allocator that owns them.
        unsafe {
            let q = self.alloc(new_layout);
            if !q.is_null() {
                ptr::copy_nonoverlapping(p, q, layout.size().min(new_size));
                self.dealloc(p, layout);
            }
            q
        }
    }
}
