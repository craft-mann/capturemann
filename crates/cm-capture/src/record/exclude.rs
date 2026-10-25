//! Which apps and windows stay out of the screen track, and when the capture filter has to be
//! rebuilt (REC-6, R3-3). Pure logic over plain data so it is tested without ScreenCaptureKit;
//! `sck.rs` maps it onto `SCContentFilter`.
//!
//! An exclusion by bundle id used to be resolved once, when the stream started: an app launched
//! afterwards (a password manager opened mid-recording) was captured. [`ExclusionWatch`] compares
//! the apps that are running now with the ones the filter was built for, and `sck.rs` calls
//! `updateContentFilter` when a matching app appeared. REC-6.AC2 ("including windows opened
//! later") holds for apps that start later too, within one poll interval
//! ([`WATCH_INTERVAL`]).
//!
//! The *window* target ignores exclusions: a desktop-independent window filter captures that one
//! window and nothing else, so there is nothing to exclude (sheets and popovers of other apps do
//! not belong to it).

use std::time::Duration;

use super::plan::Exclusions;

/// How often the running apps are compared with the filter (REC-6.AC2).
pub const WATCH_INTERVAL: Duration = Duration::from_secs(1);

/// A running application as ScreenCaptureKit lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRef {
    pub pid: u32,
    pub bundle_id: String,
}

/// A window as ScreenCaptureKit lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowRef {
    pub id: u32,
    pub owner_pid: Option<u32>,
}

/// What the capture filter excludes. The two forms cannot be mixed in one `SCContentFilter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterSpec {
    /// Exactly these windows (no app is excluded as a whole).
    Windows(Vec<u32>),
    /// Every window of these processes, including windows opened later; listed windows whose
    /// owner is not excluded as a whole are covered by excluding their owner. `keep_windows` are
    /// the listed windows of those processes that are captured anyway
    /// (`exceptingWindows` of `initWithDisplay:excludingApplications:exceptingWindows:`).
    Applications {
        pids: Vec<u32>,
        keep_windows: Vec<u32>,
    },
}

impl FilterSpec {
    /// The kept windows (empty for the `Windows` form).
    pub fn keep_windows(&self) -> &[u32] {
        match self {
            FilterSpec::Applications { keep_windows, .. } => keep_windows,
            FilterSpec::Windows(_) => &[],
        }
    }
}

/// Does `bundle_id` match one of the excluded bundle ids? Case-insensitive: bundle ids are
/// case-insensitive in practice (`com.1password.1Password` vs `com.1Password.1password`).
pub fn bundle_matches(excluded: &[String], bundle_id: &str) -> bool {
    excluded.iter().any(|b| b.eq_ignore_ascii_case(bundle_id))
}

/// The pids among `apps` that are excluded as a whole (by pid or by bundle id), sorted.
pub fn matching_pids(apps: &[AppRef], ex: &Exclusions) -> Vec<u32> {
    let mut out: Vec<u32> = apps
        .iter()
        .filter(|a| ex.pids.contains(&a.pid) || bundle_matches(&ex.bundle_ids, &a.bundle_id))
        .map(|a| a.pid)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Builds the filter spec for the display and region targets from what is running now.
pub fn filter_spec(ex: &Exclusions, apps: &[AppRef], windows: &[WindowRef]) -> FilterSpec {
    let listed: Vec<&WindowRef> = windows
        .iter()
        .filter(|w| ex.window_ids.contains(&w.id))
        .collect();
    let mut pids = matching_pids(apps, ex);
    if pids.is_empty() {
        let mut ids: Vec<u32> = listed.iter().map(|w| w.id).collect();
        ids.sort_unstable();
        return FilterSpec::Windows(ids);
    }
    for w in listed {
        if let Some(owner) = w.owner_pid {
            if !pids.contains(&owner) {
                pids.push(owner);
            }
        }
    }
    pids.sort_unstable();
    // Only listed windows of an excluded process can be an exception; an id SCK does not list
    // (yet) stays excluded with its process.
    let mut keep_windows: Vec<u32> = ex
        .current_keep_ids()
        .into_iter()
        .filter(|id| {
            windows
                .iter()
                .any(|w| w.id == *id && w.owner_pid.is_some_and(|o| pids.contains(&o)))
        })
        .collect();
    keep_windows.sort_unstable();
    keep_windows.dedup();
    FilterSpec::Applications { pids, keep_windows }
}

/// Tells when the running filter no longer covers every excluded app.
#[derive(Debug, Clone, Default)]
pub struct ExclusionWatch {
    applied: Vec<u32>,
    applied_keep: Vec<u32>,
}

impl ExclusionWatch {
    /// `applied`: the excluded pids the current filter was built from.
    pub fn new(mut applied: Vec<u32>) -> Self {
        applied.sort_unstable();
        applied.dedup();
        ExclusionWatch {
            applied,
            applied_keep: Vec::new(),
        }
    }

    /// Also remembers the kept windows the current filter was built with.
    pub fn with_keep(mut self, mut keep: Vec<u32>) -> Self {
        keep.sort_unstable();
        keep.dedup();
        self.applied_keep = keep;
        self
    }

    /// Compares the apps running now with the filter. `Some(pids)` when an excluded app that the
    /// filter does not cover is running (launched after the stream started): rebuild the filter
    /// and call [`ExclusionWatch::applied`] once it was updated. An excluded app that quit needs
    /// no update.
    pub fn needs_update(&self, apps: &[AppRef], ex: &Exclusions) -> Option<Vec<u32>> {
        let now = matching_pids(apps, ex);
        now.iter().any(|p| !self.applied.contains(p)).then_some(now)
    }

    /// Like [`ExclusionWatch::needs_update`], and also fires when the set of kept windows that
    /// SCK lists changed (the library window opened, closed or was replaced). `Some((pids,
    /// keep))`: rebuild the filter and call [`ExclusionWatch::applied_with_keep`].
    pub fn needs_filter_update(
        &self,
        apps: &[AppRef],
        windows: &[WindowRef],
        ex: &Exclusions,
    ) -> Option<(Vec<u32>, Vec<u32>)> {
        let now = matching_pids(apps, ex);
        let keep = filter_spec(ex, apps, windows).keep_windows().to_vec();
        (now.iter().any(|p| !self.applied.contains(p)) || keep != self.applied_keep)
            .then_some((now, keep))
    }

    /// The filter now covers `pids`.
    pub fn applied(&mut self, pids: Vec<u32>) {
        *self = ExclusionWatch::new(pids);
    }

    /// The filter now covers `pids` and keeps `keep`.
    pub fn applied_with_keep(&mut self, pids: Vec<u32>, keep: Vec<u32>) {
        *self = ExclusionWatch::new(pids).with_keep(keep);
    }

    pub fn current(&self) -> &[u32] {
        &self.applied
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(pid: u32, id: &str) -> AppRef {
        AppRef {
            pid,
            bundle_id: id.to_owned(),
        }
    }

    fn ex(bundles: &[&str]) -> Exclusions {
        Exclusions {
            window_ids: vec![],
            pids: vec![1],
            bundle_ids: bundles.iter().map(|b| (*b).to_owned()).collect(),
            ..Exclusions::default()
        }
    }

    /// R3-3: bundle ids match case-insensitively.
    #[test]
    fn bundle_ids_match_case_insensitively() {
        let e = ex(&["com.1password.1Password"]);
        let apps = [
            app(10, "com.1password.1password"),
            app(11, "com.apple.finder"),
        ];
        assert_eq!(matching_pids(&apps, &e), vec![10]);
        assert!(bundle_matches(&e.bundle_ids, "COM.1PASSWORD.1PASSWORD"));
        assert!(!bundle_matches(&e.bundle_ids, "com.1password"));
    }

    /// R3-3: an excluded app launched after the stream started is noticed once, and the filter
    /// is rebuilt with it; an app that quit needs no rebuild.
    #[test]
    fn an_app_launched_later_triggers_one_filter_update() {
        let e = ex(&["com.agilebits.onepassword7", "org.keepassxc.keepassxc"]);
        let mut apps = vec![app(1, "dev.capturemann.app"), app(20, "com.apple.finder")];
        let mut watch = ExclusionWatch::new(matching_pids(&apps, &e));
        assert_eq!(watch.current(), [1]);
        assert_eq!(watch.needs_update(&apps, &e), None);

        // The password manager is launched mid-recording.
        apps.push(app(77, "com.agilebits.OnePassword7"));
        let now = watch.needs_update(&apps, &e).expect("must rebuild");
        assert_eq!(now, vec![1, 77]);
        watch.applied(now);
        assert_eq!(watch.needs_update(&apps, &e), None, "covered now");

        // It quits: nothing to do. A second one starts: another update.
        apps.retain(|a| a.pid != 77);
        assert_eq!(watch.needs_update(&apps, &e), None);
        apps.push(app(90, "org.keepassxc.keepassxc"));
        assert_eq!(watch.needs_update(&apps, &e), Some(vec![1, 90]));
    }

    /// The filter construction: apps win over single windows; a listed window of a process that
    /// is not excluded as a whole is covered through its owner; with no app match only the
    /// listed windows are excluded.
    #[test]
    fn filter_spec_covers_apps_and_listed_windows() {
        let mut e = ex(&["com.vault"]);
        e.pids.clear();
        e.window_ids = vec![5, 6];
        let windows = [
            WindowRef {
                id: 5,
                owner_pid: Some(40),
            },
            WindowRef {
                id: 6,
                owner_pid: None,
            },
            WindowRef {
                id: 7,
                owner_pid: Some(41),
            },
        ];
        // No excluded app is running: exactly the listed windows that exist.
        assert_eq!(
            filter_spec(&e, &[app(40, "com.other")], &windows),
            FilterSpec::Windows(vec![5, 6])
        );
        // The vault is running: its pid, plus the owner of the listed window 5.
        assert_eq!(
            filter_spec(&e, &[app(40, "com.other"), app(50, "com.VAULT")], &windows),
            FilterSpec::Applications {
                pids: vec![40, 50],
                keep_windows: vec![]
            }
        );
        // A listed window id SCK does not know is ignored.
        e.window_ids = vec![99];
        assert_eq!(filter_spec(&e, &[], &windows), FilterSpec::Windows(vec![]));
    }

    fn win(id: u32, owner: u32) -> WindowRef {
        WindowRef {
            id,
            owner_pid: Some(owner),
        }
    }

    /// The library window of the excluded own process is kept; recorder windows are not, and an
    /// id SCK does not list (yet) or a window of another process is ignored.
    #[test]
    fn filter_spec_keeps_listed_windows_of_excluded_processes() {
        let mut e = ex(&[]);
        e.keep_window_ids = vec![100, 999, 300];
        let apps = [app(1, "dev.capturemann.app"), app(2, "com.other")];
        let windows = [win(100, 1), win(101, 1), win(300, 2)];
        assert_eq!(
            filter_spec(&e, &apps, &windows),
            FilterSpec::Applications {
                pids: vec![1],
                keep_windows: vec![100]
            }
        );
        // Not listed yet: stays excluded with its process.
        assert_eq!(
            filter_spec(&e, &apps, &[win(101, 1)]).keep_windows(),
            &[] as &[u32]
        );
    }

    /// A provider is queried on every call, so a window that appears or goes away changes the
    /// filter spec.
    #[test]
    fn keep_provider_is_queried_live() {
        use std::sync::{Arc, Mutex};
        let ids = Arc::new(Mutex::new(Vec::<u32>::new()));
        let i2 = ids.clone();
        let mut e = ex(&[]);
        e.keep_provider = Some(crate::record::WindowProvider::new(move || {
            i2.lock().map(|g| g.clone()).unwrap_or_default()
        }));
        let apps = [app(1, "dev.capturemann.app")];
        let windows = [win(100, 1), win(101, 1)];
        assert!(filter_spec(&e, &apps, &windows).keep_windows().is_empty());
        *ids.lock().unwrap() = vec![100];
        assert_eq!(filter_spec(&e, &apps, &windows).keep_windows(), &[100]);
    }

    /// The watch fires when a kept window appears or disappears, and is quiet otherwise.
    #[test]
    fn watch_fires_when_a_kept_window_appears_or_disappears() {
        let mut e = ex(&[]);
        e.keep_window_ids = vec![100];
        let apps = vec![app(1, "dev.capturemann.app")];
        let mut windows = vec![win(101, 1)];
        let mut watch = ExclusionWatch::new(matching_pids(&apps, &e))
            .with_keep(filter_spec(&e, &apps, &windows).keep_windows().to_vec());
        assert_eq!(watch.needs_filter_update(&apps, &windows, &e), None);

        windows.push(win(100, 1));
        let (pids, keep) = watch
            .needs_filter_update(&apps, &windows, &e)
            .expect("kept window appeared");
        assert_eq!((pids.clone(), keep.clone()), (vec![1], vec![100]));
        watch.applied_with_keep(pids, keep);
        assert_eq!(watch.needs_filter_update(&apps, &windows, &e), None);

        windows.retain(|w| w.id != 100);
        let (pids, keep) = watch
            .needs_filter_update(&apps, &windows, &e)
            .expect("kept window went away");
        assert!(keep.is_empty());
        watch.applied_with_keep(pids, keep);
        assert_eq!(watch.needs_filter_update(&apps, &windows, &e), None);
    }
}
