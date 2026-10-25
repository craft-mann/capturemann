//! The publish sheet's logic (PUB-2), free of UI: the form and what is wrong with it, the job's
//! progress and result, the stage of the sheet, the thumbnail frame, and the one module that
//! talks to the daemon. `ui::publish_sheet` only draws it.

pub mod backend;
pub mod cancel;
pub mod form;
pub mod job;
pub mod sheet;
pub mod thumb;
