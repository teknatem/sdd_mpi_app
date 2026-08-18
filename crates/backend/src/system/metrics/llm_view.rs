//! Срез метрик для LLM: тот же снимок, но в форме, пригодной для рассуждения.
//!
//! Общий для двух потребителей — контекста страницы (`a018_llm_chat::context`)
//! и инструмента `get_project_metrics`. Держать две сборки одного снимка нельзя:
//! они разъедутся, и модель начнёт получать разные числа в зависимости от того,
//! пришла страница контекстом или была запрошена инструментом.
//!
//! Что здесь важнее формата: **пороги и направление едут вместе со значением**.
//! Без `direction` модель читает любой рост как ухудшение, а без `warn`/`bad`
//! не отличает справочную метрику от нарушенной — и то и другое приводит к
//! уверенным неправильным выводам.

use contracts::system::metrics::{MetricStatus, MetricValueDto, ProjectMetricsDto};
use serde_json::{json, Value};

/// Сколько строк детализации кладём в срез.
///
/// Полные таблицы (топ файлов, топ таблиц) занимают больше, чем весь остальной
/// снимок, и почти никогда не нужны целиком: пять строк показывают порядок
/// величин, а за остальным модель сходит инструментом.
const DETAIL_ROWS: usize = 5;

fn status_word(status: MetricStatus) -> &'static str {
    match status {
        MetricStatus::Ok => "норма",
        MetricStatus::Warn => "внимание",
        MetricStatus::Bad => "за порогом",
        MetricStatus::Neutral => "справочная",
    }
}

fn metric_json(metric: &MetricValueDto) -> Value {
    let mut item = json!({
        "key": metric.key,
        "label": metric.label,
        "group": metric.group,
        "value": metric.value,
        "unit": metric.unit,
        "direction": metric.direction,
        "status": status_word(metric.status),
    });
    if let Some(warn) = metric.warn {
        item["warn"] = json!(warn);
    }
    if let Some(bad) = metric.bad {
        item["bad"] = json!(bad);
    }
    if let Some(previous) = metric.previous {
        item["previous"] = json!(previous);
    }
    if let Some(delta) = metric.delta {
        item["delta"] = json!(delta);
    }
    if let Some(hint) = &metric.hint {
        item["hint"] = json!(hint);
    }
    item
}

/// Метрики, перешедшие порог или подошедшие к нему, — тяжёлые первыми.
pub fn attention(values: &[MetricValueDto]) -> Vec<MetricValueDto> {
    let mut items: Vec<MetricValueDto> = values
        .iter()
        .filter(|m| matches!(m.status, MetricStatus::Bad | MetricStatus::Warn))
        .cloned()
        .collect();
    items.sort_by_key(|m| match m.status {
        MetricStatus::Bad => 0,
        MetricStatus::Warn => 1,
        _ => 2,
    });
    items
}

/// Полный срез снимка в JSON.
pub fn summary(data: &ProjectMetricsDto) -> Value {
    let Some(snapshot) = &data.snapshot else {
        return json!({
            "state": "снимок ещё не собран",
            "hint": "Сбор идёт примерно через полминуты после старта бэкенда.",
        });
    };

    let attention_items = attention(&data.values);

    json!({
        "passport": {
            "app_version": snapshot.app_version,
            "git_commit": snapshot.git_commit,
            "build_profile": snapshot.build_profile,
            "schema_version": snapshot.schema_version,
            "code_generated_at": snapshot.code_generated_at,
            "captured_at": snapshot.captured_at,
            "previous_captured_at": data.previous_captured_at,
            "collect_ms": snapshot.collect_ms,
        },
        "groups": data.groups.iter()
            .map(|g| json!({ "code": g.code, "label": g.label }))
            .collect::<Vec<_>>(),
        "attention": attention_items.iter().map(metric_json).collect::<Vec<_>>(),
        "values": data.values.iter().map(metric_json).collect::<Vec<_>>(),
        "details": data.details.iter().map(|table| json!({
            "code": table.code,
            "label": table.label,
            "value_label": table.value_label,
            "rows_total": table.rows.len(),
            "rows": table.rows.iter().take(DETAIL_ROWS).map(|row| json!({
                "name": row.name,
                "value": row.value,
                "extra": row.extra,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

/// Компактный текст для инъекции в диалог.
///
/// В отличие от JSON — только то, что требует внимания, плюс строка паспорта.
/// Полная сводка остаётся в пакете контекста: она нужна на уточняющие вопросы,
/// но каждый раз тратить на неё бюджет диалога незачем.
pub fn rendered_text(data: &ProjectMetricsDto) -> String {
    let Some(snapshot) = &data.snapshot else {
        return "Метрики проекта: снимок ещё не собран.".to_string();
    };

    let mut out = format!(
        "Метрики проекта. Версия {}, коммит {}, профиль {}, схема БД {}. \
         Снимок {}, предыдущий {}.\n",
        snapshot.app_version,
        snapshot.git_commit.as_deref().unwrap_or("—"),
        snapshot.build_profile,
        snapshot.schema_version,
        snapshot.captured_at,
        data.previous_captured_at.as_deref().unwrap_or("нет"),
    );

    let attention_items = attention(&data.values);
    if attention_items.is_empty() {
        out.push_str(&format!(
            "Ни одна из {} метрик не перешла порог.\n",
            data.values.len()
        ));
        return out;
    }

    out.push_str(&format!(
        "\nЗа порогом или близко ({} из {}):\n\n| Метрика | Значение | Порог | Дельта | Оценка |\n\
         |---|---|---|---|---|\n",
        attention_items.len(),
        data.values.len(),
    ));
    for metric in &attention_items {
        let threshold = match (metric.warn, metric.bad) {
            (Some(warn), Some(bad)) => format!("внимание {warn}, порог {bad}"),
            _ => "—".to_string(),
        };
        let delta = metric
            .delta
            .map(|d| format!("{}{d}", if d > 0.0 { "+" } else { "" }))
            .unwrap_or_else(|| "—".to_string());
        out.push_str(&format!(
            "| {} ({}) | {} {} | {} | {} | {} |\n",
            metric.label,
            metric.key,
            metric.value,
            metric.unit,
            threshold,
            delta,
            status_word(metric.status),
        ));
    }
    out.push_str("\nПолная сводка по всем метрикам — в JSON этого пакета контекста.\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::system::metrics::{MetricDirection, MetricGroupDto, MetricSnapshotDto};

    fn metric(key: &str, status: MetricStatus) -> MetricValueDto {
        MetricValueDto {
            key: key.to_string(),
            label: key.to_string(),
            group: "code".to_string(),
            unit: "шт".to_string(),
            value: 10.0,
            precision: 0,
            direction: MetricDirection::Lower,
            status,
            warn: Some(5.0),
            bad: Some(9.0),
            previous: Some(8.0),
            delta: Some(2.0),
            hint: None,
        }
    }

    fn data(values: Vec<MetricValueDto>) -> ProjectMetricsDto {
        ProjectMetricsDto {
            snapshot: Some(MetricSnapshotDto {
                id: "s1".into(),
                captured_at: "2026-08-17T08:00:00+00:00".into(),
                trigger: "startup".into(),
                app_version: "0.1.0".into(),
                git_commit: Some("abc1234".into()),
                build_profile: "debug".into(),
                schema_version: 217,
                code_generated_at: None,
                collect_ms: 42,
            }),
            previous_captured_at: Some("2026-08-16T08:00:00+00:00".into()),
            groups: vec![MetricGroupDto {
                code: "code".into(),
                label: "Размер кода".into(),
                order: 0,
            }],
            values,
            details: Vec::new(),
        }
    }

    #[test]
    fn attention_puts_thresholds_first() {
        let values = vec![
            metric("ok", MetricStatus::Ok),
            metric("warn", MetricStatus::Warn),
            metric("bad", MetricStatus::Bad),
        ];
        let items = attention(&values);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].key, "bad");
        assert_eq!(items[1].key, "warn");
    }

    #[test]
    fn summary_carries_thresholds_and_direction() {
        let summary = summary(&data(vec![metric("a", MetricStatus::Bad)]));
        let first = &summary["values"][0];
        assert_eq!(first["warn"], 5.0);
        assert_eq!(first["bad"], 9.0);
        assert_eq!(first["status"], "за порогом");
        assert!(first.get("direction").is_some());
    }

    #[test]
    fn summary_survives_a_missing_snapshot() {
        let empty = ProjectMetricsDto {
            snapshot: None,
            previous_captured_at: None,
            groups: Vec::new(),
            values: Vec::new(),
            details: Vec::new(),
        };
        assert!(summary(&empty).get("state").is_some());
        assert!(rendered_text(&empty).contains("не собран"));
    }

    #[test]
    fn rendered_text_reports_a_clean_snapshot() {
        let text = rendered_text(&data(vec![metric("a", MetricStatus::Ok)]));
        assert!(text.contains("Ни одна"));
    }

    #[test]
    fn rendered_text_tabulates_attention() {
        let text = rendered_text(&data(vec![metric("a", MetricStatus::Bad)]));
        assert!(text.contains("| Метрика |"));
        assert!(text.contains("за порогом"));
    }
}
