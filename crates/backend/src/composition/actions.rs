//! Состав каталога Действий (ADR-0011).
//!
//! Каталог намеренно маленький и растёт поштучно: каждое новое Действие
//! расширяет то, что система вообще способна сделать с миром. Порядок здесь —
//! порядок в UI каталога Действий.

use std::sync::Arc;

use crate::processes::actions::{self, Action};

/// Установить каталог Действий.
pub fn install() {
    actions::install(catalog());
}

fn catalog() -> Vec<Arc<dyn Action>> {
    vec![
        Arc::new(crate::domain::a033_wb_day_close::action::RebuildDayClose) as Arc<dyn Action>,
        Arc::new(actions::repost_documents::RepostDocuments),
        Arc::new(actions::run_quality_check::RunQualityCheck),
        Arc::new(actions::create_agent_task::CreateAgentTask),
        Arc::new(actions::request_human_action::RequestHumanAction),
        Arc::new(actions::import_nomenclature::ImportNomenclature),
        Arc::new(crate::domain::a007_marketplace_product::action::ImportMarketplaceProducts),
        Arc::new(crate::usecases::u505_match_nomenclature::action::MatchNomenclature),
        Arc::new(actions::repair_empty_nomenclature_refs::RepairEmptyNomenclatureRefs),
    ]
}

#[cfg(test)]
mod tests {
    use super::catalog;

    /// Имя Действия — то, чем его зовёт манифест Этапа (`action:<name>`).
    /// Пустое или дублирующееся имя означает Этап, который нельзя валидировать.
    #[test]
    fn action_names_are_unique_and_non_empty() {
        let mut seen = std::collections::HashSet::new();
        for action in catalog() {
            let name = action.info().name;
            assert!(!name.is_empty(), "у Действия пустое имя");
            assert!(seen.insert(name), "Действие '{name}' заявлено дважды");
        }
    }
}
