//! Действие `rebuild_day_close` — пересобрать документ закрытия дня WB.
//!
//! Первое Действие пилота `pr0001`. Эффект внутренний: документ a033 создаётся
//! (или переиспользуется активный) и пересчитывается из проекций. Наружу
//! ничего не уходит.

use anyhow::Result;
use async_trait::async_trait;
use contracts::processes::{ActionActor, ActionInfo};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::OnceLock;

use super::Action;
use crate::domain::a033_wb_day_close::service as day_close;

#[derive(Debug, Deserialize)]
struct Input {
    connection_id: String,
    business_date: String,
}

pub struct RebuildDayClose;

fn info() -> &'static ActionInfo {
    static INFO: OnceLock<ActionInfo> = OnceLock::new();
    INFO.get_or_init(|| ActionInfo {
        name: "rebuild_day_close",
        method: "rebuildDayClose",
        title: "Пересобрать закрытие дня WB",
        description: "Создаёт активный документ a033 за дату и кабинет и пересчитывает \
                      его строки из p903/p913/a012/a015/p912.",
        capability: "action:rebuild_day_close",
        // Обратимо: прошлая версия документа уходит в архив, а не исчезает.
        reversible: true,
        write_tables: &["a033_wb_day_close"],
        input_schema: json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["connection_id", "business_date"],
            "additionalProperties": false,
            "properties": {
                "connection_id": {
                    "type": "string",
                    "minLength": 1,
                    "description": "UUID кабинета WB (a006_connection_mp)"
                },
                "business_date": {
                    "type": "string",
                    "pattern": "^\\d{4}-\\d{2}-\\d{2}$",
                    "description": "Бизнес-дата закрытия, YYYY-MM-DD"
                }
            }
        }),
    })
}

#[async_trait]
impl Action for RebuildDayClose {
    fn info(&self) -> &ActionInfo {
        info()
    }

    async fn execute(
        &self,
        _db: &DatabaseConnection,
        input: &Value,
        _actor: &ActionActor,
    ) -> Result<Value> {
        let input: Input = serde_json::from_value(input.clone())?;
        let id = day_close::create_active(&input.connection_id, &input.business_date).await?;
        day_close::recalculate(id).await?;

        // В журнал уходит не «ok», а то, по чему эффект находится глазами.
        let document = day_close::get_by_id(id).await?;
        let problems = document
            .as_ref()
            .map(|doc| doc.problems.len())
            .unwrap_or_default();
        Ok(json!({
            "document_id": id.to_string(),
            "connection_id": input.connection_id,
            "business_date": input.business_date,
            "problems": problems,
        }))
    }

    async fn plan(
        &self,
        _db: &DatabaseConnection,
        input: &Value,
        _actor: &ActionActor,
    ) -> Result<Value> {
        let input: Input = serde_json::from_value(input.clone())?;
        // План тем полезнее, чем конкретнее: показываем, что уже есть за дату,
        // иначе человек не отличит «создаст новый» от «пересчитает текущий».
        let existing = day_close::list_by_day(&input.connection_id, &input.business_date).await?;
        let active = existing.iter().filter(|doc| !doc.is_archived).count();
        Ok(json!({
            "effect": "пересборка документа закрытия дня",
            "connection_id": input.connection_id,
            "business_date": input.business_date,
            "existing_documents": existing.len(),
            "active_documents": active,
            "note": if active > 0 {
                "активный документ за эту дату уже есть — он будет пересчитан"
            } else {
                "активного документа за эту дату нет — он будет создан"
            },
        }))
    }
}
