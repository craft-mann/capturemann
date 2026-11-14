//! Scripted stand-ins for the screen, the scroller and the clock: the synthetic test page of
//! `tests/fixtures/long-shot/README.md`. No display, no input injection, no permission.
//!
//! [`SyntheticPage`] is a long page whose every content row is unique (its index is written into
//! the row), with an optional fixed header and footer. A [`PageView`] is both the
//! [`FrameSource`] and the [`Scroller`] of one viewport onto it.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use cm_core::CmError;

use super::{Direction, FrameSource, Scroller, Sleeper};
use crate::Frame;

/// A long page. Sizes are in px along the scroll direction except `cross` (the other side).
#[derive(Debug, Clone)]
pub struct SyntheticPage {
    pub cross: u32,
    /// Content rows of the whole page.
    pub content_len: u32,
    pub header: u32,
    pub footer: u32,
    /// Height of the viewport (header and footer included).
    pub view: u32,
    pub direction: Direction,
    /// `(position, skip)`: when a scroll reaches `position`, the content jumps by `skip` more
    /// (a virtualized list that redraws: no overlap between the two frames).
    pub jump: Option<(u32, u32)>,
    /// Width of a scrollbar down the right edge of the content (vertical pages only): its thumb
    /// moves with the position, so no two frames have equal rows. The expected image has no
    /// scrollbar (the engine trims it).
    pub scrollbar: u32,
    /// Width of an overlay scrollbar (Chrome on macOS): drawn over the whole viewport, bands
    /// included, only once the page has scrolled, and short, so that near the top of the page
    /// its thumb lies on the header rows alone.
    pub overlay_scrollbar: u32,
    /// A sticky header that hides once the page has scrolled to this position or further: the
    /// viewport then shows content where the header was.
    pub hide_header_from: Option<u32>,
    /// Width of a clock in the header: its pixels are different in every frame.
    pub header_clock: u32,
    /// `(content row, rows, width)`: a block cursor on the page that shows in every other frame
    /// (a terminal's), at the left edge of the content.
    pub blink: Option<(u32, u32, u32)>,
    /// Content rows at each end of the viewport that differ in every frame (a window's rounded
    /// corners and edge line drawn over what scrolls under them).
    pub corner_rows: u32,
}

impl SyntheticPage {
    pub fn new(cross: u32, content_len: u32, view: u32) -> Self {
        SyntheticPage {
            cross,
            content_len,
            header: 0,
            footer: 0,
            view,
            direction: Direction::Vertical,
            jump: None,
            scrollbar: 0,
            overlay_scrollbar: 0,
            hide_header_from: None,
            header_clock: 0,
            blink: None,
            corner_rows: 0,
        }
    }

    pub fn with_corner_rows(mut self, rows: u32) -> Self {
        self.corner_rows = rows;
        self
    }

    pub fn with_scrollbar(mut self, width: u32) -> Self {
        self.scrollbar = width;
        self
    }

    pub fn with_header_clock(mut self, width: u32) -> Self {
        self.header_clock = width;
        self
    }

    pub fn with_blinking_cursor(mut self, row: u32, rows: u32, width: u32) -> Self {
        self.blink = Some((row, rows, width));
        self
    }

    pub fn with_hiding_header(mut self, from: u32) -> Self {
        self.hide_header_from = Some(from);
        self
    }

    pub fn with_overlay_scrollbar(mut self, width: u32) -> Self {
        self.overlay_scrollbar = width;
        self
    }

    pub fn with_bands(mut self, header: u32, footer: u32) -> Self {
        self.header = header;
        self.footer = footer;
        self
    }

    pub fn horizontal(mut self) -> Self {
        self.direction = Direction::Horizontal;
        self
    }

    pub fn with_jump(mut self, at: u32, skip: u32) -> Self {
        self.jump = Some((at, skip));
        self
    }

    fn content_view(&self) -> u32 {
        self.view - self.header - self.footer
    }

    /// The pixels of one line (a row; for a horizontal page a column) at `cross` positions.
    fn line(&self, kind: Kind, index: u32) -> Vec<[u8; 4]> {
        (0..self.cross)
            .map(|x| match kind {
                Kind::Header => [200, 30, (x & 255) as u8, 255],
                Kind::Footer => [30, 200, (x & 255) as u8, 255],
                Kind::Content => {
                    // The row index in the first four pixels; a mix of both after that, so every
                    // row differs and no two rows share a prefix of change.
                    let i = index;
                    match x {
                        0 => [
                            (i & 255) as u8,
                            ((i >> 8) & 255) as u8,
                            ((i >> 16) & 255) as u8,
                            255,
                        ],
                        _ => [
                            (i.wrapping_mul(31).wrapping_add(x.wrapping_mul(17)) & 255) as u8,
                            ((i >> 3) ^ x) as u8,
                            (i.wrapping_mul(7) ^ x.wrapping_mul(3)) as u8,
                            255,
                        ],
                    }
                }
            })
            .collect()
    }

    fn assemble(&self, lines: Vec<Vec<[u8; 4]>>) -> Frame {
        let n = lines.len() as u32;
        let mut data = Vec::new();
        let (w, h) = match self.direction {
            Direction::Vertical => {
                for l in &lines {
                    for p in l {
                        data.extend_from_slice(p);
                    }
                }
                (lines.first().map_or(0, Vec::len) as u32, n)
            }
            Direction::Horizontal => {
                for y in 0..self.cross as usize {
                    for l in &lines {
                        data.extend_from_slice(&l[y]);
                    }
                }
                (n, self.cross)
            }
        };
        Frame::new(w, h, data).unwrap_or_else(|_| unreachable!())
    }

    /// What the viewport shows with the content scrolled to `pos`.
    pub fn frame_at(&self, pos: u32) -> Frame {
        let mut lines = Vec::new();
        let hidden = self.hide_header_from.is_some_and(|t| pos >= t);
        // With the header hidden, the rows it covered show the content above the first row.
        let (top_rows, base) = if hidden {
            (self.header, pos - self.header)
        } else {
            (0, pos)
        };
        if !hidden {
            for i in 0..self.header {
                let mut line = self.line(Kind::Header, i);
                // A clock near the right edge: changes every frame.
                let end = (self.cross as usize).saturating_sub(10);
                for (k, p) in line[..end]
                    .iter_mut()
                    .rev()
                    .take(self.header_clock as usize)
                    .enumerate()
                {
                    if (k as u32 / 2 + pos / 7).is_multiple_of(2) {
                        *p = [255, 255, 255, 255];
                    }
                }
                lines.push(line);
            }
        }
        let view = self.content_view();
        // The thumb: a bar whose top follows the position.
        let thumb_len = (view / 4).max(1);
        let thumb_top = (u64::from(pos) * u64::from(view - thumb_len)
            / u64::from(self.max_pos().max(1))) as u32;
        for i in 0..view + top_rows {
            let mut line = self.line(Kind::Content, base + i);
            let in_thumb = (thumb_top..thumb_top + thumb_len).contains(&i);
            for p in line.iter_mut().skip((self.cross - self.scrollbar) as usize) {
                *p = if in_thumb {
                    [90, 90, 90, 255]
                } else {
                    [235, 235, 235, 255]
                };
            }
            if let Some((row, rows, w)) = self.blink {
                let at = base + i;
                if (row..row + rows).contains(&at) && (pos / 5) % 2 == 1 {
                    for p in line
                        .iter_mut()
                        .skip(self.cross as usize / 3)
                        .take(w as usize)
                    {
                        *p = [0, 0, 0, 255];
                    }
                }
            }
            if self.corner_rows > 0 && (i < self.corner_rows || i >= view - self.corner_rows) {
                for p in line.iter_mut() {
                    *p = [(pos & 255) as u8, (i & 255) as u8, 7, 255];
                }
            }
            lines.push(line);
        }
        for i in 0..self.footer {
            lines.push(self.line(Kind::Footer, i));
        }
        if self.overlay_scrollbar > 0 && pos > 0 {
            let total = lines.len() as u32;
            let len = (self.header.max(8) / 2).max(1);
            let top =
                (u64::from(pos) * u64::from(total - len) / u64::from(self.max_pos().max(1))) as u32;
            for i in top..top + len {
                for p in lines[i as usize]
                    .iter_mut()
                    .skip((self.cross - self.overlay_scrollbar) as usize)
                {
                    *p = [90, 90, 90, 255];
                }
            }
        }
        self.assemble(lines)
    }

    /// The image a perfect Long Shot makes: header once, every content row once, footer once.
    pub fn expected(&self) -> Frame {
        self.expected_range(0, self.content_len)
    }

    /// Header, content rows `from..to`, footer.
    pub fn expected_range(&self, from: u32, to: u32) -> Frame {
        let mut lines = Vec::new();
        let keep = (self.cross - self.scrollbar) as usize;
        for i in 0..self.header {
            lines.push(self.line(Kind::Header, i)[..keep].to_vec());
        }
        for i in from..to {
            lines.push(self.line(Kind::Content, i)[..keep].to_vec());
        }
        for i in 0..self.footer {
            lines.push(self.line(Kind::Footer, i)[..keep].to_vec());
        }
        self.assemble(lines)
    }

    fn max_pos(&self) -> u32 {
        self.content_len.saturating_sub(self.content_view())
    }

    pub fn view(self) -> PageView {
        PageView(Rc::new(RefCell::new(State {
            page: self,
            pos: 0,
            jumped: false,
            scrolls: 0,
            smooth: Vec::new(),
            pending: None,
        })))
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Header,
    Footer,
    Content,
}

struct State {
    page: SyntheticPage,
    pos: u32,
    jumped: bool,
    scrolls: u32,
    /// Smooth scrolling: the share of the scroll distance shown at each grab after a scroll.
    smooth: Vec<f64>,
    /// (start, target, next entry of `smooth`) of the scroll in flight.
    pending: Option<(u32, u32, usize)>,
}

/// One viewport onto a [`SyntheticPage`]; clone it to use it as both source and scroller.
#[derive(Clone)]
pub struct PageView(Rc<RefCell<State>>);

impl PageView {
    pub fn scrolls(&self) -> u32 {
        self.0.borrow().scrolls
    }

    /// A smooth-scrolling app on the first scroll: the next grabs show `shares` of the distance,
    /// one per grab (a repeated share looks settled; the last one stays). Later scrolls are instant.
    pub fn with_smooth(self, shares: &[f64]) -> Self {
        self.0.borrow_mut().smooth = shares.to_vec();
        self
    }
}

impl FrameSource for PageView {
    fn grab(&mut self) -> Result<Frame, CmError> {
        let mut s = self.0.borrow_mut();
        if let Some((start, target, i)) = s.pending {
            let share = s.smooth.get(i).copied().unwrap_or(1.0);
            let moved = (f64::from(target - start) * share).round() as u32;
            s.pos = start + moved;
            s.pending = (share < 1.0 && i + 1 < s.smooth.len()).then_some((start, target, i + 1));
        }
        Ok(s.page.frame_at(s.pos))
    }
}

impl Scroller for PageView {
    fn scroll(&mut self, direction: Direction, amount_px: u32) -> Result<(), CmError> {
        let mut s = self.0.borrow_mut();
        if direction != s.page.direction {
            return Err(CmError::invalid_input("scrolling the other way"));
        }
        s.scrolls += 1;
        let mut pos = s.pos + amount_px;
        if let Some((at, skip)) = s.page.jump {
            if !s.jumped && pos >= at {
                s.jumped = true;
                pos += skip;
            }
        }
        let target = pos.min(s.page.max_pos());
        if s.smooth.is_empty() || s.scrolls > 1 {
            s.pos = target;
        } else {
            s.pending = Some((s.pos, target, 0));
        }
        Ok(())
    }
}

/// A hand scroll: the viewport visits `positions` one per grab (a mid-scroll position repeated
/// less than twice is "still moving"); the last one stays.
pub struct HandScript {
    page: SyntheticPage,
    positions: VecDeque<u32>,
    last: u32,
}

impl HandScript {
    pub fn new(page: SyntheticPage, positions: Vec<u32>) -> Self {
        HandScript {
            page,
            positions: positions.into(),
            last: 0,
        }
    }

    /// Grabs left in the script.
    pub fn remaining(&self) -> usize {
        self.positions.len()
    }
}

impl FrameSource for HandScript {
    fn grab(&mut self) -> Result<Frame, CmError> {
        if let Some(p) = self.positions.pop_front() {
            self.last = p;
        }
        Ok(self.page.frame_at(self.last))
    }
}

/// A clock that does not wait; counts the time asked for.
#[derive(Debug, Default)]
pub struct NoSleep {
    pub slept: Duration,
}

impl Sleeper for NoSleep {
    fn sleep(&mut self, d: Duration) {
        self.slept += d;
    }
}

/// A source that never settles: every grab differs (an animation).
pub struct Flicker(pub u32);

impl FrameSource for Flicker {
    fn grab(&mut self) -> Result<Frame, CmError> {
        self.0 += 1;
        let v = (self.0 & 255) as u8;
        Frame::new(2, 2, vec![v; 16])
    }
}
