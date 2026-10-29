//! Primitives (MediaTime, geometry), documents, actions, commands, undo, revisions, shared error codes.
//!
//! Headless and free of file or network I/O: everything here is plain data plus logic, with serde as the
//! only serialization surface. The one OS call is reading the host clock for the recording session clock.

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod action;
pub mod annotation;
pub mod camera_layout;
pub mod clock;
pub mod edit;
pub mod edit_view;
pub mod error;
pub mod geometry;
pub mod guard;
pub mod idempotency;
pub mod logfile;
pub mod logging;
pub mod media_time;
pub mod message;
pub mod record;
pub mod undo;
pub mod wav;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use action::{Actor, Revision};
pub use annotation::{
    parse_ops, parse_ops_str, validate_op, visible_rect, AnnotationOp, Color, OpKind, Pt, Rc,
    BLUR_STRENGTH_RANGE, MAX_TEXT_CHARS, PIXELATE_BLOCK_RANGE,
};
pub use camera_layout::{
    layout_at, BubbleKind, CameraLayoutSpec, CameraMode, LayoutAt, LayoutSegment, LayoutTransition,
};
pub use clock::{HostClock, ManualHostClock, SessionClock, SystemHostClock};
pub use edit::{
    parse_time, session_to_file, track_time, AudioMix, AudioMixUpdate, Background, BackgroundKind,
    BackgroundUpdate, EditContext, EditList, Piece, Sample, Span, SpeedSpan, Timeline,
    EDIT_VERSION, MIN_OUTPUT,
};
pub use edit_view::{
    Aspect, ClickEffect, CursorStyle, CursorStyleUpdate, Reframe, ReframeMode, ReframeUpdate,
    ReframeWindow, Smoothing, ViewEdits, ZoomRegion, CURSOR_SIZE_RANGE, IDLE_HIDE,
    ZOOM_DEFAULT_SCALE, ZOOM_EASE, ZOOM_MIN_LENGTH, ZOOM_SCALE_RANGE,
};
pub use error::{CmError, ErrorCode, ErrorPayload, Permission};
pub use geometry::{
    NormPoint, NormRect, PhysicalPoint, PhysicalRect, PhysicalSize, Point, Rect, ScaleFactor, Size,
};
pub use idempotency::IdempotencyCache;
pub use media_time::{Fps, MediaTime, TICKS_PER_SECOND};
pub use record::{PauseGap, TrackContext, TrackInfo, TrackKind, TrackRecorder, TrackStats};
pub use undo::{Action, Command, UndoStack, DEFAULT_REVISION_LIMIT};
