//! The text-model catalogue and store for the built-in engine (AI-7, provider `embedded`).
//!
//! The catalogue (`data/text_models.json`) has the shape and the rules of the Whisper one
//! (`data/models.json`): pinned URL, SHA-256, size, memory, `pending_pin` entries that are hidden
//! and refused. This module adds only what a text model needs on top: the memory tier it is meant
//! for, its licence and the context the engine starts it with. Everything else is the Whisper
//! [`ModelStore`]: this store wraps one rooted at `<config dir>/ai/text`, so listing, the
//! resumable download with its progress callback, the checksum check, the active choice and
//! removal are the same code, not a copy.
//!
//! Nothing is downloaded unless an action calls [`TextModelStore::download`].

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use cm_core::CmError;
use serde::{Deserialize, Serialize};

use crate::model::{
    ignored_override, override_path, Catalog, CatalogEntry, DownloadOutcome, DownloadProgress,
    ModelInfo, ModelStore,
};

/// Environment variable naming a catalogue file that replaces the built-in text-model list.
pub const ENV_TEXT_CATALOG: &str = "CAPTUREMANN_TEXT_CATALOG";

const BUILTIN: &str = include_str!("../../data/text_models.json");

/// The memory tiers a catalogue entry may name, in GB.
pub const TIERS_GB: [u32; 3] = [8, 16, 32];

/// One catalogue row: the shared fields plus the text-model ones.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TextCatalogEntry {
    #[serde(flatten)]
    pub base: CatalogEntry,
    /// The smallest Mac memory this entry is meant for: 8, 16 or 32.
    pub tier_gb: u32,
    /// SPDX id of the model's licence. Only Apache-2.0 and MIT ship.
    #[serde(default)]
    pub license: String,
    /// The context the engine starts the model with.
    #[serde(default)]
    pub context_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct CatalogFile {
    models: Vec<TextCatalogEntry>,
}

/// The list of text models the app offers.
#[derive(Debug, Clone, PartialEq)]
pub struct TextCatalog {
    pub models: Vec<TextCatalogEntry>,
}

/// Licences the app may offer (the project is Apache-2.0; no copyleft or use-restricted model).
pub const ALLOWED_LICENSES: [&str; 2] = ["Apache-2.0", "MIT"];

impl TextCatalog {
    /// The embedded list.
    pub fn builtin() -> TextCatalog {
        // Checked by a unit test, so a parse failure cannot ship.
        TextCatalog::parse(BUILTIN).unwrap_or(TextCatalog { models: Vec::new() })
    }

    /// Parses and checks a catalogue: the Whisper rules (ids, file names, URLs, checksums) and,
    /// for an entry that is not `pending_pin`, a tier of 8, 16 or 32, an allowed licence, a memory
    /// figure and a context size.
    pub fn parse(text: &str) -> Result<TextCatalog, CmError> {
        let f: CatalogFile = serde_json::from_str(text)
            .map_err(|e| CmError::invalid_input(format!("text model catalog: {e}")))?;
        let c = TextCatalog { models: f.models };
        c.check()?;
        Ok(c)
    }

    fn check(&self) -> Result<(), CmError> {
        self.base().check()?;
        for m in &self.models {
            if !TIERS_GB.contains(&m.tier_gb) {
                return Err(CmError::invalid_input(format!(
                    "text model catalog: `{}` has tier_gb {} (use 8, 16 or 32)",
                    m.base.id, m.tier_gb
                )));
            }
            if m.base.pending_pin {
                continue;
            }
            if !ALLOWED_LICENSES.contains(&m.license.as_str()) {
                return Err(CmError::invalid_input(format!(
                    "text model catalog: `{}` needs an Apache-2.0 or MIT licence",
                    m.base.id
                )));
            }
            if m.base.ram_mb == 0 || m.context_tokens < 512 {
                return Err(CmError::invalid_input(format!(
                    "text model catalog: `{}` needs ram_mb and context_tokens",
                    m.base.id
                )));
            }
        }
        Ok(())
    }

    fn base(&self) -> Catalog {
        Catalog {
            models: self.models.iter().map(|m| m.base.clone()).collect(),
        }
    }

    /// The catalogue for a store root: `CAPTUREMANN_TEXT_CATALOG`, then `<root>/catalog.json`,
    /// else the built-in list. A broken override is not used; the second value says why.
    pub fn load(root: &Path) -> (TextCatalog, Option<String>) {
        let Some(path) = override_path(root, ENV_TEXT_CATALOG) else {
            return (TextCatalog::builtin(), None);
        };
        let loaded = fs::read_to_string(&path)
            .map_err(|e| CmError::invalid_input(format!("{}: {e}", path.display())))
            .and_then(|t| TextCatalog::parse(&t));
        match loaded {
            Ok(c) => (c, None),
            Err(e) => (TextCatalog::builtin(), Some(ignored_override(&path, &e))),
        }
    }
}

/// A text model as the UI and the actions show it: the shared fields plus tier, licence and
/// context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextModelInfo {
    #[serde(flatten)]
    pub info: ModelInfo,
    pub tier_gb: u32,
    pub license: String,
    pub context_tokens: u32,
}

/// The state `list_text_models` returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextModelList {
    pub models: Vec<TextModelInfo>,
    /// The model the built-in engine uses by default (the installed one chosen last).
    pub active: Option<String>,
    pub dir: String,
}

/// An installed model, resolved for the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTextModel {
    pub id: String,
    pub path: PathBuf,
    pub ram_mb: u32,
    pub context_tokens: u32,
}

/// Installed text models and the active choice, in one folder.
#[derive(Debug, Clone)]
pub struct TextModelStore {
    store: ModelStore,
    meta: HashMap<String, (u32, String, u32)>,
}

impl TextModelStore {
    /// `<config dir>/ai/text`.
    pub fn default_root() -> PathBuf {
        ModelStore::default_root().join("text")
    }

    /// Opens the store at the default root. The second value is a warning about an ignored
    /// catalogue override.
    pub fn open_default() -> (TextModelStore, Option<String>) {
        TextModelStore::open(TextModelStore::default_root())
    }

    /// Opens a store at `root` (created on first write, not now).
    pub fn open(root: impl Into<PathBuf>) -> (TextModelStore, Option<String>) {
        let root = root.into();
        let (catalog, warning) = TextCatalog::load(&root);
        (TextModelStore::with_catalog(root, catalog), warning)
    }

    /// A store with an explicit catalogue (tests, tools).
    pub fn with_catalog(root: impl Into<PathBuf>, catalog: TextCatalog) -> TextModelStore {
        let meta = catalog
            .models
            .iter()
            .map(|m| {
                (
                    m.base.id.clone(),
                    (m.tier_gb, m.license.clone(), m.context_tokens),
                )
            })
            .collect();
        TextModelStore {
            store: ModelStore::with_catalog(root, catalog.base()),
            meta,
        }
    }

    /// Every offered model (never a `pending_pin` one) with its state.
    pub fn list(&self) -> TextModelList {
        let l = self.store.list();
        TextModelList {
            models: l
                .models
                .into_iter()
                .map(|info| {
                    let (tier_gb, license, context_tokens) =
                        self.meta.get(&info.id).cloned().unwrap_or_default();
                    TextModelInfo {
                        info,
                        tier_gb,
                        license,
                        context_tokens,
                    }
                })
                .collect(),
            active: l.active,
            dir: l.dir,
        }
    }

    /// The model to run: the named one, else the active one, else the only installed one.
    /// Errors with `invalid_input` (the daemon adds `data.reason = "no_text_model"`) when none is
    /// installed.
    pub fn resolve(&self, requested: Option<&str>) -> Result<ResolvedTextModel, CmError> {
        let (id, path) = self.store.resolve(requested).map_err(|e| match e {
            CmError::InvalidInput { .. } if crate::model::is_no_model(&e) => {
                no_text_model(requested)
            }
            other => other,
        })?;
        let list = self.store.list();
        let ram_mb = list
            .models
            .iter()
            .find(|m| m.id == id)
            .map_or(0, |m| m.ram_mb);
        let context_tokens = self.meta.get(&id).map_or(0, |m| m.2);
        Ok(ResolvedTextModel {
            id,
            path,
            ram_mb,
            context_tokens,
        })
    }

    /// Downloads a model (resumable, checksum verified): the Whisper downloader. **Call it only
    /// from an explicit user or agent action.**
    pub fn download(
        &self,
        id: &str,
        progress: &mut dyn FnMut(DownloadProgress) -> bool,
    ) -> Result<DownloadOutcome, CmError> {
        self.store.download(id, progress)
    }

    pub fn select(&self, id: &str) -> Result<(), CmError> {
        self.store.select(id)
    }

    pub fn remove(&self, id: &str) -> Result<bool, CmError> {
        self.store.remove(id)
    }

    pub fn is_installed(&self, id: &str) -> bool {
        self.store.is_installed(id)
    }
}

/// The message when no text model is installed.
pub fn no_text_model(requested: Option<&str>) -> CmError {
    match requested {
        Some(id) => CmError::invalid_input(format!(
            "The text model `{id}` is not downloaded. Open the AI page to download it."
        )),
        None => CmError::invalid_input(
            "No text model is downloaded yet. Open the AI page to download one.",
        ),
    }
}

/// The model to suggest for a Mac with `ram_gb` of memory: the largest tier that fits, among the
/// models on offer. `None` when the Mac is below the smallest tier or nothing is on offer.
pub fn recommended(list: &TextModelList, ram_gb: u64) -> Option<&TextModelInfo> {
    list.models
        .iter()
        .filter(|m| u64::from(m.tier_gb) <= ram_gb)
        .max_by_key(|m| m.tier_gb)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, tier: u32, pending: bool) -> String {
        format!(
            r#"{{ "id": "{id}", "label": "L", "file": "{id}.gguf", "url": "https://example.test/{id}.gguf",
                 "sha256": "{}", "bytes": 5, "ram_mb": 100, "tier_gb": {tier}, "license": "Apache-2.0",
                 "context_tokens": 4096, "pending_pin": {pending} }}"#,
            "a".repeat(64)
        )
    }

    #[test]
    fn the_builtin_catalogue_is_pinned_and_open_licensed() {
        let c = TextCatalog::parse(BUILTIN).unwrap();
        assert_eq!(
            c.models.iter().map(|m| m.tier_gb).collect::<Vec<_>>(),
            [8, 16, 32]
        );
        assert!(c.models.iter().all(|m| !m.base.pending_pin));
        assert!(c.models.iter().all(|m| m.base.sha256.len() == 64));
        assert!(c
            .models
            .iter()
            .all(|m| m.base.url.contains("/resolve/") && m.license == "Apache-2.0"));
        let dir = tempfile::tempdir().unwrap();
        let s = TextModelStore::with_catalog(dir.path(), c);
        assert_eq!(s.list().models.len(), 3);
    }

    #[test]
    fn an_unpinned_entry_is_hidden_and_refused() {
        let pending = format!(r#"{{ "models": [{}] }}"#, entry("p", 8, true));
        let c = TextCatalog::parse(&pending).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let s = TextModelStore::with_catalog(dir.path(), c);
        assert!(s.list().models.is_empty(), "pending entries are hidden");
        let e = s.download("p", &mut |_| true).unwrap_err();
        assert!(e.to_string().contains("cannot be downloaded yet"), "{e}");
        assert!(s.select("p").is_err());
        assert!(s.remove("p").is_err());
    }

    #[test]
    fn rules_are_the_whisper_rules_plus_tier_and_licence() {
        let ok = format!(r#"{{ "models": [{}] }}"#, entry("a", 8, false));
        assert!(TextCatalog::parse(&ok).is_ok());
        // Whisper rules: duplicate ids, bad checksum.
        let dup = format!(
            r#"{{ "models": [{}, {}] }}"#,
            entry("a", 8, false),
            entry("a", 16, false)
        );
        assert!(TextCatalog::parse(&dup).is_err());
        let bad = ok.replace(&"a".repeat(64), "abc");
        assert!(TextCatalog::parse(&bad).is_err());
        // Tier and licence.
        let tier = format!(r#"{{ "models": [{}] }}"#, entry("a", 12, false));
        assert!(TextCatalog::parse(&tier).is_err());
        let lic = ok.replace("Apache-2.0", "Llama-3");
        assert!(TextCatalog::parse(&lic).is_err());
        // A pending entry skips the pin checks but still needs a valid tier.
        let pending = ok.replace("\"sha256\": \"", "\"sha256\": \"zz");
        assert!(TextCatalog::parse(&pending).is_err());
        let pending = format!(r#"{{ "models": [{}] }}"#, entry("a", 8, true))
            .replace(&"a".repeat(64), "")
            .replace("Apache-2.0", "");
        assert!(TextCatalog::parse(&pending).is_ok());
    }

    #[test]
    fn recommends_the_largest_tier_that_fits() {
        let c = TextCatalog::parse(&format!(
            r#"{{ "models": [{}, {}, {}] }}"#,
            entry("s", 8, false),
            entry("m", 16, false),
            entry("l", 32, false)
        ))
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let list = TextModelStore::with_catalog(dir.path(), c).list();
        let pick = |gb| recommended(&list, gb).map(|m| m.info.id.as_str());
        assert_eq!(pick(4), None);
        assert_eq!(pick(8), Some("s"));
        assert_eq!(pick(17), Some("m"));
        assert_eq!(pick(64), Some("l"));
        assert_eq!(list.models[1].license, "Apache-2.0");
        assert_eq!(list.models[1].context_tokens, 4096);
    }

    #[test]
    fn resolve_needs_an_installed_model() {
        let c =
            TextCatalog::parse(&format!(r#"{{ "models": [{}] }}"#, entry("s", 8, false))).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let s = TextModelStore::with_catalog(dir.path(), c);
        let e = s.resolve(None).unwrap_err();
        assert!(e.to_string().contains("No text model"), "{e}");
        let e = s.resolve(Some("s")).unwrap_err();
        assert!(e.to_string().contains("not downloaded"), "{e}");
        fs::create_dir_all(dir.path().join("models")).unwrap();
        fs::write(dir.path().join("models/s.gguf"), b"GGUF").unwrap();
        let r = s.resolve(None).unwrap();
        assert_eq!(
            (r.id.as_str(), r.ram_mb, r.context_tokens),
            ("s", 100, 4096)
        );
    }
}
