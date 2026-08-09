//! Судья качества ответов: разбирает диалоги за окно и ставит структурированные
//! вердикты в `sys_llm_verdict`.
//!
//! Отличие от `task014_kb_analyze`: тот ищет пробелы в бизнес-знаниях и заводит
//! тикеты на правку KB, то есть улучшает знание. Здесь измеряется исход работы —
//! помог ответ человеку или нет. Без этой метрики изменения промптов и навыков
//! вносятся вслепую: механизмов становится больше, а стало ли лучше — неизвестно.

use anyhow::Result;
use async_trait::async_trait;
use contracts::domain::a017_llm_agent::aggregate::AgentType;
use contracts::domain::common::AggregateId;
use contracts::system::tasks::aggregate::ScheduledTask;
use contracts::system::tasks::metadata::{TaskConfigField, TaskConfigFieldType, TaskMetadata};
use contracts::system::tasks::progress::TaskProgress;
use serde::Deserialize;
use std::sync::Arc;

use crate::domain::{a017_llm_agent, a018_llm_chat};
use crate::system::tasks::logger::TaskLogger;
use crate::system::tasks::manager::{TaskManager, TaskRunOutcome};

static METADATA: TaskMetadata = TaskMetadata {
    task_type: "task027_llm_judge",
    // Те же таблицы, что у KB-заданий: координатор ресурсов сериализует фоновые
    // LLM-задачи между собой, чтобы они не делили один чат-пайплайн одновременно.
    write_tables: &[
        "sys_llm_verdict",
        "a018_llm_chat",
        "a018_llm_chat_message",
        "sys_tool_trace",
    ],
    display_name: "LLM — оценка качества ответов",
    description: "Координатор разбирает диалоги агентов за окно и ставит вердикты \
        solved/partial/failed с причиной провала. Метрика для дашборда качества LLM.",
    external_apis: &[],
    constraints: &[
        "Требует активного сотрудника специализации coordinator_admin",
        "Только оценка: задача ничего не правит в системе и не заводит тикеты",
        "Уже оценённые диалоги повторно не разбираются",
    ],
    config_fields: &[
        TaskConfigField {
            key: "lookback_days",
            label: "Период разбора (дней)",
            hint: "Глубина окна: какие диалоги считать свежими",
            field_type: TaskConfigFieldType::Integer,
            required: false,
            default_value: Some("2"),
            min_value: Some(1),
            max_value: Some(90),
        },
        TaskConfigField {
            key: "max_chats",
            label: "Максимум диалогов за прогон",
            hint: "Верхняя граница: каждый диалог — это вызовы модели",
            field_type: TaskConfigFieldType::Integer,
            required: false,
            default_value: Some("15"),
            min_value: Some(1),
            max_value: Some(50),
        },
    ],
    max_duration_seconds: 1800,
};

#[derive(Debug, Deserialize)]
struct JudgeConfig {
    #[serde(default = "default_lookback_days")]
    lookback_days: i64,
    #[serde(default = "default_max_chats")]
    max_chats: i64,
}

fn default_lookback_days() -> i64 {
    2
}

fn default_max_chats() -> i64 {
    15
}

impl Default for JudgeConfig {
    fn default() -> Self {
        Self {
            lookback_days: default_lookback_days(),
            max_chats: default_max_chats(),
        }
    }
}

pub struct Task027LlmJudgeManager;

impl Task027LlmJudgeManager {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Task027LlmJudgeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TaskManager for Task027LlmJudgeManager {
    fn task_type(&self) -> &'static str {
        "task027_llm_judge"
    }

    fn metadata(&self) -> &'static TaskMetadata {
        &METADATA
    }

    async fn run(
        &self,
        task: &ScheduledTask,
        session_id: &str,
        logger: Arc<TaskLogger>,
    ) -> Result<TaskRunOutcome> {
        let config: JudgeConfig =
            serde_json::from_str(&task.config_json).unwrap_or_else(|_| JudgeConfig::default());
        let lookback_days = config.lookback_days.clamp(1, 90);
        let max_chats = config.max_chats.clamp(1, 50);

        // Пустой прогон не стоит вызова модели: если оценивать нечего, выходим
        // до создания чата, чтобы не плодить служебный мусор и не жечь токены.
        let pending =
            crate::shared::llm::verdicts::chats_awaiting_verdict(lookback_days, max_chats)
                .await
                .map_err(|error| anyhow::anyhow!(error))?;
        if pending.is_empty() {
            logger.write_log(
                session_id,
                &format!("Нет диалогов без вердикта за {lookback_days} дн. — прогон пропущен."),
            )?;
            return Ok(TaskRunOutcome::completed());
        }

        let agent = a017_llm_agent::service::ensure_employee_for(AgentType::CoordinatorAdmin).await?;

        logger.write_log(
            session_id,
            &format!(
                "LLM judge started: agent={}, lookback_days={}, pending_chats={}",
                agent.base.description,
                lookback_days,
                pending.len()
            ),
        )?;

        let chat_id = a018_llm_chat::service::create(
            a018_llm_chat::service::LlmChatDto {
                id: None,
                code: Some(format!("LLM-JUDGE-{}", session_id)),
                description: format!("Оценка качества {}", session_id),
                comment: Some("Служебный чат task027_llm_judge".to_string()),
                agent_id: agent.base.id.as_string(),
                model_name: Some(agent.model_name.clone()),
            },
            None,
        )
        .await?;

        let trigger = format!(
            "Режим: llm_quality_review.\n\
             Активируй навык llm-quality-review через use_skill, если он ещё не активен.\n\
             Оцени качество работы агентов за последние {lookback_days} дн.\n\
             1. Вызови list_chats_for_review(lookback_days={lookback_days}, limit={max_chats}).\n\
             2. По каждому диалогу из списка вызови get_chat_digest и разберись, \
             получил ли человек то, что просил.\n\
             3. Одним вызовом record_chat_verdicts запиши вердикты по ВСЕМ разобранным \
             диалогам сразу, передав judge_session_id='{session_id}'.\n\
             Оценивай исход для человека, а не старательность агента. \
             В reason указывай, на чьей стороне причина: модель, инструмент, \
             отсутствующие данные или пробел в базе знаний.\n\
             После успешного record_chat_verdicts инструменты больше не вызывай: \
             сразу дай короткий финальный отчёт — сколько разобрано, распределение \
             вердиктов и 2-3 повторяющиеся проблемы."
        );

        let response = a018_llm_chat::service::send_message(
            &chat_id.to_string(),
            a018_llm_chat::service::SendMessageRequest {
                content: trigger,
                model_name: Some(agent.model_name.clone()),
                attachment_ids: Vec::new(),
                request_id: None,
            },
            None,
            None, // фоновая задача: собеседника-человека нет
        )
        .await?;

        // Считаем реально записанное, а не то, что модель написала в отчёте:
        // отчёт — это текст, а метрика должна опираться на строки в таблице.
        let recorded = crate::shared::llm::verdicts::verdicts_by_session(session_id)
            .await
            .unwrap_or(0);

        logger.write_log(
            session_id,
            &format!(
                "LLM judge completed. assistant_msg={}, verdicts_recorded={}, tool_trace={}",
                response.id,
                recorded,
                response.tool_trace.unwrap_or_else(|| "[]".to_string())
            ),
        )?;

        if recorded == 0 {
            // Молчаливый ноль — самый опасный исход: задача «отработала», а метрика
            // не наполнилась. Пусть это видно как отказ, а не как успех.
            anyhow::bail!(
                "Судья не записал ни одного вердикта при {} кандидатах — см. лог сессии",
                pending.len()
            );
        }

        Ok(TaskRunOutcome::completed())
    }

    fn get_progress(&self, _session_id: &str) -> Option<TaskProgress> {
        None
    }
}
