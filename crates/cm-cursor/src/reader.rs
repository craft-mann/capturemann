//! Reading `cursor.jsonl` back (editor EDT-4, `get_cursor_track`, tests).

use std::fs;
use std::path::Path;

use cm_core::{CmError, MediaTime};

use crate::image::ImageIndex;
use crate::schema::{CursorEvent, CursorHeader, CURSORS_DIR, SCHEMA_VERSION};

/// A parsed cursor track.
#[derive(Debug, Clone, PartialEq)]
pub struct CursorLog {
    pub header: CursorHeader,
    /// Events in time order.
    pub events: Vec<CursorEvent>,
    /// The file ended in an incomplete line (crash or power loss) that was ignored.
    pub truncated: bool,
}

impl CursorLog {
    /// Reads and parses `path`. A truncated last line is tolerated and reported through
    /// [`CursorLog::truncated`]; a malformed line anywhere else is an error, as is a missing
    /// or foreign header. Unknown event types are skipped.
    pub fn read(path: &Path) -> Result<Self, CmError> {
        let bytes =
            fs::read(path).map_err(|e| CmError::not_found(format!("{}: {e}", path.display())))?;
        Self::parse(&bytes)
    }

    /// Parses an in-memory `cursor.jsonl`.
    pub fn parse(bytes: &[u8]) -> Result<Self, CmError> {
        // Invalid UTF-8 can only come from a torn write at the very end.
        let (text, torn_utf8) = match std::str::from_utf8(bytes) {
            Ok(t) => (t, false),
            Err(e) => (
                std::str::from_utf8(&bytes[..e.valid_up_to()]).unwrap_or(""),
                true,
            ),
        };
        let ends_with_newline = !torn_utf8 && text.ends_with('\n');
        let lines: Vec<&str> = text.lines().collect();
        let mut it = lines.iter().enumerate();
        let header: CursorHeader = match it.next() {
            Some((_, l)) => serde_json::from_str(l)
                .ok()
                .filter(|h: &CursorHeader| h.ty == "session")
                .ok_or_else(|| {
                    CmError::invalid_input("cursor.jsonl: first line is not a session header")
                })?,
            None => return Err(CmError::invalid_input("cursor.jsonl is empty")),
        };
        if header.schema > SCHEMA_VERSION {
            return Err(CmError::invalid_input(format!(
                "cursor.jsonl schema {} is newer than supported {SCHEMA_VERSION}",
                header.schema
            )));
        }
        let last = lines.len() - 1;
        let mut truncated = torn_utf8;
        let mut events = Vec::with_capacity(lines.len());
        for (i, l) in it {
            if l.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<CursorEvent>(l) {
                Ok(CursorEvent::Unknown) => {}
                Ok(e) => events.push(e),
                Err(e) if i == last && !ends_with_newline => {
                    let _ = e;
                    truncated = true;
                }
                Err(e) => {
                    return Err(CmError::invalid_input(format!(
                        "cursor.jsonl line {}: {e}",
                        i + 1
                    )))
                }
            }
        }
        if !events.windows(2).all(|w| w[0].t() <= w[1].t()) {
            events.sort_by_key(CursorEvent::t);
        }
        Ok(CursorLog {
            header,
            events,
            truncated,
        })
    }

    /// Events with `from <= t < to`.
    pub fn range(&self, from: MediaTime, to: MediaTime) -> &[CursorEvent] {
        let lo = self.events.partition_point(|e| e.t() < from.ticks());
        let hi = self.events.partition_point(|e| e.t() < to.ticks());
        &self.events[lo..hi.max(lo)]
    }

    /// Iterates events with `from <= t < to`.
    pub fn iter_range(&self, from: MediaTime, to: MediaTime) -> impl Iterator<Item = &CursorEvent> {
        self.range(from, to).iter()
    }

    /// The last position event (move, down or up) at or before `t`: `(x, y, outside)`.
    pub fn position_at(&self, t: MediaTime) -> Option<(i32, i32, bool)> {
        let hi = self.events.partition_point(|e| e.t() <= t.ticks());
        self.events[..hi].iter().rev().find_map(|e| match e {
            CursorEvent::Move { x, y, outside, .. }
            | CursorEvent::Down { x, y, outside, .. }
            | CursorEvent::Up { x, y, outside, .. } => Some((*x, *y, *outside)),
            _ => None,
        })
    }

    /// Reads `<bundle>/cursors/index.json`.
    pub fn image_index(bundle: &Path) -> Result<ImageIndex, CmError> {
        ImageIndex::read(&bundle.join(CURSORS_DIR))
    }
}
