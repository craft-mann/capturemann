//! Auto-redact in the annotation editor (ANN-2.AC4): the rectangles `suggest_redactions`
//! proposed, shown as dashed outlines, and what accepting them does. No GPUI types here.
//!
//! Proposals are only suggestions: they live in the editor state, never in the capture. Accepting
//! one or all of them is one `Edit::Append` of blur ops, so it goes through the normal op
//! pipeline (`annotate`) and is one undo step. Nothing is applied until then.

use cm_core::{AnnotationOp, OpKind, Pt, Rc};
use cm_i18n::tr_ctx;
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
        match self.kind.as_str() {
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

/// The params of the daemon call.
pub fn request(capture_id: &str) -> Value {
    json!({ "capture_id": capture_id })
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
