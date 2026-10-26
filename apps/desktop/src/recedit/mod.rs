//! The recording editor's logic, free of any display (EDT-1 to EDT-3, DESIGN §5.5): timeline
//! geometry and hit tests, snapping, the key map, the inspector's choices and the model that turns
//! commands into daemon calls. The GPUI views are in `ui::recording_editor`; frames for the
//! preview come from [`source`] and [`render`].

pub mod audio;
pub mod audio_export;
pub mod camera;
pub mod captions;
#[cfg(test)]
mod contract_tests;
pub mod geom;
pub mod inspector;
pub mod keys;
pub mod model;
pub mod pacer;
pub mod range;
pub mod render;
pub mod snap;
pub mod source;
pub mod viewedit;
pub mod voice;

pub use geom::{Geom, Hit, Seg, SegKind};
pub use inspector::{BgSpec, Preset};
pub use keys::Cmd;
pub use model::{Basis, EditRequest, EditorModel, Selection};
pub use snap::FrameGrid;
pub use viewedit::{PreviewAspect, ZoomAdjust};
