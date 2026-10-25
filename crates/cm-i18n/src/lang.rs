//! The languages the UI ships in, and their plural rules.
//!
//! Plural rules are hard-coded for the languages we ship (English: one form when `n == 1`;
//! Vietnamese: a single form) instead of evaluating the `Plural-Forms` expression of a catalogue.
//! A test checks that each shipped `.po` header says the same. Adding a language means adding a
//! variant here, its rule, its native name and its catalogue (`locales/README.md`).

/// A UI language. [`Lang::EnXa`] is the pseudo-locale for layout checks; it is not a choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    /// English, the source text of every message (no catalogue).
    En,
    /// Vietnamese.
    Vi,
    /// Pseudo-locale: accented, about 30 percent longer, in brackets. Only through
    /// `CAPTUREMANN_PSEUDO_LOCALE=1`; never offered in the UI and never stored in a setting.
    EnXa,
}

impl Lang {
    /// The languages a user can pick, in the order Settings lists them.
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Vi];

    /// The setting value and the `.po` file stem (`locales/<code>.po`).
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Vi => "vi",
            Lang::EnXa => "en-XA",
        }
    }

    /// The language's own name, shown in the language picker. Never translated.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En | Lang::EnXa => "English",
            Lang::Vi => "Ti\u{1ebf}ng Vi\u{1ec7}t",
        }
    }

    /// Parses a setting value. Only shipped languages parse (so `en-XA` cannot be chosen).
    pub fn parse(code: &str) -> Option<Lang> {
        Lang::ALL
            .into_iter()
            .find(|l| l.code().eq_ignore_ascii_case(code.trim()))
    }

    /// The language of a stored setting; anything missing or unknown is English.
    pub fn from_setting(value: Option<&str>) -> Lang {
        value.and_then(Lang::parse).unwrap_or(Lang::En)
    }

    /// How many plural forms the catalogue of this language has.
    pub fn plural_count(self) -> usize {
        match self {
            Lang::En | Lang::EnXa => 2,
            Lang::Vi => 1,
        }
    }

    /// Which plural form `n` uses (`0` ..< [`Lang::plural_count`]).
    pub fn plural_index(self, n: u64) -> usize {
        match self {
            Lang::En | Lang::EnXa => usize::from(n != 1),
            Lang::Vi => 0,
        }
    }

    /// The `Plural-Forms` header a catalogue of this language must carry.
    pub fn plural_forms_header(self) -> &'static str {
        match self {
            Lang::En | Lang::EnXa => "nplurals=2; plural=(n != 1);",
            Lang::Vi => "nplurals=1; plural=0;",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip_and_unknown_falls_back_to_english() {
        for l in Lang::ALL {
            assert_eq!(Lang::parse(l.code()), Some(l));
        }
        assert_eq!(Lang::parse(" VI "), Some(Lang::Vi));
        assert_eq!(
            Lang::parse("en-XA"),
            None,
            "the pseudo-locale is not a choice"
        );
        assert_eq!(Lang::parse("system"), None);
        assert_eq!(Lang::from_setting(None), Lang::En);
        assert_eq!(Lang::from_setting(Some("klingon")), Lang::En);
        assert_eq!(Lang::from_setting(Some("vi")), Lang::Vi);
    }

    #[test]
    fn plural_rules() {
        assert_eq!(Lang::En.plural_index(1), 0);
        assert_eq!(Lang::En.plural_index(0), 1);
        assert_eq!(Lang::En.plural_index(2), 1);
        for n in [0, 1, 2, 5, 100] {
            assert_eq!(Lang::Vi.plural_index(n), 0);
        }
        assert_eq!(Lang::Vi.plural_count(), 1);
    }

    #[test]
    fn native_names_are_not_translated() {
        assert_eq!(Lang::En.native_name(), "English");
        assert_eq!(Lang::Vi.native_name(), "Tiếng Việt");
    }
}
