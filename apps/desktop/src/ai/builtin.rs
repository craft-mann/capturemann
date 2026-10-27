//! The built-in text models on the AI page, display-free (AI-7, provider `embedded`): the list,
//! the downloads, the one recommended model and the report card's offer.
//!
//! The downloads, the Use and Remove confirmations and the row buttons are the speech models'
//! ([`ModelsVm`]), fed with the text-model list; this adds the licence, the recommendation, the
//! "in use" state (the text-model setting, not the store's last choice) and a memory warning. Nothing here starts a download by itself: the only path is the confirm or
//! resume press of a row (or the report card's button, which names the size).

use std::collections::HashMap;

use cm_ai::model::ModelList;
use cm_ai::text_model::store::TextModelList;
use cm_i18n::tr;

use crate::ai::models::{size_label, ModelRow, ModelsVm, RowAction};

#[derive(Debug, Clone, Default)]
pub struct BuiltInVm {
    /// Rows, downloads and confirmations: the speech models' machinery on the text-model list.
    pub models: ModelsVm,
    meta: HashMap<String, Meta>,
    /// `list_text_models` has answered.
    pub loaded: bool,
    /// A model whose download was started while none was in use: once it is there, it is
    /// selected (and the built-in engine turned on) without another press.
    pub use_when_ready: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Meta {
    ram_mb: u32,
    /// Offered only on a Mac with at least this much memory, in GB (0: every Mac).
    min_memory_gb: u32,
    recommended: bool,
    license: String,
    context_tokens: u32,
}

/// One row of the Built in list.
#[derive(Debug, Clone, PartialEq)]
pub struct BuiltInRow {
    pub row: ModelRow,
    pub license: String,
    /// The default model, recommended on every Mac.
    pub recommended: bool,
    /// The model the built-in engine is set to use.
    pub in_use: bool,
    /// The model needs more memory than this Mac has to spare.
    pub low_memory: bool,
}

impl BuiltInRow {
    /// The warning under a row whose model may not fit in memory.
    pub fn memory_warning(&self) -> Option<String> {
        self.low_memory
            .then(|| tr!("This Mac may not have enough free memory for this model."))
    }
}

/// Memory the system and other apps keep, in MB, when judging whether a model fits.
const HEADROOM_MB: u64 = 2048;

impl BuiltInVm {
    pub fn set_list(&mut self, l: TextModelList) {
        self.meta = l
            .models
            .iter()
            .map(|m| {
                (
                    m.info.id.clone(),
                    Meta {
                        recommended: m.recommended,
                        min_memory_gb: m.min_memory_gb,
                        ram_mb: m.info.ram_mb,
                        license: m.license.clone(),
                        context_tokens: m.context_tokens,
                    },
                )
            })
            .collect();
        self.models.set_list(ModelList {
            models: l.models.into_iter().map(|m| m.info).collect(),
            active: l.active,
            dir: l.dir,
        });
        self.loaded = true;
    }

    /// Nothing is on offer (the catalogue's models are not pinned yet) or the list failed.
    pub fn is_empty(&self) -> bool {
        self.models
            .list
            .as_ref()
            .is_none_or(|l| l.models.is_empty())
    }

    pub fn installed_any(&self) -> bool {
        self.models.installed_any()
    }

    /// The recommended model: the catalogue's default entry, on every Mac.
    pub fn recommended_id(&self) -> Option<String> {
        self.meta
            .iter()
            .find(|(_, m)| m.recommended)
            .map(|(id, _)| id.clone())
    }

    /// The recommended model if it is not downloaded and not downloading: what the report
    /// card offers. `(id, label, size_mb)`.
    pub fn offer(&self) -> Option<(String, String, u32)> {
        let id = self.recommended_id()?;
        let row = self.models.rows().into_iter().find(|r| r.id == id)?;
        let idle = matches!(row.status, crate::ai::models::RowStatus::NotInstalled);
        idle.then_some((row.id, row.label, row.size_mb))
    }

    /// The rows in catalogue order. `in_use` is the model the engine is set to use (`TextVm`): its
    /// row shows "In use" and "Don't use a model"; the other downloaded ones show Use. Import is
    /// not offered for text models. `mem_gb` is this Mac's memory, for the warning.
    pub fn rows(&self, mem_gb: Option<u32>, in_use: Option<&str>) -> Vec<BuiltInRow> {
        use crate::ai::models::RowStatus;
        self.models
            .rows()
            .into_iter()
            // A higher-quality model for big Macs is hidden on a smaller one, unless it is
            // already downloaded (then it can still be used or removed).
            .filter(|row| {
                let min = self.meta.get(&row.id).map_or(0, |m| m.min_memory_gb);
                let there = matches!(
                    row.status,
                    crate::ai::models::RowStatus::Installed | crate::ai::models::RowStatus::Active
                );
                there || min == 0 || mem_gb.is_some_and(|g| g >= min)
            })
            .map(|mut row| {
                row.actions.retain(|a| *a != RowAction::Import);
                let meta = self.meta.get(&row.id).cloned().unwrap_or_default();
                let used = in_use == Some(row.id.as_str());
                if matches!(row.status, RowStatus::Installed | RowStatus::Active) {
                    if used {
                        row.status = RowStatus::Active;
                        row.actions = vec![RowAction::StopUsing, RowAction::Remove];
                    } else {
                        row.status = RowStatus::Installed;
                        row.actions = vec![RowAction::Select, RowAction::Remove];
                    }
                }
                let low_memory = mem_gb
                    .is_some_and(|g| u64::from(meta.ram_mb) + HEADROOM_MB > u64::from(g) * 1024);
                BuiltInRow {
                    recommended: meta.recommended,
                    in_use: used,
                    low_memory,
                    license: meta.license,
                    row,
                }
            })
            .collect()
    }

    /// The context a downloaded model runs with, for the licence line.
    pub fn context_of(&self, id: &str) -> Option<u32> {
        self.meta.get(id).map(|m| m.context_tokens)
    }

    /// The model waiting to be selected once it is downloaded (see
    /// [`BuiltInVm::use_when_ready`]): returned, and forgotten, when its file is there.
    pub fn take_ready_to_use(&mut self) -> Option<String> {
        let id = self.use_when_ready.clone()?;
        let there = self
            .models
            .list
            .as_ref()
            .is_some_and(|l| l.models.iter().any(|m| m.id == id && m.installed));
        if there {
            self.use_when_ready = None;
            Some(id)
        } else {
            None
        }
    }
}

/// "Recommended" tag text.
pub fn recommended_tag() -> String {
    tr!("Recommended")
}

/// The caption under the list.
pub fn list_caption(mem_gb: Option<u32>) -> String {
    match mem_gb {
        Some(g) => tr!(
            "This Mac has {memory} GB of memory. The model runs inside CaptureMann and nothing leaves this Mac.",
            memory = g
        ),
        None => tr!("The model runs inside CaptureMann and nothing leaves this Mac."),
    }
}

/// `5.0 GB to download`, for the report card's button.
pub fn download_button_label(size_mb: u32) -> String {
    tr!(
        "Download the recommended model ({size})",
        size = size_label(size_mb)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_ai::model::ModelInfo;
    use cm_ai::text_model::store::TextModelInfo;

    fn entry(id: &str, recommended: bool, mb: u32, installed: bool) -> TextModelInfo {
        TextModelInfo {
            info: ModelInfo {
                id: id.into(),
                label: format!("Model {id}"),
                size_mb: mb,
                ram_mb: mb * 2,
                hint: String::new(),
                installed,
                active: installed,
                path: installed.then(|| format!("/t/{id}.gguf")),
                ..Default::default()
            },
            recommended,
            min_memory_gb: 0,
            license: "Apache-2.0".into(),
            context_tokens: 8192,
        }
    }

    fn list(models: Vec<TextModelInfo>) -> TextModelList {
        let active = models
            .iter()
            .find(|m| m.info.installed)
            .map(|m| m.info.id.clone());
        TextModelList {
            models,
            active,
            dir: "/t".into(),
        }
    }

    fn vm() -> BuiltInVm {
        let mut vm = BuiltInVm::default();
        vm.set_list(list(vec![
            entry("a", true, 2800, false),
            entry("b", false, 2700, false),
            entry("c", false, 1300, false),
        ]));
        vm
    }

    #[test]
    fn an_empty_catalogue_is_a_plain_empty_state() {
        let mut vm = BuiltInVm::default();
        assert!(vm.is_empty() && !vm.loaded);
        vm.set_list(list(vec![]));
        assert!(vm.is_empty() && vm.loaded);
        assert!(vm.rows(Some(16), None).is_empty());
        assert_eq!(vm.recommended_id(), None);
        assert_eq!(vm.offer(), None);
    }

    #[test]
    fn rows_keep_the_catalogue_order_and_one_is_recommended() {
        let vm = vm();
        let rows = vm.rows(Some(16), None);
        assert_eq!(
            rows.iter()
                .map(|r| (r.row.id.as_str(), r.recommended))
                .collect::<Vec<_>>(),
            [("a", true), ("b", false), ("c", false)]
        );
        assert_eq!(rows[0].license, "Apache-2.0");
        assert_eq!(vm.recommended_id().as_deref(), Some("a"));
        // The recommendation does not depend on the Mac's memory.
        assert!(vm.rows(None, None)[0].recommended);
        assert!(vm.rows(Some(4), None)[0].recommended);
    }

    #[test]
    fn a_model_for_big_macs_is_hidden_on_a_smaller_one() {
        let mut big = entry("big", false, 14_000, false);
        big.min_memory_gb = 32;
        let mut vm = BuiltInVm::default();
        vm.set_list(list(vec![entry("a", true, 2_000, false), big]));
        let ids = |gb| {
            vm.rows(gb, None)
                .iter()
                .map(|r| r.row.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(Some(16)), ["a"]);
        assert_eq!(ids(None), ["a"], "unknown memory hides it too");
        assert_eq!(ids(Some(32)), ["a", "big"]);
        assert_eq!(ids(Some(64)), ["a", "big"]);
    }

    #[test]
    fn a_model_that_does_not_fit_in_memory_gets_a_warning() {
        let vm = vm();
        // ram_mb is twice the size: a 5600 MB, b 5400 MB, c 2600 MB.
        let warn = |gb| {
            vm.rows(Some(gb), None)
                .iter()
                .map(|r| r.memory_warning().is_some())
                .collect::<Vec<_>>()
        };
        assert_eq!(warn(16), [false, false, false]);
        assert_eq!(warn(6), [true, true, false]);
        assert_eq!(warn(8), [false, false, false]);
        assert_eq!(warn(4), [true, true, true]);
        assert!(vm
            .rows(None, None)
            .iter()
            .all(|r| r.memory_warning().is_none()));
    }

    #[test]
    fn in_use_follows_the_text_model_setting_not_the_stores_last_choice() {
        let mut vm = BuiltInVm::default();
        // Both downloaded; the store says "a" is active.
        vm.set_list(list(vec![
            entry("a", true, 2800, true),
            entry("b", false, 2700, true),
        ]));
        let row = |vm: &BuiltInVm, in_use: Option<&str>, id: &str| {
            vm.rows(Some(16), in_use)
                .into_iter()
                .find(|r| r.row.id == id)
                .unwrap()
        };
        // Nothing selected (provider none): no row is in use, both offer Use.
        let a = row(&vm, None, "a");
        assert!(!a.in_use);
        assert_eq!(a.row.actions, [RowAction::Select, RowAction::Remove]);
        // "b" selected: it offers Don't use, "a" offers Use.
        let b = row(&vm, Some("b"), "b");
        assert!(b.in_use);
        assert_eq!(b.row.actions, [RowAction::StopUsing, RowAction::Remove]);
        assert_eq!(
            row(&vm, Some("b"), "a").row.actions,
            [RowAction::Select, RowAction::Remove]
        );
        // Pressing the buttons asks the daemon; the row state itself comes back from it.
        assert_eq!(
            vm.models.apply("a", &RowAction::Select),
            Some(crate::ai::models::Effect::Select)
        );
        assert_eq!(
            vm.models.apply("b", &RowAction::StopUsing),
            Some(crate::ai::models::Effect::StopUsing)
        );
    }

    #[test]
    fn a_download_asks_first_and_text_rows_have_no_import() {
        let mut vm = vm();
        let actions = |vm: &BuiltInVm| {
            vm.rows(Some(16), None)
                .into_iter()
                .find(|r| r.row.id == "b")
                .unwrap()
                .row
                .actions
        };
        assert_eq!(actions(&vm), [RowAction::Download]);
        assert_eq!(vm.models.apply("b", &RowAction::Download), None);
        assert_eq!(
            actions(&vm),
            [RowAction::ConfirmDownload, RowAction::CancelConfirm]
        );
        // Only the second press starts it.
        assert_eq!(
            vm.models.apply("b", &RowAction::ConfirmDownload),
            Some(crate::ai::models::Effect::StartDownload)
        );
    }

    #[test]
    fn the_report_card_offers_the_recommended_model_until_it_is_there() {
        let mut vm = vm();
        assert_eq!(vm.offer(), Some(("a".into(), "Model a".into(), 2800)));
        vm.models.apply("a", &RowAction::Download);
        assert_eq!(vm.offer(), None, "already asked or running");
        vm.set_list(list(vec![entry("a", true, 2800, true)]));
        assert_eq!(vm.offer(), None, "downloaded");
        assert!(vm.installed_any());
        assert_eq!(vm.context_of("a"), Some(8192));
    }

    #[test]
    fn a_download_started_with_nothing_in_use_is_selected_when_it_arrives() {
        let mut vm = vm();
        vm.use_when_ready = Some("b".into());
        assert_eq!(vm.take_ready_to_use(), None, "not downloaded yet");
        assert_eq!(vm.use_when_ready.as_deref(), Some("b"), "still waiting");
        vm.set_list(list(vec![entry("b", false, 2700, true)]));
        assert_eq!(vm.take_ready_to_use().as_deref(), Some("b"));
        assert_eq!(vm.take_ready_to_use(), None, "only once");
    }
}
