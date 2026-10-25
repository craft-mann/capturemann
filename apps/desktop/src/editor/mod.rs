//! The annotation editor's display-free logic (ANN-1..3): coordinate mapping, tools, hit
//! testing, the interaction state machine and the daemon session. The GPUI view in
//! `ui::editor_view` only draws this state and forwards events.

pub mod geom;
pub mod keys;
pub mod preview;
pub mod redact;
pub mod session;
pub mod state;
pub mod tools;
pub mod viewport;

pub use geom::{Handle, TextMetrics};
pub use keys::{KeyCommand, Mods};
pub use redact::Proposal;
pub use state::{Chrome, Edit, EditorState, EscapeOutcome, Interaction, TextEdit};
pub use tools::{Param, ParamValue, Styles, Tool, ToolStyle};
pub use viewport::{Insets, Viewport};
