//! The per-display picker state machine (CAP-1, CAP-3): pointer, drag, handles, hover, keys.
//!
//! A [`Picker`] belongs to one display and knows nothing about windows, the daemon or pixels
//! on screen. The windowing layer feeds it OS events (pointer in physical px relative to the
//! display, button, keys) and acts on the returned [`Effect`]. One picker per display is how a
//! selection can never cross displays (CAP-1.AC7): the pointer is clamped to its own display.

use cm_core::{PhysicalPoint, PhysicalRect, PhysicalSize};

use crate::selection::{
    hit_handle, is_click, is_selection, move_rect, pointer_to_line, pointer_to_pixel,
    rect_contains, Edit, Handle,
};
use crate::targets::{window_at, PickableWindow};

/// What the overlay was asked to pick (CAP-1: Display, Window, Region).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Region,
    Window,
    Display,
}

/// The user's decision on one display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// A rectangle in physical px relative to the display (CAP-1.AC2).
    Region { rect: PhysicalRect },
    /// A window picked by a click (CAP-1.AC4, AC5).
    Window { window_id: u32 },
    /// The whole display.
    Display,
}

/// Keys the picker reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Escape,
    Enter,
    /// Arrow key: nudge by 1 px, or 10 px with Shift (CAP-3).
    Arrow {
        dx: i32,
        dy: i32,
        shift: bool,
    },
    /// `C`: copy the hex color under the pointer (CAP-3.AC4).
    CopyColor,
}

/// What the caller must do after an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Nothing,
    Redraw,
    /// Close everything, nothing is saved (CAP-1.AC6).
    Cancel,
    Done(Outcome),
    /// Move the OS pointer to this display pixel (arrow keys with no selection, CAP-3).
    WarpPointer(PhysicalPoint),
    /// Copy the color of [`Picker::pixel`] to the clipboard.
    CopyColor,
}

/// Which cursor the window should show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorHint {
    Crosshair,
    Pointer,
    Move,
    Resize(Handle),
}

/// What a click would pick right now (drawn as a highlight).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hover {
    None,
    /// Index into the picker's window list.
    Window(usize),
    Display,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    /// Left button down in Region mode, drawing a new selection.
    Drawing {
        edit: Edit,
        down: PhysicalPoint,
        had_selection: bool,
    },
    Resizing {
        edit: Edit,
        before: PhysicalRect,
        handle: Handle,
    },
    Moving {
        before: PhysicalRect,
        down: PhysicalPoint,
    },
    /// Left button down in Window or Display mode, waiting for the release.
    Pressed {
        down: PhysicalPoint,
    },
}

/// Per-display picker. See the module docs.
#[derive(Debug, Clone)]
pub struct Picker {
    mode: Mode,
    size: PhysicalSize,
    /// Physical px per logical px, to size handle hit areas.
    scale: f64,
    windows: Vec<PickableWindow>,
    commit_on_release: bool,
    pointer: Option<PhysicalPoint>,
    pixel: Option<PhysicalPoint>,
    selection: Option<PhysicalRect>,
    /// The edit that produced `selection`, so arrow keys keep moving the same edge.
    last_edit: Option<Edit>,
    state: State,
}

impl Picker {
    pub fn new(
        mode: Mode,
        size: PhysicalSize,
        scale: f64,
        windows: Vec<PickableWindow>,
        commit_on_release: bool,
    ) -> Picker {
        Picker {
            mode,
            size,
            scale: if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            },
            windows,
            commit_on_release,
            pointer: None,
            pixel: None,
            selection: None,
            last_edit: None,
            state: State::Idle,
        }
    }

    /// Replaces the pickable windows (they may arrive after the picker was created).
    pub fn set_windows(&mut self, windows: Vec<PickableWindow>) {
        self.windows = windows;
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn size(&self) -> PhysicalSize {
        self.size
    }

    /// Pointer on a grid line (selection coordinates), if it is over this display.
    pub fn pointer(&self) -> Option<PhysicalPoint> {
        self.pointer
    }

    /// Pixel under the pointer: what the magnifier centers on and its readout shows.
    pub fn pixel(&self) -> Option<PhysicalPoint> {
        self.pixel
    }

    /// The selection including one being drawn, resized or moved.
    pub fn selection(&self) -> Option<PhysicalRect> {
        match self.state {
            State::Drawing { edit, .. } | State::Resizing { edit, .. } => Some(edit.rect()),
            State::Moving { before, down } => {
                let cur = self.pointer.unwrap_or(down);
                Some(move_rect(before, cur.x - down.x, cur.y - down.y, self.size))
            }
            _ => self.selection,
        }
    }

    /// True while a selection exists or is being drawn (a click then does not pick windows).
    pub fn has_selection(&self) -> bool {
        self.selection()
            .is_some_and(|r| r.size.width > 0 || r.size.height > 0)
    }

    /// True while a new selection is being drawn (handles are hidden then).
    pub fn is_drawing(&self) -> bool {
        matches!(self.state, State::Drawing { .. })
    }

    /// True between button press and release in Region mode.
    pub fn is_dragging(&self) -> bool {
        matches!(
            self.state,
            State::Drawing { .. } | State::Resizing { .. } | State::Moving { .. }
        )
    }

    /// The highlight for what a click would pick (none while a selection exists).
    pub fn hover(&self) -> Hover {
        match self.mode {
            Mode::Display => {
                if self.pointer.is_some() {
                    Hover::Display
                } else {
                    Hover::None
                }
            }
            Mode::Window | Mode::Region => {
                if self.has_selection() || self.is_dragging() {
                    return Hover::None;
                }
                match self.pixel.and_then(|p| self.window_index_at(p)) {
                    Some(i) => Hover::Window(i),
                    None => Hover::None,
                }
            }
        }
    }

    /// Rectangle (display px) of the current hover target.
    pub fn hover_rect(&self) -> Option<PhysicalRect> {
        match self.hover() {
            Hover::None => None,
            Hover::Display => Some(PhysicalRect {
                origin: PhysicalPoint { x: 0, y: 0 },
                size: self.size,
            }),
            Hover::Window(i) => self.windows.get(i).map(|w| w.rect),
        }
    }

    pub fn hover_window(&self) -> Option<&PickableWindow> {
        match self.hover() {
            Hover::Window(i) => self.windows.get(i),
            _ => None,
        }
    }

    pub fn cursor(&self) -> CursorHint {
        match self.mode {
            Mode::Window | Mode::Display => CursorHint::Pointer,
            Mode::Region => match (self.state, self.selection, self.pointer) {
                (State::Resizing { handle, .. }, _, _) => CursorHint::Resize(handle),
                (State::Moving { .. }, _, _) => CursorHint::Move,
                (State::Idle, Some(sel), Some(p)) => {
                    if let Some(h) = hit_handle(sel, p, self.grab_radius()) {
                        CursorHint::Resize(h)
                    } else if rect_contains(sel, p) {
                        CursorHint::Move
                    } else {
                        CursorHint::Crosshair
                    }
                }
                _ => CursorHint::Crosshair,
            },
        }
    }

    fn grab_radius(&self) -> i32 {
        (6.0 * self.scale).round() as i32
    }

    fn window_index_at(&self, p: PhysicalPoint) -> Option<usize> {
        let w = window_at(&self.windows, p)?;
        self.windows.iter().position(|x| x.id == w.id)
    }

    /// Pointer moved to `(x, y)` physical px relative to the display (may be outside it while a
    /// button is held; it is clamped).
    pub fn pointer_moved(&mut self, x: f64, y: f64) -> Effect {
        let line = pointer_to_line(x, y, self.size);
        self.pointer = Some(line);
        self.pixel = Some(pointer_to_pixel(x, y, self.size));
        match self.state {
            State::Drawing {
                edit,
                down,
                had_selection,
            } => {
                self.state = State::Drawing {
                    edit: edit.drag_to(line, self.size),
                    down,
                    had_selection,
                };
            }
            State::Resizing {
                edit,
                before,
                handle,
            } => {
                self.state = State::Resizing {
                    edit: edit.drag_to(line, self.size),
                    before,
                    handle,
                };
            }
            _ => {}
        }
        Effect::Redraw
    }

    /// Pointer left the display (no button held).
    pub fn pointer_left(&mut self) -> Effect {
        if !self.is_dragging() {
            self.pointer = None;
            self.pixel = None;
        }
        Effect::Redraw
    }

    /// Left button pressed at the current pointer position.
    pub fn press(&mut self) -> Effect {
        let Some(p) = self.pointer else {
            return Effect::Nothing;
        };
        if self.mode != Mode::Region {
            self.state = State::Pressed { down: p };
            return Effect::Nothing;
        }
        if let Some(sel) = self.selection {
            if let Some(h) = hit_handle(sel, p, self.grab_radius()) {
                self.state = State::Resizing {
                    edit: Edit::resize(sel, h),
                    before: sel,
                    handle: h,
                };
                return Effect::Redraw;
            }
            if rect_contains(sel, p) {
                self.state = State::Moving {
                    before: sel,
                    down: p,
                };
                return Effect::Redraw;
            }
        }
        self.state = State::Drawing {
            edit: Edit::draw(p),
            down: p,
            had_selection: self.selection.is_some(),
        };
        self.selection = None;
        Effect::Redraw
    }

    /// Left button released.
    pub fn release(&mut self) -> Effect {
        let state = std::mem::replace(&mut self.state, State::Idle);
        let up = self.pointer;
        match state {
            State::Idle => Effect::Nothing,
            State::Pressed { down } => match up {
                Some(up) if is_click(down, up) => self.pick_hover(),
                _ => Effect::Redraw,
            },
            State::Drawing {
                edit,
                down,
                had_selection,
            } => {
                let rect = edit.rect();
                if up.is_some_and(|u| is_click(down, u)) {
                    // CAP-1.AC5: a click is not a selection. With no earlier selection it
                    // picks the window under the pointer; otherwise it only clears.
                    self.selection = None;
                    self.last_edit = None;
                    return if had_selection {
                        Effect::Redraw
                    } else {
                        self.pick_hover()
                    };
                }
                if !is_selection(rect) {
                    self.selection = None;
                    self.last_edit = None;
                    return Effect::Redraw;
                }
                self.selection = Some(rect);
                self.last_edit = Some(edit);
                self.after_adjust(rect)
            }
            State::Resizing { edit, before, .. } => {
                let rect = edit.rect();
                if is_selection(rect) {
                    self.selection = Some(rect);
                    self.last_edit = Some(edit);
                    self.after_adjust(rect)
                } else {
                    self.selection = Some(before);
                    Effect::Redraw
                }
            }
            State::Moving { before, down } => {
                let cur = up.unwrap_or(down);
                let rect = move_rect(before, cur.x - down.x, cur.y - down.y, self.size);
                self.selection = Some(rect);
                self.last_edit = None;
                self.after_adjust(rect)
            }
        }
    }

    fn after_adjust(&self, rect: PhysicalRect) -> Effect {
        if self.commit_on_release {
            Effect::Done(Outcome::Region { rect })
        } else {
            Effect::Redraw
        }
    }

    fn pick_hover(&self) -> Effect {
        match self.mode {
            Mode::Display => Effect::Done(Outcome::Display),
            Mode::Window | Mode::Region => match self.hover_window() {
                Some(w) => Effect::Done(Outcome::Window { window_id: w.id }),
                None => Effect::Redraw,
            },
        }
    }

    /// A key press.
    pub fn key(&mut self, key: Key) -> Effect {
        match key {
            Key::Escape => Effect::Cancel,
            Key::CopyColor => {
                if self.pixel.is_some() {
                    Effect::CopyColor
                } else {
                    Effect::Nothing
                }
            }
            Key::Enter => match self.selection {
                Some(rect) if self.mode == Mode::Region && !self.is_dragging() => {
                    Effect::Done(Outcome::Region { rect })
                }
                _ => match self.mode {
                    Mode::Region | Mode::Window if self.hover_window().is_none() => Effect::Nothing,
                    _ => self.pick_hover(),
                },
            },
            Key::Arrow { dx, dy, shift } => self.nudge(dx, dy, if shift { 10 } else { 1 }),
        }
    }

    fn nudge(&mut self, dx: i32, dy: i32, step: i32) -> Effect {
        if self.mode != Mode::Region {
            return Effect::Nothing;
        }
        let (dx, dy) = (dx * step, dy * step);
        match self.state {
            State::Drawing {
                edit,
                down,
                had_selection,
            } => {
                self.state = State::Drawing {
                    edit: edit.nudge(dx, dy, self.size),
                    down,
                    had_selection,
                };
                Effect::Redraw
            }
            State::Resizing {
                edit,
                before,
                handle,
            } => {
                self.state = State::Resizing {
                    edit: edit.nudge(dx, dy, self.size),
                    before,
                    handle,
                };
                Effect::Redraw
            }
            State::Moving { .. } | State::Pressed { .. } => Effect::Nothing,
            State::Idle => {
                if let Some(sel) = self.selection {
                    if let Some(edit) = self.last_edit {
                        let edit = edit.nudge(dx, dy, self.size);
                        let rect = edit.rect();
                        if is_selection(rect) {
                            self.last_edit = Some(edit);
                            self.selection = Some(rect);
                        }
                    } else {
                        self.selection = Some(move_rect(sel, dx, dy, self.size));
                    }
                    return Effect::Redraw;
                }
                // No selection: move the pointer itself by whole pixels.
                match self.pointer {
                    Some(p) => {
                        let np = PhysicalPoint {
                            x: (p.x + dx).clamp(0, self.size.width as i32 - 1),
                            y: (p.y + dy).clamp(0, self.size.height as i32 - 1),
                        };
                        self.pointer = Some(np);
                        self.pixel = Some(np);
                        Effect::WarpPointer(np)
                    }
                    None => Effect::Nothing,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: PhysicalSize = PhysicalSize {
        width: 3024,
        height: 1964,
    };

    fn r(x: i32, y: i32, w: u32, h: u32) -> PhysicalRect {
        PhysicalRect {
            origin: PhysicalPoint { x, y },
            size: PhysicalSize {
                width: w,
                height: h,
            },
        }
    }

    fn win(id: u32, rect: PhysicalRect) -> PickableWindow {
        PickableWindow {
            id,
            title: String::new(),
            app: String::new(),
            rect,
        }
    }

    fn region(windows: Vec<PickableWindow>) -> Picker {
        Picker::new(Mode::Region, SIZE, 2.0, windows, false)
    }

    fn drag(p: &mut Picker, from: (f64, f64), to: (f64, f64)) -> Effect {
        p.pointer_moved(from.0, from.1);
        p.press();
        p.pointer_moved(to.0, to.1);
        p.release()
    }

    /// CAP-1.AC2: drag 420x649 at (2207, 669), confirm with Enter: that exact region.
    #[test]
    fn cap1_ac2_drag_then_enter_returns_the_physical_rect() {
        let mut p = region(vec![]);
        assert_eq!(
            drag(&mut p, (2207.0, 669.0), (2627.0, 1318.0)),
            Effect::Redraw
        );
        assert_eq!(p.selection(), Some(r(2207, 669, 420, 649)));
        assert_eq!(
            p.key(Key::Enter),
            Effect::Done(Outcome::Region {
                rect: r(2207, 669, 420, 649)
            })
        );
    }

    #[test]
    fn commit_on_release_finishes_without_enter() {
        let mut p = Picker::new(Mode::Region, SIZE, 1.0, vec![], true);
        assert_eq!(
            drag(&mut p, (10.0, 10.0), (110.0, 60.0)),
            Effect::Done(Outcome::Region {
                rect: r(10, 10, 100, 50)
            })
        );
    }

    /// CAP-1.AC6: Esc cancels, with or without a selection.
    #[test]
    fn cap1_ac6_escape_cancels() {
        let mut p = region(vec![]);
        assert_eq!(p.key(Key::Escape), Effect::Cancel);
        drag(&mut p, (10.0, 10.0), (110.0, 60.0));
        assert_eq!(p.key(Key::Escape), Effect::Cancel);
    }

    /// CAP-1.AC5: hovering highlights the window; a click (< 4 px) captures it.
    #[test]
    fn cap1_ac5_click_without_drag_captures_the_hovered_window() {
        let mut p = region(vec![win(7, r(100, 100, 800, 600))]);
        p.pointer_moved(300.0, 300.0);
        assert_eq!(p.hover(), Hover::Window(0));
        assert_eq!(p.hover_rect(), Some(r(100, 100, 800, 600)));
        p.press();
        p.pointer_moved(302.0, 301.0);
        assert_eq!(p.release(), Effect::Done(Outcome::Window { window_id: 7 }));
    }

    /// CAP-1.AC5: 4 px of movement is a drag, not a click.
    #[test]
    fn cap1_ac5_four_px_of_movement_is_a_selection_not_a_click() {
        let mut p = region(vec![win(7, r(100, 100, 800, 600))]);
        let e = drag(&mut p, (300.0, 300.0), (304.0, 304.0));
        assert_eq!(e, Effect::Redraw);
        assert_eq!(p.selection(), Some(r(300, 300, 4, 4)));
    }

    #[test]
    fn click_on_empty_desktop_does_nothing() {
        let mut p = region(vec![win(7, r(100, 100, 50, 50))]);
        assert_eq!(drag(&mut p, (900.0, 900.0), (900.0, 900.0)), Effect::Redraw);
        assert_eq!(p.selection(), None);
    }

    #[test]
    fn a_click_with_an_existing_selection_only_clears_it() {
        let mut p = region(vec![win(7, r(0, 0, 3000, 1900))]);
        drag(&mut p, (500.0, 500.0), (700.0, 700.0));
        assert!(p.selection().is_some());
        assert_eq!(
            drag(&mut p, (1500.0, 1500.0), (1500.0, 1500.0)),
            Effect::Redraw
        );
        assert_eq!(p.selection(), None);
        // The next click picks the window again.
        assert_eq!(
            drag(&mut p, (1500.0, 1500.0), (1500.0, 1500.0)),
            Effect::Done(Outcome::Window { window_id: 7 })
        );
    }

    #[test]
    fn hover_is_off_while_a_selection_exists() {
        let mut p = region(vec![win(7, r(0, 0, 3000, 1900))]);
        drag(&mut p, (500.0, 500.0), (700.0, 700.0));
        p.pointer_moved(1500.0, 1500.0);
        assert_eq!(p.hover(), Hover::None);
    }

    #[test]
    fn selection_under_two_px_is_discarded() {
        let mut p = region(vec![]);
        drag(&mut p, (100.0, 100.0), (300.0, 101.0));
        assert_eq!(p.selection(), None);
    }

    /// CAP-1.AC7: dragging past the display edge stops at the edge; the selection stays on
    /// its own display.
    #[test]
    fn cap1_ac7_drag_past_the_edge_is_clamped_to_the_display() {
        let mut p = region(vec![]);
        drag(&mut p, (3000.0, 1900.0), (4500.0, 2500.0));
        assert_eq!(p.selection(), Some(r(3000, 1900, 24, 64)));
    }

    #[test]
    fn dragging_a_handle_resizes_and_dragging_inside_moves() {
        let mut p = region(vec![]);
        drag(&mut p, (100.0, 100.0), (500.0, 400.0));
        // Grab the east handle (500, 250) and drag to 700.
        assert_eq!(drag(&mut p, (500.0, 250.0), (700.0, 250.0)), Effect::Redraw);
        assert_eq!(p.selection(), Some(r(100, 100, 600, 300)));
        // Drag inside by (+50, +20).
        drag(&mut p, (300.0, 250.0), (350.0, 270.0));
        assert_eq!(p.selection(), Some(r(150, 120, 600, 300)));
    }

    #[test]
    fn cursor_hint_follows_handles_and_interior() {
        let mut p = region(vec![]);
        assert_eq!(p.cursor(), CursorHint::Crosshair);
        drag(&mut p, (100.0, 100.0), (500.0, 400.0));
        p.pointer_moved(500.0, 400.0);
        assert_eq!(p.cursor(), CursorHint::Resize(Handle::SouthEast));
        p.pointer_moved(300.0, 250.0);
        assert_eq!(p.cursor(), CursorHint::Move);
        p.pointer_moved(900.0, 900.0);
        assert_eq!(p.cursor(), CursorHint::Crosshair);
    }

    /// CAP-3.AC2: during a drag, five right-arrow presses move the right edge 5 px.
    #[test]
    fn cap3_ac2_arrows_during_a_drag_move_the_edge_exactly() {
        let mut p = region(vec![]);
        p.pointer_moved(100.0, 100.0);
        p.press();
        p.pointer_moved(300.0, 250.0);
        for _ in 0..5 {
            p.key(Key::Arrow {
                dx: 1,
                dy: 0,
                shift: false,
            });
        }
        assert_eq!(p.selection(), Some(r(100, 100, 205, 150)));
        p.key(Key::Arrow {
            dx: -1,
            dy: 0,
            shift: true,
        });
        assert_eq!(p.selection(), Some(r(100, 100, 195, 150)));
    }

    #[test]
    fn arrows_after_release_keep_moving_the_same_edge() {
        let mut p = region(vec![]);
        drag(&mut p, (100.0, 100.0), (300.0, 250.0));
        p.key(Key::Arrow {
            dx: 0,
            dy: 1,
            shift: false,
        });
        assert_eq!(p.selection(), Some(r(100, 100, 200, 151)));
    }

    /// CAP-3: with nothing selected, arrows move the pointer one pixel.
    #[test]
    fn cap3_arrows_without_a_selection_warp_the_pointer() {
        let mut p = region(vec![]);
        p.pointer_moved(100.0, 200.0);
        let e = p.key(Key::Arrow {
            dx: 1,
            dy: 0,
            shift: true,
        });
        assert_eq!(e, Effect::WarpPointer(PhysicalPoint { x: 110, y: 200 }));
        assert_eq!(p.pixel(), Some(PhysicalPoint { x: 110, y: 200 }));
    }

    /// CAP-3.AC1: the magnifier pixel is the pixel under the pointer.
    #[test]
    fn cap3_ac1_pixel_under_pointer() {
        let mut p = region(vec![]);
        p.pointer_moved(100.4, 200.6);
        assert_eq!(p.pixel(), Some(PhysicalPoint { x: 100, y: 200 }));
        assert_eq!(p.key(Key::CopyColor), Effect::CopyColor);
    }

    #[test]
    fn window_mode_picks_on_click_and_ignores_drags() {
        let mut p = Picker::new(
            Mode::Window,
            SIZE,
            1.0,
            vec![win(3, r(0, 0, 500, 500))],
            false,
        );
        p.pointer_moved(10.0, 10.0);
        p.press();
        p.pointer_moved(200.0, 200.0);
        assert_eq!(p.release(), Effect::Redraw);
        p.pointer_moved(10.0, 10.0);
        p.press();
        assert_eq!(p.release(), Effect::Done(Outcome::Window { window_id: 3 }));
        assert_eq!(p.selection(), None);
    }

    #[test]
    fn display_mode_click_picks_the_display() {
        let mut p = Picker::new(Mode::Display, SIZE, 1.0, vec![], false);
        p.pointer_moved(10.0, 10.0);
        assert_eq!(p.hover(), Hover::Display);
        p.press();
        assert_eq!(p.release(), Effect::Done(Outcome::Display));
    }

    #[test]
    fn enter_in_region_mode_without_selection_does_nothing() {
        let mut p = region(vec![]);
        assert_eq!(p.key(Key::Enter), Effect::Nothing);
    }
}
