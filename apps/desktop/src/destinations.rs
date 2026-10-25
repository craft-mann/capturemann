//! Settings → Destinations, the local-folder part (STO-1): the rules for adding, editing,
//! defaulting and removing a destination, kept free of UI so they can be tested.
//!
//! The app is the only writer of `destinations.json` (through `cm_storage::DestinationStore`);
//! the daemon and agents read it. Nothing here deletes files: removing a destination only
//! forgets it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use cm_i18n::{tr, tr_ctx};
use cm_storage::{AutoUpload, Destination, Kind, LinkMode, What, DEFAULT_FOLDER_PATTERN};

pub const MAX_NAME_CHARS: usize = 60;
const TOKENS: [&str; 4] = ["{yyyy}", "{MM}", "{dd}", "{title}"];

/// Why a destination rule refused a value. The words are made when the reason is drawn
/// (`Display`), so a line that stays on screen follows a language switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    NameMissing,
    NameTooLong(usize),
    NameTaken(String),
    PatternEmpty,
    /// Starts with a separator or a drive letter.
    PatternAbsolute,
    PatternDotDot,
    PatternTokens,
    FolderNotAbsolute,
    FolderMissing(String),
    NotAFolder(String),
    /// The folder is already this destination.
    FolderTaken(String),
    DestinationGone,
    /// The provider named `name` cannot make this kind of link.
    LinkNotOffered {
        name: String,
        link: LinkMode,
    },
    /// An OS or file error, as it came.
    Os(String),
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self {
            RuleError::NameMissing => tr!("Give the destination a name."),
            RuleError::NameTooLong(max) => {
                tr!("Keep the name to {max} characters.", max = max)
            }
            RuleError::NameTaken(name) => tr!(
                "There is already a destination named \u{201c}{name}\u{201d}.",
                name = name
            ),
            RuleError::PatternEmpty => tr!("The folder pattern can\u{2019}t be empty."),
            RuleError::PatternAbsolute => tr!(
                "The pattern is a path below the folder, so it can\u{2019}t start with / or a drive letter."
            ),
            RuleError::PatternDotDot => {
                tr!("The pattern can\u{2019}t contain \u{201c}..\u{201d}.")
            }
            RuleError::PatternTokens => {
                tr!("Use only {yyyy}, {MM}, {dd} and {title} between braces.")
            }
            RuleError::FolderNotAbsolute => tr!("Choose a folder by its full path."),
            RuleError::FolderMissing(path) => tr!("{path} does not exist.", path = path),
            RuleError::NotAFolder(path) => tr!("{path} is not a folder.", path = path),
            RuleError::FolderTaken(name) => tr!(
                "That folder is already the destination \u{201c}{name}\u{201d}.",
                name = name
            ),
            RuleError::DestinationGone => tr!("That destination no longer exists."),
            RuleError::LinkNotOffered { name, link } => tr!(
                "{name} can\u{2019}t make \u{201c}{link}\u{201d} links.",
                name = name,
                link = link_label(*link)
            ),
            RuleError::Os(e) => e.clone(),
        })
    }
}

/// What the edit form changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub name: String,
    pub folder_pattern: String,
    pub what: What,
    /// Off, after recording or after export (STO-3).
    pub auto_upload: AutoUpload,
    /// Who can open the link after an upload (STO-4); only a cloud destination has one.
    pub link_mode: LinkMode,
}

impl Edit {
    pub fn of(d: &Destination) -> Edit {
        Edit {
            name: d.name.clone(),
            folder_pattern: d.folder_pattern.clone(),
            what: d.what,
            auto_upload: d.auto_upload,
            link_mode: d.account.as_ref().map(|a| a.link_mode).unwrap_or_default(),
        }
    }
}

/// Lower-case ASCII letters and digits joined by single `_`; empty when nothing is left.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_owned()
}

/// `dest_<slug>`, with `_2`, `_3`, ... when taken. A name with no usable letters gives `dest_local`.
pub fn unique_id(name: &str, existing: &[Destination]) -> String {
    let base = match slug(name) {
        s if s.is_empty() => "local".to_owned(),
        s => s,
    };
    let taken = |id: &str| existing.iter().any(|d| d.id == id);
    let first = format!("dest_{base}");
    if !taken(&first) {
        return first;
    }
    (2..)
        .map(|n| format!("dest_{base}_{n}"))
        .find(|id| !taken(id))
        .expect("an unbounded range has a free id")
}

fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// `base`, or `base (2)`, `base (3)`, ... so no two destinations share a name.
pub fn unique_name(base: &str, existing: &[Destination]) -> String {
    let base = base.trim();
    let base = if base.is_empty() {
        "Local folder" // i18n-ignore: stored as the destination's name
    } else {
        base
    };
    let taken = |n: &str| existing.iter().any(|d| same_name(&d.name, n));
    if !taken(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base} ({n})"))
        .find(|n| !taken(n))
        .expect("an unbounded range has a free name")
}

/// A trimmed, non-empty name of at most 60 characters that no other destination uses.
pub fn validate_name(name: &str, others: &[&Destination]) -> Result<String, RuleError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(RuleError::NameMissing);
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(RuleError::NameTooLong(MAX_NAME_CHARS));
    }
    if others.iter().any(|d| same_name(&d.name, name)) {
        return Err(RuleError::NameTaken(name.to_owned()));
    }
    Ok(name.to_owned())
}

/// The folder pattern under the destination root: `/` or `\` separated, made of text and the
/// tokens `{yyyy}`, `{MM}`, `{dd}`, `{title}`. It may not leave the root.
pub fn validate_pattern(pattern: &str) -> Result<String, RuleError> {
    let p = pattern.trim();
    if p.is_empty() {
        return Err(RuleError::PatternEmpty);
    }
    if p.starts_with(['/', '\\']) || p.as_bytes().get(1) == Some(&b':') {
        return Err(RuleError::PatternAbsolute);
    }
    if p.split(['/', '\\']).any(|s| s.trim() == "..") {
        return Err(RuleError::PatternDotDot);
    }
    let mut rest = p;
    while let Some(i) = rest.find('{') {
        let tail = &rest[i..];
        match TOKENS.iter().find(|t| tail.starts_with(**t)) {
            Some(t) => rest = &tail[t.len()..],
            None => return Err(RuleError::PatternTokens),
        }
    }
    if rest.contains('}') {
        return Err(RuleError::PatternTokens);
    }
    Ok(p.to_owned())
}

/// The chosen folder must be an absolute path to an existing folder.
pub fn check_folder(path: &Path) -> Result<PathBuf, RuleError> {
    if !path.is_absolute() {
        return Err(RuleError::FolderNotAbsolute);
    }
    if !path.exists() {
        return Err(RuleError::FolderMissing(path.display().to_string()));
    }
    if !path.is_dir() {
        return Err(RuleError::NotAFolder(path.display().to_string()));
    }
    Ok(path.to_path_buf())
}

fn same_folder(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// A suggested name for a folder: its own name, or `Local folder` for a root.
pub fn name_for_folder(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Local folder".to_owned()) // i18n-ignore: stored as the destination's name
}

/// Adds a local folder with the defaults (STO-5: a local copy takes the original bundle). The
/// first destination becomes the default. Returns the new id.
pub fn add_folder(list: &mut Vec<Destination>, path: &Path) -> Result<String, RuleError> {
    let path = check_folder(path)?;
    if let Some(d) = list.iter().find(|d| same_folder(&d.path, &path)) {
        return Err(RuleError::FolderTaken(d.name.clone()));
    }
    let name = unique_name(&name_for_folder(&path), list);
    let id = unique_id(&name, list);
    let default = list.is_empty();
    list.push(Destination {
        id: id.clone(),
        name,
        kind: Kind::Local,
        path,
        account: None,
        folder_pattern: DEFAULT_FOLDER_PATTERN.to_owned(),
        what: What::Original,
        auto_upload: cm_storage::AutoUpload::Off,
        default,
    });
    Ok(id)
}

/// Applies the edit form to `id`. Nothing changes when any field is invalid.
pub fn apply_edit(list: &mut [Destination], id: &str, edit: &Edit) -> Result<(), RuleError> {
    let others: Vec<&Destination> = list.iter().filter(|d| d.id != id).collect();
    let name = validate_name(&edit.name, &others)?;
    let pattern = validate_pattern(&edit.folder_pattern)?;
    let d = list
        .iter_mut()
        .find(|d| d.id == id)
        .ok_or(RuleError::DestinationGone)?;
    d.name = name;
    d.folder_pattern = pattern;
    d.what = edit.what;
    d.auto_upload = edit.auto_upload;
    if let Some(account) = d.account.as_mut() {
        if !link_choices(d.kind).contains(&edit.link_mode) {
            return Err(RuleError::LinkNotOffered {
                name: d.name.clone(),
                link: edit.link_mode,
            });
        }
        account.link_mode = edit.link_mode;
    }
    Ok(())
}

/// Makes `id` the only default.
pub fn set_default(list: &mut [Destination], id: &str) -> Result<(), RuleError> {
    if !list.iter().any(|d| d.id == id) {
        return Err(RuleError::DestinationGone);
    }
    for d in list.iter_mut() {
        d.default = d.id == id;
    }
    Ok(())
}

/// Forgets a destination (its copied files stay where they are). When it was the default the
/// first remaining destination takes over, so `upload` keeps working without a name.
pub fn remove(list: &mut Vec<Destination>, id: &str) -> Option<Destination> {
    let at = list.iter().position(|d| d.id == id)?;
    let gone = list.remove(at);
    if gone.default {
        if let Some(first) = list.first_mut() {
            first.default = true;
        }
    }
    Some(gone)
}

/// Whether each destination's folder is reachable now, by id. Touches the disk (a dropped
/// network share can block), so call it off the UI thread.
pub fn probe(list: &[Destination]) -> HashMap<String, bool> {
    list.iter()
        .filter(|d| !d.kind.is_cloud())
        .map(|d| (d.id.clone(), d.path.is_dir()))
        .collect()
}

/// The two link choices of a cloud destination, in the order the segmented control shows them.
pub const LINK_CHOICES: [LinkMode; 2] = [LinkMode::Anyone, LinkMode::None];

/// The link choices a provider can honour: Google Drive has no organization mode; OneDrive
/// (work accounts) and Dropbox (teams) do.
pub fn link_choices(kind: Kind) -> &'static [LinkMode] {
    match kind {
        Kind::OneDrive | Kind::Dropbox => {
            &[LinkMode::Anyone, LinkMode::Organization, LinkMode::None]
        }
        _ => &LINK_CHOICES,
    }
}

pub fn link_label(m: LinkMode) -> String {
    match m {
        LinkMode::Anyone => tr!("Anyone with the link"),
        LinkMode::None => tr!("No link"),
        LinkMode::Organization => tr!("People in my organization"),
    }
}

pub fn link_hint(m: LinkMode) -> String {
    match m {
        LinkMode::Anyone => tr!(
            "After an upload the file gets a link that anyone who has it can open, without signing in."
        ),
        LinkMode::None => tr!("Files stay private to your account; no link is made."),
        LinkMode::Organization => tr!(
            "After an upload the file gets a link that only people in your organization can open."
        ),
    }
}

pub fn what_label(w: What) -> String {
    match w {
        What::Export => tr_ctx!("what to copy", "Export"),
        What::Original => tr!("Original bundle"),
        What::Both => tr_ctx!("what to copy", "Both"),
    }
}

pub fn what_hint(w: What) -> String {
    match w {
        What::Export => tr!("Copies the exported video or image."),
        What::Original => {
            tr!("Copies the whole capture folder with its tracks, so it can be edited later.")
        }
        What::Both => tr!("Copies the export and the whole capture folder."),
    }
}

pub fn auto_label(a: AutoUpload) -> String {
    match a {
        AutoUpload::Off => tr_ctx!("automatic upload", "Off"),
        AutoUpload::OnStop => tr!("After recording"),
        AutoUpload::OnExport => tr!("After export"),
    }
}

pub fn auto_hint(a: AutoUpload, what: What) -> String {
    match a {
        AutoUpload::Off => {
            tr!("Nothing is copied by itself; use Upload from the library or the CLI.")
        }
        AutoUpload::OnStop if what.wants_export() => {
            tr!("Copies a new recording as soon as it stops. The export is made first, then copied.")
        }
        AutoUpload::OnStop => tr!("Copies a new recording as soon as it stops."),
        AutoUpload::OnExport => {
            tr!("Copies a capture each time an export of it finishes, once the export is done.")
        }
    }
}

/// The three choices, in the order the segmented control shows them.
pub const AUTO_CHOICES: [AutoUpload; 3] =
    [AutoUpload::Off, AutoUpload::OnStop, AutoUpload::OnExport];

#[cfg(test)]
mod tests {
    use super::*;

    fn dest(id: &str, name: &str, default: bool) -> Destination {
        Destination {
            id: id.into(),
            name: name.into(),
            kind: Kind::Local,
            path: PathBuf::from("/nowhere").join(id),
            account: None,
            folder_pattern: DEFAULT_FOLDER_PATTERN.into(),
            what: What::Original,
            auto_upload: cm_storage::AutoUpload::Off,
            default,
        }
    }

    #[test]
    fn slugs_and_ids() {
        assert_eq!(slug("My NAS (office)"), "my_nas_office");
        assert_eq!(slug("  --  "), "");
        let list = vec![dest("dest_nas", "NAS", true)];
        assert_eq!(unique_id("NAS", &list), "dest_nas_2");
        assert_eq!(unique_id("Backup", &list), "dest_backup");
        assert_eq!(unique_id("\u{1f4be}", &list), "dest_local");
        let list = vec![
            dest("dest_nas", "NAS", true),
            dest("dest_nas_2", "NAS 2", false),
        ];
        assert_eq!(unique_id("nas", &list), "dest_nas_3");
    }

    #[test]
    fn names_are_unique_ignoring_case() {
        let list = vec![dest("a", "NAS", true), dest("b", "nas (2)", false)];
        assert_eq!(unique_name("Backup", &list), "Backup");
        assert_eq!(unique_name("nas", &list), "nas (3)");
        assert_eq!(unique_name("  ", &list), "Local folder");
        let refs: Vec<&Destination> = list.iter().collect();
        assert!(validate_name("Nas", &refs).is_err());
        assert!(validate_name("   ", &refs).is_err());
        assert!(validate_name(&"x".repeat(61), &refs).is_err());
        assert_eq!(validate_name("  Office  ", &refs).unwrap(), "Office");
    }

    #[test]
    fn patterns() {
        assert!(validate_pattern(DEFAULT_FOLDER_PATTERN).is_ok());
        assert!(validate_pattern("Shots/{yyyy}-{MM}-{dd}").is_ok());
        assert!(validate_pattern("flat").is_ok());
        assert!(validate_pattern("  ").is_err());
        assert!(validate_pattern("/abs").is_err());
        assert!(validate_pattern("\\share").is_err());
        assert!(validate_pattern("C:/x").is_err());
        assert!(validate_pattern("a/../b").is_err());
        assert!(validate_pattern("a/{year}").is_err());
        assert!(validate_pattern("a/}").is_err());
        assert_eq!(validate_pattern(" a/{title} ").unwrap(), "a/{title}");
    }

    #[test]
    fn folder_checks() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(check_folder(tmp.path()).is_ok());
        assert!(check_folder(&tmp.path().join("missing")).is_err());
        let file = tmp.path().join("f.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(check_folder(&file)
            .unwrap_err()
            .to_string()
            .contains("not a folder"));
        assert!(check_folder(Path::new("relative/dir")).is_err());
    }

    #[test]
    fn add_sets_defaults_and_first_is_default() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("NAS");
        let b = tmp.path().join("sub").join("NAS");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let mut list = Vec::new();
        let id1 = add_folder(&mut list, &a).unwrap();
        assert_eq!(id1, "dest_nas");
        assert!(list[0].default);
        assert_eq!(list[0].name, "NAS");
        assert_eq!(list[0].what, What::Original);
        assert_eq!(list[0].folder_pattern, DEFAULT_FOLDER_PATTERN);
        assert_eq!(list[0].auto_upload, AutoUpload::Off);
        // The same folder twice is refused, also through a different spelling.
        assert!(add_folder(&mut list, &a).is_err());
        assert!(add_folder(&mut list, &a.join("..").join("NAS")).is_err());
        // Another folder with the same name gets a unique name and id, and is not the default.
        let id2 = add_folder(&mut list, &b).unwrap();
        assert_eq!(id2, "dest_nas_2");
        assert_eq!(list[1].name, "NAS (2)");
        assert!(!list[1].default);
        assert_eq!(list.iter().filter(|d| d.default).count(), 1);
    }

    #[test]
    fn edit_validates_everything_before_changing_anything() {
        let mut list = vec![dest("a", "A", true), dest("b", "B", false)];
        let mut e = Edit::of(&list[1]);
        e.name = "A".into();
        assert!(apply_edit(&mut list, "b", &e).is_err());
        e.name = "Backup".into();
        e.folder_pattern = "../out".into();
        assert!(apply_edit(&mut list, "b", &e).is_err());
        assert_eq!(list[1].name, "B");
        e.folder_pattern = "Out/{title}".into();
        e.what = What::Both;
        e.auto_upload = AutoUpload::OnExport;
        apply_edit(&mut list, "b", &e).unwrap();
        assert_eq!(list[1].auto_upload, AutoUpload::OnExport);
        assert_eq!(list[1].name, "Backup");
        assert_eq!(list[1].folder_pattern, "Out/{title}");
        assert_eq!(list[1].what, What::Both);
        // Keeping its own name is fine.
        let same = Edit::of(&list[1]);
        apply_edit(&mut list, "b", &same).unwrap();
        assert!(apply_edit(&mut list, "zzz", &same).is_err());
    }

    #[test]
    fn default_moves_and_survives_removal() {
        let mut list = vec![
            dest("a", "A", true),
            dest("b", "B", false),
            dest("c", "C", false),
        ];
        set_default(&mut list, "c").unwrap();
        let defaults: Vec<&str> = list
            .iter()
            .filter(|d| d.default)
            .map(|d| d.id.as_str())
            .collect();
        assert_eq!(defaults, ["c"]);
        assert!(set_default(&mut list, "zzz").is_err());
        // Removing a non-default keeps the default.
        assert!(remove(&mut list, "a").is_some());
        assert!(list.iter().find(|d| d.id == "c").unwrap().default);
        // Removing the default promotes the first remaining.
        assert!(remove(&mut list, "c").is_some());
        assert!(list[0].default);
        assert!(remove(&mut list, "c").is_none());
        // Removing the last leaves an empty list.
        assert!(remove(&mut list, "b").is_some());
        assert!(list.is_empty());
    }

    #[test]
    fn removing_never_touches_files() {
        let tmp = tempfile::tempdir().unwrap();
        let keep = tmp.path().join("copied.txt");
        std::fs::write(&keep, "x").unwrap();
        let mut list = Vec::new();
        let id = add_folder(&mut list, tmp.path()).unwrap();
        remove(&mut list, &id);
        assert!(keep.exists());
    }

    #[test]
    fn probe_reports_missing_folders_offline() {
        let tmp = tempfile::tempdir().unwrap();
        let mut here = dest("here", "Here", true);
        here.path = tmp.path().to_path_buf();
        let gone = dest("gone", "Gone", false);
        let r = probe(&[here, gone]);
        assert!(r["here"]);
        assert!(!r["gone"]);
    }

    #[test]
    fn the_whole_flow_round_trips_through_the_store() {
        let cfg = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let store = cm_storage::DestinationStore::new(cfg.path());
        let mut list = store.list().unwrap();
        let id = add_folder(&mut list, folder.path()).unwrap();
        store.save(&list).unwrap();
        let mut list = store.list().unwrap();
        let mut e = Edit::of(&list[0]);
        e.what = What::Export;
        apply_edit(&mut list, &id, &e).unwrap();
        store.save(&list).unwrap();
        assert_eq!(store.resolve(None).unwrap().what, What::Export);
    }

    #[test]
    fn the_auto_upload_choice_is_stored_and_survives_other_edits() {
        let cfg = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let store = cm_storage::DestinationStore::new(cfg.path());
        let mut list = Vec::new();
        let id = add_folder(&mut list, folder.path()).unwrap();
        assert_eq!(list[0].auto_upload, AutoUpload::Off, "off by default");
        for choice in AUTO_CHOICES {
            let mut e = Edit::of(&list[0]);
            e.auto_upload = choice;
            apply_edit(&mut list, &id, &e).unwrap();
            store.save(&list).unwrap();
            assert_eq!(store.get(&id).unwrap().auto_upload, choice);
        }
        // Renaming with the form's other fields untouched keeps the choice.
        let mut e = Edit::of(&list[0]);
        e.name = "Renamed".into();
        apply_edit(&mut list, &id, &e).unwrap();
        assert_eq!(list[0].auto_upload, AutoUpload::OnExport);
        // An invalid edit changes nothing, including the choice.
        let mut bad = Edit::of(&list[0]);
        bad.auto_upload = AutoUpload::Off;
        bad.folder_pattern = "../x".into();
        assert!(apply_edit(&mut list, &id, &bad).is_err());
        assert_eq!(list[0].auto_upload, AutoUpload::OnExport);
    }

    fn cloud(id: &str) -> Destination {
        Destination {
            id: id.into(),
            name: "Google Drive".into(),
            kind: Kind::GoogleDrive,
            path: PathBuf::new(),
            account: Some(cm_storage::CloudAccount {
                account: "gdrive-x".into(),
                label: "me@example.org".into(),
                folder_id: Some("f1".into()),
                folder_name: "CaptureMann".into(),
                link_mode: LinkMode::Anyone,
                app_folder: false,
            }),
            folder_pattern: DEFAULT_FOLDER_PATTERN.into(),
            what: What::Export,
            auto_upload: AutoUpload::Off,
            default: false,
        }
    }

    #[test]
    fn a_cloud_destination_edits_its_link_mode_and_is_not_probed_as_a_folder() {
        let mut list = vec![cloud("drive"), dest("a", "A", true)];
        let mut e = Edit::of(&list[0]);
        assert_eq!(e.link_mode, LinkMode::Anyone);
        e.link_mode = LinkMode::None;
        e.what = What::Both;
        apply_edit(&mut list, "drive", &e).unwrap();
        let acc = list[0].account.as_ref().unwrap();
        assert_eq!(acc.link_mode, LinkMode::None);
        assert_eq!(acc.account, "gdrive-x", "the account is never edited here");
        assert_eq!(acc.folder_id.as_deref(), Some("f1"));
        // A local destination has no link mode to change.
        let mut local = Edit::of(&list[1]);
        local.link_mode = LinkMode::None;
        apply_edit(&mut list, "a", &local).unwrap();
        assert!(list[1].account.is_none());
        // Cloud destinations have no folder to stat.
        let probed = probe(&list);
        assert!(!probed.contains_key("drive"));
        assert!(probed.contains_key("a"));
        for m in LINK_CHOICES {
            assert!(!link_label(m).is_empty() && !link_hint(m).is_empty());
        }
    }

    #[test]
    fn the_organization_link_is_only_offered_where_the_provider_can_make_it() {
        assert_eq!(
            link_choices(Kind::GoogleDrive),
            [LinkMode::Anyone, LinkMode::None]
        );
        for kind in [Kind::OneDrive, Kind::Dropbox] {
            assert!(link_choices(kind).contains(&LinkMode::Organization));
            assert!(link_choices(kind).contains(&LinkMode::Anyone));
            assert!(link_choices(kind).contains(&LinkMode::None));
        }
        let mut list = vec![cloud("drive")];
        let mut e = Edit::of(&list[0]);
        e.link_mode = LinkMode::Organization;
        let err = apply_edit(&mut list, "drive", &e).unwrap_err().to_string();
        assert!(
            err.contains("Google Drive") && err.contains("organization"),
            "{err}"
        );
        assert_eq!(
            list[0].account.as_ref().unwrap().link_mode,
            LinkMode::Anyone,
            "nothing changed"
        );
        for (kind, name) in [(Kind::OneDrive, "OneDrive"), (Kind::Dropbox, "Dropbox")] {
            list[0].kind = kind;
            list[0].name = name.into();
            apply_edit(&mut list, "drive", &e).unwrap();
            assert_eq!(
                list[0].account.as_ref().unwrap().link_mode,
                LinkMode::Organization
            );
        }
        for m in [LinkMode::Anyone, LinkMode::Organization, LinkMode::None] {
            assert!(!link_label(m).is_empty() && !link_hint(m).is_empty());
        }
    }

    #[test]
    fn labels_and_hints_exist_for_every_choice() {
        for a in AUTO_CHOICES {
            assert!(!auto_label(a).is_empty());
            for w in [What::Export, What::Original, What::Both] {
                assert!(!auto_hint(a, w).is_empty());
            }
        }
        assert!(auto_hint(AutoUpload::OnStop, What::Export).contains("export is made first"));
        assert!(!auto_hint(AutoUpload::OnStop, What::Original).contains("export"));
    }

    /// I18N P2: the messages with values keep every value in every language, and the labels are
    /// built when asked for.
    #[test]
    fn messages_keep_their_values_in_every_language() {
        use cm_i18n::{with_language, Lang};
        let list = [dest("a", "NAS", true)];
        let refs: Vec<&Destination> = list.iter().collect();
        for lang in [Lang::Vi, Lang::EnXa] {
            with_language(lang, || {
                let e = validate_name("nas", &refs).unwrap_err().to_string();
                assert!(e.contains("nas") && !e.contains("{name}"), "{lang:?}: {e}");
                let e = validate_name(&"x".repeat(61), &refs)
                    .unwrap_err()
                    .to_string();
                assert!(e.contains("60") && !e.contains("{max}"), "{lang:?}: {e}");
            });
        }
        assert_eq!(link_label(LinkMode::None), "No link");
        assert_ne!(
            with_language(Lang::EnXa, || link_label(LinkMode::None)),
            "No link"
        );
        assert_eq!(auto_label(AutoUpload::Off), "Off");
    }
}
