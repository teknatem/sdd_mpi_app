//! Снимки инвентаризации в БД.
//!
//! Две таблицы, а не одна с JSON: главный сценарий страницы — фильтр по девяти
//! осям, и в колонках это обычный `WHERE`, тогда как в JSON — разбор всего
//! снимка на каждый чих. Сводка при этом лежит именно JSON-ом: списки
//! недостижимых поверхностей и мёртвых инструментов в ряд не ложатся и по
//! колонкам не раскладываются.

use anyhow::Result;
use contracts::knowledge::{
    InventorySnapshotDto, InventorySummaryDto, KnowledgeUnitDto, CLASSIFIER_VERSION,
};
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, FromQueryResult, Statement};

/// Сколько снимков держим.
///
/// Полсотни хватает, чтобы увидеть движение за пару месяцев ежедневных
/// запусков; строк при этом порядка двадцати тысяч — для SQLite незаметно.
pub const KEEP_SNAPSHOTS: i64 = 50;

#[derive(Debug, FromQueryResult)]
struct SnapshotRow {
    id: String,
    captured_at: String,
    trigger: String,
    classifier_version: i32,
    app_version: String,
    unit_count: i32,
    surface_count: i32,
    stored_tokens: i32,
    collect_ms: i64,
    summary_json: String,
    diagnostics_json: String,
}

impl SnapshotRow {
    fn to_dto(&self) -> InventorySnapshotDto {
        InventorySnapshotDto {
            id: self.id.clone(),
            captured_at: self.captured_at.clone(),
            trigger: self.trigger.clone(),
            classifier_version: self.classifier_version as u16,
            app_version: self.app_version.clone(),
            unit_count: self.unit_count.max(0) as usize,
            surface_count: self.surface_count.max(0) as usize,
            stored_tokens: self.stored_tokens.max(0) as u32,
            collect_ms: self.collect_ms,
            diagnostics: serde_json::from_str(&self.diagnostics_json).unwrap_or_default(),
        }
    }

    fn summary(&self) -> InventorySummaryDto {
        serde_json::from_str(&self.summary_json).unwrap_or_default()
    }
}

const SNAPSHOT_COLUMNS: &str = "id, captured_at, trigger, classifier_version, app_version, \
     unit_count, surface_count, stored_tokens, collect_ms, summary_json, diagnostics_json";

/// Записать снимок вместе со всеми единицами.
///
/// Единицы вставляются пачками: четыреста отдельных `INSERT` внутри одной
/// транзакции SQLite стоят заметно дороже десятка многострочных.
pub async fn insert_snapshot(
    db: &DatabaseConnection,
    snapshot: &InventorySnapshotDto,
    summary: &InventorySummaryDto,
    units: &[KnowledgeUnitDto],
) -> Result<()> {
    let summary_json = serde_json::to_string(summary)?;
    let diagnostics_json = serde_json::to_string(&snapshot.diagnostics)?;

    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        &format!(
            "INSERT INTO sys_knowledge_snapshot ({SNAPSHOT_COLUMNS}) \
                  VALUES (?,?,?,?,?,?,?,?,?,?,?)"
        ),
        [
            snapshot.id.clone().into(),
            snapshot.captured_at.clone().into(),
            snapshot.trigger.clone().into(),
            (snapshot.classifier_version as i32).into(),
            snapshot.app_version.clone().into(),
            (snapshot.unit_count as i32).into(),
            (snapshot.surface_count as i32).into(),
            (snapshot.stored_tokens as i32).into(),
            snapshot.collect_ms.into(),
            summary_json.into(),
            diagnostics_json.into(),
        ],
    ))
    .await?;

    const CHUNK: usize = 50;
    for batch in units.chunks(CHUNK) {
        let placeholders = std::iter::repeat("(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .take(batch.len())
            .collect::<Vec<_>>()
            .join(",");
        let mut values: Vec<sea_orm::Value> = Vec::with_capacity(batch.len() * 24);
        for unit in batch {
            values.push(snapshot.id.clone().into());
            values.push(unit.unit_id.clone().into());
            values.push(unit.surface_id.clone().into());
            values.push(unit.family.as_str().into());
            values.push(unit.origin.as_str().into());
            values.push(unit.storage_form.as_str().into());
            values.push(unit.editor.as_str().into());
            values.push(unit.reachability.as_str().into());
            values.push(unit.lifecycle.as_str().into());
            values.push(unit.scope.as_str().into());
            values.push(unit.channel.as_str().into());
            values.push(unit.code_role.map(|role| role.as_str()).into());
            values.push(unit.title.clone().into());
            values.push(unit.subtitle.clone().into());
            values.push(unit.source_ref.clone().into());
            values.push(unit.bytes.map(|v| v as i64).into());
            values.push(unit.tokens.map(|v| v as i64).into());
            values.push(unit.search_hits.into());
            values.push(unit.read_hits.into());
            values.push(unit.cited_hits.into());
            values.push(unit.updated.clone().into());
            values.push(unit.staleness_pct.map(|v| v as i64).into());
            values.push(serde_json::to_string(&unit.tags)?.into());
            values.push(serde_json::to_string(&unit.issues)?.into());
        }
        db.execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            &format!(
                "INSERT OR REPLACE INTO sys_knowledge_unit (snapshot_id, unit_id, surface_id, \
                 family, origin, storage_form, editor, reachability, lifecycle, scope, channel, \
                 code_role, title, subtitle, source_ref, bytes, tokens, search_hits, read_hits, \
                 cited_hits, updated, staleness_pct, tags_json, issues_json) VALUES {placeholders}"
            ),
            values,
        ))
        .await?;
    }
    Ok(())
}

/// Последний снимок вместе со сводкой.
pub async fn latest(
    db: &DatabaseConnection,
) -> Result<Option<(InventorySnapshotDto, InventorySummaryDto)>> {
    let row = SnapshotRow::find_by_statement(Statement::from_string(
        DatabaseBackend::Sqlite,
        format!(
            "SELECT {SNAPSHOT_COLUMNS} FROM sys_knowledge_snapshot \
             ORDER BY captured_at DESC LIMIT 1"
        ),
    ))
    .one(db)
    .await?;
    Ok(row.map(|row| (row.to_dto(), row.summary())))
}

/// Снимок, предшествующий указанному, — для дельты.
///
/// Сравнимым он считается только при совпадающей версии классификатора: иначе
/// дельта посчиталась бы по разрезу, которого в прошлой версии не было.
pub async fn previous(
    db: &DatabaseConnection,
    before: &str,
    classifier_version: u16,
) -> Result<Option<InventorySnapshotDto>> {
    let row = SnapshotRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        &format!(
            "SELECT {SNAPSHOT_COLUMNS} FROM sys_knowledge_snapshot \
             WHERE captured_at < ? AND classifier_version = ? \
             ORDER BY captured_at DESC LIMIT 1"
        ),
        [before.into(), (classifier_version as i32).into()],
    ))
    .one(db)
    .await?;
    Ok(row.map(|row| row.to_dto()))
}

#[derive(Debug, FromQueryResult)]
struct UnitRow {
    unit_id: String,
    surface_id: String,
    family: String,
    origin: String,
    storage_form: String,
    editor: String,
    reachability: String,
    lifecycle: String,
    scope: String,
    channel: String,
    code_role: Option<String>,
    title: String,
    subtitle: String,
    source_ref: Option<String>,
    bytes: Option<i64>,
    tokens: Option<i64>,
    search_hits: i64,
    read_hits: i64,
    cited_hits: i64,
    updated: Option<String>,
    staleness_pct: Option<i64>,
    tags_json: String,
    issues_json: String,
}

/// Единицы снимка.
///
/// Код, которого нет в текущем классификаторе, — не повод потерять строку: она
/// приехала из снимка другой версии. Такая единица получает нейтральное
/// значение оси, а факт расхождения виден по `classifier_version` снимка.
pub async fn units_of(db: &DatabaseConnection, snapshot_id: &str) -> Result<Vec<KnowledgeUnitDto>> {
    use contracts::knowledge::{
        CodeRole, Editor, ExposureChannel, Lifecycle, Origin, Reachability, Scope, StorageForm,
        UnitFamily,
    };

    let rows = UnitRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT unit_id, surface_id, family, origin, storage_form, editor, reachability, \
         lifecycle, scope, channel, code_role, title, subtitle, source_ref, bytes, tokens, \
         search_hits, read_hits, cited_hits, updated, staleness_pct, tags_json, issues_json \
         FROM sys_knowledge_unit WHERE snapshot_id = ? ORDER BY unit_id",
        [snapshot_id.into()],
    ))
    .all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| KnowledgeUnitDto {
            unit_id: row.unit_id,
            surface_id: row.surface_id,
            family: UnitFamily::from_code(&row.family).unwrap_or(UnitFamily::Computed),
            origin: Origin::from_code(&row.origin).unwrap_or(Origin::CodeRegistry),
            storage_form: StorageForm::from_code(&row.storage_form)
                .unwrap_or(StorageForm::RustConst),
            editor: Editor::from_code(&row.editor).unwrap_or(Editor::Developer),
            reachability: Reachability::from_code(&row.reachability)
                .unwrap_or(Reachability::Unreachable),
            lifecycle: Lifecycle::from_code(&row.lifecycle).unwrap_or(Lifecycle::Active),
            scope: Scope::from_code(&row.scope).unwrap_or(Scope::Application),
            channel: ExposureChannel::from_code(&row.channel)
                .unwrap_or(ExposureChannel::InternalRuntime),
            code_role: row.code_role.as_deref().and_then(CodeRole::from_code),
            title: row.title,
            subtitle: row.subtitle,
            source_ref: row.source_ref,
            bytes: row.bytes.map(|v| v as u32),
            tokens: row.tokens.map(|v| v as u32),
            search_hits: row.search_hits,
            read_hits: row.read_hits,
            cited_hits: row.cited_hits,
            updated: row.updated,
            staleness_pct: row.staleness_pct.map(|v| v as u32),
            tags: serde_json::from_str(&row.tags_json).unwrap_or_default(),
            issues: serde_json::from_str(&row.issues_json).unwrap_or_default(),
        })
        .collect())
}

/// Список снимков — для истории и графика.
pub async fn list(db: &DatabaseConnection, limit: u64) -> Result<Vec<InventorySnapshotDto>> {
    let rows = SnapshotRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        &format!(
            "SELECT {SNAPSHOT_COLUMNS} FROM sys_knowledge_snapshot \
             ORDER BY captured_at DESC LIMIT ?"
        ),
        [(limit as i64).into()],
    ))
    .all(db)
    .await?;
    Ok(rows.iter().map(SnapshotRow::to_dto).collect())
}

/// Оставить последние `keep` снимков.
///
/// Единицы удаляются первыми: осиротевшие строки без снимка не отфильтровать
/// ничем, они просто занимали бы место и путали выборки.
pub async fn prune(db: &DatabaseConnection, keep: i64) -> Result<()> {
    let stale = "SELECT id FROM sys_knowledge_snapshot \
                 ORDER BY captured_at DESC LIMIT -1 OFFSET ?";
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        &format!("DELETE FROM sys_knowledge_unit WHERE snapshot_id IN ({stale})"),
        [keep.into()],
    ))
    .await?;
    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        &format!("DELETE FROM sys_knowledge_snapshot WHERE id IN ({stale})"),
        [keep.into()],
    ))
    .await?;
    Ok(())
}

/// Версия классификатора этой сборки — пишется в каждый снимок.
pub const fn classifier_version() -> u16 {
    CLASSIFIER_VERSION
}
