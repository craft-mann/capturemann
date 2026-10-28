//! The desktop side of the local AI features (M5, `specs/features/07-ai.md`): the typed client
//! of the daemon's AI actions, and the display-free state and rules behind the AI page, the
//! "no model yet" notice, the caption editor and the suggestion lists.
//!
//! Nothing here downloads anything by itself: the only way a model download starts is the user
//! pressing Download (and confirming) on the AI page, which sends `download_model`.

pub mod builtin;
pub mod caption_find;
pub mod caption_view;
pub mod captions;
pub mod client;
pub mod glossary;
pub mod jobs;
pub mod models;
pub mod report_offer;
pub mod suggest;
pub mod summary;
pub mod voice;

#[cfg(test)]
mod details_contract;

#[cfg(test)]
mod flow_tests {
    use cm_ai::model::{ModelInfo, ModelList};
    use serde_json::json;

    use super::client::{AiClient, AiError, ScriptedTransport};
    use super::models::{Effect, ModelsVm, NoticeSite, RowAction};

    fn list(installed: bool) -> ModelList {
        ModelList {
            models: vec![ModelInfo {
                id: "small".into(),
                label: "Small".into(),
                size_mb: 466,
                ram_mb: 852,
                hint: "Balanced".into(),
                installed,
                active: installed,
                path: None,
                ..Default::default()
            }],
            active: installed.then(|| "small".to_owned()),
            dir: "/models".into(),
        }
    }

    fn run<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(f)
    }

    /// Nothing is downloaded without the second press: pressing Download only asks, and only the
    /// confirm button produces the effect that sends `download_model` (and it sends it once).
    #[test]
    fn no_download_is_sent_before_the_confirm_press() {
        let t = ScriptedTransport::new();
        t.answer(
            "list_models",
            Ok(serde_json::to_value(list(false)).unwrap()),
        );
        t.answer("download_model", Ok(json!({"job_id": "job_5"})));
        let client = AiClient::new(t.clone());
        let mut vm = ModelsVm::default();
        vm.set_list(run(client.list_models()).unwrap());
        // The app reading the list at start, and the user pressing Download, send nothing.
        assert_eq!(vm.apply("small", &RowAction::Download), None);
        assert!(t.calls_of("download_model").is_empty());
        // Backing out of the question sends nothing either.
        assert_eq!(vm.apply("small", &RowAction::CancelConfirm), None);
        assert!(t.calls_of("download_model").is_empty());
        // The second press is the only thing that sends it.
        vm.apply("small", &RowAction::Download);
        let effect = vm.apply("small", &RowAction::ConfirmDownload);
        assert_eq!(effect, Some(Effect::StartDownload));
        let job = run(client.download_model("small")).unwrap();
        vm.download_started("small", job);
        assert_eq!(
            t.calls_of("download_model"),
            vec![json!({"model": "small"})]
        );
        // Pressing the confirm again while it runs does not start a second download.
        assert_eq!(vm.apply("small", &RowAction::ConfirmDownload), None);
        // Cancel stops that job; Resume (the user pressing again) restarts it.
        assert_eq!(
            vm.apply("small", &RowAction::Cancel),
            Some(Effect::CancelJob("job_5".into()))
        );
        assert_eq!(
            vm.apply("small", &RowAction::Resume),
            Some(Effect::StartDownload)
        );
    }

    /// Without an installed model the notice shows, Transcribe is disabled, and a stale list that
    /// still lets a press through gets `no_model` back, which is recognised.
    #[test]
    fn transcribe_without_a_model_shows_the_notice_and_the_daemon_refuses() {
        let t = ScriptedTransport::new();
        t.answer(
            "transcribe",
            Err(AiError {
                code: Some("invalid_input".into()),
                reason: Some("no_model".into()),
                message: "No speech model is installed".into(),
            }),
        );
        let client = AiClient::new(t);
        let mut vm = ModelsVm::default();
        vm.set_list(list(false));
        assert!(vm.notice_visible(NoticeSite::AiPage));
        assert!(!vm.can_transcribe());
        let e = run(client.transcribe("rec_1", None, None, false)).unwrap_err();
        assert!(e.is_no_model());
        // Installing a model clears the notice everywhere.
        vm.set_list(list(true));
        assert!(!vm.notice_visible(NoticeSite::Transcribe));
        assert!(vm.can_transcribe());
    }
}
