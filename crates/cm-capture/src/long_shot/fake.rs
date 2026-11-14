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
        }
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
                (self.cross, n)
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
        for i in 0..self.header {
            lines.push(self.line(Kind::Header, i));
        }
        for i in 0..self.content_view() {
            lines.push(self.line(Kind::Content, pos + i));
        }
        for i in 0..self.footer {
            lines.push(self.line(Kind::Footer, i));
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
        for i in 0..self.header {
            lines.push(self.line(Kind::Header, i));
        }
        for i in from..to {
            lines.push(self.line(Kind::Content, i));
        }
        for i in 0..self.footer {
            lines.push(self.line(Kind::Footer, i));
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
}

/// One viewport onto a [`SyntheticPage`]; clone it to use it as both source and scroller.
#[derive(Clone)]
pub struct PageView(Rc<RefCell<State>>);

impl PageView {
    pub fn scrolls(&self) -> u32 {
        self.0.borrow().scrolls
    }
}

impl FrameSource for PageView {
    fn grab(&mut self) -> Result<Frame, CmError> {
        let s = self.0.borrow();
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
        s.pos = pos.min(s.page.max_pos());
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
