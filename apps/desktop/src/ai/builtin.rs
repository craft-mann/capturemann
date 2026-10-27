//! The built-in text models on the AI page, display-free (AI-7, provider `embedded`): the list by
//! memory tier, the downloads, "recommended for this Mac" and the report card's offer.
//!
//! The downloads, the Use and Remove confirmations and the row buttons are the speech models'
//! ([`ModelsVm`]), fed with the text-model list; this adds the tier, the licence and the
//! recommendation. Nothing here starts a download by itself: the only path is the confirm or
//! resume press of a row (or the report card's button, which names the size).

use std::collections::HashMap;

use cm_ai::model::ModelList;
use cm_ai::text_model::store::TextModelList;
use cm_i18n::{tr, tr_ctx};

use crate::ai::models::{size_label, ModelRow, ModelsVm, RowAction};

#[derive(Debug, Clone, Default)]
pub struct BuiltInVm {
    /// Rows, downloads and confirmations: the speech models' machinery on the text-model list.
    pub models: ModelsVm,
    meta: HashMap<String, Meta>,
    /// `list_text_models` has answered.
    pub loaded: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Meta {
    tier_gb: u32,
    license: String,
    context_tokens: u32,
}

/// One row of the Built in list.
#[derive(Debug, Clone, PartialEq)]
pub struct BuiltInRow {
    pub row: ModelRow,
    pub tier_gb: u32,
    pub license: String,
    /// The one suggested for this Mac's memory.
    pub recommended: bool,
}

impl BuiltInRow {
    /// `8 GB Macs`.
    pub fn tier_label(&self) -> String {
        if self.tier_gb >= 32 {
            tr_ctx!("memory tier", "{gb} GB Macs and up", gb = self.tier_gb)
        } else {
            tr_ctx!("memory tier", "{gb} GB Macs", gb = self.tier_gb)
        }
    }
}

impl BuiltInVm {
    pub fn set_list(&mut self, l: TextModelList) {
        self.meta = l
            .models
            .iter()
            .map(|m| {
                (
                    m.info.id.clone(),
                    Meta {
                        tier_gb: m.tier_gb,
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

    /// The model to suggest for a Mac with `mem_gb` of memory: the largest tier that fits.
    /// `None` when the Mac is below the smallest tier or nothing is on offer.
    pub fn recommended_id(&self, mem_gb: Option<u32>) -> Option<String> {
        let gb = mem_gb?;
        self.meta
            .iter()
            .filter(|(_, m)| m.tier_gb <= gb)
            .max_by_key(|(_, m)| m.tier_gb)
            .map(|(id, _)| id.clone())
    }

    /// The recommended model if it is not downloaded and not downloading: what the report
    /// card offers. `(id, label, size_mb)`.
    pub fn offer(&self, mem_gb: Option<u32>) -> Option<(String, String, u32)> {
        let id = self.recommended_id(mem_gb)?;
        let row = self.models.rows().into_iter().find(|r| r.id == id)?;
        let idle = matches!(row.status, crate::ai::models::RowStatus::NotInstalled);
        idle.then_some((row.id, row.label, row.size_mb))
    }

    /// The rows by tier, smallest first. Import is not offered for text models.
    pub fn rows(&self, mem_gb: Option<u32>) -> Vec<BuiltInRow> {
        let rec = self.recommended_id(mem_gb);
        let mut rows: Vec<BuiltInRow> = self
            .models
            .rows()
            .into_iter()
            .map(|mut row| {
                row.actions.retain(|a| *a != RowAction::Import);
                let meta = self.meta.get(&row.id).cloned().unwrap_or_default();
                BuiltInRow {
                    recommended: rec.as_deref() == Some(row.id.as_str()),
                    tier_gb: meta.tier_gb,
                    license: meta.license,
                    row,
                }
            })
            .collect();
        rows.sort_by_key(|r| r.tier_gb);
        rows
    }

    /// The context a downloaded model runs with, for the licence line.
    pub fn context_of(&self, id: &str) -> Option<u32> {
        self.meta.get(id).map(|m| m.context_tokens)
    }
}

/// "Recommended for this Mac" tag text.
pub fn recommended_tag() -> String {
    tr!("Recommended for this Mac")
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

    fn entry(id: &str, tier: u32, mb: u32, installed: bool) -> TextModelInfo {
        TextModelInfo {
            info: ModelInfo {
                id: id.into(),
                label: format!("Model {tier}"),
                size_mb: mb,
                ram_mb: mb * 2,
                hint: String::new(),
                installed,
                active: installed,
                path: installed.then(|| format!("/t/{id}.gguf")),
            },
            tier_gb: tier,
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
            entry("c", 32, 9000, false),
            entry("a", 8, 2500, false),
            entry("b", 16, 5000, false),
        ]));
        vm
    }

    #[test]
    fn an_empty_catalogue_is_a_plain_empty_state() {
        let mut vm = BuiltInVm::default();
        assert!(vm.is_empty() && !vm.loaded);
        vm.set_list(list(vec![]));
        assert!(vm.is_empty() && vm.loaded);
        assert!(vm.rows(Some(16)).is_empty());
        assert_eq!(vm.recommended_id(Some(16)), None);
        assert_eq!(vm.offer(Some(16)), None);
    }

    #[test]
    fn rows_are_by_tier_and_the_recommendation_follows_the_memory() {
        let vm = vm();
        let ids = |gb| {
            vm.rows(gb)
                .into_iter()
                .map(|r| (r.row.id, r.recommended))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(Some(16)),
            [("a".into(), false), ("b".into(), true), ("c".into(), false)]
        );
        assert_eq!(ids(Some(8))[0], ("a".into(), true));
        assert!(
            ids(Some(4)).iter().all(|(_, r)| !r),
            "below the smallest tier"
        );
        assert!(ids(None).iter().all(|(_, r)| !r));
        assert_eq!(vm.recommended_id(Some(64)).as_deref(), Some("c"));
        let rows = vm.rows(Some(16));
        assert_eq!(rows[1].tier_label(), "16 GB Macs");
        assert_eq!(rows[2].tier_label(), "32 GB Macs and up");
        assert_eq!(rows[1].license, "Apache-2.0");
    }

    #[test]
    fn a_download_asks_first_and_text_rows_have_no_import() {
        let mut vm = vm();
        let actions = |vm: &BuiltInVm| {
            vm.rows(Some(16))
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
        assert_eq!(
            vm.offer(Some(16)),
            Some(("b".into(), "Model 16".into(), 5000))
        );
        vm.models.apply("b", &RowAction::Download);
        assert_eq!(vm.offer(Some(16)), None, "already asked or running");
        vm.set_list(list(vec![entry("b", 16, 5000, true)]));
        assert_eq!(vm.offer(Some(16)), None, "downloaded");
        assert!(vm.installed_any());
        assert_eq!(vm.context_of("b"), Some(8192));
    }
}
