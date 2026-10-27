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
        let name = cm_ai::report::find_template(template)
            .map(|t| t.name.clone())
            .unwrap_or_default();
        let name = crate::ui::recording_editor::report_template_name(template, &name);
        cx.update(|cx| {
            app::push_toast(
                &app2,
                ToastSpec::new(ToastKind::Info, tr!("Write a report?"))
                    .body(tr!(
                        "{template} suits this recording. Nothing is written until you ask.",
                        template = name
                    ))
                    .button(
                        tr_ctx!("button", "Set up report"),
                        ToastAction::SuggestReport(id),
                    ),
                cx,
            );
        });
    })
    .detach();
}
