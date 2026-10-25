//! UI languages for CaptureMann (`specs/i18n-plan.md`, decision D1).
//!
//! English is the source text: code writes `tr!("Copy link")` and the English stays in place, so
//! a missing translation shows the English, never a key. Other languages are gettext `.po`
//! catalogues in `locales/`, embedded in the binary.
//!
//! ```
//! use cm_i18n::{tr, tr_ctx, trn};
//! assert_eq!(tr!("Copy link"), "Copy link");
//! assert_eq!(tr!("Delete {title}?", title = "Demo"), "Delete Demo?");
//! assert_eq!(tr_ctx!("button", "Open"), "Open");
//! assert_eq!(trn!("{n} recording", "{n} recordings", 2), "2 recordings");
//! ```
//!
//! The language is global and thread-safe ([`set_language`], [`language`]); every switch bumps an
//! [`epoch`] so a view can tell that its cached text is stale. [`with_language`] overrides it for
//! the current thread only, which lets tests run in parallel.
//!
//! Only the UI is translated. The CLI, MCP, JSON-RPC, logs and error codes stay English and do
//! not call this crate.

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod extract;
pub mod fmt;
mod lang;
pub mod po;
mod pseudo;
mod subst;

pub use lang::Lang;
pub use subst::{placeholders, substitute};

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::OnceLock;

/// The environment variable that selects the pseudo-locale `en-XA`.
pub const PSEUDO_ENV: &str = "CAPTUREMANN_PSEUDO_LOCALE";

/// The embedded catalogue text of a language (`None` for English: its source is the code).
pub fn catalogue_source(lang: Lang) -> Option<&'static str> {
    match lang {
        Lang::Vi => Some(include_str!("../../../locales/vi.po")),
        Lang::En | Lang::EnXa => None,
    }
}

/// The parsed catalogue of a language, built once. A catalogue that does not parse is treated as
/// empty (English shows); a test makes sure the shipped ones parse.
fn catalogue(lang: Lang) -> Option<&'static po::Catalogue> {
    static VI: OnceLock<po::Catalogue> = OnceLock::new();
    match lang {
        Lang::Vi => Some(VI.get_or_init(|| {
            catalogue_source(Lang::Vi)
                .and_then(|s| po::parse(s).ok())
                .map(|c| sanitize(c, Lang::Vi))
                .unwrap_or_default()
        })),
        Lang::En | Lang::EnXa => None,
    }
}

/// Drops translations that would show a broken message: a placeholder the English does not have
/// (it could never be filled), or a wrong number of plural forms. English shows instead.
fn sanitize(mut cat: po::Catalogue, lang: Lang) -> po::Catalogue {
    for e in &mut cat.entries {
        if !extract::entry_is_usable(e, lang) {
            e.strs.clear();
        }
    }
    cat
}

static LANG: AtomicU8 = AtomicU8::new(0);
static EPOCH: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static OVERRIDE: Cell<Option<Lang>> = const { Cell::new(None) };
}

fn to_u8(l: Lang) -> u8 {
    match l {
        Lang::En => 0,
        Lang::Vi => 1,
        Lang::EnXa => 2,
    }
}

fn from_u8(v: u8) -> Lang {
    match v {
        1 => Lang::Vi,
        2 => Lang::EnXa,
        _ => Lang::En,
    }
}

fn pseudo_from_env() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| pseudo::requested(std::env::var(PSEUDO_ENV).ok().as_deref()))
}

/// The language messages are translated into now (this thread's override, else the global one).
pub fn language() -> Lang {
    OVERRIDE
        .with(Cell::get)
        .unwrap_or_else(|| from_u8(LANG.load(Ordering::Acquire)))
}

/// Sets the global language and bumps the [`epoch`] when it changed. With
/// `CAPTUREMANN_PSEUDO_LOCALE=1` the pseudo-locale wins over any choice.
pub fn set_language(lang: Lang) {
    let lang = if pseudo_from_env() { Lang::EnXa } else { lang };
    let old = LANG.swap(to_u8(lang), Ordering::AcqRel);
    if old != to_u8(lang) {
        EPOCH.fetch_add(1, Ordering::AcqRel);
    }
}

/// Changes on every language switch. A view that caches text compares it with the value it saw.
pub fn epoch() -> u64 {
    EPOCH.load(Ordering::Acquire)
}

/// Runs `f` with `lang` as this thread's language and restores the previous one after it.
/// Meant for tests; it never touches the global language or the epoch.
pub fn with_language<R>(lang: Lang, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Lang>);
    impl Drop for Restore {
        fn drop(&mut self) {
            OVERRIDE.with(|o| o.set(self.0));
        }
    }
    let _restore = Restore(OVERRIDE.with(|o| o.replace(Some(lang))));
    f()
}

/// Translates one message. `ctx` separates identical English with different meanings; `args` fill
/// the named placeholders. Prefer the macros, which the extractor also reads.
pub fn translate(ctx: Option<&str>, msgid: &str, args: &[(&str, String)]) -> String {
    let lang = language();
    render(lang, catalogue(lang), ctx, msgid, args)
}

fn render(
    lang: Lang,
    cat: Option<&po::Catalogue>,
    ctx: Option<&str>,
    msgid: &str,
    args: &[(&str, String)],
) -> String {
    let template = match lang {
        Lang::EnXa => pseudo::localize(msgid),
        _ => cat
            .and_then(|c| c.get(ctx, msgid))
            .and_then(|e| e.strs.first())
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| msgid.to_owned()),
    };
    subst::substitute(&template, args)
}

/// Translates a message with plural forms. `{n}` is filled with `n`.
pub fn translate_plural(
    ctx: Option<&str>,
    singular: &str,
    plural: &str,
    n: u64,
    args: &[(&str, String)],
) -> String {
    let lang = language();
    render_plural(lang, catalogue(lang), ctx, singular, plural, n, args)
}

fn render_plural(
    lang: Lang,
    cat: Option<&po::Catalogue>,
    ctx: Option<&str>,
    singular: &str,
    plural: &str,
    n: u64,
    args: &[(&str, String)],
) -> String {
    let english = if n == 1 { singular } else { plural };
    let template = match lang {
        Lang::EnXa => pseudo::localize(english),
        _ => cat
            .and_then(|c| c.get(ctx, singular))
            .filter(|e| e.plural.is_some())
            .and_then(|e| e.strs.get(lang.plural_index(n)))
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| english.to_owned()),
    };
    let mut all: Vec<(&str, String)> = Vec::with_capacity(args.len() + 1);
    all.push(("n", n.to_string()));
    all.extend(args.iter().map(|(k, v)| (*k, v.clone())));
    subst::substitute(&template, &all)
}

/// Translates a message: `tr!("Copy link")`, `tr!("Delete {title}?", title = name)`.
/// The text must be a string literal (the extractor reads it).
#[macro_export]
macro_rules! tr {
    ($msg:literal $(, $k:ident = $v:expr)* $(,)?) => {
        $crate::translate(
            ::std::option::Option::None,
            $msg,
            &[$((::std::stringify!($k), ::std::string::ToString::to_string(&$v))),*],
        )
    };
}

/// Translates a message with a context for short or ambiguous words:
/// `tr_ctx!("button", "Open")`.
#[macro_export]
macro_rules! tr_ctx {
    ($ctx:literal, $msg:literal $(, $k:ident = $v:expr)* $(,)?) => {
        $crate::translate(
            ::std::option::Option::Some($ctx),
            $msg,
            &[$((::std::stringify!($k), ::std::string::ToString::to_string(&$v))),*],
        )
    };
}

/// Translates a message with plural forms: `trn!("{n} recording", "{n} recordings", count)`.
/// `{n}` is filled with the count; more named arguments may follow.
#[macro_export]
macro_rules! trn {
    ($one:literal, $many:literal, $n:expr $(, $k:ident = $v:expr)* $(,)?) => {
        $crate::translate_plural(
            ::std::option::Option::None,
            $one,
            $many,
            ($n) as u64,
            &[$((::std::stringify!($k), ::std::string::ToString::to_string(&$v))),*],
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_returns_the_source_text_with_placeholders_filled() {
        with_language(Lang::En, || {
            assert_eq!(tr!("Copy link"), "Copy link");
            assert_eq!(tr!("Delete {title}?", title = "Demo"), "Delete Demo?");
            assert_eq!(tr_ctx!("button", "Open"), "Open");
            assert_eq!(trn!("{n} recording", "{n} recordings", 1), "1 recording");
            assert_eq!(trn!("{n} recording", "{n} recordings", 0), "0 recordings");
            assert_eq!(
                trn!("{n} recording", "{n} recordings", 7usize),
                "7 recordings"
            );
            assert_eq!(
                trn!("{n} of {total}", "{n} of {total}", 2, total = 5),
                "2 of 5"
            );
        });
    }

    #[test]
    fn a_missing_message_shows_english_never_a_key() {
        with_language(Lang::Vi, || {
            assert_eq!(
                tr!("This sentence is in no catalogue"),
                "This sentence is in no catalogue"
            );
            assert_eq!(
                trn!("{n} unknown thing", "{n} unknown things", 3),
                "3 unknown things"
            );
        });
    }

    #[test]
    fn the_override_is_per_thread_and_restores() {
        let before = language();
        with_language(Lang::Vi, || {
            assert_eq!(language(), Lang::Vi);
            with_language(Lang::EnXa, || assert_eq!(language(), Lang::EnXa));
            assert_eq!(language(), Lang::Vi);
            std::thread::spawn(|| assert_eq!(language(), Lang::En))
                .join()
                .unwrap();
        });
        assert_eq!(language(), before);
    }

    #[test]
    fn the_epoch_changes_on_a_switch_only() {
        // The global language is shared by every test of this binary: this is the only test
        // that sets it, and it puts English back.
        set_language(Lang::En);
        let e0 = epoch();
        set_language(Lang::En);
        assert_eq!(epoch(), e0, "no change, no bump");
        set_language(Lang::Vi);
        let e1 = epoch();
        assert!(e1 > e0);
        assert_eq!(language(), Lang::Vi);
        set_language(Lang::En);
        assert!(epoch() > e1);
        assert_eq!(language(), Lang::En);
    }

    #[test]
    fn pseudo_locale_wraps_and_keeps_placeholders() {
        with_language(Lang::EnXa, || {
            let s = tr!("Delete {title}?", title = "Demo");
            assert!(
                s.starts_with('[') && s.ends_with(']') && s.contains("Demo"),
                "{s}"
            );
            assert!(!s.contains("{title}"));
            assert!(trn!("{n} recording", "{n} recordings", 3).contains('3'));
        });
    }

    fn sample(lang: Lang, text: &str) -> po::Catalogue {
        sanitize(po::parse(text).unwrap(), lang)
    }

    #[test]
    fn plural_forms_context_reordering_and_fallbacks_with_a_catalogue() {
        let vi = sample(
            Lang::Vi,
            "msgid \"\"\nmsgstr \"\"\n\n\
             msgid \"{n} recording\"\nmsgid_plural \"{n} recordings\"\nmsgstr[0] \"{n} bản ghi\"\n\n\
             msgid \"Delete {title} from {where}?\"\nmsgstr \"Xóa khỏi {where}: {title}?\"\n\n\
             msgctxt \"button\"\nmsgid \"Open\"\nmsgstr \"Mở\"\n\n\
             msgid \"Open\"\nmsgstr \"\"\n\n\
             msgid \"Bad {oops}\"\nmsgstr \"Sai {other}\"\n",
        );
        let c = Some(&vi);
        let one = |n| render_plural(Lang::Vi, c, None, "{n} recording", "{n} recordings", n, &[]);
        assert_eq!(
            (one(1).as_str(), one(5).as_str()),
            ("1 bản ghi", "5 bản ghi")
        );
        let args = [
            ("title", "Demo".to_owned()),
            ("where", "Thư viện".to_owned()),
        ];
        assert_eq!(
            render(Lang::Vi, c, None, "Delete {title} from {where}?", &args),
            "Xóa khỏi Thư viện: Demo?"
        );
        // Context is part of the key; an empty msgstr falls back to English.
        assert_eq!(render(Lang::Vi, c, Some("button"), "Open", &[]), "Mở");
        assert_eq!(render(Lang::Vi, c, None, "Open", &[]), "Open");
        assert_eq!(render(Lang::Vi, c, Some("menu"), "Open", &[]), "Open");
        // A translation with a placeholder the English lacks is dropped.
        assert_eq!(render(Lang::Vi, c, None, "Bad {oops}", &[]), "Bad {oops}");
        // English plural rule: a missing or empty form falls back to the English text.
        let en = sample(
            Lang::En,
            "msgid \"{n} file\"\nmsgid_plural \"{n} files\"\nmsgstr[0] \"{n} tệp\"\nmsgstr[1] \"\"\n",
        );
        let f = |n| render_plural(Lang::En, Some(&en), None, "{n} file", "{n} files", n, &[]);
        assert_eq!((f(1).as_str(), f(2).as_str()), ("1 tệp", "2 files"));
    }

    #[test]
    fn the_shipped_catalogues_parse_and_carry_the_plural_rule() {
        for lang in Lang::ALL {
            let Some(src) = catalogue_source(lang) else {
                continue;
            };
            let cat = po::parse(src).unwrap_or_else(|e| panic!("{}: {e}", lang.code()));
            assert_eq!(cat.header_value("Language"), Some(lang.code()));
            assert_eq!(
                cat.header_value("Plural-Forms"),
                Some(lang.plural_forms_header()),
                "{}: the header and the hard-coded rule must agree",
                lang.code()
            );
        }
    }
}
