//! 0.2.9 bug 3: the one-line hints of the built-in model catalogues name no operating system or
//! chip. The hints show on every platform (the Models page on Windows once said "every Mac"), so
//! a platform noun in one is a bug. No exemptions.

const FILES: [(&str, &str); 4] = [
    ("models.json", include_str!("../data/models.json")),
    ("text_models.json", include_str!("../data/text_models.json")),
    (
        "speaker_models.json",
        include_str!("../data/speaker_models.json"),
    ),
    ("vad_models.json", include_str!("../data/vad_models.json")),
];

const PLATFORM_WORDS: [&str; 8] = [
    "mac", "macs", "macos", "apple", "metal", "windows", "linux", "silicon",
];

/// Every `hint` of every entry of the catalogue files.
fn hints() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (file, text) in FILES {
        let v: serde_json::Value = serde_json::from_str(text).expect(file);
        for m in v["models"].as_array().expect(file) {
            let id = m["id"].as_str().unwrap_or("?");
            let hint = m["hint"].as_str().unwrap_or("");
            out.push((format!("{file}:{id}"), hint.to_owned()));
        }
    }
    out
}

/// 0.2.9 bug 3: no catalogue hint contains Mac, macOS, Apple, Metal, Windows or Linux.
#[test]
fn no_catalogue_hint_names_a_platform() {
    let all = hints();
    assert!(all.len() >= 15, "the catalogues were not read: {all:?}");
    for (which, hint) in all {
        // Whole words, so "machine" is fine.
        for word in hint.split(|c: char| !c.is_alphanumeric()) {
            assert!(
                !PLATFORM_WORDS.contains(&word.to_lowercase().as_str()),
                "{which}: the hint names a platform ({word}): {hint}"
            );
        }
    }
}
