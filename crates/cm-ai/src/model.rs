//! Model catalog and store (engine stream). Nothing is downloaded unless the user asks.
//!
//! * The **catalog** is data (`data/models.json`, embedded): id, label, hint, file name, URL,
//!   SHA-256 and size of each Whisper ggml model. The URLs are pinned to a Hugging Face commit;
//!   the checksums and sizes were read from the repository's LFS pointer files. A file at
//!   `<root>/catalog.json`, or the path in `CAPTUREMANN_AI_CATALOG`, replaces the list, so a
//!   mirror, a new revision or a corrected checksum needs no rebuild.
//! * The **speaker models** (AI-8) are catalogue rows of kind [`ModelKind::Speaker`] from
//!   `data/speaker_models.json`: the same store, downloads, checksums, import and removal, so the
//!   `list_models`, `download_model`, `import_model` and `remove_model` actions (and their MCP tools
//!   and CLI commands) cover them. They never become the active speech model and `transcribe`
//!   refuses them. A row marked `import_only` has no download URL yet: the file is imported.
//! * The two speaker models **ship inside the app** (`Contents/Resources/Models/speaker/`, or the
//!   folder in `CM_SPEAKER_MODELS_DIR`): [`ModelStore`] treats those files as installed
//!   ([`ModelInfo::bundled`]). A copy in the user's models folder (imported or downloaded) wins; a
//!   bundled file is checked against the catalogue size and SHA-256 once per process and refused
//!   when it does not match ([`ModelStore::bundled_problem`]); it cannot be removed.
//! * The **store** lives in `<config dir>/ai` (`CAPTUREMANN_CONFIG_DIR` moves it, like the
//!   settings file): `models/<file>` for installed models, `state.json` for the active one.
//! * [`ModelStore::download`] runs only when a user or agent action calls it. It resumes a
//!   `.part` file with an HTTP `Range` request, verifies the SHA-256 and size, then renames the
//!   file into place, so an installed file is always complete. The store never starts a download
//!   by itself and has no background task.
//! * [`ModelStore::import`] copies a file the user already has, after checking the ggml magic and
//!   a size range (quantized variants are smaller than the catalog file, so the size is not
//!   compared with the catalog).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use cm_core::CmError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

/// Environment variable naming a catalog file that replaces the built-in list.
pub const ENV_CATALOG: &str = "CAPTUREMANN_AI_CATALOG";
/// Environment variable naming the folder that holds the bundled speaker models (tests, development).
pub const ENV_SPEAKER_MODELS_DIR: &str = "CM_SPEAKER_MODELS_DIR";

const BUILTIN_CATALOG: &str = include_str!("../data/models.json");
const BUILTIN_SPEAKER_CATALOG: &str = include_str!("../data/speaker_models.json");
/// First four bytes of a whisper.cpp ggml model: the little-endian magic `0x67676d6c`.
const GGML_MAGIC: [u8; 4] = [0x6c, 0x6d, 0x67, 0x67];
/// Imported files outside this range are not Whisper models (the smallest quantized `tiny` is
/// about 31 MB; `large` is under 3.2 GB).
const IMPORT_MIN_BYTES: u64 = 20_000_000;
const IMPORT_MAX_BYTES: u64 = 4_000_000_000;

/// What a model is for. Speech models are Whisper ggml files; speaker models are the ONNX files of
/// the speaker engine (AI-8).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    #[default]
    Speech,
    Speaker,
}

impl ModelKind {
    fn is_speech(&self) -> bool {
        *self == ModelKind::Speech
    }
}

/// One model as the UI and the actions show it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Stable id: `tiny`, `base`, `small`, `medium` (multilingual ggml models).
    pub id: String,
    pub label: String,
    /// Download size in MB (10^6 bytes).
    pub size_mb: u32,
    /// Memory the model needs while it runs, in MB (0 when unknown): the whisper.cpp README's
    /// figure for the model, from the catalogue.
    #[serde(default)]
    pub ram_mb: u32,
    /// A one-line "what it is good for" hint for the AI page (speed vs accuracy).
    pub hint: String,
    pub installed: bool,
    /// The model `transcribe` uses when none is named.
    pub active: bool,
    /// Where the file is when installed.
    pub path: Option<String>,
    /// Speech (Whisper) or speaker (AI-8). Older answers have no `kind`: speech.
    #[serde(default, skip_serializing_if = "ModelKind::is_speech")]
    pub kind: ModelKind,
    /// SPDX name of the model's licence, shown before a download (empty when not stated).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
    /// The file cannot be downloaded in this build; the user imports it (`import_model`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub import_only: bool,
    /// The file ships inside the app ("Included with CaptureMann"): it is installed without a
    /// download or an import and cannot be removed.
    #[serde(default, skip_serializing_if = "is_false")]
    pub bundled: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// The state `list_models` returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelList {
    pub models: Vec<ModelInfo>,
    pub active: Option<String>,
    /// The folder models are stored in.
    pub dir: String,
}

/// Progress of a user-started download (`model.download.progress`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub model: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

/// How a call to [`ModelStore::download`] ended without an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadOutcome {
    /// The model is installed at this path.
    Done(PathBuf),
    /// The progress callback returned `false`; the `.part` file is kept so a later call resumes.
    Cancelled,
}

/// One catalog row.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub hint: String,
    /// File name inside `models/` (no folders).
    pub file: String,
    pub url: String,
    /// Lowercase hex SHA-256 of the whole file.
    pub sha256: String,
    pub bytes: u64,
    /// Memory needed while it runs, in MB (the whisper.cpp README's figure); 0 when not given.
    #[serde(default)]
    pub ram_mb: u32,
    /// Other ids that name this model (`large-v3` for `large`, which is that file). Lookups by
    /// alias find the entry; the canonical `id` is what the store and the AI page use.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// The checksum and size are not pinned yet (no network when the entry was written). Such an
    /// entry is not offered by [`ModelStore::list`] and every store action on it is refused, so
    /// nothing unverified can be downloaded. The maintainer fills in `sha256`, `bytes` and
    /// `ram_mb`, confirms the URL and removes this flag.
    #[serde(default)]
    pub pending_pin: bool,
    /// Speech (default) or speaker model.
    #[serde(default)]
    pub kind: ModelKind,
    /// SPDX licence name shown before a download.
    #[serde(default)]
    pub license: String,
    /// No download URL is pinned (the file is only inside an archive the app does not unpack): the
    /// file is imported, and `sha256` and `bytes` still pin what an import must be. `url` may be
    /// empty; `source` says where the file comes from.
    #[serde(default)]
    pub import_only: bool,
    /// Where an `import_only` file comes from, for the error message and the AI page.
    #[serde(default)]
    pub source: String,
}

/// The list of models the app offers.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Catalog {
    pub models: Vec<CatalogEntry>,
}

impl Catalog {
    /// The embedded list.
    pub fn builtin() -> Catalog {
        // The embedded files are checked by a unit test, so a parse failure cannot ship.
        Catalog::parse(BUILTIN_CATALOG)
            .unwrap_or(Catalog { models: Vec::new() })
            .with_builtin_speakers()
    }

    /// Adds the built-in speaker models (AI-8) unless this list names speaker models itself. A
    /// catalogue override replaces the Whisper list and keeps the speaker rows.
    pub fn with_builtin_speakers(mut self) -> Catalog {
        if !self.models.iter().any(|m| m.kind == ModelKind::Speaker) {
            if let Ok(s) = Catalog::parse(BUILTIN_SPEAKER_CATALOG) {
                self.models.extend(s.models);
            }
        }
        self
    }

    /// Parses and checks a catalog file: unique ids, plain file names, 64-hex checksums.
    pub fn parse(text: &str) -> Result<Catalog, CmError> {
        let c: Catalog = serde_json::from_str(text)
            .map_err(|e| CmError::invalid_input(format!("model catalog: {e}")))?;
        c.check()?;
        Ok(c)
    }

    /// The rules every catalog follows (the Whisper list and the text-model list share them):
    /// unique ids, plain file names, http(s) URLs and, unless the entry is `pending_pin`, a
    /// 64-hex SHA-256 and a size.
    pub fn check(&self) -> Result<(), CmError> {
        let mut seen = Vec::new();
        for m in &self.models {
            let ok_id = !m.id.is_empty()
                && m.id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c));
            if !ok_id {
                return Err(CmError::invalid_input(format!(
                    "model catalog: bad id `{}`",
                    m.id
                )));
            }
            if seen.contains(&m.id) {
                return Err(CmError::invalid_input(format!(
                    "model catalog: duplicate id `{}`",
                    m.id
                )));
            }
            seen.push(m.id.clone());
            if m.file.is_empty() || m.file.contains(['/', '\\']) || m.file.starts_with('.') {
                return Err(CmError::invalid_input(format!(
                    "model catalog: `{}` has a bad file name",
                    m.id
                )));
            }
            let url_ok = m.url.starts_with("https://") || m.url.starts_with("http://");
            if !url_ok && !(m.import_only && m.url.is_empty()) {
                return Err(CmError::invalid_input(format!(
                    "model catalog: `{}` has a bad URL",
                    m.id
                )));
            }
            if m.pending_pin {
                // Not downloadable until pinned: the checks below do not apply yet.
                continue;
            }
            if m.sha256.len() != 64 || !m.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(CmError::invalid_input(format!(
                    "model catalog: `{}` has no valid SHA-256",
                    m.id
                )));
            }
            if m.bytes == 0 {
                return Err(CmError::invalid_input(format!(
                    "model catalog: `{}` has no size",
                    m.id
                )));
            }
        }
        Ok(())
    }

    /// The catalog for a store root: `CAPTUREMANN_AI_CATALOG`, then `<root>/catalog.json`, else
    /// the built-in list. A broken override is not used; the second value says why, so the
    /// caller can show it (the app keeps working with the built-in list).
    pub fn load(root: &Path) -> (Catalog, Option<String>) {
        let Some(path) = override_path(root, ENV_CATALOG) else {
            return (Catalog::builtin(), None);
        };
        let loaded = fs::read_to_string(&path)
            .map_err(|e| CmError::invalid_input(format!("{}: {e}", path.display())))
            .and_then(|t| Catalog::parse(&t));
        match loaded {
            Ok(c) => (c.with_builtin_speakers(), None),
            Err(e) => (Catalog::builtin(), Some(ignored_override(&path, &e))),
        }
    }

    pub fn get(&self, id: &str) -> Option<&CatalogEntry> {
        self.models.iter().find(|m| m.id == id).or_else(|| {
            self.models
                .iter()
                .find(|m| m.aliases.iter().any(|a| a == id))
        })
    }
}

/// The file that replaces a built-in list: the path in the environment variable `env`, else
/// `<root>/catalog.json` when it exists. Shared by the Whisper and the text-model catalogues.
pub(crate) fn override_path(root: &Path, env: &str) -> Option<PathBuf> {
    let from_env = std::env::var_os(env)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    from_env.or_else(|| {
        let p = root.join("catalog.json");
        p.is_file().then_some(p)
    })
}

/// The warning shown when a catalogue override is not used.
pub(crate) fn ignored_override(path: &Path, e: &CmError) -> String {
    format!("The model list at {} was ignored: {}", path.display(), e)
}

/// The app's config folder: `CAPTUREMANN_CONFIG_DIR`, else `<platform config dir>/CaptureMann`
/// (the same rule as the settings file).
pub fn config_dir() -> PathBuf {
    cm_platform::dirs::app_config_dir()
}

/// Installed models and the active choice, in one folder.
#[derive(Debug, Clone)]
pub struct ModelStore {
    root: PathBuf,
    catalog: Catalog,
    /// Folder of the speaker models that ship with the app, when this build has one.
    bundled_dir: Option<PathBuf>,
}

/// Where the speaker models shipped with the app are: `CM_SPEAKER_MODELS_DIR`, else
/// `<executable dir>/../Resources/Models/speaker` (`Contents/Resources/...` in the app bundle).
/// `None` when neither exists.
pub fn bundled_speaker_dir() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os(ENV_SPEAKER_MODELS_DIR).filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(v));
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?.join("Resources/Models/speaker");
    dir.is_dir().then_some(dir)
}

/// Bundled files already checked in this process: path -> `Ok` or what was wrong.
type BundledChecks = Vec<(PathBuf, Result<(), String>)>;
static BUNDLED_CHECKS: std::sync::Mutex<BundledChecks> = std::sync::Mutex::new(Vec::new());

impl ModelStore {
    /// `<config dir>/ai`.
    pub fn default_root() -> PathBuf {
        config_dir().join("ai")
    }

    /// Opens the store at the default root with the catalog the environment and the root select.
    /// The second value is a warning about an ignored catalog override.
    pub fn open_default() -> (ModelStore, Option<String>) {
        ModelStore::open(ModelStore::default_root())
    }

    /// Opens a store at `root` (created on first write, not now).
    pub fn open(root: impl Into<PathBuf>) -> (ModelStore, Option<String>) {
        let root = root.into();
        let (catalog, warning) = Catalog::load(&root);
        let bundled_dir = bundled_speaker_dir();
        (
            ModelStore {
                root,
                catalog,
                bundled_dir,
            },
            warning,
        )
    }

    /// A store with an explicit catalog (tests, tools).
    pub fn with_catalog(root: impl Into<PathBuf>, catalog: Catalog) -> ModelStore {
        ModelStore {
            root: root.into(),
            catalog,
            bundled_dir: None,
        }
    }

    /// The same store with another folder for the bundled speaker models (tests, tools).
    pub fn with_bundled_dir(mut self, dir: Option<PathBuf>) -> ModelStore {
        self.bundled_dir = dir;
        self
    }

    /// The bundled file of a speaker model: `Ok(None)` when this build has none, `Err` with what
    /// is wrong when it does not match the catalogue (size, SHA-256; checked once per process).
    fn bundled_file(&self, e: &CatalogEntry) -> Result<Option<PathBuf>, String> {
        if e.kind != ModelKind::Speaker {
            return Ok(None);
        }
        let Some(dir) = &self.bundled_dir else {
            return Ok(None);
        };
        let path = dir.join(&e.file);
        if !path.is_file() {
            return Ok(None);
        }
        let mut checks = BUNDLED_CHECKS.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((_, r)) = checks.iter().find(|(p, _)| *p == path) {
            return r.clone().map(|()| Some(path));
        }
        let result = (|| {
            let len = fs::metadata(&path).map_err(|err| err.to_string())?.len();
            if len != e.bytes || sha256_hex(&path).map_err(|err| err.to_string())? != e.sha256 {
                return Err(format!(
                    "the copy of `{}` that ships with CaptureMann ({}) does not match the expected \
                     file (size or SHA-256), so the app is damaged. Reinstall CaptureMann.",
                    e.id,
                    path.display()
                ));
            }
            Ok(())
        })();
        checks.push((path.clone(), result.clone()));
        result.map(|()| Some(path))
    }

    /// Why a bundled speaker model cannot be used (it fails its check), `None` when it is fine or
    /// absent.
    pub fn bundled_problem(&self, id: &str) -> Option<String> {
        let e = self.catalog.get(id)?;
        self.bundled_file(e).err()
    }

    /// Whether a good bundled copy of the model ships with the app.
    pub fn is_bundled(&self, id: &str) -> bool {
        self.catalog
            .get(id)
            .is_some_and(|e| matches!(self.bundled_file(e), Ok(Some(_))))
    }

    /// The file to use: a copy in the user's models folder, else the bundled one.
    fn locate(&self, e: &CatalogEntry) -> Option<PathBuf> {
        let user = self.file_of(e);
        if user.is_file() {
            return Some(user);
        }
        self.bundled_file(e).ok().flatten()
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// Folder holding the model files.
    pub fn models_dir(&self) -> PathBuf {
        self.root.join("models")
    }

    fn state_path(&self) -> PathBuf {
        self.root.join("state.json")
    }

    fn file_of(&self, entry: &CatalogEntry) -> PathBuf {
        self.models_dir().join(&entry.file)
    }

    fn part_of(&self, entry: &CatalogEntry) -> PathBuf {
        self.models_dir().join(format!("{}.part", entry.file))
    }

    fn entry(&self, id: &str) -> Result<&CatalogEntry, CmError> {
        let e = self.catalog.get(id).ok_or_else(|| {
            let known: Vec<&str> = self
                .catalog
                .models
                .iter()
                .filter(|m| !m.pending_pin)
                .map(|m| m.id.as_str())
                .collect();
            CmError::invalid_field(
                "model",
                format!("unknown model `{id}` (available: {})", known.join(", ")),
            )
        })?;
        if e.pending_pin {
            return Err(CmError::invalid_field(
                "model",
                format!(
                    "`{}` cannot be downloaded yet: its checksum has not been pinned in the model list",
                    e.id
                ),
            ));
        }
        Ok(e)
    }

    /// Installed means the final file exists: a download only renames a verified file into place
    /// and an import only copies a checked file.
    pub fn is_installed(&self, id: &str) -> bool {
        self.catalog
            .get(id)
            .is_some_and(|e| self.locate(e).is_some())
    }

    /// Path of an installed model.
    pub fn path_of(&self, id: &str) -> Option<PathBuf> {
        self.locate(self.catalog.get(id)?)
    }

    /// The active model id, when it is still installed.
    pub fn active(&self) -> Option<String> {
        let text = fs::read_to_string(self.state_path()).ok()?;
        let v: Value = serde_json::from_str(&text).ok()?;
        let id = v.get("active")?.as_str()?.to_owned();
        let speech = self.catalog.get(&id).is_some_and(|e| e.kind.is_speech());
        (speech && self.is_installed(&id)).then_some(id)
    }

    /// The model to use: the named one, else the active one, else the only installed one.
    /// Errors with [`no_model`] when none can be used.
    pub fn resolve(&self, requested: Option<&str>) -> Result<(String, PathBuf), CmError> {
        if let Some(id) = requested.filter(|s| !s.is_empty()) {
            let e = self.entry(id)?;
            if !e.kind.is_speech() {
                return Err(CmError::invalid_field(
                    "model",
                    format!("`{}` is a speaker model, not a speech model", e.id),
                ));
            }
            let p = self.path_of(&e.id).ok_or_else(|| no_model(Some(id)))?;
            return Ok((e.id.clone(), p));
        }
        if let Some(id) = self.active() {
            if let Some(p) = self.path_of(&id) {
                return Ok((id, p));
            }
        }
        let installed: Vec<&CatalogEntry> = self
            .catalog
            .models
            .iter()
            .filter(|m| !m.pending_pin && m.kind.is_speech() && self.file_of(m).is_file())
            .collect();
        if let [only] = installed.as_slice() {
            return Ok((only.id.clone(), self.file_of(only)));
        }
        Err(no_model(None))
    }

    /// Every catalog model with its state.
    pub fn list(&self) -> ModelList {
        let active = self.active();
        let models = self
            .catalog
            .models
            .iter()
            .filter(|e| !e.pending_pin)
            .map(|e| {
                let located = self.locate(e);
                let installed = located.is_some();
                let path = located.unwrap_or_else(|| self.file_of(e));
                ModelInfo {
                    id: e.id.clone(),
                    label: e.label.clone(),
                    size_mb: u32::try_from(e.bytes.div_ceil(1_000_000)).unwrap_or(u32::MAX),
                    ram_mb: e.ram_mb,
                    hint: e.hint.clone(),
                    installed,
                    active: installed && active.as_deref() == Some(e.id.as_str()),
                    path: installed.then(|| path.display().to_string()),
                    kind: e.kind,
                    license: e.license.clone(),
                    import_only: e.import_only,
                    bundled: matches!(self.bundled_file(e), Ok(Some(_))),
                }
            })
            .collect();
        ModelList {
            models,
            active,
            dir: self.models_dir().display().to_string(),
        }
    }

    /// Makes an installed model the default one.
    pub fn select(&self, id: &str) -> Result<(), CmError> {
        let entry = self.entry(id)?;
        if !entry.kind.is_speech() {
            return Err(CmError::invalid_field(
                "model",
                format!(
                    "`{}` is a speaker model; only speech models can be selected",
                    entry.id
                ),
            ));
        }
        let id = entry.id.clone();
        if !self.is_installed(&id) {
            return Err(CmError::invalid_field(
                "model",
                format!("`{id}` is not installed: download or import it first"),
            ));
        }
        self.write_state(Some(&id))
    }

    fn write_state(&self, active: Option<&str>) -> Result<(), CmError> {
        fs::create_dir_all(&self.root).map_err(io_error)?;
        let body = serde_json::json!({ "active": active });
        let tmp = self.root.join("state.json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(&body).unwrap_or_default()).map_err(io_error)?;
        fs::rename(&tmp, self.state_path()).map_err(io_error)
    }

    /// Deletes an installed model and any partial download of it. Returns whether a file was
    /// removed. Removing the active model clears the choice.
    pub fn remove(&self, id: &str) -> Result<bool, CmError> {
        let e = self.entry(id)?.clone();
        let was_active = self.active().as_deref() == Some(e.id.as_str());
        let mut removed = false;
        if matches!(self.bundled_file(&e), Ok(Some(_))) && !self.file_of(&e).is_file() {
            return Err(CmError::invalid_field(
                "model",
                format!(
                    "`{}` is bundled with CaptureMann and cannot be removed (reason: bundled)",
                    e.id
                ),
            ));
        }
        for p in [self.file_of(&e), self.part_of(&e)] {
            match fs::remove_file(&p) {
                Ok(()) => removed = true,
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(io_error(err)),
            }
        }
        if was_active {
            self.write_state(None)?;
        }
        Ok(removed)
    }

    /// Copies a model file the user already has into the store as `id`. The file must start with
    /// the ggml magic and be between 20 MB and 4 GB. The first installed model becomes the active
    /// one.
    pub fn import(&self, id: &str, src: &Path) -> Result<PathBuf, CmError> {
        let e = self.entry(id)?.clone();
        let meta = fs::metadata(src)
            .map_err(|err| CmError::invalid_field("path", format!("{}: {err}", src.display())))?;
        if !meta.is_file() {
            return Err(CmError::invalid_field("path", "not a file"));
        }
        if !e.kind.is_speech() {
            return self.import_exact(&e, src, meta.len());
        }
        if !(IMPORT_MIN_BYTES..=IMPORT_MAX_BYTES).contains(&meta.len()) {
            return Err(CmError::invalid_field(
                "path",
                format!(
                    "{} bytes is not the size of a Whisper model (expected 20 MB to 4 GB)",
                    meta.len()
                ),
            ));
        }
        let mut head = [0u8; 4];
        File::open(src)
            .and_then(|mut f| f.read_exact(&mut head))
            .map_err(|err| CmError::invalid_field("path", format!("cannot read it: {err}")))?;
        if head != GGML_MAGIC {
            return Err(CmError::invalid_field(
                "path",
                "not a whisper.cpp ggml model (wrong file header)",
            ));
        }
        fs::create_dir_all(self.models_dir()).map_err(io_error)?;
        let dest = self.file_of(&e);
        let tmp = self.part_of(&e);
        fs::copy(src, &tmp).map_err(io_error)?;
        fs::rename(&tmp, &dest).map_err(io_error)?;
        self.activate_if_first(&e.id)?;
        Ok(dest)
    }

    /// Import of a speaker model: the file must be exactly the pinned one (size and SHA-256), so a
    /// wrong or damaged file is refused before it is copied.
    fn import_exact(&self, e: &CatalogEntry, src: &Path, len: u64) -> Result<PathBuf, CmError> {
        if len != e.bytes || sha256_hex(src).map_err(io_error)? != e.sha256 {
            let from = if e.source.is_empty() {
                String::new()
            } else {
                format!(" It is {}.", e.source)
            };
            return Err(CmError::invalid_field(
                "path",
                format!(
                    "this is not the `{}` file this version of CaptureMann expects \
                     (expected {} bytes, SHA-256 {}).{from}",
                    e.id, e.bytes, e.sha256
                ),
            ));
        }
        fs::create_dir_all(self.models_dir()).map_err(io_error)?;
        let dest = self.file_of(e);
        let tmp = self.part_of(e);
        fs::copy(src, &tmp).map_err(io_error)?;
        fs::rename(&tmp, &dest).map_err(io_error)?;
        Ok(dest)
    }

    fn activate_if_first(&self, id: &str) -> Result<(), CmError> {
        if self.catalog.get(id).is_some_and(|e| !e.kind.is_speech()) {
            return Ok(());
        }
        if self.active().is_none() {
            self.write_state(Some(id))?;
        }
        Ok(())
    }

    /// Downloads a model. **Call this only from an explicit user or agent action** (the
    /// `download_model` action behind a confirmation): it is the one function that uses the
    /// network. `progress` gets the bytes so far and returns `false` to cancel (the `.part` file
    /// stays for a resume). A finished file is checked against the catalog size and SHA-256; a
    /// mismatch deletes the file and fails.
    ///
    /// Errors: `network` (retryable; the `.part` file is kept), `disk_full`, `internal` for a
    /// checksum mismatch (the partial file is deleted so a retry starts clean).
    pub fn download(
        &self,
        id: &str,
        progress: &mut dyn FnMut(DownloadProgress) -> bool,
    ) -> Result<DownloadOutcome, CmError> {
        let e = self.entry(id)?.clone();
        if self.file_of(&e).is_file() {
            return Ok(DownloadOutcome::Done(self.file_of(&e)));
        }
        if e.import_only {
            return Err(CmError::invalid_field(
                "model",
                format!(
                    "`{}` cannot be downloaded in this version: import the file instead \
                     (`import_model`). It is {}.",
                    e.id,
                    if e.source.is_empty() {
                        "a file you already have"
                    } else {
                        e.source.as_str()
                    }
                ),
            ));
        }
        fs::create_dir_all(self.models_dir()).map_err(io_error)?;
        let part = self.part_of(&e);
        let mut have = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        if have > e.bytes {
            // Not a prefix of this file: start over.
            let _ = fs::remove_file(&part);
            have = 0;
        }

        if have < e.bytes {
            let agent = download_agent();
            let mut req = agent.get(&e.url);
            if have > 0 {
                req = req.header("Range", &format!("bytes={have}-"));
            }
            let resp = req
                .call()
                .map_err(|err| CmError::network(format!("download failed: {err}")))?;
            let status = resp.status().as_u16();
            let resumed = match status {
                206 => true,
                200 => false,
                other => {
                    return Err(CmError::network(format!(
                        "the download server answered HTTP {other}"
                    )))
                }
            };
            if !resumed && have > 0 {
                // The server ignored the range: write the whole body from the start.
                have = 0;
            }
            let mut out = if resumed {
                OpenOptions::new().append(true).open(&part)
            } else {
                File::create(&part)
            }
            .map_err(io_error)?;
            let mut body = resp.into_body().into_reader();
            let mut buf = vec![0u8; 256 * 1024];
            let mut done = have;
            let mut last_report = std::time::Instant::now();
            loop {
                let n = match body.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(err) => {
                        let _ = out.flush();
                        return Err(CmError::network(format!("download interrupted: {err}")));
                    }
                };
                out.write_all(&buf[..n]).map_err(io_error)?;
                done += n as u64;
                if last_report.elapsed() >= Duration::from_millis(200) || done == e.bytes {
                    last_report = std::time::Instant::now();
                    let keep_going = progress(DownloadProgress {
                        model: e.id.clone(),
                        bytes_done: done,
                        bytes_total: e.bytes,
                    });
                    if !keep_going {
                        out.flush().map_err(io_error)?;
                        return Ok(DownloadOutcome::Cancelled);
                    }
                }
                if done > e.bytes {
                    break;
                }
            }
            out.flush().map_err(io_error)?;
            out.sync_all().map_err(io_error)?;
        }

        let size = fs::metadata(&part).map_err(io_error)?.len();
        if size < e.bytes {
            return Err(CmError::network(format!(
                "the download stopped at {size} of {} bytes; run it again to resume",
                e.bytes
            )));
        }
        let verified = size == e.bytes && sha256_hex(&part).map_err(io_error)? == e.sha256;
        if !verified {
            let _ = fs::remove_file(&part);
            return Err(CmError::internal(format!(
                "the downloaded `{}` file failed its checksum and was deleted; try again",
                e.id
            )));
        }
        let dest = self.file_of(&e);
        fs::rename(&part, &dest).map_err(io_error)?;
        self.activate_if_first(&e.id)?;
        let _ = progress(DownloadProgress {
            model: e.id.clone(),
            bytes_done: e.bytes,
            bytes_total: e.bytes,
        });
        Ok(DownloadOutcome::Done(dest))
    }
}

const NO_MODEL_ANY: &str = "No speech model is installed yet.";
const NO_MODEL_NAMED_END: &str =
    "model is not installed. Open the AI page to download or import it.";

/// The error `transcribe` returns when no usable model is installed: `invalid_input`. The shared
/// error type has no free-form `data`, so the daemon attaches `{"reason": "no_model"}` itself with
/// [`error_data`]; the UI turns that reason into the gentle "load a model" notice.
pub fn no_model(requested: Option<&str>) -> CmError {
    let message = match requested {
        Some(id) => format!("The `{id}` {NO_MODEL_NAMED_END}"),
        None => format!("{NO_MODEL_ANY} Open the AI page to download or import one."),
    };
    CmError::invalid_input(message)
}

/// Whether an error is the one [`no_model`] builds.
pub fn is_no_model(e: &CmError) -> bool {
    matches!(e, CmError::InvalidInput { message }
        if message.starts_with(NO_MODEL_ANY) || message.ends_with(NO_MODEL_NAMED_END))
}

/// The JSON-RPC `error.data` an AI action adds to an error: `{"reason": "no_model"}` for
/// [`no_model`], `None` for every other error (their own `data()` applies).
pub fn error_data(e: &CmError) -> Option<Value> {
    is_no_model(e).then(|| serde_json::json!({ "reason": "no_model" }))
}

/// The HTTP client for model downloads. This crate enables only ureq's `native-tls-no-default`
/// feature (the system TLS stack), but ureq defaults to Rustls and **panics** on the first `https://`
/// request ("uri scheme is https, provider is Rustls but feature is not enabled"). So the provider is
/// set to the system stack explicitly, with the platform's own root certificates.
fn download_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_per_read(Some(Duration::from_secs(45)))
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::NativeTls)
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into()
}

fn io_error(e: io::Error) -> CmError {
    if e.kind() == io::ErrorKind::StorageFull {
        CmError::disk_full("There is not enough free disk space for the model")
    } else {
        CmError::internal(format!("model store: {e}"))
    }
}

/// Lowercase hex SHA-256 of a file.
pub fn sha256_hex(path: &Path) -> io::Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The embedded catalog parses and holds the four models the AI page offers.
    #[test]
    fn builtin_catalog_is_valid() {
        let c = Catalog::parse(BUILTIN_CATALOG).unwrap();
        let ids: Vec<&str> = c.models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            ids,
            ["tiny", "base", "small", "medium", "large", "large-v3-turbo"]
        );
        assert!(c.models.iter().all(|m| m.url.contains("/resolve/")));
        assert_eq!(Catalog::builtin(), c.clone().with_builtin_speakers());
        // Every entry except the ones waiting for the maintainer's pin has a real checksum.
        for m in c.models.iter().filter(|m| !m.pending_pin) {
            assert_eq!(m.sha256.len(), 64, "{}", m.id);
            assert!(m.bytes > 0, "{}", m.id);
        }
        // `large` is large-v3 under its stable id.
        assert_eq!(c.get("large-v3").map(|m| m.id.as_str()), Some("large"));
        assert_eq!(c.get("large").unwrap().file, "ggml-large-v3.bin");
    }

    /// The turbo model is pinned (checksum and size from the LFS pointer file) and listed. An
    /// entry flagged `pending_pin` is hidden and the store refuses every action on it (the
    /// downloader never sees an entry without a checksum).
    #[test]
    fn an_unpinned_entry_is_hidden_and_refused() {
        let c = Catalog::builtin();
        let turbo = c.get("large-v3-turbo").unwrap();
        assert!(!turbo.pending_pin);
        assert_eq!(turbo.sha256.len(), 64);
        assert_eq!(turbo.bytes, 1_624_555_275);
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::with_catalog(dir.path(), c);
        let listed: Vec<String> = store
            .list()
            .models
            .into_iter()
            .filter(|m| m.kind == ModelKind::Speech)
            .map(|m| m.id)
            .collect();
        assert_eq!(
            listed,
            ["tiny", "base", "small", "medium", "large", "large-v3-turbo"]
        );
        let pending = r#"{"models":[{"id":"p","label":"P","file":"p.bin","url":"https://x/p","sha256":"","bytes":0,"pending_pin":true}]}"#;
        let store = ModelStore::with_catalog(dir.path(), Catalog::parse(pending).unwrap());
        assert!(store.list().models.is_empty());
        let mut never =
            |_p: DownloadProgress| -> bool { panic!("no network for an unpinned model") };
        let e = store.download("p", &mut never).unwrap_err();
        assert!(
            e.to_string().contains("checksum has not been pinned"),
            "{e}"
        );
        assert!(store.select("p").is_err());
        assert!(store.resolve(Some("p")).is_err());
        assert!(store.remove("p").is_err());
        // A pinned catalogue row still needs a valid checksum.
        let bad = r#"{"models":[{"id":"a","label":"A","file":"a.bin","url":"https://x/a","sha256":"","bytes":0}]}"#;
        assert!(Catalog::parse(bad).is_err());
        let pending = bad.replace("\"bytes\":0", "\"bytes\":0,\"pending_pin\":true");
        assert!(Catalog::parse(&pending).is_ok());
    }

    /// An alias finds the canonical entry everywhere the store takes an id.
    #[test]
    fn an_alias_resolves_to_the_canonical_id() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::with_catalog(dir.path(), Catalog::builtin());
        fs::create_dir_all(store.models_dir()).unwrap();
        fs::write(store.models_dir().join("ggml-large-v3.bin"), b"x").unwrap();
        assert!(store.is_installed("large-v3"));
        store.select("large-v3").unwrap();
        assert_eq!(store.active().as_deref(), Some("large"));
        assert_eq!(store.resolve(Some("large-v3")).unwrap().0, "large");
    }

    /// Regression: the first `https://` download used to panic because the agent defaulted to a TLS
    /// provider that is not compiled in. The panic fires when the connection reaches the TLS step,
    /// so a local listener accepts the TCP connection and hangs up; the request must then fail with
    /// an error (a failed handshake), not panic.
    #[test]
    fn download_agent_handles_https_without_panicking() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || drop(listener.accept()));
        let result = download_agent()
            .get(&format!("https://127.0.0.1:{port}/model.bin"))
            .call();
        server.join().unwrap();
        assert!(result.is_err(), "a hung-up TLS handshake is an error");
    }

    #[test]
    fn catalog_rejects_bad_rows() {
        let row = |extra: &str| {
            format!(
                r#"{{"models":[{{"id":"a","label":"A","file":"a.bin","url":"https://x/a","sha256":"{}","bytes":1{extra}}}]}}"#,
                "0".repeat(64)
            )
        };
        assert!(Catalog::parse(&row("")).is_ok());
        assert!(Catalog::parse(&row("").replace("a.bin", "../a.bin")).is_err());
        assert!(Catalog::parse(&row("").replace("https://x/a", "ftp://x/a")).is_err());
        assert!(Catalog::parse(&row("").replace(&"0".repeat(64), "abc")).is_err());
        assert!(Catalog::parse(&row("").replace("\"id\":\"a\"", "\"id\":\"A b\"")).is_err());
        assert!(Catalog::parse("{}").is_err());
    }

    #[test]
    fn broken_catalog_override_falls_back_with_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("catalog.json"), "{ not json").unwrap();
        let (store, warning) = ModelStore::open(dir.path());
        assert_eq!(store.catalog(), &Catalog::builtin());
        assert!(warning.unwrap().contains("catalog.json"));
    }

    #[test]
    fn root_override_replaces_the_list() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("catalog.json"),
            format!(
                r#"{{"models":[{{"id":"mine","label":"Mine","file":"m.bin","url":"http://h/m","sha256":"{}","bytes":5}}]}}"#,
                "a".repeat(64)
            ),
        )
        .unwrap();
        let (store, warning) = ModelStore::open(dir.path());
        assert!(warning.is_none());
        // The override replaces the speech list; the two speaker rows stay.
        let speech: Vec<_> = (store.list().models.into_iter())
            .filter(|m| m.kind == ModelKind::Speech)
            .collect();
        assert_eq!(speech.len(), 1);
    }

    #[test]
    fn no_model_error_carries_the_reason() {
        for e in [no_model(None), no_model(Some("small"))] {
            assert_eq!(e.code().as_str(), "invalid_input");
            assert!(is_no_model(&e));
            assert_eq!(error_data(&e).unwrap()["reason"], "no_model");
        }
        assert!(!is_no_model(&CmError::invalid_input("bad field")));
        assert!(error_data(&CmError::internal("x")).is_none());
    }
}
