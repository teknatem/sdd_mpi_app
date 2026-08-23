//! Хранилище определений: Этапы в `sys_stage_definition`, Процессы в
//! `sys_process_definition`.
//!
//! Модуль отвечает ровно за одно: версия — это строка. Черновик правится по
//! месту, опубликованное не трогается никогда (ADR-0011 п.7), активная версия
//! на код одна — и последнее держится частичным уникальным индексом БД, а не
//! порядком операций в коде: активацию может нажать второй администратор, пока
//! идёт первая.
//!
//! Бизнес-правил здесь нет — они в `definitions.rs`. Здесь запросы.

use anyhow::Result;
use chrono::Utc;
use contracts::processes::{
    DefinitionRecord, DefinitionStatus, DefinitionVersion, ProcessDefinition, ProcessManifest,
    ProcessRecord, StageDefinition, StageManifest, StagePin, StageRecord,
};
use sea_orm::entity::prelude::*;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Строка каталога Этапов.
pub mod stage_row {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "sys_stage_definition")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub code: String,
        pub version: i32,
        pub status: String,
        pub title: String,
        pub manifest_json: String,
        pub script: String,
        pub digest: String,
        pub created_at: String,
        #[sea_orm(nullable)]
        pub created_by: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// Строка каталога Процессов.
pub mod process_row {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "sys_process_definition")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub code: String,
        pub version: i32,
        pub status: String,
        pub title: String,
        pub manifest_json: String,
        pub digest: String,
        pub pins_json: String,
        pub created_at: String,
        #[sea_orm(nullable)]
        pub created_by: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// Что кладём в строку при сохранении черновика.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftInput<T> {
    pub definition: T,
    pub digest: String,
    pub created_by: Option<String>,
}

fn parse_manifest<T: for<'de> Deserialize<'de>>(raw: &str, what: &str) -> Result<T> {
    serde_json::from_str(raw)
        .map_err(|error| anyhow::anyhow!("манифест {what} не читается: {error}"))
}

fn stage_record(row: stage_row::Model) -> Result<StageRecord> {
    let manifest: StageManifest = parse_manifest(&row.manifest_json, &row.code)?;
    Ok(DefinitionRecord {
        id: row.id,
        code: row.code,
        version: row.version,
        status: DefinitionStatus::from_str(&row.status),
        digest: row.digest.clone(),
        created_at: row.created_at,
        created_by: row.created_by,
        definition: StageDefinition {
            manifest,
            script: row.script,
            digest: row.digest,
        },
    })
}

fn process_record(row: process_row::Model) -> Result<ProcessRecord> {
    let manifest: ProcessManifest = parse_manifest(&row.manifest_json, &row.code)?;
    Ok(DefinitionRecord {
        id: row.id,
        code: row.code,
        version: row.version,
        status: DefinitionStatus::from_str(&row.status),
        digest: row.digest.clone(),
        created_at: row.created_at,
        created_by: row.created_by,
        definition: ProcessDefinition {
            manifest,
            digest: row.digest,
        },
    })
}

// ---------------------------------------------------------------------------
// Этапы
// ---------------------------------------------------------------------------

/// Версия, с которой начинается история кода.
const FIRST_VERSION: i32 = 1;

async fn next_stage_version(db: &DatabaseConnection, code: &str) -> Result<i32> {
    let last = stage_row::Entity::find()
        .filter(stage_row::Column::Code.eq(code))
        .order_by_desc(stage_row::Column::Version)
        .one(db)
        .await?;
    Ok(last.map(|row| row.version + 1).unwrap_or(FIRST_VERSION))
}

/// Сохранить черновик Этапа: правкой существующего либо новой версией поверх
/// опубликованных.
///
/// Черновик один на код (частичный уникальный индекс), поэтому повторное
/// сохранение не плодит версии: история остаётся историей публикаций, а не
/// нажатий «сохранить».
pub async fn save_stage_draft(
    db: &DatabaseConnection,
    input: DraftInput<StageDefinition>,
) -> Result<StageRecord> {
    let code = input.definition.manifest.code.clone();
    let manifest_json = serde_json::to_string(&input.definition.manifest)?;
    let title = input.definition.manifest.title.clone();

    if let Some(existing) = find_stage_draft_row(db, &code).await? {
        let mut active: stage_row::ActiveModel = existing.into();
        active.title = Set(title);
        active.manifest_json = Set(manifest_json);
        active.script = Set(input.definition.script.clone());
        active.digest = Set(input.digest.clone());
        active.created_at = Set(Utc::now().to_rfc3339());
        active.created_by = Set(input.created_by.clone());
        return stage_record(active.update(db).await?);
    }

    let row = stage_row::ActiveModel {
        id: Set(Uuid::new_v4().to_string()),
        code: Set(code.clone()),
        version: Set(next_stage_version(db, &code).await?),
        status: Set(DefinitionStatus::Draft.as_str().to_string()),
        title: Set(title),
        manifest_json: Set(manifest_json),
        script: Set(input.definition.script.clone()),
        digest: Set(input.digest),
        created_at: Set(Utc::now().to_rfc3339()),
        created_by: Set(input.created_by),
    };
    stage_record(row.insert(db).await?)
}

async fn find_stage_draft_row(
    db: &DatabaseConnection,
    code: &str,
) -> Result<Option<stage_row::Model>> {
    Ok(stage_row::Entity::find()
        .filter(stage_row::Column::Code.eq(code))
        .filter(stage_row::Column::Status.eq(DefinitionStatus::Draft.as_str()))
        .one(db)
        .await?)
}

pub async fn find_stage(
    db: &DatabaseConnection,
    code: &str,
    version: i32,
) -> Result<Option<StageRecord>> {
    let row = stage_row::Entity::find()
        .filter(stage_row::Column::Code.eq(code))
        .filter(stage_row::Column::Version.eq(version))
        .one(db)
        .await?;
    row.map(stage_record).transpose()
}

/// Активная версия Этапа — та, которую запинит следующая активация Процесса.
pub async fn active_stage(db: &DatabaseConnection, code: &str) -> Result<Option<StageRecord>> {
    let row = stage_row::Entity::find()
        .filter(stage_row::Column::Code.eq(code))
        .filter(stage_row::Column::Status.eq(DefinitionStatus::Active.as_str()))
        .one(db)
        .await?;
    row.map(stage_record).transpose()
}

pub async fn stage_draft(db: &DatabaseConnection, code: &str) -> Result<Option<StageRecord>> {
    find_stage_draft_row(db, code)
        .await?
        .map(stage_record)
        .transpose()
}

/// История версий Этапа, свежие сверху.
pub async fn list_stage_versions(
    db: &DatabaseConnection,
    code: &str,
) -> Result<Vec<DefinitionVersion>> {
    Ok(stage_row::Entity::find()
        .filter(stage_row::Column::Code.eq(code))
        .order_by_desc(stage_row::Column::Version)
        .all(db)
        .await?
        .into_iter()
        .map(|row| DefinitionVersion {
            code: row.code,
            version: row.version,
            title: row.title,
            status: DefinitionStatus::from_str(&row.status),
            digest: row.digest,
            created_at: row.created_at,
            created_by: row.created_by,
        })
        .collect())
}

/// Все Этапы каталога: активная версия каждого кода, а при её отсутствии —
/// черновик. Каталог глобальный, Этапы переиспользуются между Процессами.
pub async fn list_stage_heads(db: &DatabaseConnection) -> Result<Vec<DefinitionVersion>> {
    let rows = stage_row::Entity::find()
        .order_by_asc(stage_row::Column::Code)
        .order_by_desc(stage_row::Column::Version)
        .all(db)
        .await?;
    Ok(heads(rows.into_iter().map(|row| DefinitionVersion {
        code: row.code,
        version: row.version,
        title: row.title,
        status: DefinitionStatus::from_str(&row.status),
        digest: row.digest,
        created_at: row.created_at,
        created_by: row.created_by,
    })))
}

/// Головные версии Этапов **целиком**: манифест и код, а не строка списка.
///
/// Отдельный запрос на каждый код был бы лишним кругом: строки всё равно
/// читаются полностью, и заголовок отбрасывает определение уже после разбора.
pub async fn list_stage_head_records(db: &DatabaseConnection) -> Result<Vec<StageRecord>> {
    let rows = stage_row::Entity::find()
        .order_by_asc(stage_row::Column::Code)
        .order_by_desc(stage_row::Column::Version)
        .all(db)
        .await?;
    let records = rows
        .into_iter()
        .map(stage_record)
        .collect::<Result<Vec<_>>>()?;
    Ok(heads_by(
        records.into_iter(),
        |record| record.code.clone(),
        |record| record.status,
    ))
}

/// Перевести версию Этапа в активные, прежнюю активную — в архив.
///
/// Порядок операций именно такой: сначала снять старую, потом поставить новую.
/// Обратный порядок упёрся бы в уникальный индекс.
pub async fn activate_stage(db: &DatabaseConnection, code: &str, version: i32) -> Result<()> {
    if let Some(current) = active_stage(db, code).await? {
        if current.version == version {
            return Ok(());
        }
        set_stage_status(db, code, current.version, DefinitionStatus::Archived).await?;
    }
    set_stage_status(db, code, version, DefinitionStatus::Active).await
}

async fn set_stage_status(
    db: &DatabaseConnection,
    code: &str,
    version: i32,
    status: DefinitionStatus,
) -> Result<()> {
    let Some(row) = stage_row::Entity::find()
        .filter(stage_row::Column::Code.eq(code))
        .filter(stage_row::Column::Version.eq(version))
        .one(db)
        .await?
    else {
        anyhow::bail!("версия Этапа {code} v{version} не найдена");
    };
    let mut active: stage_row::ActiveModel = row.into();
    active.status = Set(status.as_str().to_string());
    active.update(db).await?;
    Ok(())
}

/// Удалить черновик Этапа.
///
/// Опубликованное не удаляется вообще: на нём могут доживать экземпляры, а его
/// прогоны уже записаны в журнале эффектов. Поэтому проверка статуса здесь
/// жёсткая, а не «если нет ссылок» — ссылок может ещё не быть в момент запроса.
pub async fn delete_stage_draft(db: &DatabaseConnection, code: &str, version: i32) -> Result<()> {
    let Some(record) = find_stage(db, code, version).await? else {
        anyhow::bail!("версия Этапа {code} v{version} не найдена");
    };
    if record.status.is_published() {
        anyhow::bail!(
            "версия Этапа {code} v{version} опубликована ({}) и не удаляется: \
             на ней могут доживать экземпляры",
            record.status.as_str()
        );
    }
    stage_row::Entity::delete_by_id(record.id).exec(db).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Процессы
// ---------------------------------------------------------------------------

async fn next_process_version(db: &DatabaseConnection, code: &str) -> Result<i32> {
    let last = process_row::Entity::find()
        .filter(process_row::Column::Code.eq(code))
        .order_by_desc(process_row::Column::Version)
        .one(db)
        .await?;
    Ok(last.map(|row| row.version + 1).unwrap_or(FIRST_VERSION))
}

pub async fn save_process_draft(
    db: &DatabaseConnection,
    input: DraftInput<ProcessDefinition>,
) -> Result<ProcessRecord> {
    let code = input.definition.manifest.code.clone();
    let manifest_json = serde_json::to_string(&input.definition.manifest)?;
    let title = input.definition.manifest.title.clone();

    if let Some(existing) = find_process_draft_row(db, &code).await? {
        let mut active: process_row::ActiveModel = existing.into();
        active.title = Set(title);
        active.manifest_json = Set(manifest_json);
        active.digest = Set(input.digest.clone());
        active.created_at = Set(Utc::now().to_rfc3339());
        active.created_by = Set(input.created_by.clone());
        return process_record(active.update(db).await?);
    }

    let row = process_row::ActiveModel {
        id: Set(Uuid::new_v4().to_string()),
        code: Set(code.clone()),
        version: Set(next_process_version(db, &code).await?),
        status: Set(DefinitionStatus::Draft.as_str().to_string()),
        title: Set(title),
        manifest_json: Set(manifest_json),
        digest: Set(input.digest),
        // Пины появляются в момент активации, а не сохранения: пока версия
        // черновик, «что именно она запустит» ещё не решено.
        pins_json: Set("[]".to_string()),
        created_at: Set(Utc::now().to_rfc3339()),
        created_by: Set(input.created_by),
    };
    process_record(row.insert(db).await?)
}

async fn find_process_draft_row(
    db: &DatabaseConnection,
    code: &str,
) -> Result<Option<process_row::Model>> {
    Ok(process_row::Entity::find()
        .filter(process_row::Column::Code.eq(code))
        .filter(process_row::Column::Status.eq(DefinitionStatus::Draft.as_str()))
        .one(db)
        .await?)
}

pub async fn find_process(
    db: &DatabaseConnection,
    code: &str,
    version: i32,
) -> Result<Option<ProcessRecord>> {
    let row = process_row::Entity::find()
        .filter(process_row::Column::Code.eq(code))
        .filter(process_row::Column::Version.eq(version))
        .one(db)
        .await?;
    row.map(process_record).transpose()
}

pub async fn active_process(db: &DatabaseConnection, code: &str) -> Result<Option<ProcessRecord>> {
    let row = process_row::Entity::find()
        .filter(process_row::Column::Code.eq(code))
        .filter(process_row::Column::Status.eq(DefinitionStatus::Active.as_str()))
        .one(db)
        .await?;
    row.map(process_record).transpose()
}

pub async fn process_draft(db: &DatabaseConnection, code: &str) -> Result<Option<ProcessRecord>> {
    find_process_draft_row(db, code)
        .await?
        .map(process_record)
        .transpose()
}

/// Активные Процессы — те, чьи триггеры слушает воркер.
pub async fn list_active_processes(db: &DatabaseConnection) -> Result<Vec<ProcessRecord>> {
    process_row::Entity::find()
        .filter(process_row::Column::Status.eq(DefinitionStatus::Active.as_str()))
        .order_by_asc(process_row::Column::Code)
        .all(db)
        .await?
        .into_iter()
        .map(process_record)
        .collect()
}

pub async fn list_process_versions(
    db: &DatabaseConnection,
    code: &str,
) -> Result<Vec<DefinitionVersion>> {
    Ok(process_row::Entity::find()
        .filter(process_row::Column::Code.eq(code))
        .order_by_desc(process_row::Column::Version)
        .all(db)
        .await?
        .into_iter()
        .map(|row| DefinitionVersion {
            code: row.code,
            version: row.version,
            title: row.title,
            status: DefinitionStatus::from_str(&row.status),
            digest: row.digest,
            created_at: row.created_at,
            created_by: row.created_by,
        })
        .collect())
}

pub async fn list_process_heads(db: &DatabaseConnection) -> Result<Vec<DefinitionVersion>> {
    let rows = process_row::Entity::find()
        .order_by_asc(process_row::Column::Code)
        .order_by_desc(process_row::Column::Version)
        .all(db)
        .await?;
    Ok(heads(rows.into_iter().map(|row| DefinitionVersion {
        code: row.code,
        version: row.version,
        title: row.title,
        status: DefinitionStatus::from_str(&row.status),
        digest: row.digest,
        created_at: row.created_at,
        created_by: row.created_by,
    })))
}

/// Головные версии Процессов **целиком** — вместе с графом.
///
/// Без графа заголовок Процесса не отвечает ни на один содержательный вопрос:
/// «куда идёт этот выход» и «чем всё кончается» записаны рёбрами.
pub async fn list_process_head_records(db: &DatabaseConnection) -> Result<Vec<ProcessRecord>> {
    let rows = process_row::Entity::find()
        .order_by_asc(process_row::Column::Code)
        .order_by_desc(process_row::Column::Version)
        .all(db)
        .await?;
    let records = rows
        .into_iter()
        .map(process_record)
        .collect::<Result<Vec<_>>>()?;
    Ok(heads_by(
        records.into_iter(),
        |record| record.code.clone(),
        |record| record.status,
    ))
}

/// Перевести версию Процесса в активные, записав пины Этапов.
///
/// Пины пишутся здесь и больше не меняются: с этого момента известно, какой
/// именно код побежит у экземпляров, стартовавших на этой версии. Повторная
/// активация той же версии переписывает пины — это законный способ подтянуть
/// новую версию Этапа, и он проходит через тот же двухуровневый diff.
pub async fn activate_process(
    db: &DatabaseConnection,
    code: &str,
    version: i32,
    pins: &[StagePin],
) -> Result<()> {
    if let Some(current) = active_process(db, code).await? {
        if current.version != version {
            set_process_status(db, code, current.version, DefinitionStatus::Archived).await?;
        }
    }
    let Some(row) = process_row::Entity::find()
        .filter(process_row::Column::Code.eq(code))
        .filter(process_row::Column::Version.eq(version))
        .one(db)
        .await?
    else {
        anyhow::bail!("версия Процесса {code} v{version} не найдена");
    };
    let mut active: process_row::ActiveModel = row.into();
    active.status = Set(DefinitionStatus::Active.as_str().to_string());
    active.pins_json = Set(serde_json::to_string(pins)?);
    active.update(db).await?;
    Ok(())
}

/// Версии Этапов, запиненные версией Процесса в момент её активации.
///
/// Пустой список у черновика — норма: пины появляются при активации. У
/// опубликованной версии пустой список означает, что активировали до того, как
/// граф разрешился, и это дефект данных, а не «Этапов нет».
pub async fn process_pins(
    db: &DatabaseConnection,
    code: &str,
    version: i32,
) -> Result<Vec<StagePin>> {
    let Some(row) = process_row::Entity::find()
        .filter(process_row::Column::Code.eq(code))
        .filter(process_row::Column::Version.eq(version))
        .one(db)
        .await?
    else {
        anyhow::bail!("версия Процесса {code} v{version} не найдена");
    };
    Ok(serde_json::from_str(&row.pins_json).unwrap_or_default())
}

/// Снять Процесс с работы: активная версия уходит в архив, новых экземпляров
/// больше не будет. Живые экземпляры продолжают идти по запиненной версии —
/// иначе остановка Процесса потеряла бы незавершённую работу.
pub async fn deactivate_process(db: &DatabaseConnection, code: &str) -> Result<()> {
    let Some(current) = active_process(db, code).await? else {
        anyhow::bail!("у Процесса {code} нет активной версии");
    };
    set_process_status(db, code, current.version, DefinitionStatus::Archived).await
}

async fn set_process_status(
    db: &DatabaseConnection,
    code: &str,
    version: i32,
    status: DefinitionStatus,
) -> Result<()> {
    let Some(row) = process_row::Entity::find()
        .filter(process_row::Column::Code.eq(code))
        .filter(process_row::Column::Version.eq(version))
        .one(db)
        .await?
    else {
        anyhow::bail!("версия Процесса {code} v{version} не найдена");
    };
    let mut active: process_row::ActiveModel = row.into();
    active.status = Set(status.as_str().to_string());
    active.update(db).await?;
    Ok(())
}

pub async fn delete_process_draft(db: &DatabaseConnection, code: &str, version: i32) -> Result<()> {
    let Some(record) = find_process(db, code, version).await? else {
        anyhow::bail!("версия Процесса {code} v{version} не найдена");
    };
    if record.status.is_published() {
        anyhow::bail!(
            "версия Процесса {code} v{version} опубликована ({}) и не удаляется: \
             на ней могут доживать экземпляры",
            record.status.as_str()
        );
    }
    process_row::Entity::delete_by_id(record.id)
        .exec(db)
        .await?;
    Ok(())
}

/// Оставить по одной строке на код: активную, а если её нет — самую свежую.
///
/// Порядок строк на входе задан запросом (код по возрастанию, версия по
/// убыванию), поэтому «самая свежая» — первая встреченная.
fn heads(rows: impl Iterator<Item = DefinitionVersion>) -> Vec<DefinitionVersion> {
    heads_by(rows, |row| row.code.clone(), |row| row.status)
}

/// То же правило отбора, но над любым носителем кода и статуса.
///
/// Обобщение не ради красоты: заголовок списка и полное определение обязаны
/// выбирать **одну и ту же** строку, иначе карточка показывала бы одну версию,
/// а кнопки под ней работали бы с другой.
fn heads_by<T>(
    rows: impl Iterator<Item = T>,
    code: impl Fn(&T) -> String,
    status: impl Fn(&T) -> DefinitionStatus,
) -> Vec<T> {
    let mut heads: Vec<T> = Vec::new();
    for row in rows {
        let row_code = code(&row);
        match heads.iter().position(|head| code(head) == row_code) {
            None => heads.push(row),
            Some(index) => {
                if status(&heads[index]) != DefinitionStatus::Active
                    && status(&row) == DefinitionStatus::Active
                {
                    heads[index] = row;
                }
            }
        }
    }
    heads
}
