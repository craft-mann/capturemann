//! A failure kept in the state of the Destinations or YouTube panel and worded when the panel is
//! drawn (I18N integration). The panels used to store the sentence made at the moment of the
//! event, so a line still on screen kept its language after a switch; now they store the reason
//! (the error code, the provider's text, the name and the outcome) and the words are made in
//! `render`.

use cm_auth::RevokeOutcome;
use cm_core::{CmError, ErrorCode};

use crate::cloud_connect::ProviderInfo;

/// Why a step of a panel failed, or what a sign-out did, ready to be worded.
#[derive(Clone)]
pub(super) enum Problem {
    /// A connect, test or reconnect step of a cloud drive failed. `message` is the error as it
    /// came (the provider's text stays as it is).
    Connect {
        provider: Option<&'static ProviderInfo>,
        code: ErrorCode,
        message: String,
    },
    /// The same for the YouTube panel.
    YouTube { code: ErrorCode, message: String },
    /// What Disconnect did for a cloud destination called `name`.
    SignOutCloud {
        name: String,
        outcome: RevokeOutcome,
    },
    /// What Disconnect did for a YouTube channel called `name`.
    SignOutYouTube {
        name: String,
        outcome: RevokeOutcome,
    },
    /// A destination rule refused a value (worded when drawn).
    Rule(crate::destinations::RuleError),
    /// Text that is shown as it is (an OS or file error).
    Raw(String),
}

impl Problem {
    pub(super) fn connect(provider: Option<&'static ProviderInfo>, e: &CmError) -> Problem {
        Problem::Connect {
            provider,
            code: e.code(),
            message: e.to_string(),
        }
    }

    pub(super) fn youtube(e: &CmError) -> Problem {
        Problem::YouTube {
            code: e.code(),
            message: e.to_string(),
        }
    }

    pub(super) fn raw(text: impl Into<String>) -> Problem {
        Problem::Raw(text.into())
    }

    /// The words, in the current language.
    pub(super) fn text(&self) -> String {
        match self {
            Problem::Connect {
                provider,
                code,
                message,
            } => super::destinations::connect_error_text(*provider, *code, message),
            Problem::YouTube { code, message } => {
                super::publish_card::yt_error_text(*code, message)
            }
            Problem::SignOutCloud { name, outcome } => {
                crate::cloud_connect::sign_out_message(name, outcome)
            }
            Problem::SignOutYouTube { name, outcome } => {
                crate::publish_connect::sign_out_message(name, outcome)
            }
            Problem::Rule(e) => e.to_string(),
            Problem::Raw(text) => text.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_i18n::{with_language, Lang};

    /// The same stored problem reads in the language of the moment it is drawn.
    #[test]
    fn a_stored_problem_follows_the_language() {
        let p = Problem::YouTube {
            code: ErrorCode::Network,
            message: "timeout".into(),
        };
        let en = with_language(Lang::En, || p.text());
        assert_eq!(
            en,
            "Could not reach the server: timeout Check your connection and try again."
        );
        let xa = with_language(Lang::EnXa, || p.text());
        assert!(xa.starts_with('['), "{xa}");
        assert!(xa.contains("timeout"), "{xa}");
        let out = Problem::SignOutYouTube {
            name: "My channel".into(),
            outcome: RevokeOutcome::NotAttempted,
        };
        assert_eq!(
            with_language(Lang::En, || out.text()),
            "Disconnected \u{201c}My channel\u{201d}."
        );
        assert!(with_language(Lang::EnXa, || out.text()).starts_with('['));
        assert_eq!(Problem::raw("disk full").text(), "disk full");
    }

    /// A destination rule that was refused is kept as a reason and worded when drawn.
    #[test]
    fn a_refused_destination_rule_follows_the_language() {
        let p = Problem::Rule(crate::destinations::RuleError::NameMissing);
        assert_eq!(
            with_language(Lang::En, || p.text()),
            "Give the destination a name."
        );
        assert_ne!(
            with_language(Lang::Vi, || p.text()),
            "Give the destination a name."
        );
        assert!(with_language(Lang::EnXa, || p.text()).starts_with('['));
    }
}
