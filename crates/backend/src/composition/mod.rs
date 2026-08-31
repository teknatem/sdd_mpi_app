//! Composition root — единственное место, где перечислен состав системы.
//!
//! Ядро (`shared/`, `general_ledger/`, `quality/`, `system/`) знает только
//! трейты; кто именно их реализует, решается здесь. Это та граница, вдоль
//! которой приложение режется на крейты: всё, что перечислено ниже, при
//! разрезе переезжает в крейт `app-backend`, а модули-реализации — в свои.
//!
//! Правило простое: **имя агрегата не должно встречаться нигде в ядре**.
//! Встречается — значит, ядро зависит от прикладного слоя, и разрез не
//! состоится. Соблюдение проверяет правило `core_does_not_know_marketplaces`
//! в `architecture.toml`.

pub mod actions;
pub mod aggregate_reposts;
pub mod change_tokens;
pub mod gl_detail_sources;
pub mod llm_tools;
pub mod nomenclature_orders;
pub mod projection_reposts;
pub mod reference_resolvers;
pub mod registrators;
pub mod schema_registry;

/// Собрать и установить все реестры процесса.
///
/// Зовётся из `main` до сборки роутера и из интеграционных тестов, которые
/// линкуются против библиотеки и своего `main` не имеют. Повторный вызов
/// безопасен и ничего не делает: список один, и второй его установки не бывает
/// — а вот тестов, каждый из которых хочет рабочие реестры, бывает много.
///
/// Идемпотентность живёт **здесь, а не в самих реестрах**: попытка установить
/// им *другой* состав по-прежнему должна падать, иначе часть системы тихо
/// заработает с чужим набором типов.
pub fn install_all() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        registrators::install();
        change_tokens::install();
        projection_reposts::install();
        aggregate_reposts::install();
        nomenclature_orders::install();
        gl_detail_sources::install();
        schema_registry::install();
        reference_resolvers::install();
        actions::install();
        llm_tools::install();
    });
}
