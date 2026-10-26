//! The typed action registry: one table of names, tiers and mutability, and one [`Action`]
//! enum holding the validated input of each action.
//!
//! JSON-RPC method names equal MCP tool names (AGT-1). Parsing is strict about shapes but
//! lenient about extra keys only where noted; error messages name the field (`invalid_input`).

use cm_capture::{CaptureTarget, RegionRect};
use cm_core::{AnnotationOp, CmError, PhysicalPoint, PhysicalRect, PhysicalSize};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

mod accessors;
mod fields;
mod params;
mod parse;
mod registry;
#[cfg(test)]
mod tests;

pub(crate) use self::fields::*;
pub use self::params::*;
pub use self::registry::*;
