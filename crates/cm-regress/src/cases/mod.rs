//! The case registry. One module per area; `all()` lists every case in the order they run.

use crate::model::Case;

mod annotate;
pub mod common;
mod daemon;
pub mod desktop;
mod export;
mod import;
mod installer;
mod library;
mod linux;
mod mcp;
mod models;
mod package;
mod record;
mod regress;
mod targets;

pub fn all() -> Vec<Case> {
    let mut v = Vec::new();
    v.extend(package::cases());
    v.extend(daemon::cases());
    v.extend(targets::cases());
    v.extend(library::cases());
    v.extend(annotate::cases());
    v.extend(import::cases());
    v.extend(export::cases());
    v.extend(mcp::cases());
    v.extend(record::cases());
    v.extend(models::cases());
    v.extend(installer::cases());
    v.extend(desktop::cases());
    v.extend(linux::cases());
    v.extend(regress::cases());
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Tier;
    use std::collections::BTreeSet;

    #[test]
    fn ids_are_unique_and_shaped_area_dot_name() {
        let all = all();
        let mut seen = BTreeSet::new();
        for c in &all {
            assert!(seen.insert(c.id), "duplicate id {}", c.id);
            let (area, _) =
                c.id.split_once('.')
                    .unwrap_or_else(|| panic!("{} has no area prefix", c.id));
            assert_eq!(area, c.area, "{}: area does not match the id prefix", c.id);
            assert!(c.timeout_s >= 10, "{} has a silly timeout", c.id);
            assert!(!c.title.is_empty() && !c.platforms.is_empty());
        }
    }

    #[test]
    fn every_tier_has_cases_and_smoke_is_small() {
        let all = all();
        for t in [Tier::Smoke, Tier::Standard, Tier::Full, Tier::Hardware] {
            assert!(all.iter().any(|c| c.tier == t), "no case in {}", t.name());
        }
        let smoke_budget: u64 = all
            .iter()
            .filter(|c| c.tier == Tier::Smoke)
            .map(|c| c.timeout_s)
            .sum();
        // Worst case time limits add up, real runs are a fraction of them.
        assert!(
            smoke_budget < 1500,
            "smoke time limits add up to {smoke_budget} s"
        );
    }
}
