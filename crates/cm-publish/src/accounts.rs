//! The connected publishing accounts: `publish_accounts.json` in the config folder.
//!
//! The desktop app is the only writer (the connect flow is human-only, PUB-1); the daemon reads
//! it for `list_publish_accounts` and to find the keychain entries of the account a publish uses.
//! The file holds no secret: an account's tokens live in the keychain under
//! `oauth.<provider>.<id>` and its client credentials under `oauth.<provider>.<id>.client`
//! (`cm_auth::credentials`), where `<id>` is [`PublishAccount::id`].
//!
//! ```json
//! { "version": 1,
//!   "accounts": [ { "id": "youtube-k3x9a2bq", "platform": "youtube", "name": "My Channel",
//!                   "account_id": "UC...", "default_privacy": "unlisted" } ] }
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use cm_core::CmError;
use serde::{Deserialize, Serialize};

use crate::meta::rfc3339_utc;
use crate::Privacy;

pub const FILE_NAME: &str = "publish_accounts.json";
const VERSION: u32 = 1;
const QUOTA_VERSION: u32 = 2;
const QUOTA_FILE: &str = "publish_quota.json";

/// Upload calls per day for one YouTube API project (PUB-2: "x of 100 uploads used today").
pub const YOUTUBE_UPLOADS_PER_DAY: u32 = 100;

/// One connected account (a YouTube channel; later a TikTok account or a Facebook Page).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishAccount {
    /// Stable key, such as `youtube-k3x9a2bq`; also the keychain account key.
    pub id: String,
    /// `youtube`, later `tiktok` and `facebook`.
    pub platform: String,
    /// What the user sees: the channel name.
    pub name: String,
    /// The platform's own id (the YouTube channel id).
    pub account_id: String,
    /// The privacy the publish sheet preselects for this account.
    pub default_privacy: Privacy,
}

impl PublishAccount {
    /// The privacy a publish takes when the caller gives none: the account's default, but never
    /// `public` (an agent that says nothing must not publish to the world).
    pub fn fallback_privacy(&self) -> Privacy {
        match self.default_privacy {
            Privacy::Public => Privacy::Unlisted,
            other => other,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct FileBody {
    version: u32,
    accounts: Vec<PublishAccount>,
}

/// Reads and writes `publish_accounts.json`.
#[derive(Debug, Clone)]
pub struct PublishAccountStore {
    path: PathBuf,
    /// The daily upload counter, written by the daemon after each upload.
    quota_path: PathBuf,
}

fn io_error(context: &str, path: &Path, e: &std::io::Error) -> CmError {
    CmError::Internal {
        message: format!("{context} {}: {e}", path.display()),
    }
}

impl PublishAccountStore {
    pub fn new(config_dir: impl AsRef<Path>) -> Self {
        PublishAccountStore {
            path: config_dir.as_ref().join(FILE_NAME),
            quota_path: config_dir.as_ref().join(QUOTA_FILE),
        }
    }

    /// All accounts. A missing file is an empty list; a damaged one is an error that names the
    /// file (it is not silently replaced).
    pub fn list(&self) -> Result<Vec<PublishAccount>, CmError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io_error("Cannot read", &self.path, &e)),
        };
        let body: FileBody = serde_json::from_str(&text).map_err(|e| CmError::Internal {
            message: format!("{} is damaged: {e}", self.path.display()),
        })?;
        Ok(body.accounts)
    }

    pub fn get(&self, id: &str) -> Result<PublishAccount, CmError> {
        self.list()?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or_else(|| CmError::NotFound {
                message: format!("no publishing account `{id}`"),
            })
    }

    /// The account named `id`, or the default account of `platform` (the only one, or the one
    /// chosen with [`make_default`](Self::make_default)): an agent that names no account publishes
    /// to the same channel the person would get in the app. The confirmation dialog shows it.
    pub fn resolve(&self, platform: &str, id: Option<&str>) -> Result<PublishAccount, CmError> {
        if let Some(id) = id {
            let a = self.get(id)?;
            if a.platform != platform {
                return Err(CmError::invalid_field(
                    "account_id",
                    format!("`{id}` is a {} account, not {platform}", a.platform),
                ));
            }
            return Ok(a);
        }
        self.default_for(platform)?.ok_or_else(|| {
            CmError::not_found(format!(
                "no {platform} account is connected; connect one in Settings"
            ))
        })
    }

    fn read_quota(&self) -> QuotaBody {
        fs::read_to_string(&self.quota_path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Uploads counted for `account_id` on the Pacific day that contains `now_unix`, when the
    /// project of the account is not known (counted per account).
    pub fn uploads_today(&self, account_id: &str, now_unix: i64) -> u32 {
        self.uploads_today_in(account_id, None, now_unix)
    }

    /// Uploads counted today for the Google Cloud project `project` (see [`project_key`]); two
    /// accounts that use one client share the count, as YouTube's 100 uploads a day are counted
    /// per project. A count an older version kept for `account_id` alone is added until the
    /// next upload moves it into the project's counter, so nothing is forgotten or counted twice.
    /// Without a project, the count of the account.
    pub fn uploads_today_in(&self, account_id: &str, project: Option<&str>, now_unix: i64) -> u32 {
        let today = pacific_day(now_unix);
        let q = self.read_quota();
        let on_today = |d: Option<&DayCount>| d.filter(|d| d.day == today).map_or(0, |d| d.count);
        let own = on_today(q.accounts.get(account_id));
        match project {
            Some(p) => on_today(q.projects.get(p)).saturating_add(own),
            None => own,
        }
    }

    /// Counts one successful upload for the account alone (project not known).
    pub fn record_upload(&self, account_id: &str, now_unix: i64) -> Result<u32, CmError> {
        self.record_upload_in(account_id, None, now_unix)
    }

    /// Counts one successful upload. With a project the count goes to the project's counter (and
    /// takes along the account's own older count); without one it stays per account. Counters of
    /// other days are dropped. Returns the count that [`uploads_today_in`](Self::uploads_today_in)
    /// now reports.
    pub fn record_upload_in(
        &self,
        account_id: &str,
        project: Option<&str>,
        now_unix: i64,
    ) -> Result<u32, CmError> {
        let today = pacific_day(now_unix);
        let mut q = self.read_quota();
        q.accounts.retain(|_, d| d.day == today);
        q.projects.retain(|_, d| d.day == today);
        let n = match project {
            Some(p) => {
                let moved = q.accounts.remove(account_id).map_or(0, |d| d.count);
                let entry = q.projects.entry(p.to_owned()).or_insert_with(|| DayCount {
                    day: today.clone(),
                    count: 0,
                });
                entry.count = entry.count.saturating_add(moved).saturating_add(1);
                entry.count
            }
            None => {
                let entry = q
                    .accounts
                    .entry(account_id.to_owned())
                    .or_insert_with(|| DayCount {
                        day: today.clone(),
                        count: 0,
                    });
                entry.count += 1;
                entry.count
            }
        };
        q.version = QUOTA_VERSION;
        let text =
            serde_json::to_string_pretty(&q).map_err(|e| CmError::internal(e.to_string()))?;
        if let Some(dir) = self.quota_path.parent() {
            fs::create_dir_all(dir).map_err(|e| io_error("Cannot create", dir, &e))?;
        }
        let temp = self.quota_path.with_extension("json.tmp");
        fs::write(&temp, text).map_err(|e| io_error("Cannot write", &temp, &e))?;
        fs::rename(&temp, &self.quota_path)
            .map_err(|e| io_error("Cannot write", &self.quota_path, &e))?;
        // `n` is the project's count; the account's own older count was folded into it.
        Ok(n)
    }

    /// Replaces the whole list (temp file, then rename). Ids must be unique.
    pub fn save(&self, accounts: &[PublishAccount]) -> Result<(), CmError> {
        let mut ids: Vec<&str> = accounts.iter().map(|a| a.id.as_str()).collect();
        ids.sort_unstable();
        if ids.windows(2).any(|w| w[0] == w[1]) {
            return Err(CmError::InvalidInput {
                message: "publishing account ids must be unique".into(),
            });
        }
        let text = serde_json::to_string_pretty(&FileBody {
            version: VERSION,
            accounts: accounts.to_vec(),
        })
        .map_err(|e| CmError::Internal {
            message: e.to_string(),
        })?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| io_error("Cannot create", dir, &e))?;
        }
        let temp = self.path.with_extension("json.tmp");
        let mut f = fs::File::create(&temp).map_err(|e| io_error("Cannot write", &temp, &e))?;
        f.write_all(text.as_bytes())
            .map_err(|e| io_error("Cannot write", &temp, &e))?;
        f.sync_all()
            .map_err(|e| io_error("Cannot write", &temp, &e))?;
        fs::rename(&temp, &self.path).map_err(|e| io_error("Cannot write", &self.path, &e))
    }

    /// Adds the account, or replaces the one with the same id.
    pub fn upsert(&self, account: PublishAccount) -> Result<(), CmError> {
        let mut list = self.list()?;
        match list.iter_mut().find(|a| a.id == account.id) {
            Some(slot) => *slot = account,
            None => list.push(account),
        }
        self.save(&list)
    }

    /// Removes the account; `None` when there was none.
    pub fn remove(&self, id: &str) -> Result<Option<PublishAccount>, CmError> {
        let mut list = self.list()?;
        let Some(at) = list.iter().position(|a| a.id == id) else {
            return Ok(None);
        };
        let gone = list.remove(at);
        self.save(&list)?;
        Ok(Some(gone))
    }

    /// Makes `id` the default account of its platform: the first of its platform in the file,
    /// which the publish sheet preselects. The order of the others is kept.
    pub fn make_default(&self, id: &str) -> Result<(), CmError> {
        let mut list = self.list()?;
        let at = list
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| CmError::NotFound {
                message: format!("no publishing account `{id}`"),
            })?;
        let platform = list[at].platform.clone();
        let first = list
            .iter()
            .position(|a| a.platform == platform)
            .unwrap_or(at);
        if first != at {
            let account = list.remove(at);
            list.insert(first, account);
            self.save(&list)?;
        }
        Ok(())
    }

    /// The default account of `platform`: the first one connected or chosen with
    /// [`make_default`](Self::make_default).
    pub fn default_for(&self, platform: &str) -> Result<Option<PublishAccount>, CmError> {
        Ok(self.list()?.into_iter().find(|a| a.platform == platform))
    }
}

/// `publish_quota.json`. Version 1 had only `accounts` (a counter per account id). Version 2
/// adds `projects` (a counter per Google Cloud project). The shape stays readable both ways: a
/// version 1 file reads as it is, and an older program ignores `projects`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct QuotaBody {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    accounts: BTreeMap<String, DayCount>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    projects: BTreeMap<String, DayCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DayCount {
    /// `YYYY-MM-DD`, Pacific.
    day: String,
    count: u32,
}

/// The key of the counter for a Google OAuth client: the project number that starts every Google
/// client ID (`123456789012-abc.apps.googleusercontent.com` gives `google-project:123456789012`),
/// else a short hash of the whole ID. Two accounts that use the same client, or two clients of
/// one project, get the same key. The client ID is not a secret, but only this key is stored.
pub fn project_key(client_id: &str) -> Option<String> {
    let id = client_id.trim();
    if id.is_empty() {
        return None;
    }
    if let Some((number, rest)) = id.split_once('-') {
        if !number.is_empty()
            && number.chars().all(|c| c.is_ascii_digit())
            && rest.ends_with(".apps.googleusercontent.com")
        {
            return Some(format!("google-project:{number}"));
        }
    }
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(id.as_bytes());
    let short: String = hash.iter().take(8).map(|b| format!("{b:02x}")).collect();
    Some(format!("google-client:{short}"))
}

// ---- US Pacific time -----------------------------------------------------------------------

/// Days since 1970-01-01 of a civil date (Howard Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Day of the week of a day number: 0 = Sunday.
fn weekday(days: i64) -> i64 {
    (days + 4).rem_euclid(7)
}

/// The `n`th (1-based) Sunday of a month, as a day number.
fn nth_sunday(y: i64, m: i64, n: i64) -> i64 {
    let first = days_from_civil(y, m, 1);
    first + (7 - weekday(first)) % 7 + 7 * (n - 1)
}

/// Seconds to add to UTC to get Pacific time at this instant: -8 h (PST) or -7 h (PDT).
fn pacific_offset(unix: i64) -> i64 {
    let (y, _, _) = civil_from_days(unix.div_euclid(86_400));
    // DST starts at 02:00 PST (10:00 UTC) and ends at 02:00 PDT (09:00 UTC).
    let start = nth_sunday(y, 3, 2) * 86_400 + 10 * 3600;
    let end = nth_sunday(y, 11, 1) * 86_400 + 9 * 3600;
    if (start..end).contains(&unix) {
        -7 * 3600
    } else {
        -8 * 3600
    }
}

/// The Pacific calendar day (`YYYY-MM-DD`) that contains this instant.
pub fn pacific_day(unix: i64) -> String {
    let local = unix + pacific_offset(unix);
    let (y, m, d) = civil_from_days(local.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// The instant the next Pacific day starts, as Unix seconds (when the daily counter resets).
pub fn next_pacific_midnight(unix: i64) -> i64 {
    let local = unix + pacific_offset(unix);
    let next_local = (local.div_euclid(86_400) + 1) * 86_400;
    // Try both offsets; the one that maps back into the next day is right (the hour of the
    // DST change is never near midnight).
    for off in [-7 * 3600, -8 * 3600] {
        let candidate = next_local - off;
        if pacific_offset(candidate) == off {
            return candidate;
        }
    }
    next_local + 8 * 3600
}

/// [`next_pacific_midnight`] as RFC 3339 UTC.
pub fn next_pacific_midnight_rfc3339(unix: i64) -> String {
    rfc3339_utc(next_pacific_midnight(unix))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acct(id: &str) -> PublishAccount {
        PublishAccount {
            id: id.into(),
            platform: "youtube".into(),
            name: "My Channel".into(),
            account_id: "UC1".into(),
            default_privacy: Privacy::Unlisted,
        }
    }

    #[test]
    fn round_trip_upsert_remove() {
        let dir = tempfile::tempdir().unwrap();
        let s = PublishAccountStore::new(dir.path());
        assert!(s.list().unwrap().is_empty());
        s.upsert(acct("youtube-a")).unwrap();
        let mut b = acct("youtube-b");
        b.default_privacy = Privacy::Private;
        s.upsert(b).unwrap();
        let mut a2 = acct("youtube-a");
        a2.name = "Renamed".into();
        s.upsert(a2).unwrap();
        let list = s.list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(s.get("youtube-a").unwrap().name, "Renamed");
        let text = fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
        assert!(text.contains("\"default_privacy\": \"unlisted\""));
        assert!(s.remove("youtube-a").unwrap().is_some());
        assert!(s.remove("youtube-a").unwrap().is_none());
        assert_eq!(s.list().unwrap().len(), 1);
    }

    #[test]
    fn making_an_account_the_default_moves_it_first_among_its_platform() {
        let dir = tempfile::tempdir().unwrap();
        let s = PublishAccountStore::new(dir.path());
        for id in ["youtube-a", "youtube-b", "youtube-c"] {
            s.upsert(acct(id)).unwrap();
        }
        assert_eq!(s.default_for("youtube").unwrap().unwrap().id, "youtube-a");
        s.make_default("youtube-c").unwrap();
        let ids: Vec<String> = s.list().unwrap().into_iter().map(|a| a.id).collect();
        assert_eq!(ids, ["youtube-c", "youtube-a", "youtube-b"]);
        assert_eq!(s.default_for("youtube").unwrap().unwrap().id, "youtube-c");
        s.make_default("youtube-c").unwrap();
        assert_eq!(s.list().unwrap()[0].id, "youtube-c", "already the default");
        assert_eq!(
            s.make_default("youtube-zzz").unwrap_err().code(),
            cm_core::ErrorCode::NotFound
        );
        assert_eq!(s.default_for("tiktok").unwrap(), None);
    }

    #[test]
    fn a_damaged_file_is_an_error_not_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), "{ nope").unwrap();
        let s = PublishAccountStore::new(dir.path());
        assert!(s.list().unwrap_err().to_string().contains("damaged"));
        assert!(s.upsert(acct("x")).is_err());
    }

    fn store_in(dir: &tempfile::TempDir) -> PublishAccountStore {
        PublishAccountStore::new(dir.path())
    }

    #[test]
    fn resolve_takes_the_named_or_default_account() {
        let dir = tempfile::tempdir().unwrap();
        let s = store_in(&dir);
        assert_eq!(
            s.resolve("youtube", None).unwrap_err().code(),
            cm_core::ErrorCode::NotFound
        );
        s.upsert(acct("youtube-a")).unwrap();
        assert_eq!(s.resolve("youtube", None).unwrap().id, "youtube-a");
        s.upsert(acct("youtube-b")).unwrap();
        // Two accounts: an agent that names none gets the default channel, as a person would.
        assert_eq!(s.resolve("youtube", None).unwrap().id, "youtube-a");
        s.make_default("youtube-b").unwrap();
        assert_eq!(s.resolve("youtube", None).unwrap().id, "youtube-b");
        assert_eq!(
            s.resolve("youtube", Some("youtube-a")).unwrap().id,
            "youtube-a"
        );
        assert!(s.resolve("tiktok", Some("youtube-b")).is_err());
    }

    #[test]
    fn a_stored_public_default_is_never_used_silently() {
        let mut a = acct("x");
        a.default_privacy = Privacy::Public;
        assert_eq!(a.fallback_privacy(), Privacy::Unlisted);
        a.default_privacy = Privacy::Private;
        assert_eq!(a.fallback_privacy(), Privacy::Private);
    }

    // 2026-10-23 12:00 UTC.
    const OCT_23: i64 = 1_792_756_800;

    #[test]
    fn pacific_days_follow_daylight_saving_time() {
        assert_eq!(rfc3339_utc(OCT_23), "2026-10-23T12:00:00Z");
        assert_eq!(pacific_day(OCT_23), "2026-10-23");
        // 2026-10-24 06:59:59 UTC is still Oct 23 in PDT (UTC-7); 07:00:00 is Oct 24.
        let midnight = days_from_civil(2026, 10, 24) * 86_400 + 7 * 3600;
        assert_eq!(pacific_day(midnight - 1), "2026-10-23");
        assert_eq!(pacific_day(midnight), "2026-10-24");
        assert_eq!(next_pacific_midnight(OCT_23), midnight);
        assert_eq!(
            next_pacific_midnight_rfc3339(OCT_23),
            "2026-10-24T07:00:00Z"
        );
        // In winter (PST, UTC-8) the day turns at 08:00 UTC.
        let jan = days_from_civil(2027, 1, 15) * 86_400 + 12 * 3600;
        assert_eq!(pacific_day(jan), "2027-01-15");
        assert_eq!(next_pacific_midnight_rfc3339(jan), "2027-01-16T08:00:00Z");
    }

    #[test]
    fn dst_changes_on_the_right_sundays() {
        // 2026: starts Sunday March 8, ends Sunday November 1.
        assert_eq!(civil_from_days(nth_sunday(2026, 3, 2)), (2026, 3, 8));
        assert_eq!(civil_from_days(nth_sunday(2026, 11, 1)), (2026, 11, 1));
        let spring = nth_sunday(2026, 3, 2) * 86_400 + 10 * 3600;
        assert_eq!(pacific_offset(spring - 1), -8 * 3600);
        assert_eq!(pacific_offset(spring), -7 * 3600);
        let autumn = nth_sunday(2026, 11, 1) * 86_400 + 9 * 3600;
        assert_eq!(pacific_offset(autumn - 1), -7 * 3600);
        assert_eq!(pacific_offset(autumn), -8 * 3600);
        // The reset on the day DST ends lands at 08:00 UTC of the next day.
        let nov1_noon = days_from_civil(2026, 11, 1) * 86_400 + 20 * 3600;
        assert_eq!(pacific_day(nov1_noon), "2026-11-01");
        assert_eq!(
            next_pacific_midnight_rfc3339(nov1_noon),
            "2026-11-02T08:00:00Z"
        );
    }

    #[test]
    fn the_counter_counts_per_account_and_resets_at_pacific_midnight() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PublishAccountStore::new(tmp.path());
        assert_eq!(store.uploads_today("a", OCT_23), 0);
        assert_eq!(store.record_upload("a", OCT_23).unwrap(), 1);
        assert_eq!(store.record_upload("a", OCT_23 + 60).unwrap(), 2);
        assert_eq!(store.record_upload("b", OCT_23).unwrap(), 1);
        assert_eq!(store.uploads_today("a", OCT_23), 2);
        assert_eq!(store.uploads_today("b", OCT_23), 1);
        // The next Pacific day starts again at zero, and old counters are dropped.
        let next = next_pacific_midnight(OCT_23);
        assert_eq!(store.uploads_today("a", next - 1), 2);
        assert_eq!(store.uploads_today("a", next), 0);
        assert_eq!(store.record_upload("a", next).unwrap(), 1);
        assert_eq!(store.uploads_today("b", next), 0);
    }

    const PROJECT: &str = "google-project:123456789012";

    #[test]
    fn the_project_key_is_the_number_in_the_client_id() {
        let a = project_key("123456789012-abcdefg.apps.googleusercontent.com").unwrap();
        let b = project_key("123456789012-zzzzzzz.apps.googleusercontent.com").unwrap();
        assert_eq!(a, PROJECT);
        assert_eq!(a, b, "two clients of one project share the key");
        assert_ne!(
            a,
            project_key("999-abc.apps.googleusercontent.com").unwrap()
        );
        // Anything else is hashed, never stored as it is.
        let odd = project_key("my-custom-client").unwrap();
        assert!(odd.starts_with("google-client:") && !odd.contains("custom"));
        assert_eq!(odd, project_key(" my-custom-client ").unwrap());
        assert_eq!(project_key("  "), None);
    }

    #[test]
    fn two_accounts_with_one_client_share_the_daily_count() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PublishAccountStore::new(tmp.path());
        assert_eq!(
            store.record_upload_in("a", Some(PROJECT), OCT_23).unwrap(),
            1
        );
        assert_eq!(
            store.record_upload_in("b", Some(PROJECT), OCT_23).unwrap(),
            2
        );
        assert_eq!(
            store.record_upload_in("a", Some(PROJECT), OCT_23).unwrap(),
            3
        );
        for acct in ["a", "b"] {
            assert_eq!(store.uploads_today_in(acct, Some(PROJECT), OCT_23), 3);
        }
        // An account of another project is counted on its own.
        let other = "google-project:555";
        assert_eq!(store.uploads_today_in("c", Some(other), OCT_23), 0);
        assert_eq!(store.record_upload_in("c", Some(other), OCT_23).unwrap(), 1);
        assert_eq!(store.uploads_today_in("a", Some(PROJECT), OCT_23), 3);
    }

    #[test]
    fn the_project_count_resets_at_pacific_midnight() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PublishAccountStore::new(tmp.path());
        for _ in 0..3 {
            store.record_upload_in("a", Some(PROJECT), OCT_23).unwrap();
        }
        let next = next_pacific_midnight(OCT_23);
        assert_eq!(store.uploads_today_in("b", Some(PROJECT), next - 1), 3);
        assert_eq!(store.uploads_today_in("b", Some(PROJECT), next), 0);
        assert_eq!(store.record_upload_in("b", Some(PROJECT), next).unwrap(), 1);
        // The old day's counter was dropped when the new day wrote.
        let text = fs::read_to_string(tmp.path().join(QUOTA_FILE)).unwrap();
        assert!(
            text.contains("2026-10-24") && !text.contains("2026-10-23"),
            "{text}"
        );
        // Across the autumn change of clocks: Pacific midnight is 08:00 UTC after it.
        let nov1_noon = days_from_civil(2026, 11, 1) * 86_400 + 20 * 3600;
        store
            .record_upload_in("a", Some(PROJECT), nov1_noon)
            .unwrap();
        let nov2 = next_pacific_midnight(nov1_noon);
        assert_eq!(store.uploads_today_in("b", Some(PROJECT), nov2 - 1), 1);
        assert_eq!(store.uploads_today_in("b", Some(PROJECT), nov2), 0);
    }

    #[test]
    fn an_old_per_account_file_is_read_and_folded_into_the_project() {
        let tmp = tempfile::tempdir().unwrap();
        // The shape a version 1 program wrote (today's counters for two accounts).
        let day = pacific_day(OCT_23);
        fs::write(
            tmp.path().join(QUOTA_FILE),
            format!(
                r#"{{"version":1,"accounts":{{"a":{{"day":"{day}","count":4}},"b":{{"day":"{day}","count":2}}}}}}"#
            ),
        )
        .unwrap();
        let store = PublishAccountStore::new(tmp.path());
        // Without a project the old counters read as before.
        assert_eq!(store.uploads_today("a", OCT_23), 4);
        // With a project the account's own old count still counts: nothing is forgotten.
        assert_eq!(store.uploads_today_in("a", Some(PROJECT), OCT_23), 4);
        // The next upload moves it into the project (4 + 1) and does not count it twice.
        assert_eq!(
            store.record_upload_in("a", Some(PROJECT), OCT_23).unwrap(),
            5
        );
        assert_eq!(store.uploads_today_in("a", Some(PROJECT), OCT_23), 5);
        // The other account's old count is still its own until it uploads: 5 + 2.
        assert_eq!(store.uploads_today_in("b", Some(PROJECT), OCT_23), 7);
        assert_eq!(
            store.record_upload_in("b", Some(PROJECT), OCT_23).unwrap(),
            8
        );
        assert_eq!(store.uploads_today_in("a", Some(PROJECT), OCT_23), 8);
        // An old counter from an earlier day is ignored, and dropped by the next write.
        fs::write(
            tmp.path().join(QUOTA_FILE),
            r#"{"version":1,"accounts":{"a":{"day":"2020-01-01","count":99}}}"#,
        )
        .unwrap();
        assert_eq!(store.uploads_today_in("a", Some(PROJECT), OCT_23), 0);
    }

    #[test]
    fn the_file_with_projects_stays_readable_by_the_old_shape() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PublishAccountStore::new(tmp.path());
        store.record_upload_in("a", Some(PROJECT), OCT_23).unwrap();
        store.record_upload("z", OCT_23).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(tmp.path().join(QUOTA_FILE)).unwrap())
                .unwrap();
        assert_eq!(v["version"], 2);
        assert_eq!(v["accounts"]["z"]["count"], 1);
        assert_eq!(v["projects"][PROJECT]["count"], 1);
        // The account-only reader (version 1 program) still finds its counter.
        assert_eq!(store.uploads_today("z", OCT_23), 1);
    }

    #[test]
    fn a_damaged_counter_file_counts_as_zero() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(QUOTA_FILE), "garbage").unwrap();
        let store = PublishAccountStore::new(tmp.path());
        assert_eq!(store.uploads_today("a", OCT_23), 0);
        assert_eq!(store.record_upload("a", OCT_23).unwrap(), 1);
    }
}
