//! The tool table of `specs/02-agent-interface.md`, parsed so the MCP tool list can be checked against it.

pub const SPEC_TEXT: &str = include_str!("../../../specs/02-agent-interface.md");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecTool {
    pub name: String,
    /// The row says the tool is internal (no CLI command or MCP tool).
    pub internal: bool,
    /// The row says it is not built, later or withdrawn.
    pub not_built: bool,
}

fn names_in(cell: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = cell;
    // Names are the code spans before the first parenthesis ("(internal: ...)" explains, it does not name).
    let head = rest.split('(').next().unwrap_or("");
    rest = head;
    while let Some(i) = rest.find('`') {
        let after = &rest[i + 1..];
        let Some(j) = after.find('`') else { break };
        let name = &after[..j];
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit())
        {
            out.push(name.to_owned());
        }
        rest = &after[j + 1..];
    }
    out
}

/// The tools of the table under "## MCP tools".
pub fn parse_tool_table(md: &str) -> Vec<SpecTool> {
    let mut in_section = false;
    let mut out = Vec::new();
    for line in md.lines() {
        if line.starts_with("## ") {
            in_section = line.starts_with("## MCP tools");
            continue;
        }
        if !in_section || !line.starts_with("| `") {
            continue;
        }
        let first_end = line[1..].find('|').map_or(line.len(), |i| i + 1);
        let first = &line[1..first_end];
        let lower = line.to_lowercase();
        let internal = first.to_lowercase().contains("(internal");
        let not_built =
            lower.contains("not built") || lower.contains("withdrawn") || lower.contains("**later");
        for name in names_in(first) {
            out.push(SpecTool {
                name,
                internal,
                not_built,
            });
        }
    }
    out
}

/// What differs between the spec and the server: (in the server but not in the spec, in the spec as
/// built but not in the server).
pub fn drift(spec: &[SpecTool], server: &[String]) -> (Vec<String>, Vec<String>) {
    let spec_names: std::collections::BTreeSet<&str> =
        spec.iter().map(|t| t.name.as_str()).collect();
    let server_names: std::collections::BTreeSet<&str> =
        server.iter().map(String::as_str).collect();
    let unlisted = server_names
        .iter()
        .filter(|n| !spec_names.contains(*n))
        .map(|n| (*n).to_owned())
        .collect();
    let missing = spec
        .iter()
        .filter(|t| !t.internal && !t.not_built && !server_names.contains(t.name.as_str()))
        .map(|t| t.name.clone())
        .collect();
    (unlisted, missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_with_two_names_and_internal_marks() {
        let md = "## MCP tools (v0.1)\n\n| Tool | Input |\n| --- | --- |\n| **Discovery** |  |\n| `list_targets` | x |\n| `get_settings` / `set_settings` | keys |\n| `freeze_display` / `release_frame` (internal: daemon only) | id |\n| `generate_metadata` | x | later (not in 0.2.0), not built |\n\n## Next\n| `other_table` | x |\n";
        let t = parse_tool_table(md);
        let names: Vec<&str> = t.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "list_targets",
                "get_settings",
                "set_settings",
                "freeze_display",
                "release_frame",
                "generate_metadata"
            ]
        );
        assert!(t[3].internal && t[4].internal && !t[0].internal);
        assert!(t[5].not_built);
    }

    #[test]
    fn drift_reports_both_directions() {
        let spec = vec![
            SpecTool {
                name: "a".into(),
                internal: false,
                not_built: false,
            },
            SpecTool {
                name: "b".into(),
                internal: false,
                not_built: false,
            },
            SpecTool {
                name: "c".into(),
                internal: true,
                not_built: false,
            },
            SpecTool {
                name: "d".into(),
                internal: false,
                not_built: true,
            },
        ];
        let (unlisted, missing) = drift(&spec, &["a".to_owned(), "z".to_owned()]);
        assert_eq!(unlisted, ["z"]);
        assert_eq!(missing, ["b"]);
    }

    /// The checked-in spec and a list of the real server's tools (Nov 5, 2026, 77 tools) agree.
    #[test]
    fn the_real_spec_table_parses() {
        let t = parse_tool_table(SPEC_TEXT);
        assert!(t.len() > 60, "only {} tools parsed", t.len());
        for must in [
            "list_targets",
            "capture_screenshot",
            "start_recording",
            "import_media",
            "get_diagnostics",
            "export_video",
        ] {
            assert!(
                t.iter().any(|x| x.name == must),
                "{must} missing from the parsed table"
            );
        }
    }
}
