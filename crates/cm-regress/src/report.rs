//! `results.json` and the Markdown report (the format of `verification/README.md`). The report says
//! PARTIAL unless the maintainer overrides it with `--verdict`; the suite never writes "verified".

use crate::util::{cut, one_line};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseResult {
    pub id: String,
    pub title: String,
    pub area: String,
    pub tier: String,
    pub status: String,
    pub duration_ms: u64,
    pub message: String,
    pub evidence: Vec<String>,
    pub specs: Vec<String>,
    pub needs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunInfo {
    pub started: String,
    pub finished: String,
    pub who: String,
    pub tier: String,
    pub flags: String,
    pub platform: String,
    pub os: String,
    pub gpu: String,
    pub commit: String,
    pub version: String,
    pub package_dir: String,
    /// (file name, SHA-256)
    pub hashes: Vec<(String, String)>,
    pub ffmpeg: String,
    pub verdict: String,
}

pub fn results_json(info: &RunInfo, cases: &[CaseResult]) -> serde_json::Value {
    let count = |s: &str| cases.iter().filter(|c| c.status == s).count();
    serde_json::json!({
        "schema": 1,
        "run": info,
        "summary": { "pass": count("PASS"), "fail": count("FAIL"), "skip": count("SKIP"), "total": cases.len() },
        "cases": cases,
    })
}

fn cell(s: &str) -> String {
    one_line(s).replace('|', "\\|")
}

pub fn markdown(info: &RunInfo, cases: &[CaseResult]) -> String {
    let count = |s: &str| cases.iter().filter(|c| c.status == s).count();
    let mut m = String::new();
    m.push_str(&format!(
        "# L1 {} {} regression report ({})\n\n",
        info.platform, info.version, info.tier
    ));
    m.push_str(&format!(
        "- **Date:** {} to {}\n",
        info.started, info.finished
    ));
    m.push_str(&format!("- **Who:** {}\n", info.who));
    m.push_str(&format!(
        "- **Machine:** {}; GPU: {}\n",
        cell(&info.os),
        cell(&info.gpu)
    ));
    m.push_str(&format!(
        "- **Build:** commit {}, package folder `{}`\n",
        if info.commit.is_empty() {
            "unknown"
        } else {
            &info.commit
        },
        info.package_dir
    ));
    m.push_str(&format!("- **FFmpeg:** {}\n", cell(&info.ffmpeg)));
    m.push_str(&format!(
        "- **Suite:** `cm-regress {}`, tier `{}`\n",
        info.flags, info.tier
    ));
    m.push_str(&format!(
        "- **Result:** {} passed, {} failed, {} skipped of {} cases\n",
        count("PASS"),
        count("FAIL"),
        count("SKIP"),
        cases.len()
    ));
    if !info.hashes.is_empty() {
        m.push_str("- **SHA-256 of the files under test:**\n");
        for (n, h) in &info.hashes {
            m.push_str(&format!("  - `{n}` `{h}`\n"));
        }
    }
    m.push_str("\n## Steps\n\n| Case | Result | Notes |\n| --- | --- | --- |\n");
    for c in cases {
        let note = if c.message.is_empty() {
            c.title.clone()
        } else {
            format!("{}: {}", c.title, c.message)
        };
        m.push_str(&format!(
            "| `{}` | {} | {} |\n",
            c.id,
            c.status,
            cell(&cut(&note, 260))
        ));
    }

    // Spec ids and bug classes protected, with the worst result among their cases.
    let mut by_spec: BTreeMap<&str, Vec<&CaseResult>> = BTreeMap::new();
    for c in cases {
        for s in &c.specs {
            by_spec.entry(s.as_str()).or_default().push(c);
        }
    }
    m.push_str("\n## Acceptance criteria and bug classes\n\n| Item | Result | Cases |\n| --- | --- | --- |\n");
    for (spec, cs) in &by_spec {
        let worst = if cs.iter().any(|c| c.status == "FAIL") {
            "FAIL"
        } else if cs.iter().all(|c| c.status == "SKIP") {
            "not run"
        } else {
            "PASS"
        };
        let ids: Vec<String> = cs.iter().map(|c| format!("`{}`", c.id)).collect();
        m.push_str(&format!(
            "| {} | {} | {} |\n",
            cell(spec),
            worst,
            ids.join(", ")
        ));
    }

    m.push_str("\n## Problems found\n\n");
    let fails: Vec<&CaseResult> = cases.iter().filter(|c| c.status == "FAIL").collect();
    if fails.is_empty() {
        m.push_str("- None in the cases that ran.\n");
    }
    for c in fails {
        m.push_str(&format!(
            "### `{}`: {}\n\n- {}\n",
            c.id,
            c.title,
            cell(&c.message)
        ));
        if !c.evidence.is_empty() {
            m.push_str("\nEvidence:\n\n```text\n");
            for e in c.evidence.iter().take(60) {
                m.push_str(&cut(e, 400));
                m.push('\n');
            }
            m.push_str("```\n");
        }
        m.push('\n');
    }

    m.push_str("## Not run\n\n");
    let mut by_reason: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for c in cases.iter().filter(|c| c.status == "SKIP") {
        by_reason.entry(c.message.clone()).or_default().push(&c.id);
    }
    if by_reason.is_empty() {
        m.push_str("- Every selected case ran.\n");
    }
    for (reason, ids) in &by_reason {
        m.push_str(&format!(
            "- {} ({}): {}\n",
            cell(reason),
            ids.len(),
            ids.join(", ")
        ));
    }

    m.push_str("\n## Evidence of passing cases\n\n");
    for c in cases
        .iter()
        .filter(|c| c.status == "PASS" && !c.evidence.is_empty())
    {
        m.push_str(&format!(
            "- `{}` ({} ms): {}\n",
            c.id,
            c.duration_ms,
            cell(&cut(&c.evidence.join("; "), 500))
        ));
    }

    m.push_str(&format!(
        "\n## Verdict\n\n{}. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is \"verified\" until a person has read it and says so.\n",
        info.verdict
    ));
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(id: &str, status: &str, msg: &str, specs: &[&str]) -> CaseResult {
        CaseResult {
            id: id.into(),
            title: format!("title {id}"),
            area: "a".into(),
            tier: "smoke".into(),
            status: status.into(),
            duration_ms: 12,
            message: msg.into(),
            evidence: vec!["line one".into()],
            specs: specs.iter().map(|s| (*s).to_owned()).collect(),
            needs: vec![],
        }
    }

    fn info() -> RunInfo {
        RunInfo {
            started: "2026-11-05T10:00:00Z".into(),
            finished: "2026-11-05T10:02:00Z".into(),
            who: "cm-regress".into(),
            tier: "smoke".into(),
            flags: "--tier smoke".into(),
            platform: "windows".into(),
            version: "0.2.8".into(),
            verdict: "PARTIAL".into(),
            hashes: vec![("capturemann.exe".into(), "ab".repeat(32))],
            ..RunInfo::default()
        }
    }

    #[test]
    fn markdown_has_the_sections_and_never_says_verified() {
        let cases = vec![
            r("a.one", "PASS", "", &["CAP-1.AC1"]),
            r(
                "b.two",
                "FAIL",
                "size 1 != 2 | pipe",
                &["CAP-1.AC1", "REC-2"],
            ),
            r("c.three", "SKIP", "needs --allow sound", &["AUD-1"]),
            r("c.four", "SKIP", "needs --allow sound", &[]),
        ];
        let md = markdown(&info(), &cases);
        for h in [
            "## Steps",
            "## Acceptance criteria and bug classes",
            "## Problems found",
            "## Not run",
            "## Verdict",
        ] {
            assert!(md.contains(h), "{h}");
        }
        assert!(md.contains("1 passed, 1 failed, 2 skipped of 4"));
        assert!(md.contains("| CAP-1.AC1 | FAIL |"));
        assert!(md.contains("| AUD-1 | not run |"));
        assert!(md.contains("needs --allow sound (2): c.three, c.four"));
        assert!(md.contains("size 1 != 2 \\| pipe"));
        assert!(md.contains("PARTIAL."));
        assert!(!md.to_lowercase().contains("verdict\n\nverified"));
        assert!(md.contains(&"ab".repeat(32)));
    }

    #[test]
    fn json_summary_counts() {
        let v = results_json(
            &info(),
            &[r("a", "PASS", "", &[]), r("b", "FAIL", "x", &[])],
        );
        assert_eq!(v["summary"]["pass"], 1);
        assert_eq!(v["summary"]["fail"], 1);
        assert_eq!(v["summary"]["total"], 2);
        assert_eq!(v["cases"][1]["id"], "b");
        // Round trip.
        let back: Vec<CaseResult> = serde_json::from_value(v["cases"].clone()).unwrap();
        assert_eq!(back.len(), 2);
    }
}
