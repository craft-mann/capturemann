//! `CAPTUREMANN_PSEUDO_LOCALE=1` selects the pseudo-locale whatever language is chosen. This file
//! is its own test binary with a single test, so setting the variable before first use is safe.

use cm_i18n::{epoch, language, set_language, tr, trn, Lang, PSEUDO_ENV};

#[test]
fn the_environment_variable_turns_the_pseudo_locale_on() {
    std::env::set_var(PSEUDO_ENV, "1");
    let e0 = epoch();
    set_language(Lang::En);
    assert_eq!(
        language(),
        Lang::EnXa,
        "pseudo wins over the chosen language"
    );
    assert!(epoch() > e0);
    let s = tr!("Delete {title}?", title = "Demo");
    assert!(s.starts_with('[') && s.ends_with(']'), "{s}");
    assert!(s.contains("Demo") && !s.contains("{title}"), "{s}");
    assert!(trn!("{n} recording", "{n} recordings", 2).contains('2'));
    // Even Vietnamese is replaced: the pseudo-locale is for layout checks of the English source.
    set_language(Lang::Vi);
    assert_eq!(language(), Lang::EnXa);
}
