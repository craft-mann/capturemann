//! Auto scroll on an X11 session (CAP-6): the XTEST extension fakes the wheel as buttons 4 to 7
//! (4 up, 5 down, 6 left, 7 right), and the pointer is moved over the area first because the
//! wheel goes to the window under it. No permission is involved. Wayland has no such call.

use std::time::Duration;

use cm_core::CmError;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, MOTION_NOTIFY_EVENT};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

use crate::long_shot::{Direction, Scroller};

/// What one click of the wheel is first taken to move, in physical px. The real distance differs
/// per app (GTK, Qt, browsers) and is learned by the driver from the joins.
const PX_PER_CLICK: u32 = 80;
const CLICK_GAP: Duration = Duration::from_millis(3);

/// True when an X server answers and has the XTEST extension.
pub fn xtest_available() -> bool {
    let Ok((conn, _)) = RustConnection::connect(None) else {
        return false;
    };
    conn.xtest_get_version(2, 1)
        .ok()
        .and_then(|c| c.reply().ok())
        .is_some()
}

struct X11Scroller {
    conn: RustConnection,
    root: u32,
    anchor: Option<(i32, i32)>,
}

impl X11Scroller {
    fn fake(&self, kind: u8, detail: u8, x: i16, y: i16) -> Result<(), CmError> {
        self.conn
            .xtest_fake_input(kind, detail, 0, self.root, x, y, 0)
            .map_err(|e| CmError::internal(format!("XTest refused an event: {e}")))?;
        Ok(())
    }
}

impl Scroller for X11Scroller {
    fn scroll(&mut self, direction: Direction, amount_px: u32) -> Result<(), CmError> {
        self.wheel(direction, amount_px, true)
    }

    fn scroll_back(&mut self, direction: Direction, amount_px: u32) -> Result<bool, CmError> {
        self.wheel(direction, amount_px, false)?;
        Ok(true)
    }
}

impl X11Scroller {
    /// `forward`: the content moves down or to the right; otherwise up or left.
    fn wheel(
        &mut self,
        direction: Direction,
        amount_px: u32,
        forward: bool,
    ) -> Result<(), CmError> {
        if let Some((x, y)) = self.anchor {
            let (x, y) = (
                x.clamp(0, i32::from(i16::MAX)) as i16,
                y.clamp(0, i32::from(i16::MAX)) as i16,
            );
            self.fake(MOTION_NOTIFY_EVENT, 0, x, y)?;
        }
        // Forward is down (button 5) or right (button 7), back is up (4) or left (6).
        let button = match (direction, forward) {
            (Direction::Vertical, true) => 5,
            (Direction::Horizontal, true) => 7,
            (Direction::Vertical, false) => 4,
            (Direction::Horizontal, false) => 6,
        };
        for _ in 0..super::long_scroll::notches_for(amount_px, PX_PER_CLICK) {
            self.fake(BUTTON_PRESS_EVENT, button, 0, 0)?;
            self.fake(BUTTON_RELEASE_EVENT, button, 0, 0)?;
            self.conn
                .flush()
                .map_err(|e| CmError::internal(format!("the X connection failed: {e}")))?;
            std::thread::sleep(CLICK_GAP);
        }
        Ok(())
    }
}

/// The scroller of the current X11 session.
pub fn scroller(anchor: Option<(i32, i32)>) -> Result<Box<dyn Scroller + Send>, CmError> {
    let (conn, screen) = RustConnection::connect(None)
        .map_err(|e| CmError::internal(format!("cannot reach the X server: {e}")))?;
    let root = conn.setup().roots[screen].root;
    Ok(Box::new(X11Scroller { conn, root, anchor }))
}
