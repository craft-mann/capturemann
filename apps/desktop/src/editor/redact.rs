//! Auto-redact in the annotation editor (ANN-2.AC4): the rectangles `suggest_redactions`
//! proposed, shown as dashed outlines, and what accepting them does. No GPUI types here.
//!
//! Proposals are only suggestions: they live in the editor state, never in the capture. Accepting
//! one or all of them is one `Edit::Append` of blur ops, so it goes through the normal op
//! pipeline (`annotate`) and is one undo step. Nothing is applied until then.

use cm_core::{AnnotationOp, OpKind, Pt, Rc};
use cm_i18n::tr_ctx;
use cm_render::ocr::RedactionKind;
use serde_json::{json, Value};

use super::state::{Edit, EditorState};

/// Blur strength used for accepted proposals when the Blur tool's own setting is weaker: text
/// has to be unreadable (AC1), not just soft.
pub const MIN_STRENGTH: u32 = 16;

/// One proposed rectangle, in original-image pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub rect: Rc,
    /// A `RedactionKind` name such as `email`, `card` or `ssn`.
    pub kind: String,
    pub confidence: f64,
    /// The text, masked by the daemon (first and last two characters only).
    pub preview: String,
}

impl Proposal {
    /// "Email" / "Key or token" / ...
    pub fn kind_label(&self) -> String {
        kind_label(&self.kind)
    }
}

/// The translated name of a `RedactionKind` (`as_str()` name); "Text" for an unknown one.
pub fn kind_label(kind: &str) -> String {
    {
        match kind {
            "email" => tr_ctx!("redaction kind", "Email"),
            "token" => tr_ctx!("redaction kind", "Key or token"),
            "ip_address" => tr_ctx!("redaction kind", "IP address"),
            "phone" => tr_ctx!("redaction kind", "Phone number"),
            "private_key" => tr_ctx!("redaction kind", "Private key"),
            "secret_assignment" => tr_ctx!("redaction kind", "Secret value"),
            "url_secret" => tr_ctx!("redaction kind", "Link with a secret"),
            "card" => tr_ctx!("redaction kind", "Card number"),
            "card_extra" => tr_ctx!("redaction kind", "Card expiry or code"),
            "ssn" => tr_ctx!("redaction kind", "Social Security number"),
            "itin" => tr_ctx!("redaction kind", "Taxpayer ID (ITIN)"),
            "ein" => tr_ctx!("redaction kind", "Employer ID (EIN)"),
            "aba" => tr_ctx!("redaction kind", "Bank routing number"),
            "iban" => tr_ctx!("redaction kind", "IBAN"),
            "npi" => tr_ctx!("redaction kind", "Provider ID (NPI)"),
            "dea" => tr_ctx!("redaction kind", "DEA number"),
            "mbi" => tr_ctx!("redaction kind", "Medicare ID"),
            "passport" => tr_ctx!("redaction kind", "Passport number"),
            "vin" => tr_ctx!("redaction kind", "Vehicle ID (VIN)"),
            "imei" => tr_ctx!("redaction kind", "Device ID (IMEI)"),
            "mac_address" => tr_ctx!("redaction kind", "MAC address"),
            _ => tr_ctx!("redaction kind", "Text"),
        }
    }
}

/// Reads the `rects` of a `suggest_redactions` result.
pub fn parse_proposals(v: &Value) -> Result<Vec<Proposal>, String> {
    let rects = v
        .get("rects")
        .and_then(Value::as_array)
        .ok_or_else(|| "suggest_redactions returned no rects".to_owned())?;
    rects
        .iter()
        .map(|r| {
            let a = r
                .get("rect")
                .and_then(Value::as_array)
                .filter(|a| a.len() == 4)
                .ok_or_else(|| "a proposed rect is not [x, y, w, h]".to_owned())?;
            let n = |i: usize| a[i].as_f64().ok_or_else(|| "bad rect number".to_owned());
            Ok(Proposal {
                rect: Rc::new(n(0)?, n(1)?, n(2)?, n(3)?),
                kind: r
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("text")
                    .to_owned(),
                confidence: r.get("confidence").and_then(Value::as_f64).unwrap_or(0.0),
                preview: r
                    .get("text_preview")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            })
        })
        .collect()
}

/// Whether this machine has a text recognizer. The editor hides Auto-redact when it does not
/// (ANN-2 edge case). Checked once: loading Vision is not free and the answer does not change.
pub fn available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(cm_render::ocr::system_available)
}

/// The number of kind groups in the Auto-redact panel.
pub const GROUP_COUNT: usize = 4;

/// The title of group `i` (see [`group_kinds`]).
pub fn group_title(i: usize) -> String {
    match i {
        0 => tr_ctx!("redaction group", "Contact and network"),
        1 => tr_ctx!("redaction group", "Secrets"),
        2 => tr_ctx!("redaction group", "Payment"),
        _ => tr_ctx!("redaction group", "Identity and health"),
    }
}

/// The kinds of group `i`. Together the groups cover every `RedactionKind` exactly once.
pub fn group_kinds(i: usize) -> &'static [RedactionKind] {
    use RedactionKind::*;
    match i {
        0 => &[Email, Phone, IpAddress, MacAddress],
        1 => &[Token, PrivateKey, SecretAssignment, UrlSecret],
        2 => &[Card, CardExtra, Aba, Iban],
        _ => &[Ssn, Itin, Ein, Passport, Npi, Dea, Mbi, Vin, Imei],
    }
}

/// Which kinds Auto-redact looks for. Starts as the daemon's defaults; never empty (an empty
/// `kinds` param means "defaults", so the last kind cannot be switched off).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindChoice {
    on: [bool; RedactionKind::ALL.len()],
}

impl Default for KindChoice {
    fn default() -> Self {
        let mut on = [false; RedactionKind::ALL.len()];
        for k in RedactionKind::DEFAULT {
            on[Self::index(k)] = true;
        }
        KindChoice { on }
    }
}

impl KindChoice {
    fn index(k: RedactionKind) -> usize {
        RedactionKind::ALL
            .iter()
            .position(|a| *a == k)
            .unwrap_or_default()
    }

    pub fn is_on(&self, k: RedactionKind) -> bool {
        self.on[Self::index(k)]
    }

    pub fn is_default(&self) -> bool {
        *self == KindChoice::default()
    }

    fn count(&self) -> usize {
        self.on.iter().filter(|b| **b).count()
    }

    /// Switches one kind; refused (false) when it would leave nothing to look for.
    pub fn toggle(&mut self, k: RedactionKind) -> bool {
        let i = Self::index(k);
        if self.on[i] && self.count() == 1 {
            return false;
        }
        self.on[i] = !self.on[i];
        true
    }

    /// Whether every kind of group `g` is on.
    pub fn group_on(&self, g: usize) -> bool {
        group_kinds(g).iter().all(|k| self.is_on(*k))
    }

    /// Switches a whole group: off when it is fully on, otherwise on. Refused (false) when it
    /// would leave nothing to look for.
    pub fn toggle_group(&mut self, g: usize) -> bool {
        let turn_on = !self.group_on(g);
        let mut next = self.clone();
        for k in group_kinds(g) {
            next.on[Self::index(*k)] = turn_on;
        }
        if next.count() == 0 {
            return false;
        }
        *self = next;
        true
    }

    pub fn reset(&mut self) {
        *self = KindChoice::default();
    }

    /// The `kinds` param: empty (= the daemon's defaults) when the choice is the default,
    /// otherwise the names in `RedactionKind::ALL` order.
    pub fn kinds_param(&self) -> Vec<&'static str> {
        if self.is_default() {
            return Vec::new();
        }
        RedactionKind::ALL
            .iter()
            .filter(|k| self.is_on(**k))
            .map(|k| k.as_str())
            .collect()
    }
}

/// The choice for this run of the app (not saved: the app has no app-only UI preference store
/// for the editor).
static SESSION_KINDS: std::sync::Mutex<Option<KindChoice>> = std::sync::Mutex::new(None);

pub fn session_kinds() -> KindChoice {
    SESSION_KINDS
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default()
}

pub fn set_session_kinds(c: KindChoice) {
    if let Ok(mut g) = SESSION_KINDS.lock() {
        *g = Some(c);
    }
}

/// The params of the daemon call: `kinds` only when the choice differs from the defaults.
pub fn request(capture_id: &str, kinds: &KindChoice) -> Value {
    let names = kinds.kinds_param();
    if names.is_empty() {
        json!({ "capture_id": capture_id })
    } else {
        json!({ "capture_id": capture_id, "kinds": names })
    }
}

impl EditorState {
    /// Shows these proposals (replacing earlier ones). Nothing is applied.
    pub fn set_proposals(&mut self, proposals: Vec<Proposal>) {
        self.proposals = proposals;
    }

    /// Dismiss: forget the proposals without applying any.
    pub fn dismiss_proposals(&mut self) {
        self.proposals.clear();
    }

    /// The proposal under `p` (image px), the smallest one when several overlap.
    pub fn proposal_at(&self, p: Pt) -> Option<usize> {
        self.proposals
            .iter()
            .enumerate()
            .filter(|(_, pr)| {
                let r = pr.rect.normalized();
                p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h
            })
            .min_by(|(_, a), (_, b)| (a.rect.w * a.rect.h).total_cmp(&(b.rect.w * b.rect.h)))
            .map(|(i, _)| i)
    }

    fn blur_op(&mut self, rect: Rc) -> AnnotationOp {
        let strength = self
            .styles
            .get(super::Tool::Blur)
            .strength
            .max(MIN_STRENGTH);
        let id = self.tmp_id();
        AnnotationOp::new(id, OpKind::Blur { rect, strength })
    }

    /// Accepts one proposal (`Some(i)`) or all of them (`None`): they become blur ops in the
    /// local list and one `Edit::Append`, so it is a single undo step. The accepted proposals
    /// leave the list; the others stay.
    pub fn accept_proposals(&mut self, which: Option<usize>) -> Option<Edit> {
        let taken: Vec<Proposal> = match which {
            Some(i) if i < self.proposals.len() => vec![self.proposals.remove(i)],
            Some(_) => return None,
            None => std::mem::take(&mut self.proposals),
        };
        if taken.is_empty() {
            return None;
        }
        let ops: Vec<AnnotationOp> = taken.into_iter().map(|p| self.blur_op(p.rect)).collect();
        self.ops.extend(ops.iter().cloned());
        Some(Edit::Append(ops))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{ParamValue, Styles, Tool};

    fn st() -> EditorState {
        EditorState::new((800, 600), Vec::new(), Styles::default())
    }

    fn two() -> Vec<Proposal> {
        vec![
            Proposal {
                rect: Rc::new(100.0, 50.0, 200.0, 26.0),
                kind: "email".into(),
                confidence: 0.95,
                preview: "ja...om".into(),
            },
            Proposal {
                rect: Rc::new(100.0, 150.0, 240.0, 26.0),
                kind: "email".into(),
                confidence: 0.95,
                preview: "ap...rg".into(),
            },
        ]
    }

    #[test]
    fn groups_cover_every_kind_once() {
        let mut seen: Vec<&str> = (0..GROUP_COUNT)
            .flat_map(|g| group_kinds(g).iter().map(|k| k.as_str()))
            .collect();
        seen.sort_unstable();
        let mut all = RedactionKind::names();
        all.sort_unstable();
        assert_eq!(seen, all);
        for k in RedactionKind::ALL {
            assert_ne!(kind_label(k.as_str()), "Text", "{} has a label", k.as_str());
        }
    }

    #[test]
    fn default_selection_is_the_daemon_default_and_sends_no_kinds() {
        let c = KindChoice::default();
        assert!(c.is_default());
        assert!(c.is_on(RedactionKind::Email) && !c.is_on(RedactionKind::Npi));
        assert!(c.kinds_param().is_empty());
        assert_eq!(request("c1", &c), json!({"capture_id": "c1"}));
    }

    #[test]
    fn toggling_builds_an_explicit_list_and_toggling_back_returns_to_defaults() {
        let mut c = KindChoice::default();
        assert!(c.toggle(RedactionKind::Npi));
        let names = c.kinds_param();
        assert_eq!(names.len(), 21);
        assert!(names.contains(&"npi"));
        assert_eq!(request("c1", &c)["kinds"].as_array().unwrap().len(), 21);
        assert!(c.toggle(RedactionKind::Npi));
        assert!(c.is_default() && c.kinds_param().is_empty());

        assert!(c.toggle(RedactionKind::Phone));
        assert!(!c.kinds_param().contains(&"phone"));
        assert_eq!(c.kinds_param().len(), 19);
    }

    #[test]
    fn group_toggle_switches_the_whole_group() {
        let mut c = KindChoice::default();
        assert!(c.group_on(0) && !c.group_on(3), "npi is off by default");
        assert!(c.toggle_group(0));
        assert!(!c.group_on(0));
        assert!(!c.kinds_param().contains(&"email"));
        assert!(c.toggle_group(0));
        assert!(c.group_on(0));
        assert!(c.toggle_group(3), "a partly-on group switches fully on");
        assert!(c.is_on(RedactionKind::Npi));
    }

    #[test]
    fn the_last_kind_cannot_be_switched_off_and_reset_restores_defaults() {
        let mut c = KindChoice::default();
        c.toggle_group(3); // partly on -> fully on
        for g in 1..GROUP_COUNT {
            c.toggle_group(g);
        }
        for k in group_kinds(0).iter().skip(1) {
            c.toggle(*k);
        }
        assert_eq!(c.kinds_param(), vec!["email"]);
        assert!(!c.toggle(RedactionKind::Email));
        // A partly-on group switches on, not off; with only `email` left, no group is fully on
        // except by turning it on, so the guard cannot be hit through groups here.
        assert!(c.toggle_group(0) && c.group_on(0));
        c.reset();
        c.reset();
        assert!(c.is_default());
    }

    #[test]
    fn the_session_choice_is_remembered() {
        let mut c = KindChoice::default();
        c.toggle(RedactionKind::Npi);
        set_session_kinds(c.clone());
        assert_eq!(session_kinds(), c);
        set_session_kinds(KindChoice::default());
        assert!(session_kinds().is_default());
    }

    #[test]
    fn parses_the_daemon_result() {
        let v = json!({"rects": [
            {"rect": [10, 20, 30, 40], "kind": "token", "confidence": 0.6, "text_preview": "sk...wx"},
            {"rect": [1, 2, 3, 4], "kind": "email", "confidence": 0.95}]});
        let p = parse_proposals(&v).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].rect, Rc::new(10.0, 20.0, 30.0, 40.0));
        assert_eq!(p[0].kind_label(), "Key or token");
        assert_eq!(p[1].preview, "");
        assert!(parse_proposals(&json!({})).is_err());
        assert!(parse_proposals(&json!({"rects": [{"rect": [1, 2, 3]}]})).is_err());
    }

    /// ANN-2.AC4: proposing changes nothing; accepting all is one append of blur ops.
    #[test]
    fn ann_2_ac4_nothing_is_applied_until_accepted() {
        let mut s = st();
        s.set_proposals(two());
        assert!(s.ops.is_empty(), "proposals are not ops");
        assert_eq!(s.proposals.len(), 2);

        let edit = s.accept_proposals(None).unwrap();
        let Edit::Append(ops) = edit else {
            panic!("one append expected")
        };
        assert_eq!(ops.len(), 2);
        assert!(ops.iter().all(|o| matches!(o.kind, OpKind::Blur { .. })));
        assert_eq!(s.ops.len(), 2, "optimistic local list");
        assert!(s.proposals.is_empty());
        assert!(s.accept_proposals(None).is_none());
    }

    #[test]
    fn accepting_one_keeps_the_others() {
        let mut s = st();
        s.set_proposals(two());
        let Some(Edit::Append(ops)) = s.accept_proposals(Some(1)) else {
            panic!("append expected")
        };
        assert_eq!(ops.len(), 1);
        assert!(matches!(&ops[0].kind, OpKind::Blur { rect, .. } if rect.y == 150.0));
        assert_eq!(s.proposals.len(), 1);
        assert_eq!(s.proposals[0].rect.y, 50.0);
        assert!(s.accept_proposals(Some(5)).is_none());
    }

    #[test]
    fn dismiss_forgets_everything_and_adds_no_op() {
        let mut s = st();
        s.set_proposals(two());
        s.dismiss_proposals();
        assert!(s.proposals.is_empty() && s.ops.is_empty());
    }

    #[test]
    fn blur_is_at_least_strength_16_but_keeps_a_stronger_tool_setting() {
        let mut s = st();
        s.set_proposals(two());
        let Some(Edit::Append(ops)) = s.accept_proposals(Some(0)) else {
            panic!()
        };
        assert!(matches!(ops[0].kind, OpKind::Blur { strength: 16, .. }));
        s.styles.apply(Tool::Blur, ParamValue::Strength(28));
        let Some(Edit::Append(ops)) = s.accept_proposals(Some(0)) else {
            panic!()
        };
        assert!(matches!(ops[0].kind, OpKind::Blur { strength: 28, .. }));
    }

    #[test]
    fn hit_testing_prefers_the_smallest_rect() {
        let mut s = st();
        let mut p = two();
        p.push(Proposal {
            rect: Rc::new(120.0, 55.0, 30.0, 10.0),
            ..p[0].clone()
        });
        s.set_proposals(p);
        assert_eq!(s.proposal_at(Pt::new(130.0, 60.0)), Some(2));
        assert_eq!(s.proposal_at(Pt::new(250.0, 60.0)), Some(0));
        assert_eq!(s.proposal_at(Pt::new(10.0, 10.0)), None);
    }
}
