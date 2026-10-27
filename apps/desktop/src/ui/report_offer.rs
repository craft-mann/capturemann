//! The toast that offers a report after an audio recording's transcript is ready (V1, AI-9).
//! Rules: `crate::ai::report_offer`. It asks the daemon for the text model and reads the
//! recording's manifest, and says nothing unless a local model is set up, the recording is audio
//! with a preset that suggests a template, and it has no report. It never starts a report.

use cm_i18n::{tr, tr_ctx};
use gpui::{App, Entity};

use crate::ai::report_offer::{template_to_offer, text_model_ready};
use crate::toast::{ToastAction, ToastKind, ToastSpec};
use crate::ui::app::{self, AppModel};

/// Looks at recording `capture_id` and shows the offer when it fits.
pub fn offer(app: &Entity<AppModel>, capture_id: &str, cx: &mut App) {
    let Some(client) = crate::ui::ai::client_of(app, cx) else {
        return;
    };
    let root = app.read(cx).backend.library_root.clone();
    let (app2, id) = (app.clone(), capture_id.to_owned());
    cx.spawn(async move |cx| {
        // No model, or a daemon that cannot say: stay silent.
        let Ok(answer) = client.get_text_model().await else {
            return;
        };
        if !text_model_ready(&answer) {
            return;
        }
        let id2 = id.clone();
        let template = cx
            .background_executor()
            .spawn(async move {
                let dir = crate::recording::files::find_bundle(&root?, &id2)?;
                let m = cm_library::read_manifest(&dir).ok()?;
                template_to_offer(&m)
            })
            .await;
        let Some(template) = template else { return };
        cx.update(|cx| {
            app::push_toast(&app2, offer_toast(id, template), cx);
        });
    })
    .detach();
}

/// The offer toast for recording `id` and template `template`. Live: the title, the body and the
/// template's name are worded again when the UI language changes while it is on screen.
pub fn offer_toast(id: String, template: &'static str) -> ToastSpec {
    ToastSpec::live(move || {
        let name = cm_ai::report::find_template(template)
            .map(|t| t.name.clone())
            .unwrap_or_default();
        let name = crate::ui::recording_editor::report_template_name(template, &name);
        ToastSpec::new(ToastKind::Info, tr!("Write a report?"))
            .body(tr!(
                "{template} suits this recording. Nothing is written until you ask.",
                template = name
            ))
            .button(
                tr_ctx!("button", "Set up report"),
                ToastAction::SuggestReport(id.clone()),
            )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toast::ToastQueue;
    use cm_i18n::{with_language, Lang};
    use std::time::Instant;

    /// I18N: the offer follows a language switch like the other toasts.
    #[test]
    fn the_offer_toast_follows_the_language() {
        let mut q = ToastQueue::default();
        let id = with_language(Lang::En, || {
            q.push(offer_toast("rec1".into(), "minutes"), Instant::now())
        });
        assert!(!q.relabel());
        assert!(with_language(Lang::EnXa, || q.relabel()));
        let t = q.toasts().iter().find(|x| x.id == id).unwrap();
        assert!(t.title.starts_with('['), "{}", t.title);
        assert!(t.buttons[0].label.starts_with('['));
        assert_eq!(
            t.buttons[0].action,
            ToastAction::SuggestReport("rec1".into())
        );
    }
}
