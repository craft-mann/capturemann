//! Destinations (STO-1): where copies go, and the file that stores them.
//!
//! The list lives in `destinations.json` in the config directory. It is written by the app
//! (Settings → Destinations); agents and the CLI only read it, so a destination (a folder path,
//! later an account) can't be added or pointed somewhere else by a socket client.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use cm_core::CmError;
use serde::{Deserialize, Serialize};

pub const DEFAULT_FOLDER_PATTERN: &str = "CaptureMann/{yyyy}/{MM}/{title}";
const FILE_NAME: &str = "destinations.json";
const VERSION: u32 = 1;

/// What a destination receives (STO-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum What {
    Export,
    Original,
    Both,
}

impl What {
    pub fn parse(s: &str) -> Option<What> {
        match s {
            "export" => Some(What::Export),
            "original" => Some(What::Original),
            "both" => Some(What::Both),
            _ => None,
        }
    }

    pub fn wants_export(self) -> bool {
        matches!(self, What::Export | What::Both)
    }

    pub fn wants_original(self) -> bool {
        matches!(self, What::Original | What::Both)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Local,
    GoogleDrive,
    #[serde(rename = "onedrive")]
    OneDrive,
    Dropbox,
}

impl Kind {
    pub fn is_cloud(self) -> bool {
        !matches!(self, Kind::Local)
    }
}

/// The account a cloud destination uploads as (STO-2). Holds no token and no secret: `account`
/// is only the key under which `cm-auth` keeps the tokens in the keychain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudAccount {
    /// The `TokenManager` account key.
    pub account: String,
    /// What the user sees: the account email.
    pub label: String,
    /// The folder uploads go below. Absent until it is created (the first upload creates
    /// `folder_name` and the daemon may store the id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_id: Option<String>,
    #[serde(default = "default_cloud_folder")]
    pub folder_name: String,
    /// STO-4.
    #[serde(default)]
    pub link_mode: crate::gdrive::LinkMode,
    /// Dropbox only: the app was registered with "App folder" access, so paths start inside
    /// `Apps/<app>` and the root folder name is not added.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub app_folder: bool,
}

fn default_cloud_folder() -> String {
    "CaptureMann".to_owned()
}

/// When a destination receives new captures by itself (STO-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoUpload {
    /// Only when asked (`upload`).
    #[default]
    Off,
    /// As soon as a recording stops normally; an export is rendered first when the destination
    /// wants one.
    OnStop,
    /// After every export of a capture.
    OnExport,
}

impl AutoUpload {
    pub fn as_str(self) -> &'static str {
        match self {
            AutoUpload::Off => "off",
            AutoUpload::OnStop => "on_stop",
            AutoUpload::OnExport => "on_export",
        }
    }

    pub fn parse(s: &str) -> Option<AutoUpload> {
        match s {
            "off" => Some(AutoUpload::Off),
            "on_stop" => Some(AutoUpload::OnStop),
            "on_export" => Some(AutoUpload::OnExport),
            _ => None,
        }
    }
}

/// One destination. Cloud kinds (STO-2) add an account reference here later.
///
/// # Migration of `auto_on_stop`
///
/// Files written before STO-3 hold `"auto_on_stop": true|false`. They still load: `true` becomes
/// [`AutoUpload::OnStop`], `false` or a missing key becomes [`AutoUpload::Off`]. An explicit
/// `auto_upload` wins over the old key. The next save writes `auto_upload` only (the old key
/// is dropped), so a downgraded app would read such a file as "off".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "RawDestination")]
pub struct Destination {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    /// Root folder (local kind only; empty for cloud kinds).
    #[serde(default)]
    pub path: PathBuf,
    /// The account (cloud kinds only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<CloudAccount>,
    pub folder_pattern: String,
    pub what: What,
    pub auto_upload: AutoUpload,
    /// The destination `upload` uses when none is named.
    pub default: bool,
}

/// The stored form, tolerant of older files (see the migration note on [`Destination`]).
#[derive(Deserialize)]
struct RawDestination {
    id: String,
    name: String,
    kind: Kind,
    #[serde(default)]
    path: PathBuf,
    #[serde(default)]
    account: Option<CloudAccount>,
    #[serde(default = "default_pattern")]
    folder_pattern: String,
    #[serde(default = "default_what")]
    what: What,
    #[serde(default)]
    auto_upload: Option<AutoUpload>,
    #[serde(default)]
    auto_on_stop: Option<bool>,
    #[serde(default)]
    default: bool,
}

impl From<RawDestination> for Destination {
    fn from(r: RawDestination) -> Destination {
        let auto_upload = r.auto_upload.unwrap_or(if r.auto_on_stop == Some(true) {
            AutoUpload::OnStop
        } else {
            AutoUpload::Off
        });
        Destination {
            id: r.id,
            name: r.name,
            kind: r.kind,
            path: r.path,
            account: r.account,
            folder_pattern: r.folder_pattern,
            what: r.what,
            auto_upload,
            default: r.default,
        }
    }
}

fn default_pattern() -> String {
    DEFAULT_FOLDER_PATTERN.to_owned()
}

fn default_what() -> What {
    // Local and NAS copies default to the full bundle (STO-5).
    What::Original
}

#[derive(Debug, Serialize, Deserialize)]
struct FileBody {
    version: u32,
    destinations: Vec<Destination>,
}

/// Reads and writes `destinations.json`.
#[derive(Debug, Clone)]
pub struct DestinationStore {
    path: PathBuf,
}

impl DestinationStore {
    pub fn new(config_dir: impl AsRef<Path>) -> Self {
        DestinationStore {
            path: config_dir.as_ref().join(FILE_NAME),
        }
    }

    /// All destinations. A missing file is an empty list; a damaged one is an error that names
    /// the file (it is not silently replaced, the user may want to repair it).
    pub fn list(&self) -> Result<Vec<Destination>, CmError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(crate::io_error("Cannot read", &self.path, &e)),
        };
        let body: FileBody = serde_json::from_str(&text).map_err(|e| CmError::Internal {
            message: format!("{} is damaged: {e}", self.path.display()),
        })?;
        Ok(body.destinations)
    }

    pub fn get(&self, id: &str) -> Result<Destination, CmError> {
        self.list()?
            .into_iter()
            .find(|d| d.id == id)
            .ok_or_else(|| CmError::NotFound {
                message: format!("no destination `{id}`"),
            })
    }

    /// The destination named `id`, or the default one, or the only one.
    pub fn resolve(&self, id: Option<&str>) -> Result<Destination, CmError> {
        if let Some(id) = id {
            return self.get(id);
        }
        let all = self.list()?;
        match all.len() {
            0 => Err(CmError::NotFound {
                message: "no destination is set up; add one in Settings → Destinations".into(),
            }),
            1 => Ok(all.into_iter().next().expect("one element")),
            _ => all.into_iter().find(|d| d.default).ok_or_else(|| CmError::InvalidInput {
                message: "several destinations exist and none is the default; name one with `destination_id`".into(),
            }),
        }
    }

    /// Replaces the whole list (temp file, then rename). Ids must be unique and at most one
    /// destination may be the default.
    pub fn save(&self, destinations: &[Destination]) -> Result<(), CmError> {
        let mut ids: Vec<&str> = destinations.iter().map(|d| d.id.as_str()).collect();
        ids.sort_unstable();
        if ids.windows(2).any(|w| w[0] == w[1]) {
            return Err(CmError::InvalidInput {
                message: "destination ids must be unique".into(),
            });
        }
        if destinations.iter().filter(|d| d.default).count() > 1 {
            return Err(CmError::InvalidInput {
                message: "only one destination can be the default".into(),
            });
        }
        let body = FileBody {
            version: VERSION,
            destinations: destinations.to_vec(),
        };
        let text = serde_json::to_string_pretty(&body).map_err(|e| CmError::Internal {
            message: e.to_string(),
        })?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| crate::io_error("Cannot create", dir, &e))?;
        }
        let temp = self.path.with_extension("json.tmp");
        let mut f =
            fs::File::create(&temp).map_err(|e| crate::io_error("Cannot write", &temp, &e))?;
        f.write_all(text.as_bytes())
            .map_err(|e| crate::io_error("Cannot write", &temp, &e))?;
        f.sync_all()
            .map_err(|e| crate::io_error("Cannot write", &temp, &e))?;
        fs::rename(&temp, &self.path).map_err(|e| crate::io_error("Cannot write", &self.path, &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dest(id: &str, default: bool) -> Destination {
        Destination {
            id: id.into(),
            name: id.into(),
            kind: Kind::Local,
            path: PathBuf::from("/Volumes/nas"),
            account: None,
            folder_pattern: DEFAULT_FOLDER_PATTERN.into(),
            what: What::Export,
            auto_upload: AutoUpload::Off,
            default,
        }
    }

    #[test]
    fn missing_file_is_empty_and_save_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let store = DestinationStore::new(tmp.path());
        assert!(store.list().unwrap().is_empty());
        store.save(&[dest("a", true), dest("b", false)]).unwrap();
        assert_eq!(
            store.list().unwrap(),
            vec![dest("a", true), dest("b", false)]
        );
        assert_eq!(store.get("b").unwrap().id, "b");
        assert!(matches!(store.get("zzz"), Err(CmError::NotFound { .. })));
    }

    #[test]
    fn resolve_picks_named_default_or_only() {
        let tmp = tempfile::tempdir().unwrap();
        let store = DestinationStore::new(tmp.path());
        assert!(matches!(store.resolve(None), Err(CmError::NotFound { .. })));
        store.save(&[dest("only", false)]).unwrap();
        assert_eq!(store.resolve(None).unwrap().id, "only");
        store.save(&[dest("a", false), dest("b", false)]).unwrap();
        assert!(matches!(
            store.resolve(None),
            Err(CmError::InvalidInput { .. })
        ));
        store.save(&[dest("a", false), dest("b", true)]).unwrap();
        assert_eq!(store.resolve(None).unwrap().id, "b");
        assert_eq!(store.resolve(Some("a")).unwrap().id, "a");
    }

    #[test]
    fn rejects_duplicate_ids_and_two_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let store = DestinationStore::new(tmp.path());
        assert!(store.save(&[dest("a", false), dest("a", false)]).is_err());
        assert!(store.save(&[dest("a", true), dest("b", true)]).is_err());
    }

    #[test]
    fn a_damaged_file_is_reported_not_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(FILE_NAME), "{ nope").unwrap();
        let err = DestinationStore::new(tmp.path()).list().unwrap_err();
        assert!(err.to_string().contains("damaged"), "{err}");
        assert_eq!(
            fs::read_to_string(tmp.path().join(FILE_NAME)).unwrap(),
            "{ nope"
        );
    }

    #[test]
    fn defaults_fill_missing_fields() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join(FILE_NAME),
            r#"{"version":1,"destinations":[{"id":"n","name":"NAS","kind":"local","path":"/x"}]}"#,
        )
        .unwrap();
        let d = DestinationStore::new(tmp.path()).get("n").unwrap();
        assert_eq!(d.folder_pattern, DEFAULT_FOLDER_PATTERN);
        assert_eq!(d.what, What::Original);
        assert!(d.auto_upload == AutoUpload::Off && !d.default);
    }

    #[test]
    fn old_auto_on_stop_files_still_load() {
        let tmp = tempfile::tempdir().unwrap();
        let load = |extra: &str| {
            let body = format!(
                r#"{{"version":1,"destinations":[{{"id":"n","name":"NAS","kind":"local","path":"/x"{extra}}}]}}"#
            );
            fs::write(tmp.path().join(FILE_NAME), body).unwrap();
            DestinationStore::new(tmp.path()).get("n").unwrap()
        };
        assert_eq!(
            load(r#","auto_on_stop":true"#).auto_upload,
            AutoUpload::OnStop
        );
        assert_eq!(
            load(r#","auto_on_stop":false"#).auto_upload,
            AutoUpload::Off
        );
        assert_eq!(
            load(r#","auto_upload":"on_export""#).auto_upload,
            AutoUpload::OnExport
        );
        // The new key wins over the old one.
        assert_eq!(
            load(r#","auto_on_stop":true,"auto_upload":"off""#).auto_upload,
            AutoUpload::Off
        );
        // Saving writes the new key only.
        let d = load(r#","auto_on_stop":true"#);
        let store = DestinationStore::new(tmp.path());
        store.save(&[d]).unwrap();
        let text = fs::read_to_string(tmp.path().join(FILE_NAME)).unwrap();
        assert!(text.contains(r#""auto_upload": "on_stop""#), "{text}");
        assert!(!text.contains("auto_on_stop"), "{text}");
    }

    #[test]
    fn auto_upload_names_round_trip() {
        for a in [AutoUpload::Off, AutoUpload::OnStop, AutoUpload::OnExport] {
            assert_eq!(AutoUpload::parse(a.as_str()), Some(a));
        }
        assert_eq!(AutoUpload::parse("nope"), None);
    }

    #[test]
    fn a_google_drive_destination_round_trips_without_secrets() {
        let tmp = tempfile::tempdir().unwrap();
        let store = DestinationStore::new(tmp.path());
        let mut d = dest("drive", false);
        d.kind = Kind::GoogleDrive;
        d.path = PathBuf::new();
        d.what = What::Export;
        d.account = Some(CloudAccount {
            account: "gdrive-1".into(),
            label: "teacher@example.org".into(),
            folder_id: Some("F1".into()),
            folder_name: "CaptureMann".into(),
            link_mode: crate::gdrive::LinkMode::None,
            app_folder: false,
        });
        store.save(&[dest("a", true), d.clone()]).unwrap();
        assert_eq!(store.get("drive").unwrap(), d);
        let text = fs::read_to_string(tmp.path().join(FILE_NAME)).unwrap();
        assert!(text.contains("\"google_drive\"") && text.contains("\"link_mode\": \"none\""));
        assert!(!text.contains("token") && !text.contains("secret"));
        assert!(!text.contains("app_folder"), "the default is not written");
    }

    #[test]
    fn onedrive_and_dropbox_destinations_round_trip_without_secrets() {
        let tmp = tempfile::tempdir().unwrap();
        let store = DestinationStore::new(tmp.path());
        let mut one = dest("one", false);
        one.kind = Kind::OneDrive;
        one.path = PathBuf::new();
        one.account = Some(CloudAccount {
            account: "onedrive-1".into(),
            label: "teacher@school.example".into(),
            folder_id: None,
            folder_name: "CaptureMann".into(),
            link_mode: crate::gdrive::LinkMode::Organization,
            app_folder: false,
        });
        let mut dbx = dest("dbx", false);
        dbx.kind = Kind::Dropbox;
        dbx.path = PathBuf::new();
        dbx.account = Some(CloudAccount {
            account: "dropbox-1".into(),
            label: "teacher@example.org".into(),
            folder_id: None,
            folder_name: "CaptureMann".into(),
            link_mode: crate::gdrive::LinkMode::Anyone,
            app_folder: true,
        });
        store
            .save(&[dest("a", true), one.clone(), dbx.clone()])
            .unwrap();
        assert_eq!(store.get("one").unwrap(), one);
        assert_eq!(store.get("dbx").unwrap(), dbx);
        assert!(Kind::OneDrive.is_cloud() && Kind::Dropbox.is_cloud());
        let text = fs::read_to_string(tmp.path().join(FILE_NAME)).unwrap();
        assert!(text.contains("\"kind\": \"onedrive\"") && text.contains("\"kind\": \"dropbox\""));
        assert!(
            text.contains("\"link_mode\": \"organization\"")
                && text.contains("\"app_folder\": true")
        );
        assert!(!text.contains("token") && !text.contains("secret"));
    }

    #[test]
    fn old_files_and_minimal_cloud_entries_still_load() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join(FILE_NAME),
            r#"{"version":1,"destinations":[
                {"id":"n","name":"NAS","kind":"local","path":"/x"},
                {"id":"g","name":"Drive","kind":"google_drive",
                 "account":{"account":"k","label":"a@b.c"}}]}"#,
        )
        .unwrap();
        let store = DestinationStore::new(tmp.path());
        assert!(store.get("n").unwrap().account.is_none());
        let g = store.get("g").unwrap();
        assert_eq!(g.kind, Kind::GoogleDrive);
        let a = g.account.unwrap();
        assert_eq!(a.folder_name, "CaptureMann");
        assert_eq!(a.link_mode, crate::gdrive::LinkMode::Anyone);
        assert!(g.path.as_os_str().is_empty());
    }
}
