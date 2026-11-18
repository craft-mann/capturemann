//! Quick Capture presets in the daemon (QC-4, QC-5).
//!
//! * [`PresetStore`] keeps `presets.json` in the app's settings folder (the file format and the
//!   rules are in [`cm_core::preset`]). Every call reads the file again, so a hand edit or a
//!   shared team file is seen at once; writes go through a temp file and a rename.
//! * Actions: `list_presets` (Free, read-only, an MCP tool and a CLI command) and the plumbing for
//!   the app's own preset screens, `save_preset`, `delete_preset` and `reorder_presets`, which
//!   are internal (not an MCP tool or CLI command) and refused for agents: a preset is the user's
//!   own setup.
//! * `capture_screenshot` and `start_recording` take `preset` (a name or an id). The preset is
//!   written into the call's parameters **before** the call is checked, gated or confirmed
//!   ([`Daemon::resolve_preset_params`]), so the call equals the same call with the options
//!   written out, an explicit parameter in the same call wins, the tier and the user's
//!   `agents.tool_policy` see an ordinary call (a preset never loosens them) and the
//!   confirmation of a recording shows the resolved plan.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use cm_core::preset::{
    apply_to_recording, apply_to_screenshot, new_id, unknown_preset, Preset, PresetFile,
    AUDIO_PROFILE_NAMES, PRESETS_FILE,
};
use cm_core::CmError;
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::action::{Action, Fields};
use crate::daemon::{blocking, Daemon, Inner};
use crate::error::DaemonError;

pub(crate) const LIST: &str = "list_presets";
pub(crate) const SAVE: &str = "save_preset";
pub(crate) const DELETE: &str = "delete_preset";
pub(crate) const REORDER: &str = "reorder_presets";

/// What the preset actions do.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PresetOp {
    List,
    /// A whole preset as JSON; no `id` makes a new one, a known `id` replaces that preset.
    Save {
        preset: Value,
    },
    /// A preset id or name.
    Delete {
        preset: String,
    },
    /// Preset ids (or names) in the new order.
    Reorder {
        ids: Vec<String>,
    },
}

/// The parameters of every preset action.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PresetParams {
    #[serde(flatten)]
    pub op: PresetOp,
    pub request_id: Option<String>,
}

impl PresetOp {
    pub fn method(&self) -> &'static str {
        match self {
            PresetOp::List => LIST,
            PresetOp::Save { .. } => SAVE,
            PresetOp::Delete { .. } => DELETE,
            PresetOp::Reorder { .. } => REORDER,
        }
    }
}

pub(crate) fn handles(method: &str) -> bool {
    matches!(method, LIST | SAVE | DELETE | REORDER)
}

pub(crate) fn parse(method: &str, f: &Fields<'_>) -> Result<Action, CmError> {
    let op = match method {
        LIST => {
            f.only(&[])?;
            PresetOp::List
        }
        SAVE => {
            f.only(&["preset", "request_id"])?;
            match f.obj.get("preset") {
                Some(v @ Value::Object(_)) => PresetOp::Save { preset: v.clone() },
                Some(_) => return Err(CmError::invalid_field("preset", "must be an object")),
                None => return Err(CmError::invalid_field("preset", "is required")),
            }
        }
        DELETE => {
            f.only(&["preset", "request_id"])?;
            PresetOp::Delete {
                preset: f.str_req("preset")?,
            }
        }
        _ => {
            f.only(&["ids", "request_id"])?;
            let ids = match f.obj.get("ids") {
                Some(Value::Array(a)) => a
                    .iter()
                    .map(|v| v.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| CmError::invalid_field("ids", "must be a list of strings"))?,
                _ => return Err(CmError::invalid_field("ids", "must be a list of strings")),
            };
            PresetOp::Reorder { ids }
        }
    };
    Ok(Action::Preset(PresetParams {
        op,
        request_id: f.request_id()?,
    }))
}

pub(crate) fn summary(p: &PresetParams) -> String {
    match &p.op {
        PresetOp::List => "List the presets".to_owned(),
        PresetOp::Save { preset } => format!(
            "Save the preset \"{}\"",
            preset.get("name").and_then(Value::as_str).unwrap_or("?")
        ),
        PresetOp::Delete { preset } => format!("Delete the preset \"{preset}\""),
        PresetOp::Reorder { .. } => "Reorder the presets".to_owned(),
    }
}

/// The presets of the file, and the notice about the ones that were skipped.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedPresets {
    pub presets: Vec<Preset>,
    /// One sentence naming every skipped preset (QC-4.AC4), or a file that cannot be read.
    pub notice: Option<String>,
}

/// `presets.json` on disk.
#[derive(Debug, Clone)]
pub struct PresetStore {
    path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl PresetStore {
    /// The store for `path` (the file need not exist yet).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        PresetStore {
            path: path.into(),
            lock: Arc::new(Mutex::new(())),
        }
    }

    /// `presets.json` inside the settings folder `dir`.
    pub fn in_dir(dir: &Path) -> Self {
        Self::new(dir.join(PRESETS_FILE))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the file; a missing one is created with the four starter presets, once (QC-4.AC6:
    /// deleting them later does not bring them back, because the file then exists).
    fn read(&self) -> Result<(PresetFile, Option<String>), CmError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => PresetFile::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let f = PresetFile::with_starters();
                // A folder that cannot be written still gets the starters in memory.
                if let Err(e) = self.write(&f) {
                    log::warn!("presets: cannot create {}: {e}", self.path.display());
                }
                Ok((f, None))
            }
            Err(e) => Err(CmError::internal(format!(
                "cannot read {}: {e}",
                self.path.display()
            ))),
        }
    }

    fn write(&self, f: &PresetFile) -> Result<(), CmError> {
        let io = |e: std::io::Error| {
            CmError::internal(format!("cannot write {}: {e}", self.path.display()))
        };
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let mut tmp = self.path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let res =
            std::fs::write(&tmp, f.to_text()).and_then(|()| std::fs::rename(&tmp, &self.path));
        res.map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            io(e)
        })
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The valid presets in file order and the notice. A file that is not JSON gives no presets
    /// and a notice (and no save succeeds until it is fixed, so a hand edit is never overwritten).
    pub fn load(&self) -> LoadedPresets {
        let _g = self.guard();
        match self.read() {
            Ok((f, notice)) => LoadedPresets {
                presets: f.presets,
                notice,
            },
            Err(e) => LoadedPresets {
                presets: Vec::new(),
                notice: Some(e.to_string()),
            },
        }
    }

    /// The preset with this id or name.
    pub fn find(&self, id_or_name: &str) -> Result<Preset, CmError> {
        let _g = self.guard();
        let (f, _) = self.read()?;
        f.find(id_or_name)
            .cloned()
            .ok_or_else(|| unknown_preset(id_or_name))
    }

    /// Adds a preset (no `id`) or replaces the one with that id. Returns it as stored.
    pub fn save(&self, mut raw: Value) -> Result<Preset, CmError> {
        let _g = self.guard();
        let (mut f, _) = self.read()?;
        let obj = raw
            .as_object_mut()
            .ok_or_else(|| CmError::invalid_field("preset", "must be an object"))?;
        if obj.get("id").is_none_or(Value::is_null) {
            let name = obj.get("name").and_then(Value::as_str).unwrap_or("preset");
            let taken: Vec<String> = f.presets.iter().map(|p| p.id.clone()).collect();
            obj.insert("id".into(), json!(new_id(name, &taken)));
        }
        let preset: Preset = serde_json::from_value(raw)
            .map_err(|e| CmError::invalid_field("preset", e.to_string()))?;
        f.upsert(preset.clone())?;
        self.write(&f)?;
        Ok(preset)
    }

    /// Deletes a preset; the file keeps its "starters were written" flag.
    pub fn delete(&self, id_or_name: &str) -> Result<Preset, CmError> {
        let _g = self.guard();
        let (mut f, _) = self.read()?;
        let gone = f.remove(id_or_name)?;
        self.write(&f)?;
        Ok(gone)
    }

    pub fn reorder(&self, ids: &[String]) -> Result<Vec<Preset>, CmError> {
        let _g = self.guard();
        let (mut f, _) = self.read()?;
        f.reorder(ids)?;
        self.write(&f)?;
        Ok(f.presets)
    }
}

/// Today's date on this computer's clock (for `{date}` in a preset's folder).
pub fn local_today() -> time::Date {
    cm_library::index::to_local(time::OffsetDateTime::now_utc()).date()
}

/// A preset as the tools return it: its fields and a one-line `summary`.
pub fn preset_json(p: &Preset) -> Value {
    let mut v = serde_json::to_value(p).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        o.insert("summary".into(), json!(p.summary()));
    }
    v
}

impl Inner {
    fn preset_store(&self) -> Result<&PresetStore, DaemonError> {
        self.presets.as_ref().ok_or_else(|| {
            DaemonError::NotImplemented("presets are not available in this daemon".into())
        })
    }

    /// `list_presets`, `save_preset`, `delete_preset`, `reorder_presets`.
    pub(crate) async fn preset_op(
        &self,
        agent: bool,
        p: PresetParams,
    ) -> Result<Value, DaemonError> {
        if agent && !matches!(p.op, PresetOp::List) {
            return Err(DaemonError::Declined {
                reason: "app_only_action",
                message: "presets can only be changed in the CaptureMann app (Settings > Capture > Presets)"
                    .into(),
            });
        }
        let store = match (&p.op, &self.presets) {
            // Without a store the list is empty (headless tests, a daemon with no settings folder).
            (PresetOp::List, None) => {
                return Ok(json!({ "presets": [], "notice": null }));
            }
            _ => self.preset_store()?.clone(),
        };
        blocking(move || {
            Ok(match p.op {
                PresetOp::List => {
                    let l = store.load();
                    json!({
                        "presets": l.presets.iter().map(preset_json).collect::<Vec<_>>(),
                        "notice": l.notice,
                    })
                }
                PresetOp::Save { preset } => {
                    let saved = store.save(preset)?;
                    json!({ "preset": preset_json(&saved) })
                }
                PresetOp::Delete { preset } => {
                    let gone = store.delete(&preset)?;
                    json!({ "deleted": { "id": gone.id, "name": gone.name } })
                }
                PresetOp::Reorder { ids } => {
                    let all = store.reorder(&ids)?;
                    json!({ "presets": all.iter().map(preset_json).collect::<Vec<_>>() })
                }
            })
        })
        .await
    }

    /// Writes a call's `preset` into its parameters (see the module text). Other calls and calls
    /// without a `preset` come back unchanged.
    pub(crate) fn resolve_preset_params(
        &self,
        method: &str,
        params: &Value,
    ) -> Result<Value, CmError> {
        if !matches!(method, "capture_screenshot" | "start_recording") {
            return Ok(params.clone());
        }
        let Some(obj) = params.as_object() else {
            return Ok(params.clone());
        };
        let Some(raw) = obj.get("preset").filter(|v| !v.is_null()) else {
            return Ok(params.clone());
        };
        let name = raw
            .as_str()
            .ok_or_else(|| CmError::invalid_field("preset", "must be a preset name or id"))?;
        let is_audio_profile = method == "start_recording"
            && obj.get("mode").and_then(Value::as_str) == Some("audio")
            && AUDIO_PROFILE_NAMES.contains(&name);
        if is_audio_profile {
            // The older meaning of `start_recording.preset` (VOC-1): the audio profile.
            return Ok(params.clone());
        }
        let found = match &self.presets {
            Some(store) => store.find(name),
            None => Err(unknown_preset(name)),
        };
        let preset = match found {
            Ok(p) => p,
            // `preset: "room"` without `mode: "audio"` keeps the old error text.
            Err(_) if method == "start_recording" && AUDIO_PROFILE_NAMES.contains(&name) => {
                return Ok(params.clone())
            }
            Err(e) => return Err(e),
        };
        let mut out: Map<String, Value> = obj.clone();
        out.remove("preset");
        if method == "capture_screenshot" {
            apply_to_screenshot(&preset, &mut out)?;
        } else {
            apply_to_recording(&preset, &mut out)?;
        }
        Ok(Value::Object(out))
    }
}

impl Daemon {
    /// Writes the `preset` of a `capture_screenshot` or `start_recording` call into its
    /// parameters, as [`Daemon::execute_method`] does before it parses the call. The app calls
    /// it for a preset it runs itself, then parses and executes the result.
    pub fn resolve_preset_params(&self, method: &str, params: &Value) -> Result<Value, CmError> {
        self.inner.resolve_preset_params(method, params)
    }

    /// The preset store, when the daemon has one (`DaemonBuilder::presets`).
    pub fn preset_store(&self) -> Option<&PresetStore> {
        self.inner.presets.as_ref()
    }
}
