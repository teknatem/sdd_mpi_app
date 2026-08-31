//! Получение detail-строк для одной GL-проводки и сверка с её amount.

use anyhow::Result;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

use contracts::general_ledger::resource_detail::{
    GlResourceDetailIntegrity, GlResourceDetailResponse, GlResourceDetailTotals,
};

use super::detail_links::{
    descriptor_for_resource_table, GlDetailLinkDescriptor, GlDetailLinkKind,
};
use crate::shared::data::db::get_connection;

const MATCH_TOLERANCE: f64 = 0.01;
const MISMATCH_SAMPLE_LIMIT: usize = 5;

fn conn() -> &'static sea_orm::DatabaseConnection {
    get_connection()
}

pub async fn get_resource_details(gl_id: &str) -> Result<GlResourceDetailResponse> {
    let gl = super::repository::get_by_id(gl_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("GL entry {gl_id} not found"))?;

    let resource_table = gl.resource_table.clone();
    let resource_field = gl.resource_field.clone();
    let resource_sign = gl.resource_sign;
    let gl_amount = gl.amount;

    let descriptor = descriptor_for_resource_table(&resource_table);

    let (rows, error) = match descriptor {
        Some(descriptor) => match fetch_rows(descriptor, &gl).await {
            Ok(rows) => (rows, None),
            Err(err) => (Vec::new(), Some(err.to_string())),
        },
        None => (
            Vec::new(),
            Some(format!(
                "Resource table '{resource_table}' is not registered in detail_links"
            )),
        ),
    };

    let integrity = compute_integrity(&rows, &gl.id, descriptor);

    let sum_resource = rows
        .iter()
        .map(|row| {
            row.get(&resource_field)
                .and_then(JsonValue::as_f64)
                .unwrap_or(0.0)
        })
        .sum::<f64>();
    let sum_signed = sum_resource * f64::from(resource_sign);
    let delta = sum_signed - gl_amount;
    let row_count = rows.len();
    let sum_ok = row_count >= 1 && delta.abs() <= MATCH_TOLERANCE;
    let is_match = sum_ok && integrity.is_ok;

    Ok(GlResourceDetailResponse {
        gl_id: gl.id,
        resource_table,
        resource_field,
        resource_sign,
        supported: descriptor.is_some(),
        rows,
        totals: GlResourceDetailTotals {
            row_count,
            sum_resource,
            sum_signed,
            gl_amount,
            delta,
            is_match,
        },
        integrity,
        error,
    })
}

/// Классифицирует каждую detail-строку по полю `general_ledger_ref`.
///
/// `ExternalLinked` (p903) не имеет колонки `general_ledger_ref` — связь
/// идёт через `gl.registrator_ref → detail.id`. Для таких таблиц
/// integrity-проверка проходит автоматически (если строки нашлись).
fn compute_integrity(
    rows: &[JsonValue],
    gl_id: &str,
    descriptor: Option<&GlDetailLinkDescriptor>,
) -> GlResourceDetailIntegrity {
    if rows.is_empty() {
        return GlResourceDetailIntegrity::default();
    }

    let is_projection_linked = matches!(
        descriptor.map(|d| d.kind),
        Some(GlDetailLinkKind::ProjectionLinked)
    );
    if !is_projection_linked {
        return GlResourceDetailIntegrity {
            matched_count: rows.len(),
            missing_count: 0,
            mismatched_count: 0,
            mismatched_refs_sample: Vec::new(),
            is_ok: true,
        };
    }

    let mut matched = 0usize;
    let mut missing = 0usize;
    let mut mismatched = 0usize;
    let mut sample: BTreeMap<String, ()> = BTreeMap::new();

    for row in rows {
        match row.get("general_ledger_ref") {
            None | Some(JsonValue::Null) => missing += 1,
            Some(JsonValue::String(s)) if s.is_empty() => missing += 1,
            Some(JsonValue::String(s)) if s == gl_id => matched += 1,
            Some(JsonValue::String(s)) => {
                mismatched += 1;
                if sample.len() < MISMATCH_SAMPLE_LIMIT {
                    sample.insert(s.clone(), ());
                }
            }
            Some(other) => {
                mismatched += 1;
                if sample.len() < MISMATCH_SAMPLE_LIMIT {
                    sample.insert(other.to_string(), ());
                }
            }
        }
    }

    GlResourceDetailIntegrity {
        matched_count: matched,
        missing_count: missing,
        mismatched_count: mismatched,
        mismatched_refs_sample: sample.into_keys().collect(),
        is_ok: matched == rows.len(),
    }
}

async fn fetch_rows(
    descriptor: &GlDetailLinkDescriptor,
    gl: &super::repository::Model,
) -> Result<Vec<JsonValue>> {
    let source = detail_source(descriptor.detail_table).ok_or_else(|| {
        anyhow::anyhow!("table '{}' has no detail loader", descriptor.detail_table)
    })?;
    source.fetch(gl).await
}

/// Источник detail-строк для одной таблицы ресурса.
///
/// **Зачем трейт.** Загрузчики жили здесь шестью функциями, и пять из них были
/// побайтово одинаковы — отличался только тип сущности. Из-за них Главная книга
/// знала имена шести маркетплейсных проекций, хотя знать ей нужно ровно одно:
/// по какой таблице ресурса искать строки. Теперь загрузчик принадлежит
/// проекции, а состав объявляет `composition::gl_detail_sources`.
#[async_trait::async_trait]
pub trait GlDetailSource: Send + Sync {
    /// Таблица ресурса — то, что стоит в `resource_table` проводки.
    fn detail_table(&self) -> &'static str;

    async fn fetch(&self, gl: &super::repository::Model) -> Result<Vec<JsonValue>>;
}

static DETAIL_SOURCES: std::sync::OnceLock<Vec<std::sync::Arc<dyn GlDetailSource>>> =
    std::sync::OnceLock::new();

/// Установить источники detail-строк. Зовётся один раз из `composition::install_all()`.
///
/// # Panics
/// При повторной установке и при конфликте таблиц.
pub fn install_detail_sources(sources: Vec<std::sync::Arc<dyn GlDetailSource>>) {
    let mut tables = std::collections::HashSet::new();
    for source in &sources {
        if !tables.insert(source.detail_table()) {
            panic!(
                "таблица ресурса '{}' заявлена двумя источниками",
                source.detail_table()
            );
        }
    }
    if DETAIL_SOURCES.set(sources).is_err() {
        panic!("источники detail-строк уже установлены");
    }
}

fn detail_source(detail_table: &str) -> Option<&'static std::sync::Arc<dyn GlDetailSource>> {
    DETAIL_SOURCES
        .get()?
        .iter()
        .find(|source| source.detail_table() == detail_table)
}

/// Кандидаты для проекции, связанной с проводкой через `general_ledger_ref`.
///
/// Берёт строки, которые либо уже указывают на эту проводку, либо имеют тот же
/// `(registrator_type, registrator_ref, turnover_code)`, но с
/// `general_ledger_ref IS NULL` — это сломанные/недоназначенные строки, и
/// `compute_integrity` пометит их как нарушение целостности. Не отбросить их
/// здесь принципиально: именно ради их обнаружения экран и существует.
///
/// Колонки передаются параметрами, потому что у каждой проекции свой `Column`,
/// а тело отбора одно на всех — пять копий этого запроса и были тем, из-за
/// чего Главная книга перечисляла проекции поимённо.
pub async fn fetch_linked_rows<E>(
    gl: &super::repository::Model,
    general_ledger_ref: E::Column,
    registrator_type: E::Column,
    registrator_ref: E::Column,
    turnover_code: E::Column,
) -> Result<Vec<JsonValue>>
where
    E: EntityTrait,
{
    let mut rows = E::find()
        .filter(general_ledger_ref.eq(gl.id.clone()))
        .into_json()
        .all(conn())
        .await?;
    let orphans = E::find()
        .filter(registrator_type.eq(gl.registrator_type.clone()))
        .filter(registrator_ref.eq(gl.registrator_ref.clone()))
        .filter(turnover_code.eq(gl.turnover_code.clone()))
        .filter(general_ledger_ref.is_null())
        .into_json()
        .all(conn())
        .await?;
    rows.extend(orphans);
    Ok(rows)
}
