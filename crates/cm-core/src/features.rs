//! The feature-stage registry (design S24, the Preview label).
//!
//! One table says which big features are still `Preview` and which are `Stable`. Everything
//! that marks a preview reads it: the app's Preview label, the MCP tool listing (`stage`, "(preview)")
//! and the CLI help (`[preview]`).
//! A preview is only a label: it is always available (the Settings switch was dropped on
//! Nov 16, 2026). **Moving a feature to stable is a one-line change here**
//! (`stage: Stage::Stable`): every label and the agent markers go away at once.

/// Where a feature is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// Ready to use and supported, but its look or behaviour may change in the next releases.
    Preview,
    /// Finished: no label, no switch.
    Stable,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Preview => "preview",
            Stage::Stable => "stable",
        }
    }
}

/// One feature of the registry.
#[derive(Debug, Clone, Copy)]
pub struct Feature {
    /// Stable id, used in the intro flag and the feedback link.
    pub id: &'static str,
    /// The English name (the UI translates it with its own catalogue).
    pub name: &'static str,
    pub stage: Stage,
    /// MCP tools (= actions) that belong to the feature and only to it.
    pub tools: &'static [&'static str],
    /// Top-level CLI subcommands that belong to the feature.
    pub cli_commands: &'static [&'static str],
    /// CLI flags `(command path, argument id)` of a shared command that start the feature.
    pub cli_args: &'static [(&'static str, &'static str)],
    /// MCP parameters `(tool, parameter)` of a shared tool that start the feature.
    pub tool_params: &'static [(&'static str, &'static str)],
}

impl Feature {
    pub fn is_preview(&self) -> bool {
        self.stage == Stage::Preview
    }
}

/// Every feature that has (or had) a stage.
pub const FEATURES: &[Feature] = &[
    Feature {
        id: "long_shot",
        name: "Long Shot",
        stage: Stage::Preview,
        // `capture_screenshot` with `mode: "long"` is the entry point; it is a shared tool.
        tools: &["long_shot_state"],
        cli_commands: &[],
        cli_args: &[
            ("capture screenshot", "long"),
            ("capture screenshot", "horizontal"),
            ("capture screenshot", "max_px"),
            ("capture screenshot", "hand"),
        ],
        tool_params: &[("capture_screenshot", "mode")],
    },
    Feature {
        id: "click_guide",
        name: "Click Guide",
        stage: Stage::Preview,
        tools: &[
            "start_guide",
            "stop_guide",
            "get_guide",
            "set_guide",
            "export_guide",
        ],
        cli_commands: &["guide"],
        cli_args: &[],
        tool_params: &[],
    },
];

/// The feature with this id.
pub fn feature(id: &str) -> Option<&'static Feature> {
    FEATURES.iter().find(|f| f.id == id)
}

/// The features that are in the Preview stage, in table order.
pub fn previews() -> impl Iterator<Item = &'static Feature> {
    FEATURES.iter().filter(|f| f.is_preview())
}

/// The feature a tool (action) belongs to, when it belongs to one only.
pub fn feature_of_tool(tool: &str) -> Option<&'static Feature> {
    FEATURES.iter().find(|f| f.tools.contains(&tool))
}

/// Whether the tool belongs to a feature that is in the Preview stage.
pub fn is_preview_tool(tool: &str) -> bool {
    feature_of_tool(tool).is_some_and(Feature::is_preview)
}

/// The preview feature that a parameter of a shared tool starts (`capture_screenshot`.`mode`).
pub fn preview_param(tool: &str, param: &str) -> Option<&'static Feature> {
    previews().find(|f| f.tool_params.contains(&(tool, param)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_are_unique_and_keys_are_valid() {
        let mut seen = std::collections::HashSet::new();
        for f in FEATURES {
            assert!(seen.insert(f.id), "duplicate {}", f.id);
            assert!(f.id.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'));
        }
        let ids: Vec<_> = previews().map(|f| f.id).collect();
        assert_eq!(ids, ["long_shot", "click_guide"]);
    }

    #[test]
    fn a_tool_belongs_to_one_feature() {
        for f in FEATURES {
            for t in f.tools {
                assert_eq!(feature_of_tool(t).unwrap().id, f.id);
            }
        }
        assert!(is_preview_tool("start_guide"));
        assert!(!is_preview_tool("capture_screenshot"));
        assert_eq!(
            preview_param("capture_screenshot", "mode").unwrap().id,
            "long_shot"
        );
        assert!(preview_param("capture_screenshot", "target").is_none());
    }

    #[test]
    fn a_stable_feature_has_no_switch() {
        // What `Stage::Stable` means: not a preview, so it carries no label.
        let f = Feature {
            stage: Stage::Stable,
            ..FEATURES[0]
        };
        assert!(!f.is_preview());
        assert_eq!(Stage::Stable.as_str(), "stable");
        assert_eq!(Stage::Preview.as_str(), "preview");
    }
}
