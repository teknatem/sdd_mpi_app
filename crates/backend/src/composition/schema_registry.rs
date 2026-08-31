//! Состав реестра схем для «Конструктора запросов» и сводных.
//!
//! Три вида записей, и они не взаимозаменяемы:
//!
//! * **custom** — базовые схемы `dsXX`, описанные кодом. Сюда попадают
//!   источники, у которых форма сложнее одной таблицы.
//! * **auto** — агрегаты, чьи метаданные разрешено выдавать как схему.
//!   Список явный: сущности с кредами (`a001`, `a006` целиком) не
//!   регистрируются оптом никогда — попадёт `a006`, попадут и токены кабинетов.
//! * **aliases** — исторические имена схем, которые ещё приходят из
//!   сохранённых настроек и ссылок.

use contracts::domain::a002_organization::{ENTITY_METADATA as A002_META, FIELDS as A002_FIELDS};
use contracts::domain::a004_nomenclature::{ENTITY_METADATA as A004_META, FIELDS as A004_FIELDS};
use contracts::domain::a005_marketplace::{ENTITY_METADATA as A005_META, FIELDS as A005_FIELDS};
use contracts::domain::a006_connection_mp::{ENTITY_METADATA as A006_META, FIELDS as A006_FIELDS};
use contracts::domain::a012_wb_sales::{ENTITY_METADATA as A012_META, FIELDS as A012_FIELDS};
use contracts::domain::a036_wb_sales_funnel_daily::{
    ENTITY_METADATA as A036_META, FIELDS as A036_FIELDS,
};
use contracts::domain::a037_wb_product_snapshot::{
    ENTITY_METADATA as A037_META, FIELDS as A037_FIELDS,
};
use contracts::shared::metadata::{EntityMetadataInfo, FieldMetadata};
use contracts::shared::universal_dashboard::DataSourceSchema;

use crate::data_schemes::ds01_wb_finance_report::schema::{DS01_SCHEMA, DS01_TABLE_NAME};
use crate::data_schemes::ds02_mp_sales_register::schema::{DS02_SCHEMA, DS02_TABLE_NAME};
use crate::data_schemes::ds03_p904_sales::schema::{DS03_SCHEMA, DS03_TABLE_NAME};
use crate::shared::universal_dashboard::entity_registry::{self, SchemaRegistry};

/// Установить реестр схем.
pub fn install() {
    entity_registry::install(build());
}

fn build() -> SchemaRegistry {
    SchemaRegistry::build(custom(), auto(), aliases(), ref_fallback())
}

fn custom() -> Vec<(&'static DataSourceSchema, &'static str)> {
    vec![
        (&DS01_SCHEMA, DS01_TABLE_NAME),
        (&DS02_SCHEMA, DS02_TABLE_NAME),
        (&DS03_SCHEMA, DS03_TABLE_NAME),
    ]
}

fn auto() -> Vec<(&'static EntityMetadataInfo, &'static [FieldMetadata])> {
    vec![
        (&A002_META, A002_FIELDS),
        (&A004_META, A004_FIELDS),
        (&A005_META, A005_FIELDS),
        (&A006_META, A006_FIELDS),
        (&A012_META, A012_FIELDS),
        // Суточные снимки WB: плоские итоги по кабинету и дате как базовая схема.
        // Подробность по номенклатуре (`lines_json`) помечена
        // `visible_in_list=false` и сюда не попадает — до неё добираются сырым
        // SQL через `json_each` (см. ai_hint поля).
        (&A036_META, A036_FIELDS),
        (&A037_META, A037_FIELDS),
    ]
}

/// Исторические имена схем → канонические.
fn aliases() -> Vec<(&'static str, &'static str)> {
    vec![
        ("p903_wb_finance_report", "ds01_wb_finance_report"),
        ("s001_wb_finance", "ds01_wb_finance_report"),
        ("p900_sales_register", "ds02_mp_sales_register"),
    ]
}

/// Индекс агрегата → таблица, когда метаданных схемы нет, а ссылку разрешить надо.
fn ref_fallback() -> Vec<(&'static str, &'static str)> {
    vec![
        ("a002", "a002_organization"),
        ("a003", "a003_counterparty"),
        ("a004", "a004_nomenclature"),
        ("a005", "a005_marketplace"),
        ("a006", "a006_connection_mp"),
    ]
}

#[cfg(test)]
mod tests {
    use super::build;
    use contracts::shared::universal_dashboard::SchemaSource;

    #[test]
    fn custom_schemas_are_registered() {
        let registry = build();
        assert!(registry.has_schema("ds01_wb_finance_report"));
        assert!(registry.has_schema("ds02_mp_sales_register"));
        assert!(registry.has_schema("a012"));
    }

    #[test]
    fn list_all_reports_custom_source() {
        let registry = build();
        let schemas = registry.list_all();
        assert!(!schemas.is_empty());

        let ds01 = schemas
            .iter()
            .find(|schema| schema.id == "ds01_wb_finance_report")
            .expect("ds01 отсутствует в списке схем");
        assert_eq!(ds01.source, SchemaSource::Custom);
    }

    #[test]
    fn custom_schema_resolves_with_fields() {
        let registry = build();
        let schema = registry
            .get_schema("ds01_wb_finance_report")
            .expect("ds01 не резолвится");
        assert_eq!(schema.id, "ds01_wb_finance_report");
        assert!(!schema.fields.is_empty());
    }

    /// Историческое имя обязано вести на ту же схему: оно приходит из
    /// сохранённых настроек отчётов, которые никто не переписывал.
    #[test]
    fn aliases_resolve_to_canonical_schemas() {
        let registry = build();
        for (alias, canonical) in super::aliases() {
            let schema = registry
                .get_schema(alias)
                .unwrap_or_else(|| panic!("алиас '{alias}' не резолвится"));
            assert_eq!(schema.id, canonical, "алиас '{alias}'");
        }
    }
}
