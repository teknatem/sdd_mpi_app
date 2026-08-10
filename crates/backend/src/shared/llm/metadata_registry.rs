//! Реестр метаданных всех сущностей для LLM.
//!
//! Список сущностей собирается автоматически: `contracts::shared::metadata::ALL_ENTITIES`
//! генерируется build-скриптом контрактов из всех `metadata.json` (домены, проекции,
//! Главная книга). Регистрировать сущность здесь руками не нужно и нельзя — достаточно
//! завести metadata.json. Скрыть сущность от LLM можно флагом `"llm_visible": false`
//! в её `ai`-блоке; сейчас исключений нет.

use contracts::shared::metadata::{EntityRegistration, FieldMetadata, FieldSource, ALL_ENTITIES};
use once_cell::sync::Lazy;
use serde_json::{json, Value};

// ─── Структуры ──────────────────────────────────────────────────────────────

type RegistryEntry = EntityRegistration;

pub struct MetadataRegistry {
    entries: Vec<&'static RegistryEntry>,
}

// ─── Глобальный экземпляр ───────────────────────────────────────────────────

pub static METADATA_REGISTRY: Lazy<MetadataRegistry> = Lazy::new(MetadataRegistry::build);

// ─── Реализация ─────────────────────────────────────────────────────────────

impl MetadataRegistry {
    /// Собирает реестр из сгенерированного каталога контрактов.
    /// Тематические теги и признак видимости живут в metadata.json (блок `ai`).
    fn build() -> Self {
        Self {
            entries: ALL_ENTITIES
                .iter()
                .filter(|entity| entity.meta.ai.llm_visible)
                .collect(),
        }
    }

    // ─── list_entities ────────────────────────────────────────────────────

    /// Вернуть список всех сущностей с кратким описанием.
    /// `category` — необязательный фильтр: "wb" | "ozon" | "ym" | "ref" | "llm"
    pub fn list_entities(&self, category: Option<&str>) -> Value {
        let items: Vec<Value> = self
            .entries
            .iter()
            .filter(|e| category.map_or(true, |cat| e.meta.ai.tags.contains(&cat)))
            .map(|e| {
                let table = e.meta.table_name.unwrap_or(e.meta.collection_name);
                json!({
                    "index":       e.meta.entity_index,
                    "table":       table,
                    "name":        e.meta.ui.element_name,
                    "description": e.meta.ai.description,
                    "questions":   e.meta.ai.questions,
                    "tags":        e.meta.ai.tags,
                })
            })
            .collect();

        let total = items.len();
        json!({
            "entities": items,
            "total": total,
            "hint": "Используй get_entity_schema(entity_index) для получения полей конкретной таблицы."
        })
    }

    // ─── architecture_overview ────────────────────────────────────────────

    /// Компактная карта всех сущностей и их связей в одном вызове.
    /// Заменяет серию `list_entities` — LLM сразу видит граф системы.
    /// `category` — необязательный фильтр по тегу ("wb", "gl", "bi", ...).
    pub fn architecture_overview(&self, category: Option<&str>) -> Value {
        let entities: Vec<Value> = self
            .entries
            .iter()
            .filter(|e| category.map_or(true, |cat| e.meta.ai.tags.contains(&cat)))
            .map(|e| {
                let table = e.meta.table_name.unwrap_or(e.meta.collection_name);
                json!({
                    "index":   e.meta.entity_index,
                    "name":    e.meta.ui.element_name,
                    "table":   table,
                    "tags":    e.meta.ai.tags,
                    "related": e.meta.ai.related,
                })
            })
            .collect();

        // Перечень всех доступных категорий (тегов) для навигации.
        let mut categories: Vec<&str> = self
            .entries
            .iter()
            .flat_map(|e| e.meta.ai.tags.iter().copied())
            .collect();
        categories.sort_unstable();
        categories.dedup();

        let total = entities.len();
        json!({
            "entities":   entities,
            "total":      total,
            "categories": categories,
            "hint": "Карта сущностей и связей (related = индексы связанных таблиц). \
                     Детали полей — get_entity_schema(index); SQL JOIN — get_join_hint(from, to); \
                     учёт — get_chart_of_accounts и list_gl_turnovers."
        })
    }

    // ─── get_entity_schema ────────────────────────────────────────────────

    /// Вернуть детальную схему сущности: таблица, поля, типы, ai_hint, FK.
    /// `entity_index` — индекс сущности, например "a012" или "a004".
    pub fn get_entity_schema(&self, entity_index: &str) -> Value {
        let Some(entry) = self.find_by_index(entity_index) else {
            let available: Vec<(&str, Option<&str>)> = self
                .entries
                .iter()
                .map(|e| (e.meta.entity_index, e.meta.table_name))
                .collect();
            let hint_list: Vec<String> = available
                .iter()
                .map(|(idx, tbl)| {
                    if let Some(t) = tbl {
                        format!("'{}' (table: {})", idx, t)
                    } else {
                        format!("'{}'", idx)
                    }
                })
                .collect();
            tracing::warn!(
                "[get_entity_schema] '{}' not found. Available: {:?}",
                entity_index,
                available
            );
            return json!({
                "error": format!(
                    "Entity '{}' not found. Works with the short index ('a012') or the table \
                     name ('a012_wb_sales'). If the table is genuinely absent from the catalog, \
                     do NOT retry this tool — query it directly with execute_query instead. \
                     Available: {}",
                    entity_index,
                    hint_list.join(", ")
                )
            });
        };

        let table = entry.meta.table_name.unwrap_or(entry.meta.collection_name);

        let fields: Vec<Value> = entry
            .fields
            .iter()
            .filter(|f| !Self::is_internal_field(f))
            .map(|f| {
                let mut field = json!({
                    "column":   f.name,
                    "type":     Self::rust_to_sql_type(f.rust_type),
                    "label":    f.ui.label,
                    "required": f.validation.required,
                });

                if let Some(hint) = f.ai_hint {
                    field["hint"] = hint.into();
                } else if let Some(ui_hint) = f.ui.hint {
                    field["hint"] = ui_hint.into();
                }

                if let Some(values) = f.enum_values {
                    field["enum_values"] = json!(values);
                }

                if let Some(ref_entity) = f.ref_aggregate {
                    field["fk_to"] = ref_entity.into();
                }

                if !f.physical {
                    field["not_a_column"] = true.into();
                }

                field
            })
            .collect();

        // Только реальные колонки таблицы: логические поля документов лежат внутри
        // JSON-колонок, и попадание их в SQL даёт "no such column".
        let columns_for_sql: Vec<&str> = entry
            .fields
            .iter()
            .filter(|f| f.physical && !Self::is_internal_field(f))
            .map(|f| f.name)
            .collect();

        tracing::info!(
            "[get_entity_schema] index='{}' table='{}' fields={}",
            entity_index,
            table,
            columns_for_sql.len()
        );

        json!({
            "index":           entity_index,
            "table":           table,
            "name":            entry.meta.ui.element_name,
            "description":     entry.meta.ai.description,
            "fields":          fields,
            "columns_for_sql": columns_for_sql,
            "related":         entry.meta.ai.related,
            "sql_hint":        format!(
                "SELECT {} FROM {} WHERE is_deleted = 0 LIMIT 100",
                columns_for_sql[..columns_for_sql.len().min(5)].join(", "),
                table
            ),
        })
    }

    // ─── get_join_hint ────────────────────────────────────────────────────

    /// Подсказка как соединить две таблицы через JOIN.
    /// Ищет FK в полях `from_entity`, ссылающиеся на `to_entity`.
    pub fn get_join_hint(&self, from_index: &str, to_index: &str) -> Value {
        let Some(from_entry) = self.find_by_index(from_index) else {
            return json!({ "error": format!("Entity '{}' not found", from_index) });
        };
        let Some(to_entry) = self.find_by_index(to_index) else {
            return json!({ "error": format!("Entity '{}' not found", to_index) });
        };

        let from_table = from_entry
            .meta
            .table_name
            .unwrap_or(from_entry.meta.collection_name);
        let to_table = to_entry
            .meta
            .table_name
            .unwrap_or(to_entry.meta.collection_name);

        // ref_aggregate может хранить любое из имён сущности (индекс, таблица,
        // коллекция, каталог модуля) — сопоставляем тем же правилом, что и поиск.
        let matches_to =
            |ref_agg: Option<&str>| ref_agg.is_some_and(|ra| Self::names_match(to_entry, ra));

        let matches_from =
            |ref_agg: Option<&str>| ref_agg.is_some_and(|ra| Self::names_match(from_entry, ra));

        // Ищем FK-поля в from_entry, ссылающиеся на to_entry
        let fk_fields: Vec<_> = from_entry
            .fields
            .iter()
            .filter(|f| matches_to(f.ref_aggregate))
            .collect();

        if let Some(fk) = fk_fields.first() {
            let hint_text = fk.ai_hint.or(fk.ui.hint).unwrap_or("");
            return json!({
                "from_table": from_table,
                "to_table":   to_table,
                "join_sql":   format!(
                    "JOIN {} ON {}.{} = {}.id",
                    to_table, from_table, fk.name, to_table
                ),
                "fk_column":  fk.name,
                "fk_label":   fk.ui.label,
                "hint":       hint_text,
            });
        }

        // Ищем в обратном направлении
        let reverse_fk: Vec<_> = to_entry
            .fields
            .iter()
            .filter(|f| matches_from(f.ref_aggregate))
            .collect();

        if let Some(fk) = reverse_fk.first() {
            let hint_text = fk.ai_hint.or(fk.ui.hint).unwrap_or("");
            return json!({
                "from_table": from_table,
                "to_table":   to_table,
                "join_sql":   format!(
                    "JOIN {} ON {}.{} = {}.id",
                    from_table, to_table, fk.name, from_table
                ),
                "fk_column":  format!("{}.{}", to_table, fk.name),
                "hint":       hint_text,
                "note":       "Reversed: FK is in the to_table side",
            });
        }

        // FK не найден — предложить через промежуточную таблицу
        json!({
            "from_table": from_table,
            "to_table":   to_table,
            "error":      "No direct FK found between these entities.",
            "suggestion": format!(
                "Check if there is an intermediate table. Use list_entities() and get_entity_schema() to explore.",
            ),
            "related_from": from_entry.meta.ai.related,
            "related_to":   to_entry.meta.ai.related,
        })
    }

    // ─── Вспомогательные ──────────────────────────────────────────────────

    /// Найти сущность по любому из её имён: entity_index ("a006"), table_name
    /// ("a006_connection_mp"), collection_name ("connection_mp") или имени модуля
    /// ("a001_connection_1c" при таблице "a001_connection_1c_database").
    fn find_by_index(&self, index: &str) -> Option<&RegistryEntry> {
        self.entries
            .iter()
            .copied()
            .find(|e| Self::names_match(e, index))
    }

    /// Одно правило сопоставления имён для всего реестра: сущность у нас известна
    /// под несколькими именами (индекс, таблица, коллекция, каталог модуля), и они
    /// не всегда совпадают. Префикс до первого `_` — это индекс, он уникален.
    fn names_match(entry: &RegistryEntry, reference: &str) -> bool {
        entry.meta.entity_index == reference
            || entry.meta.table_name == Some(reference)
            || entry.meta.collection_name == reference
            || reference
                .split_once('_')
                .is_some_and(|(index, _)| index == entry.meta.entity_index)
    }

    /// Служебные поля, не нужные LLM
    fn is_internal_field(f: &FieldMetadata) -> bool {
        // Скрываем поля из EntityMetadata (version, is_posted, events и т.п.)
        // кроме созданных/обновлённых дат
        if f.source == FieldSource::Metadata {
            return matches!(f.name, "version" | "is_posted" | "events");
        }
        // Скрываем пароли
        matches!(f.ui.widget, Some("password"))
    }

    /// Преобразовать Rust-тип в SQL-тип для контекста LLM
    fn rust_to_sql_type(rust_type: &str) -> &'static str {
        match rust_type {
            t if t.contains("bool") => "INTEGER (0/1)",
            t if t.contains("i32") || t.contains("i64") || t.contains("u32") => "INTEGER",
            t if t.contains("f32") || t.contains("f64") => "REAL",
            t if t.contains("DateTime") => "TEXT (ISO 8601)",
            t if t.ends_with("Id") || t.contains("Option<String>") => "TEXT (UUID or NULL)",
            _ => "TEXT",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Каждая сущность каталога должна быть доступна LLM и отдавать непустую схему.
    /// Регистрация автоматическая (`ALL_ENTITIES`), поэтому перечислять индексы руками
    /// больше не нужно — тест идёт по самому каталогу.
    #[test]
    fn registry_exposes_every_catalog_entity_with_fields() {
        assert_eq!(
            METADATA_REGISTRY.entries.len(),
            ALL_ENTITIES
                .iter()
                .filter(|entity| entity.meta.ai.llm_visible)
                .count(),
            "registry must mirror the generated catalog exactly"
        );

        for entity in METADATA_REGISTRY.entries.iter() {
            let index = entity.meta.entity_index;
            let schema = METADATA_REGISTRY.get_entity_schema(index);
            assert!(
                schema.get("error").is_none(),
                "entity '{index}' must be reachable via get_entity_schema, got: {schema}"
            );
            let has_fields = schema
                .get("fields")
                .and_then(Value::as_array)
                .is_some_and(|fields| !fields.is_empty());
            assert!(has_fields, "entity '{index}' must expose non-empty fields");
        }
    }

    /// Ссылки `related` ведут LLM по графу сущностей: если цель не зарегистрирована,
    /// модель упирается в «Entity not found» вместо ответа.
    #[test]
    fn related_targets_resolve_to_registered_entities() {
        let mut dangling: Vec<String> = Vec::new();

        for entity in METADATA_REGISTRY.entries.iter() {
            for target in entity.meta.ai.related {
                if METADATA_REGISTRY.find_by_index(target).is_none() {
                    dangling.push(format!("{} -> {}", entity.meta.entity_index, target));
                }
            }
        }

        assert!(
            dangling.is_empty(),
            "related[] must point at registered entities, dangling: {dangling:?}"
        );
    }

    /// Реестр обещает LLM таблицу для raw SQL — имя должно быть заполнено.
    #[test]
    fn every_entity_declares_a_table() {
        let missing: Vec<&str> = METADATA_REGISTRY
            .entries
            .iter()
            .filter(|entity| entity.meta.table_name.is_none())
            .map(|entity| entity.meta.entity_index)
            .collect();

        assert!(
            missing.is_empty(),
            "entities without table_name cannot be queried: {missing:?}"
        );
    }

    #[test]
    fn finance_tables_for_raw_sql_are_available() {
        // Регрессия на главную дыру: официальный слой реализации YM (ybuh, a034),
        // сверка расчётов (a035) и WB-возвраты (a032) должны быть видимы для raw SQL.
        for index in ["a034", "a035", "a032", "a027"] {
            let schema = METADATA_REGISTRY.get_entity_schema(index);
            assert!(schema.get("error").is_none(), "{index} missing: {schema}");
        }
    }

    #[test]
    fn ym_realization_links_to_payment_report() {
        // a034 объявляет related → p907_ym_payment_report; карта связей не должна теряться.
        let schema = METADATA_REGISTRY.get_entity_schema("a034");
        let related = schema.get("related").and_then(Value::as_array);
        assert!(
            related.is_some_and(|items| items
                .iter()
                .any(|item| item.as_str().is_some_and(|name| name.contains("p907")))),
            "a034 related must include p907_ym_payment_report, got: {schema}"
        );
    }
}
