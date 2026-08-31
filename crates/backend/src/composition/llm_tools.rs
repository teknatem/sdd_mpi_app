//! Состав инструментов чата, объявленных срезами.
//!
//! Пятнадцать наборов ядра (данные, база знаний, плагины, графики, почта,
//! расписание, тикеты, качество, …) перечисляет сам механизм чата — они его
//! часть. Сюда попадает только то, что принадлежит прикладному срезу: ядро
//! чата такие имена знать не должно, иначе `shared/llm` не отделяется от
//! `projections/`.

use std::sync::Arc;

use crate::shared::llm::tool_provider::{self, ToolProvider};

/// Установить реестр инструментов срезов.
pub fn install() {
    tool_provider::install(catalog());
}

fn catalog() -> Vec<Arc<dyn ToolProvider>> {
    vec![
        Arc::new(crate::projections::p916_mp_sales_funnel_turnovers::llm_tools::FunnelRepairTools)
            as Arc<dyn ToolProvider>,
    ]
}

#[cfg(test)]
mod tests {
    use super::catalog;

    /// Имена инструментов среза не должны совпадать ни между собой, ни с
    /// именами ядра — `install` на этом падает, и падать он должен на сборке
    /// каталога, а не у первого пользователя чата.
    #[test]
    fn slice_tool_names_are_unique_and_not_core() {
        let mut seen = std::collections::HashSet::new();
        for provider in catalog() {
            for name in provider.tool_names() {
                assert!(seen.insert(*name), "инструмент '{name}' объявлен дважды");
            }
        }
        assert!(!seen.is_empty(), "каталог инструментов срезов пуст");
    }

    /// Право объявляется только для того, что действительно меняет данные:
    /// диагностика и статус должны оставаться доступными без него.
    #[test]
    fn only_mutating_tools_require_a_capability() {
        let provider = &catalog()[0];
        assert_eq!(
            provider.required_capability("execute_funnel_repair"),
            Some(crate::shared::llm::skill_policy::DATA_REPAIR_EXECUTE)
        );
        assert_eq!(provider.required_capability("prepare_funnel_repair"), None);
        assert_eq!(
            provider.required_capability("get_funnel_repair_status"),
            None
        );
    }
}
