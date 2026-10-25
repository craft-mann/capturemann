//! CI checks of the catalogues (no network, no display): a fresh template, sound `.po` files, a
//! coverage report, and the ratchet on unwrapped UI literals. Run the fixes with
//! `cargo run -p cm-i18n -- extract` and `-- baseline` (see `locales/README.md`).

use cm_i18n::extract;

#[test]
fn the_template_is_fresh_and_every_catalogue_is_sound() {
    let root = extract::workspace_root();
    let problems = extract::check_all(&root);
    assert!(
        problems.is_empty(),
        "i18n problems (fix with `cargo run -p cm-i18n -- extract`, or edit the .po):\n{}",
        problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn the_coverage_report_lists_every_language() {
    let root = extract::workspace_root();
    let scan = extract::scan_workspace(&root);
    let cats = extract::load_catalogues(&root).expect("catalogues load");
    let report = extract::coverage_report(&scan.messages, &cats);
    println!("{report}");
    assert!(report.contains("en: 100.0%"));
    assert!(report.contains("vi: "), "{report}");
}

/// The ratchet is closed: no UI literal of `apps/desktop/src` is left unwrapped, and a new one
/// fails the build. (It went from 1,057 to 0 during the P2 and integration work.)
#[test]
fn no_ui_literal_is_left_unwrapped() {
    let root = extract::workspace_root();
    let left = extract::count_unwrapped(&root);
    assert!(
        left.is_empty(),
        "{} unwrapped UI literals: wrap them in tr!, tr_ctx! or trn! (whole sentences, with \
         placeholders), or add `// i18n-ignore` to a literal that is not UI text:\n{}",
        left.len(),
        left.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
    let baseline = extract::read_baseline(&root).expect("locales/unwrapped-baseline.txt");
    assert_eq!(
        baseline, 0,
        "locales/unwrapped-baseline.txt must say 0 (`cargo run -p cm-i18n -- baseline`)"
    );
}
