//! The icon registry against the redesign icon kit (`design/icons/`): every registered file
//! exists and follows the Lucide convention, and every icon the kit's map assigns to a built row
//! (P1, P2, keep) is available.

use gpui::AssetSource;

use crate::ui::paint::{Assets, ASSETS};

/// The kit's element -> icon map (P1, P2 and keep rows are built; `planned` rows are not).
const ICON_MAP_CSV: &str = include_str!("../../../../design/icons/icon-map.csv");
const ICONS_JSON: &str = include_str!("../../../../design/icons/icons.json");

fn registered(name: &str) -> bool {
    let path = format!("icons/{name}.svg");
    Assets.load(&path).ok().flatten().is_some()
}

/// Splits one CSV line into fields, honouring double quotes.
fn csv_fields(line: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted) = (Vec::new(), String::new(), false);
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// The icon names of one `icon` cell: "a / b", and the corner row's "cm-corner-top-left /
/// -top-right" shorthand.
fn cell_names(cell: &str) -> Vec<String> {
    cell.split(['/', ','])
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|n| {
            if n.starts_with('-') {
                format!("cm-corner{n}")
            } else {
                n.to_owned()
            }
        })
        .collect()
}

/// Every embedded icon parses as an SVG on the Lucide grid and takes the label colour
/// (`currentColor`), so `components::icon` can tint it.
#[test]
fn icons_registered_and_wellformed() {
    let mut n = 0;
    for (path, svg) in ASSETS.iter().filter(|(p, _)| p.starts_with("icons/")) {
        n += 1;
        assert!(svg.contains("<svg"), "{path}");
        assert!(svg.contains("viewBox=\"0 0 24 24\""), "{path}");
        assert!(svg.contains("currentColor"), "{path} must use currentColor");
        assert!(!svg.contains("#000"), "{path} has a hard-coded colour");
        assert!(
            svg.contains("stroke=\"currentColor\"") || svg.contains("fill=\"currentColor\""),
            "{path}"
        );
        assert!(svg.trim_end().ends_with("</svg>"), "{path}");
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join(path);
        assert!(file.exists(), "{path}");
        assert_eq!(&std::fs::read_to_string(file).unwrap(), svg, "{path}");
    }
    assert!(n >= 130, "{n} icons registered");
    let mut seen = std::collections::HashSet::new();
    assert!(ASSETS.iter().all(|(p, _)| seen.insert(*p)), "duplicate");
}

/// Every icon the kit's map assigns to a P1, P2 or keep row is available to the app.
#[test]
fn icon_map_built_rows_are_registered() {
    let kit: serde_json::Value = serde_json::from_str(ICONS_JSON).unwrap();
    let kit_names: std::collections::HashSet<String> = kit["icons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap().to_owned())
        .collect();
    let mut checked = 0;
    for line in ICON_MAP_CSV.lines().skip(1) {
        let f = csv_fields(line);
        if f.len() < 4 || !["P1", "P2", "keep"].contains(&f[3].as_str()) {
            continue;
        }
        for name in cell_names(&f[2]) {
            // Tray states are PNG/ICO files, not SVGs.
            if name.starts_with("cm-tray-") {
                continue;
            }
            // "keep" rows name marks the app already has (rec-*, launcher-*, "logo mark",
            // "TrayTemplate (existing)"): only the kit's own names are checked there.
            if f[3] == "keep" && !kit_names.contains(&name) {
                continue;
            }
            assert!(
                kit_names.contains(&name),
                "{name} (row {line:?}) is not in the kit"
            );
            assert!(registered(&name), "{name} is not registered ({line})");
            checked += 1;
        }
    }
    assert!(checked > 100, "{checked}");
}

/// The fixes of `design/icons/CLAUDE.md` section 5 and the icon usage table of
/// `design/redesign/CLAUDE.md`.
#[test]
fn fix_and_usage_icons_are_registered() {
    for n in [
        "volume-2",
        "cm-webcam-bubble",
        "trash-2",
        "cm-stop",
        "cm-record",
        "shrink",
        "shield-check",
        "info",
        "circle-check",
        "triangle-alert",
        "octagon-alert",
        "bot",
        "rec-check",
        "x",
        "locate",
        "flip-horizontal-2",
        "arrow-down",
        "arrow-up",
        "minus",
        "plus",
        "cm-trim-in",
        "cm-trim-out",
        "cm-split",
        "scissors",
        "archive-restore",
        "magnet",
        "zoom-in",
        "zoom-out",
        "folder",
        "hard-drive",
        "cm-capture-region",
        "app-window",
        "monitor",
        "send",
        "sparkles",
        "audio-lines",
        "captions",
        "lock",
        "link",
        "globe",
        "plug",
        "unplug",
        "pen-line",
        "rotate-cw",
        "cm-layout-screen",
        "cm-layout-bubble",
        "cm-layout-camera",
        "cm-layout-split",
    ] {
        assert!(registered(n), "{n}");
    }
    // The launcher's Permissions row has its own glyph.
    assert!(crate::ui::launcher_window::asset("icons/shield-check.svg").is_some());
}
