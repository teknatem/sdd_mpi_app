//! Сборка «контекста страницы» для LLM-чата.
//!
//! По ключу вкладки (`page_key`) определяет тип страницы и собирает:
//! - универсальную часть (ссылка, тип, идентичность, описание сущности);
//! - для страниц агрегатов — строку объекта (generic SELECT по имени таблицы из
//!   метаданных) и лёгкие смежные подписи по внешним ключам (FK);
//! - для прочих страниц — общее описание.
//!
//! Возвращает `BuiltContext` с полным JSON (для хранения) и компактным текстом
//! (для инъекции в диалог).

use sea_orm::{DatabaseBackend, FromQueryResult, Statement};
use serde_json::{json, Value};

use crate::shared::llm::METADATA_REGISTRY;
use crate::shared::representation;

/// Колонки, которые не несут смысла для LLM-контекста.
const SKIP_COLUMNS: &[&str] = &[
    "is_deleted",
    "is_posted",
    "version",
    "created_at",
    "updated_at",
];

/// Результат сборки контекста.
pub struct BuiltContext {
    pub title: String,
    pub page_type: String,
    pub entity_index: Option<String>,
    pub entity_id: Option<String>,
    pub context_json: Value,
    pub rendered_text: String,
}

struct PageRef {
    page_type: &'static str,
    /// Полный «kind» для representation (например `a012_wb_sales`).
    kind: Option<String>,
    /// Короткий индекс сущности (например `a012`).
    entity_index: Option<String>,
    entity_id: Option<String>,
}

fn first_segment(s: &str) -> &str {
    s.split('_').next().unwrap_or(s)
}

/// Похоже на индекс объекта: буква (a/p/d/u) + цифры (a012, p903, d400, u501).
fn looks_like_index(seg: &str) -> bool {
    let mut chars = seg.chars();
    matches!(chars.next(), Some('a') | Some('p') | Some('d') | Some('u'))
        && seg.len() >= 2
        && seg[1..].chars().all(|c| c.is_ascii_digit())
}

/// Известные плагины, для которых уже заведён DataView с реальными данными —
/// код плагина (`manifest.code`) → подсказка для LLM, каким инструментом их читать.
const PLUGIN_DATAVIEW_HINTS: &[(&str, &str)] = &[(
    "PLG-WB-SALES-FUNNEL",
    "Это отчёт «Воронка продаж WB» (a036_wb_sales_funnel_daily). Данные доступны через \
     run_data_view_drilldown(view_id=\"dv008_wb_sales_funnel\", \
     group_by=\"nm_id\"|\"date\"|\"week\"|\"month\"|\"connection_mp_ref\" (на диапазоне длиннее месяца — month/week), \
     date_from, date_to, connection_mp_refs, metric_ids=[...]) или run_data_view_scalar для сводной цифры \
     за период (params.metric = open_count|cart_count|order_count|order_sum|buyout_count|buyout_sum|\
     cart_conv_pct|order_conv_pct|buyout_pct).",
)];

/// Ключ вкладки страницы «Метрики проекта».
const METRICS_PAGE_KEY: &str = "sys_metrics";

/// Разобрать ключ вкладки в ссылку на страницу. Зеркалит правила tabs/registry.rs.
fn parse_page_key(key: &str) -> PageRef {
    // Метрики проекта: не агрегат и не отчёт, но единственная страница, чей
    // смысл целиком в числах. Без собственного типа она падала в `other` и
    // уезжала в чат одним лишь ключом вкладки — консультировать было не по чему.
    if key == METRICS_PAGE_KEY {
        return PageRef {
            page_type: "system_metrics",
            kind: None,
            entity_index: None,
            entity_id: None,
        };
    }

    // Дрилдаун-сессии
    if key.starts_with("drilldown__") || key.starts_with("gl_drilldown__") {
        let session = key.splitn(2, "__").nth(1).unwrap_or("").to_string();
        return PageRef {
            page_type: "drilldown",
            kind: None,
            entity_index: None,
            entity_id: Some(session),
        };
    }

    // Страница плагина: `plugin__<id>` — раньше падало в `"other"` без каких-либо
    // данных, т.к. "plugin" не проходит `looks_like_index` (буквы после 'p', не цифры).
    if let Some(id) = key.strip_prefix("plugin__") {
        return PageRef {
            page_type: "plugin",
            kind: None,
            entity_index: None,
            entity_id: Some(id.to_string()),
        };
    }

    // Детальные страницы: <kind>_details_<id> (иногда _details_id_<id>)
    if let Some(idx) = key.find("_details_") {
        let kind = key[..idx].to_string();
        let mut rest = &key[idx + "_details_".len()..];
        if let Some(stripped) = rest.strip_prefix("id_") {
            rest = stripped;
        }
        let seg0 = first_segment(&kind).to_string();
        let page_type = if seg0.starts_with('p') {
            "report"
        } else {
            "aggregate"
        };
        return PageRef {
            page_type,
            kind: Some(kind),
            entity_index: Some(seg0),
            entity_id: Some(rest.to_string()),
        };
    }

    // Дашборды d4XX
    let seg0 = first_segment(key);
    if seg0.starts_with('d') && looks_like_index(seg0) {
        return PageRef {
            page_type: "dashboard",
            kind: Some(key.to_string()),
            entity_index: Some(seg0.to_string()),
            entity_id: None,
        };
    }

    // Списки агрегатов/проекций (a0XX / p9XX без _details)
    if (seg0.starts_with('a') || seg0.starts_with('p')) && looks_like_index(seg0) {
        return PageRef {
            page_type: "aggregate_list",
            kind: Some(key.to_string()),
            entity_index: Some(seg0.to_string()),
            entity_id: None,
        };
    }

    PageRef {
        page_type: "other",
        kind: None,
        entity_index: None,
        entity_id: None,
    }
}

/// Прочитать одну строку объекта (generic SELECT) как JSON по имени таблицы и id.
async fn fetch_object_row(table: &str, id: &str) -> Option<Value> {
    // Имя таблицы из метаданных (доверенное), но проверим на всякий случай.
    if table.is_empty() || !table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let db = crate::shared::data::db::get_connection();
    let stmt = Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        format!("SELECT * FROM {} WHERE id = ? LIMIT 1", table),
        [sea_orm::Value::from(id.to_string())],
    );
    Value::find_by_statement(stmt)
        .all(db)
        .await
        .ok()
        .and_then(|mut rows| rows.drain(..).next())
}

/// Рекурсивно раскрывает строковые значения, которые сами являются JSON
/// (напр. колонка `line_json`), чтобы LLM видел полную структуру без экранирования.
fn expand_json_strings(v: &Value) -> Value {
    match v {
        Value::String(s) => {
            let t = s.trim();
            let looks_json = (t.starts_with('{') && t.ends_with('}'))
                || (t.starts_with('[') && t.ends_with(']'));
            if looks_json {
                if let Ok(parsed) = serde_json::from_str::<Value>(t) {
                    return expand_json_strings(&parsed);
                }
            }
            v.clone()
        }
        Value::Array(a) => Value::Array(a.iter().map(expand_json_strings).collect()),
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, vv)| (k.clone(), expand_json_strings(vv)))
                .collect(),
        ),
        _ => v.clone(),
    }
}

/// Обрезать значение поля до разумной длины (без разрыва UTF-8).
fn short_value(v: &Value) -> Option<String> {
    let s = match v {
        Value::Null => return None,
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let truncated: String = s.chars().take(200).collect();
    Some(truncated)
}

/// Назначение раздела приложения из каталога областей доступа.
///
/// Каталог ключуется теми же идентификаторами, что и вкладки, поэтому для
/// `a015_wb_orders_details_<id>` смотрим базовый ключ `a015_wb_orders`. Даёт ответ на
/// вопрос «что это за экран и зачем он» — метаданные сущности его не содержат.
fn scope_purpose(page_key: &str) -> Option<(&'static str, &'static str)> {
    let base = page_key
        .find("_details_")
        .map(|idx| &page_key[..idx])
        .unwrap_or(page_key);
    crate::system::access::scope_catalog::SCOPE_CATALOG
        .iter()
        .find(|s| s.scope_id == base)
        .map(|s| (s.label, s.description))
}

/// Собрать пакет контекста по ключу страницы.
pub async fn build_for_page_key(page_key: &str, label: Option<&str>) -> BuiltContext {
    build_for_page_key_with_session(page_key, label, None).await
}

/// То же самое плюс снимок последних страниц пользователя.
///
/// Снимок делается ОДИН раз, в момент прикрепления страницы, и дальше живёт в пакете
/// неизменным: он отвечает на вопрос «что человек делал перед обращением», а заодно не
/// ломает кэш префикса у провайдера — в отличие от списка, который пересчитывался бы
/// на каждое сообщение.
pub async fn build_for_page_key_with_session(
    page_key: &str,
    label: Option<&str>,
    session_user_id: Option<&str>,
) -> BuiltContext {
    let pr = parse_page_key(page_key);
    let deep_link = format!("?active={}", page_key);
    let label_title = label
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty());

    let mut ctx = json!({
        "page_key": page_key,
        "deep_link": deep_link,
        "page_type": pr.page_type,
    });
    if let Some(ix) = &pr.entity_index {
        ctx["entity_index"] = json!(ix);
    }
    if let Some(id) = &pr.entity_id {
        ctx["entity_id"] = json!(id);
    }

    // Назначение раздела — для консультаций «что это за экран и зачем».
    let purpose = scope_purpose(page_key);
    if let Some((label, description)) = purpose {
        ctx["section"] = json!({ "label": label, "description": description });
    }

    // Снимок навигации: путь пользователя до обращения.
    let mut recent_pages: Vec<(String, String)> = Vec::new();
    if let Some(user_id) = session_user_id {
        match crate::system::history::repository::list_recent(user_id, 10).await {
            Ok(items) => {
                recent_pages = items
                    .into_iter()
                    .map(|p| (p.tab_key, p.title))
                    .filter(|(key, _)| key != page_key)
                    .collect();
                if !recent_pages.is_empty() {
                    ctx["recent_pages"] = json!(recent_pages
                        .iter()
                        .map(|(key, title)| json!({ "page_key": key, "title": title }))
                        .collect::<Vec<_>>());
                }
            }
            Err(e) => tracing::warn!("context: не удалось прочитать историю страниц: {e}"),
        }
    }

    // Метрики проекта. Срез собирается общим кодом с инструментом
    // `get_project_metrics`, чтобы числа в контексте и в ответе инструмента не
    // разошлись; текст для диалога — только то, что перешло порог.
    let mut metrics_text: Option<String> = None;
    if pr.page_type == "system_metrics" {
        match crate::system::metrics::latest().await {
            Ok(data) => {
                ctx["metrics"] = crate::system::metrics::llm_view::summary(&data);
                metrics_text = Some(crate::system::metrics::llm_view::rendered_text(&data));
            }
            Err(e) => {
                tracing::warn!("context: не удалось прочитать метрики проекта: {e}");
                ctx["metrics"] = json!({ "error": e.to_string() });
            }
        }
    }

    // Описание сущности и список FK-полей из метаданных.
    let mut entity_name: Option<String> = None;
    let mut entity_desc: Option<String> = None;
    let mut table_name: Option<String> = None;
    let mut fk_fields: Vec<(String, String)> = Vec::new(); // (column, fk_target)
    if let Some(ix) = &pr.entity_index {
        let schema = METADATA_REGISTRY.get_entity_schema(ix);
        if schema.get("error").is_none() {
            entity_name = schema
                .get("name")
                .and_then(|v| v.as_str())
                .map(String::from);
            entity_desc = schema
                .get("description")
                .and_then(|v| v.as_str())
                .map(String::from);
            table_name = schema
                .get("table")
                .and_then(|v| v.as_str())
                .map(String::from);
            if let Some(fields) = schema.get("fields").and_then(|v| v.as_array()) {
                for f in fields {
                    if let (Some(col), Some(fk)) = (
                        f.get("column").and_then(|v| v.as_str()),
                        f.get("fk_to").and_then(|v| v.as_str()),
                    ) {
                        fk_fields.push((col.to_string(), fk.to_string()));
                    }
                }
            }
            ctx["entity"] = json!({
                "name": entity_name,
                "description": entity_desc,
                "table": table_name,
            });
        }
    }

    // Идентичность через representation (для детальных страниц).
    let mut identity_label: Option<String> = None;
    if let (Some(kind), Some(id)) = (&pr.kind, &pr.entity_id) {
        if let Some(rep) = representation::resolve(kind, id).await {
            let lbl = representation::to_label(&rep);
            identity_label = Some(lbl.clone());
            ctx["identity"] = json!({
                "title": rep.title,
                "date": rep.date,
                "doc_id": rep.doc_id,
                "label": lbl,
            });
        }
    }

    // Данные объекта (generic SELECT) для детальных страниц.
    let mut object_row: Option<Value> = None;
    if let (Some(table), Some(id)) = (&table_name, &pr.entity_id) {
        object_row = fetch_object_row(table, id).await;
        if let Some(row) = &object_row {
            ctx["object"] = row.clone();
        }
    }

    // Данные плагина: код/название + подсказка на связанный DataView (если есть).
    let mut plugin_title: Option<String> = None;
    let mut plugin_hint: Option<(String, String)> = None; // (code, hint)
    if pr.page_type == "plugin" {
        if let Some(id) = &pr.entity_id {
            if let Ok(Some(plugin)) = crate::plugins::repository::find_by_id(
                crate::shared::data::db::get_connection(),
                id,
            )
            .await
            {
                let code = plugin.bundle.manifest.code.clone();
                let title = plugin.bundle.manifest.title.clone();
                plugin_title = Some(title.clone());
                let hint = PLUGIN_DATAVIEW_HINTS
                    .iter()
                    .find(|(c, _)| *c == code)
                    .map(|(_, h)| h.to_string());
                ctx["plugin"] = json!({
                    "code": code,
                    "title": title,
                    "data_view_hint": hint,
                });
                if let Some(hint) = hint {
                    plugin_hint = Some((code, hint));
                }
            }
        }
    }

    // Смежные подписи по FK.
    let mut adjacent: Vec<Value> = Vec::new();
    if let Some(row) = &object_row {
        for (col, fk_target) in fk_fields.iter().take(8) {
            let Some(val) = row.get(col).and_then(short_value) else {
                continue;
            };
            // Представление ссылки: kind = имя таблицы FK-цели из метаданных.
            let fk_index = first_segment(fk_target).to_string();
            let ref_kind = {
                let s = METADATA_REGISTRY.get_entity_schema(&fk_index);
                s.get("table").and_then(|v| v.as_str()).map(String::from)
            };
            let label = match ref_kind {
                Some(rk) => representation::resolve(&rk, &val)
                    .await
                    .map(|r| representation::to_label(&r)),
                None => None,
            };
            adjacent.push(json!({
                "field": col,
                "ref": fk_target,
                "value": val,
                "label": label,
            }));
        }
        if !adjacent.is_empty() {
            ctx["adjacent"] = json!(adjacent);
        }
    }

    let title = identity_label
        .clone()
        .or_else(|| label_title.clone())
        .or_else(|| entity_name.clone())
        .or_else(|| plugin_title.clone())
        .unwrap_or_else(|| page_key.to_string());

    // ── Компактный текст для инъекции в диалог ──────────────────────────────
    let mut text = String::new();
    text.push_str(&format!(
        "Страница: {}\n",
        label_title.as_deref().unwrap_or(&title)
    ));
    text.push_str(&format!("Тип страницы: {}\n", pr.page_type));
    text.push_str(&format!("Ссылка: {}\n", deep_link));
    if let Some((section_label, section_desc)) = purpose {
        text.push_str(&format!(
            "Раздел приложения: {} — {}\n",
            section_label, section_desc
        ));
    }
    if let Some(name) = &entity_name {
        let ix = pr.entity_index.as_deref().unwrap_or("");
        text.push_str(&format!("Сущность: {} [{}]\n", name, ix));
    }
    if let Some(desc) = &entity_desc {
        text.push_str(&format!("Описание: {}\n", desc));
    }
    if let Some(lbl) = &identity_label {
        text.push_str(&format!("Объект: {}\n", lbl));
    }
    if let Some(metrics) = &metrics_text {
        text.push('\n');
        text.push_str(metrics);
    }
    if pr.page_type == "plugin" {
        if let Some(t) = &plugin_title {
            text.push_str(&format!("Плагин: {}\n", t));
        }
        match &plugin_hint {
            Some((code, hint)) => {
                text.push_str(&format!("Код плагина: {}\n", code));
                text.push_str(&format!("{}\n", hint));
            }
            None => {
                text.push_str(
                    "Для этого плагина пока нет связанного источника данных — уточни у \
                     пользователя, какие цифры нужны, или предложи завести под него DataView/схему.\n",
                );
            }
        }
    }
    if let Some(Value::Object(map)) = &object_row {
        let mut filtered = serde_json::Map::new();
        for (k, v) in map {
            if SKIP_COLUMNS.contains(&k.as_str()) || v.is_null() {
                continue;
            }
            filtered.insert(k.clone(), expand_json_strings(v));
        }
        if !filtered.is_empty() {
            let pretty = serde_json::to_string_pretty(&Value::Object(filtered)).unwrap_or_default();
            text.push_str("Данные объекта (JSON):\n```json\n");
            text.push_str(&pretty);
            text.push_str("\n```\n");
        }
    }
    if !recent_pages.is_empty() {
        text.push_str("Страницы, открытые пользователем перед обращением (свежие сверху):\n");
        for (key, title) in recent_pages.iter().take(10) {
            text.push_str(&format!("  {} [{}]\n", title, key));
        }
    }
    if !adjacent.is_empty() {
        text.push_str("Связанные объекты:\n");
        for a in &adjacent {
            let field = a.get("field").and_then(|v| v.as_str()).unwrap_or("");
            let label = a
                .get("label")
                .and_then(|v| v.as_str())
                .or_else(|| a.get("value").and_then(|v| v.as_str()))
                .unwrap_or("");
            text.push_str(&format!("  {}: {}\n", field, label));
        }
    }

    // Мягкий потолок размера, чтобы не раздувать контекст на гигантских объектах.
    const MAX_TEXT_CHARS: usize = 24_000;
    if text.chars().count() > MAX_TEXT_CHARS {
        let truncated: String = text.chars().take(MAX_TEXT_CHARS).collect();
        text = format!("{}\n…[контекст обрезан]", truncated);
    }

    BuiltContext {
        title,
        page_type: pr.page_type.to_string(),
        entity_index: pr.entity_index,
        entity_id: pr.entity_id,
        context_json: ctx,
        rendered_text: text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_page_gets_its_own_type() {
        // Пока `sys_metrics` падал в `other`, в чат уезжал один ключ вкладки:
        // консультировать по состоянию приложения было не по чему.
        let pr = parse_page_key(METRICS_PAGE_KEY);
        assert_eq!(pr.page_type, "system_metrics");
        assert!(pr.entity_index.is_none());
    }

    #[test]
    fn regular_pages_keep_their_types() {
        assert_eq!(parse_page_key("a015_wb_orders").page_type, "aggregate_list");
        assert_eq!(
            parse_page_key("a015_wb_orders_details_42").page_type,
            "aggregate"
        );
        // Списки проекций и агрегатов делят один тип; `report` — только деталь p9XX.
        assert_eq!(
            parse_page_key("p903_wb_finance_report").page_type,
            "aggregate_list"
        );
        assert_eq!(
            parse_page_key("p903_wb_finance_report_details_7").page_type,
            "report"
        );
        assert_eq!(parse_page_key("d407_llm_quality").page_type, "dashboard");
        assert_eq!(parse_page_key("plugin__abc").page_type, "plugin");
        assert_eq!(parse_page_key("sys_settings").page_type, "other");
    }
}
