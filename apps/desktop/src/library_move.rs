//! "Change library folder" (LIB-2): the display-free parts. What the Library row says, the
//! cache of the library's size, the words of every question and problem, and the state of a move
//! in progress. Drawn by `ui::settings::library_folder`, driven by `ui::library_move_flow`.
//!
//! Every text is made when it is drawn or when a dialog is built (`tr!`), from data that does not
//! hold words: a [`MoveProblem`], a size in bytes, a path.

use cm_daemon::library_move::{display_path, LibraryUsage};
use cm_daemon::{MoveMode, MovePlan, MoveProblem};
use cm_i18n::{fmt, tr, tr_ctx, trn};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::confirm::DialogModel;

/// How long a measured size is reused before the folder is walked again.
pub const SIZE_TTL: Duration = Duration::from_secs(60);

/// Id of the dialogs of this flow (one open at a time).
pub const DIALOG_ID: &str = "local_library_move";

// ---- size and free space -------------------------------------------------------------------

/// What the Library row shows about the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Measured {
    Ok {
        used: u64,
        free: Option<u64>,
    },
    /// The folder could not be read.
    Unknown,
}

/// Walks the library folder and reads the free space of its volume. Blocking: never call it on
/// the UI thread (a sleeping network drive can take seconds).
pub fn measure(root: &Path) -> Measured {
    use cm_capture::disk::{FreeSpace, SystemFreeSpace};
    match cm_daemon::library_move::library_usage(root) {
        Ok(LibraryUsage { bytes, .. }) => Measured::Ok {
            used: bytes,
            free: SystemFreeSpace.free_bytes(root),
        },
        Err(_) => Measured::Unknown,
    }
}

/// The size cache of the Library row: one folder, reused for [`SIZE_TTL`].
#[derive(Debug, Default)]
pub struct SizeCache {
    entry: Option<(PathBuf, Instant, Measured)>,
    walking: Option<PathBuf>,
}

/// What the cache says about a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// A measurement younger than the TTL.
    Fresh(Measured),
    /// Nothing usable and no walk running for it: start one ([`SizeCache::begin`]).
    Start,
    /// A walk is running (or the old value is stale and a walk is running).
    Calculating,
}

impl SizeCache {
    pub fn lookup(&self, root: &Path, now: Instant) -> Lookup {
        if let Some((p, at, m)) = &self.entry {
            if p == root && now.duration_since(*at) < SIZE_TTL {
                return Lookup::Fresh(m.clone());
            }
        }
        if self.walking.as_deref() == Some(root) {
            Lookup::Calculating
        } else {
            Lookup::Start
        }
    }

    /// Marks a walk as started; a second `lookup` answers `Calculating` until [`finish`].
    ///
    /// [`finish`]: SizeCache::finish
    pub fn begin(&mut self, root: &Path) {
        self.walking = Some(root.to_path_buf());
    }

    pub fn finish(&mut self, root: &Path, m: Measured, now: Instant) {
        if self.walking.as_deref() == Some(root) {
            self.walking = None;
        }
        self.entry = Some((root.to_path_buf(), now, m));
    }

    /// Forget the value (after a move, or the folder changed).
    pub fn invalidate(&mut self) {
        self.entry = None;
    }
}

/// A size for the row and the dialogs: decimal units (10^6 bytes), as in the board:
/// `3.2 GB`, `412 GB`, `56 MB`.
pub fn size_text(bytes: u64) -> String {
    let b = bytes as f64;
    let (v, unit) = if b >= 1e9 {
        (b / 1e9, "GB")
    } else if b >= 1e6 {
        (b / 1e6, "MB")
    } else if b >= 1e3 {
        (b / 1e3, "KB")
    } else {
        return format!("{bytes} B");
    };
    let places = usize::from(v < 10.0);
    format!("{} {unit}", fmt::decimal(v, places))
}

/// The path as the row shows it: the home folder shortened to `~`.
pub fn shown_path(path: &Path) -> String {
    let home = cm_platform::dirs::home_dir();
    display_path(path, home.as_deref())
}

/// The row's second line: `~/Movies/CaptureMann · 3.2 GB used · 412 GB free`.
pub fn row_subtitle(path: &str, state: &Lookup) -> String {
    match state {
        Lookup::Fresh(Measured::Ok {
            used,
            free: Some(free),
        }) => tr!(
            "{path} \u{b7} {used} used \u{b7} {free} free",
            path = path,
            used = size_text(*used),
            free = size_text(*free)
        ),
        Lookup::Fresh(Measured::Ok { used, free: None }) => tr!(
            "{path} \u{b7} {used} used",
            path = path,
            used = size_text(*used)
        ),
        Lookup::Fresh(Measured::Unknown) => tr!("{path} \u{b7} size unknown", path = path),
        Lookup::Start | Lookup::Calculating => {
            tr!("{path} \u{b7} Calculating\u{2026}", path = path)
        }
    }
}

// ---- the state of a move -------------------------------------------------------------------

/// Where the Library row's move stands. Kept by the app (the Settings page may be closed and
/// opened while a copy runs).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Phase {
    #[default]
    Idle,
    /// The folder picker is open or the folder is being checked.
    Checking,
    /// The copy runs (`job_id`), `fraction` 0 to 1.
    Moving {
        job_id: String,
        to: PathBuf,
        fraction: f64,
    },
    /// The copy stopped with an error (the old library is intact; retry resumes).
    Failed {
        job_id: String,
        to: PathBuf,
        reason: String,
    },
    Cancelled {
        to: PathBuf,
    },
    /// The copy is done and verified: the new folder is used after a restart.
    Moved(MovedInfo),
    /// `fresh` or `use_existing`: only the setting changed.
    Switched {
        from: PathBuf,
        to: PathBuf,
        mode: MoveMode,
    },
}

/// What a finished move leaves for the Trash step.
#[derive(Debug, Clone, PartialEq)]
pub struct MovedInfo {
    pub from: PathBuf,
    pub to: PathBuf,
    pub bundles: usize,
    pub bytes: u64,
    /// The bundles verified in the new folder (the Trash step may take only these).
    pub copied: Vec<String>,
    /// The old copies were moved to the Trash already.
    pub trashed: bool,
}

// ---- words: problems -----------------------------------------------------------------------

/// Each precondition in its own words.
pub fn problem_text(p: &MoveProblem) -> String {
    match p {
        MoveProblem::RecordingRunning => {
            tr!("A recording is running. Stop it, then change the library folder.")
        }
        MoveProblem::SelftestRunning => {
            tr!("The 60 fps self-test is running. Wait for it to finish, then change the library folder.")
        }
        MoveProblem::JobRunning { job_type, .. } => tr!(
            "{kind} is still running. Wait for it to finish, then change the library folder. Nothing is cancelled for you.",
            kind = job_kind(job_type)
        ),
        MoveProblem::MoveRunning { .. } => tr!("The library is already being moved."),
        MoveProblem::TargetNotAbsolute { path } => {
            tr!("\u{201c}{path}\u{201d} is not a full folder path.", path = path)
        }
        MoveProblem::TargetIsFile { path } => tr!(
            "\u{201c}{path}\u{201d} is a file, not a folder. Choose a folder.",
            path = path
        ),
        MoveProblem::TargetNotWritable { path, .. } => tr!(
            "CaptureMann can\u{2019}t write to \u{201c}{path}\u{201d}. Choose a folder you can save files in.",
            path = path
        ),
        MoveProblem::TargetInsideLibrary { .. } => {
            tr!("That folder is inside the current library. Choose a folder outside it.")
        }
        MoveProblem::TargetContainsLibrary { .. } => {
            tr!("That folder contains the current library. Choose a folder that doesn\u{2019}t.")
        }
        MoveProblem::SameFolder { .. } => tr!("That is already the library folder."),
        MoveProblem::NotEnoughSpace { needed, free, .. } => tr!(
            "There isn\u{2019}t enough room: the move needs {needed} free and the new folder has {free}.",
            needed = size_text(*needed),
            free = size_text(*free)
        ),
        MoveProblem::SourceUnreadable { path, .. } => tr!(
            "The current library folder \u{201c}{path}\u{201d} can\u{2019}t be read. If it is on a drive, connect the drive and try again.",
            path = path
        ),
        MoveProblem::TargetHasLibrary { .. } => {
            tr!("That folder already holds a CaptureMann library.")
        }
        MoveProblem::TargetHasNoLibrary { .. } => {
            tr!("That folder holds no CaptureMann captures.")
        }
    }
}

/// The job kinds the daemon has, as the start of a sentence.
fn job_kind(job_type: &str) -> String {
    match job_type {
        "export" => tr_ctx!("job kind, starts a sentence", "An export"),
        "upload" => tr_ctx!("job kind, starts a sentence", "An upload"),
        "publish" => tr_ctx!("job kind, starts a sentence", "A publish"),
        "transcribe" => tr_ctx!("job kind, starts a sentence", "A transcription"),
        _ => tr_ctx!("job kind, starts a sentence", "A background job"),
    }
}

/// The dialog that explains a problem. A busy problem offers "Check again" (the user waits,
/// nothing is cancelled for them); the others offer to choose another folder.
pub fn problem_dialog(p: &MoveProblem) -> DialogModel {
    let (title, primary) = if p.is_temporary() {
        (tr!("Not right now"), tr!("Check again"))
    } else {
        (
            tr!("Can\u{2019}t use this folder"),
            tr!("Choose another folder\u{2026}"),
        )
    };
    base(title, problem_text(p), Vec::new(), primary)
}

// ---- words: questions ----------------------------------------------------------------------

fn base(
    title: String,
    summary: String,
    details: Vec<(String, String)>,
    primary: String,
) -> DialogModel {
    DialogModel {
        id: DIALOG_ID.to_owned(),
        chip: String::new(),
        agent: String::new(),
        tool: String::new(),
        title,
        summary,
        details,
        primary_label: primary,
        primary_is_danger: false,
        primary_is_record: false,
        allow_session: false,
        option: None,
    }
}

fn rows(plan: &MovePlan, with_size: bool) -> Vec<(String, String)> {
    let mut v = vec![
        (tr_ctx!("dialog row", "From"), shown_path(&plan.from)),
        (tr_ctx!("dialog row", "To"), shown_path(&plan.to)),
    ];
    if with_size {
        v.push((tr_ctx!("dialog row", "Size"), size_text(plan.bytes)));
        if let Some(free) = plan.free_bytes {
            v.push((tr_ctx!("dialog row", "Free space"), size_text(free)));
        }
    }
    v
}

/// "Change the library folder?": move (the default), or start empty there (the checkbox).
pub fn move_dialog(plan: &MovePlan) -> DialogModel {
    let n = plan.bundles;
    if n == 0 {
        return base(
            tr!("Change the library folder?"),
            tr!("The library has no captures yet, so nothing is copied. CaptureMann uses the new folder after it restarts."),
            rows(plan, false),
            tr!("Use the new folder"),
        );
    }
    let mut m = base(
        tr!("Change the library folder?"),
        trn!(
            "Your {n} capture ({size}) is copied to the new folder and checked. Nothing is deleted from the old folder. CaptureMann uses the new folder after it restarts.",
            "Your {n} captures ({size}) are copied to the new folder and checked. Nothing is deleted from the old folder. CaptureMann uses the new folder after it restarts.",
            n,
            size = size_text(plan.bytes)
        ),
        rows(plan, true),
        tr!("Move my captures to the new folder"),
    );
    m.option = Some(tr!("Start with an empty library there instead"));
    m
}

/// The move does not fit, but a fresh start does.
pub fn fresh_only_dialog(plan: &MovePlan, needed: u64, free: u64) -> DialogModel {
    base(
        tr!("Not enough room to move"),
        tr!(
            "Your captures need {needed} free and the new folder has {free}. You can start with an empty library there instead. The captures in the old folder stay where they are.",
            needed = size_text(needed),
            free = size_text(free)
        ),
        rows(plan, false),
        tr!("Start with an empty library there"),
    )
}

/// The folder already holds a library.
pub fn use_existing_dialog(plan: &MovePlan) -> DialogModel {
    let n = plan.to_bundles;
    base(
        tr!("Use the library in this folder?"),
        trn!(
            "This folder already holds a CaptureMann library with {n} capture ({size}). CaptureMann uses it after it restarts. The captures in your current library stay where they are.",
            "This folder already holds a CaptureMann library with {n} captures ({size}). CaptureMann uses it after it restarts. The captures in your current library stay where they are.",
            n,
            size = size_text(plan.bytes)
        ),
        rows(plan, false),
        tr!("Use that library"),
    )
}

/// The copy is done: restart to use the new folder.
pub fn moved_dialog(info: &MovedInfo) -> DialogModel {
    base(
        tr!("The library moved"),
        tr!(
            "The library moved to {path}. Restart CaptureMann to use it. Your old captures were not deleted.",
            path = shown_path(&info.to)
        ),
        Vec::new(),
        tr!("Restart CaptureMann"),
    )
}

/// `fresh` or `use_existing` was chosen: only the setting changed.
pub fn switched_dialog(to: &Path) -> DialogModel {
    base(
        tr!("The library folder changed"),
        tr!(
            "The library folder changed to {path}. Restart CaptureMann to use it. The captures in the old folder were not moved or deleted.",
            path = shown_path(to)
        ),
        Vec::new(),
        tr!("Restart CaptureMann"),
    )
}

/// The separate, optional step: the old copies go to the Trash.
pub fn trash_dialog(info: &MovedInfo) -> DialogModel {
    let n = info.copied.len();
    base(
        tr!("Move the old copies to the Trash?"),
        trn!(
            "{n} capture in the old folder was copied and checked. It moves to the Trash, where you can restore it. Anything that changed since the copy stays where it is.",
            "{n} captures in the old folder were copied and checked. They move to the Trash, where you can restore them. Anything that changed since the copy stays where it is.",
            n
        ),
        vec![(tr_ctx!("dialog row", "Folder"), shown_path(&info.from))],
        tr!("Move to the Trash"),
    )
}

/// The toast after the Trash step: `(title, body)`.
pub fn trash_result_text(trashed: usize, kept: usize) -> (String, Option<String>) {
    let title = trn!(
        "Moved {n} capture to the Trash",
        "Moved {n} captures to the Trash",
        trashed
    );
    let body = (kept > 0).then(|| {
        trn!(
            "{n} capture stayed in the old folder: it changed after it was copied, or it was not found.",
            "{n} captures stayed in the old folder: they changed after they were copied, or they were not found.",
            kept
        )
    });
    (title, body)
}

// ---- words: the Library row while a move is on ---------------------------------------------

/// `(label, hint)` of the row that replaces the folder row while the phase is not idle; `None`
/// for [`Phase::Idle`] and [`Phase::Checking`].
pub fn phase_texts(p: &Phase) -> Option<(String, String)> {
    Some(match p {
        Phase::Idle | Phase::Checking => return None,
        Phase::Moving { to, fraction, .. } => (
            tr!("Moving the library"),
            tr!(
                "Copying your captures to {path}: {percent}%",
                path = shown_path(to),
                percent = (fraction.clamp(0.0, 1.0) * 100.0).floor() as u32
            ),
        ),
        Phase::Failed { to, .. } => (
            tr!("The move stopped"),
            tr!(
                "The captures copied so far are in {path}. Nothing was deleted from the old folder, which is still in use. Retry to carry on.",
                path = shown_path(to)
            ),
        ),
        Phase::Cancelled { to } => (
            tr!("The move was cancelled"),
            tr!(
                "The old library is still in use. The captures copied so far stay in {path}.",
                path = shown_path(to)
            ),
        ),
        Phase::Moved(info) => (
            tr!("The library moved"),
            tr!(
                "The library moved to {path}. Restart CaptureMann to use it.",
                path = shown_path(&info.to)
            ),
        ),
        Phase::Switched { from, to, .. } => (
            tr!("The library folder changed"),
            tr!(
                "The library folder is now {path}. Restart CaptureMann to use it. The captures in {old} were not moved or deleted.",
                path = shown_path(to),
                old = shown_path(from)
            ),
        ),
    })
}

/// Every dialog of the flow, with typical data, for the width tests.
#[cfg(test)]
pub(crate) fn all_dialogs_for_tests() -> Vec<DialogModel> {
    let plan = MovePlan {
        from: PathBuf::from("/Users/a/Movies/CaptureMann"),
        to: PathBuf::from("/Volumes/Big Drive/CaptureMann"),
        mode: MoveMode::Move,
        bytes: 3_200_000_000,
        bundles: 412,
        free_bytes: Some(412_000_000_000),
        to_has_library: true,
        to_bundles: 412,
    };
    let info = MovedInfo {
        from: plan.from.clone(),
        to: plan.to.clone(),
        bundles: 412,
        bytes: 1,
        copied: vec!["a.cmrec".into(); 412],
        trashed: false,
    };
    let mut v = vec![
        move_dialog(&plan),
        fresh_only_dialog(&plan, 3_520_000_000, 1_000_000_000),
        use_existing_dialog(&plan),
        moved_dialog(&info),
        switched_dialog(&plan.to),
        trash_dialog(&info),
    ];
    v.extend(
        [
            MoveProblem::RecordingRunning,
            MoveProblem::SelftestRunning,
            MoveProblem::JobRunning {
                job_type: "transcribe".into(),
                job_id: "j".into(),
            },
            MoveProblem::NotEnoughSpace {
                needed: 3_520_000_000,
                free: 1_000_000_000,
                size: 3_200_000_000,
            },
            MoveProblem::SourceUnreadable {
                path: "/Volumes/Old".into(),
                error: "e".into(),
            },
            MoveProblem::TargetNotWritable {
                path: "/Volumes/Big Drive".into(),
                error: "e".into(),
            },
            MoveProblem::TargetContainsLibrary {
                path: "/Users/a".into(),
                library: "l".into(),
            },
        ]
        .iter()
        .map(problem_dialog),
    );
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(bundles: usize, bytes: u64, free: Option<u64>) -> MovePlan {
        MovePlan {
            from: PathBuf::from("/Users/a/Movies/CaptureMann"),
            to: PathBuf::from("/Volumes/Big/CaptureMann"),
            mode: MoveMode::Move,
            bytes,
            bundles,
            free_bytes: free,
            to_has_library: false,
            to_bundles: 0,
        }
    }

    #[test]
    fn sizes_read_like_the_board() {
        assert_eq!(size_text(3_200_000_000), "3.2 GB");
        assert_eq!(size_text(412_000_000_000), "412 GB");
        assert_eq!(size_text(56_000_000), "56 MB");
        assert_eq!(size_text(5_500_000), "5.5 MB");
        assert_eq!(size_text(999), "999 B");
    }

    #[test]
    fn the_row_subtitle_has_four_states() {
        let ok = Lookup::Fresh(Measured::Ok {
            used: 3_200_000_000,
            free: Some(412_000_000_000),
        });
        assert_eq!(
            row_subtitle("~/Movies/CaptureMann", &ok),
            "~/Movies/CaptureMann \u{b7} 3.2 GB used \u{b7} 412 GB free"
        );
        assert_eq!(
            row_subtitle(
                "~/x",
                &Lookup::Fresh(Measured::Ok {
                    used: 0,
                    free: None
                })
            ),
            "~/x \u{b7} 0 B used"
        );
        assert_eq!(
            row_subtitle("~/x", &Lookup::Calculating),
            "~/x \u{b7} Calculating\u{2026}"
        );
        assert_eq!(
            row_subtitle("~/x", &Lookup::Fresh(Measured::Unknown)),
            "~/x \u{b7} size unknown"
        );
    }

    #[test]
    fn the_cache_reuses_a_value_for_60_seconds_and_walks_once() {
        let p = Path::new("/lib");
        let t0 = Instant::now();
        let mut c = SizeCache::default();
        assert_eq!(c.lookup(p, t0), Lookup::Start);
        c.begin(p);
        assert_eq!(c.lookup(p, t0), Lookup::Calculating);
        c.finish(p, Measured::Unknown, t0);
        assert_eq!(
            c.lookup(p, t0 + Duration::from_secs(59)),
            Lookup::Fresh(Measured::Unknown)
        );
        assert_eq!(c.lookup(p, t0 + Duration::from_secs(61)), Lookup::Start);
        // Another folder is not served from the cache.
        assert_eq!(c.lookup(Path::new("/other"), t0), Lookup::Start);
        c.invalidate();
        assert_eq!(c.lookup(p, t0), Lookup::Start);
    }

    #[test]
    fn measure_counts_bundles_and_reads_the_volume() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("a.cmshot")).unwrap();
        std::fs::write(d.path().join("a.cmshot/original.png"), [0u8; 2000]).unwrap();
        match measure(d.path()) {
            Measured::Ok { used, free } => {
                assert_eq!(used, 2000);
                assert!(free.is_some_and(|f| f > 0));
            }
            m => panic!("{m:?}"),
        }
        assert_eq!(
            measure(Path::new("/definitely/not/here")),
            Measured::Unknown
        );
    }

    #[test]
    fn every_problem_has_its_own_sentence_and_the_right_button() {
        let ps = [
            MoveProblem::RecordingRunning,
            MoveProblem::SelftestRunning,
            MoveProblem::JobRunning {
                job_type: "export".into(),
                job_id: "j".into(),
            },
            MoveProblem::MoveRunning { job_id: "j".into() },
            MoveProblem::TargetNotAbsolute { path: "x".into() },
            MoveProblem::TargetIsFile { path: "x".into() },
            MoveProblem::TargetNotWritable {
                path: "x".into(),
                error: "e".into(),
            },
            MoveProblem::TargetInsideLibrary {
                path: "x".into(),
                library: "l".into(),
            },
            MoveProblem::TargetContainsLibrary {
                path: "x".into(),
                library: "l".into(),
            },
            MoveProblem::SameFolder { path: "x".into() },
            MoveProblem::NotEnoughSpace {
                needed: 2_000_000_000,
                free: 1_000_000_000,
                size: 1_800_000_000,
            },
            MoveProblem::SourceUnreadable {
                path: "x".into(),
                error: "e".into(),
            },
            MoveProblem::TargetHasLibrary {
                path: "x".into(),
                bundles: 1,
            },
            MoveProblem::TargetHasNoLibrary { path: "x".into() },
        ];
        let mut seen = std::collections::BTreeSet::new();
        for p in &ps {
            let t = problem_text(p);
            assert!(!t.contains('{'), "{t}");
            assert!(seen.insert(t.clone()), "two problems say the same: {t}");
            let d = problem_dialog(p);
            assert_eq!(d.primary_label == "Check again", p.is_temporary(), "{p:?}");
            assert!(!d.primary_is_danger);
        }
        assert!(problem_text(&ps[2]).contains("An export is still running"));
        assert!(
            problem_text(&ps[10]).contains("2.0 GB") && problem_text(&ps[10]).contains("1.0 GB")
        );
    }

    #[test]
    fn the_move_question_offers_exactly_move_or_empty() {
        let d = move_dialog(&plan(3, 3_200_000_000, Some(412_000_000_000)));
        assert_eq!(d.primary_label, "Move my captures to the new folder");
        assert_eq!(
            d.option.as_deref(),
            Some("Start with an empty library there instead")
        );
        assert!(
            d.summary.contains("Your 3 captures (3.2 GB) are copied"),
            "{}",
            d.summary
        );
        assert!(d.summary.contains("Nothing is deleted from the old folder"));
        assert!(d
            .details
            .iter()
            .any(|(k, v)| k == "To" && v == "/Volumes/Big/CaptureMann"));
        assert!(d
            .details
            .iter()
            .any(|(k, v)| k == "Free space" && v == "412 GB"));
        assert!(!d.primary_is_danger, "OD-12: the normal primary button");
        let one = move_dialog(&plan(1, 5_000_000, None));
        assert!(
            one.summary.starts_with("Your 1 capture (5.0 MB) is copied"),
            "{}",
            one.summary
        );
        let none = move_dialog(&plan(0, 0, None));
        assert_eq!(none.primary_label, "Use the new folder");
        assert!(none.option.is_none());
    }

    #[test]
    fn the_other_questions() {
        let p = plan(3, 1_800_000_000, Some(1_000_000_000));
        let f = fresh_only_dialog(&p, 1_980_000_000, 1_000_000_000);
        assert_eq!(f.primary_label, "Start with an empty library there");
        assert!(f.option.is_none());
        let mut e = plan(0, 900_000_000, None);
        e.to_has_library = true;
        e.to_bundles = 7;
        let u = use_existing_dialog(&e);
        assert_eq!(u.primary_label, "Use that library");
        assert!(
            u.summary.contains("with 7 captures (900 MB)"),
            "{}",
            u.summary
        );
        let info = MovedInfo {
            from: "/a".into(),
            to: "/Volumes/Big/CaptureMann".into(),
            bundles: 3,
            bytes: 1,
            copied: vec!["a.cmrec".into(), "b.cmshot".into()],
            trashed: false,
        };
        let m = moved_dialog(&info);
        assert_eq!(m.primary_label, "Restart CaptureMann");
        assert!(m.summary.contains("/Volumes/Big/CaptureMann"));
        let t = trash_dialog(&info);
        assert!(
            t.summary
                .starts_with("2 captures in the old folder were copied and checked"),
            "{}",
            t.summary
        );
        assert!(t.summary.contains("Trash"));
        let (title, body) = trash_result_text(2, 1);
        assert_eq!(title, "Moved 2 captures to the Trash");
        assert!(body.unwrap().starts_with("1 capture stayed"));
        assert!(trash_result_text(1, 0).1.is_none());
    }

    #[test]
    fn the_row_texts_follow_the_phase() {
        assert!(phase_texts(&Phase::Idle).is_none());
        let (l, h) = phase_texts(&Phase::Moving {
            job_id: "j".into(),
            to: "/Volumes/Big".into(),
            fraction: 0.426,
        })
        .unwrap();
        assert_eq!(l, "Moving the library");
        assert!(h.ends_with("/Volumes/Big: 42%"), "{h}");
        let (_, h) = phase_texts(&Phase::Switched {
            from: "/a".into(),
            to: "/b".into(),
            mode: MoveMode::Fresh,
        })
        .unwrap();
        assert!(h.contains("were not moved or deleted"));
    }
}
