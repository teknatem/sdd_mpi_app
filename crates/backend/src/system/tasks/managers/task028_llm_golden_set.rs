//! Прогон эталонного набора вопросов («LLM CI»).
//!
//! Каждый кейс задаётся живому сотруднику нужной специализации, ответы собираются
//! и одним заходом отдаются судье на сверку с ожиданиями. Вердикты пишутся в тот же
//! `sys_llm_verdict` с `source='golden'`, поэтому динамика регрессии смотрится
//! рядом с оценками живых диалогов.
//!
//! Смысл именно в неизменности набора: вердикты по живым чатам каждый день считаются
//! на разных вопросах, и по ним нельзя сказать, что изменила правка промпта.

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
use crate::shared::llm::golden_set;
use crate::system::tasks::logger::TaskLogger;
use crate::system::tasks::manager::{TaskManager, TaskRunOutcome};

static METADATA: TaskMetadata = TaskMetadata {
    task_type: "task028_llm_golden_set",
    write_tables: &[
        "sys_llm_verdict",
        "a018_llm_chat",
        "a018_llm_chat_message",
        "sys_tool_trace",
    ],
    display_name: "LLM CI — прогон эталонных вопросов",
    description: "Задаёт неизменный набор эталонных вопросов сотрудникам нужных \
        специализаций и сверяет ответы с ожиданиями. Запускать после правок промптов \
        и навыков, чтобы изменение качества было видно на одном и том же наборе.",
    external_apis: &[],
    constraints: &[
        "Кейсы лежат в каталоге [llm].golden_set_path: <case-id>.md с полями question и expect",
        "Каждый кейс — реальный вызов модели; набор держи компактным",
        "Пустой каталог кейсов означает пропуск прогона, а не ошибку",
    ],
    config_fields: &[TaskConfigField {
        key: "max_cases",
        label: "Максимум кейсов за прогон",
        hint: "Верхняя граница расхода вызовов модели",
        field_type: TaskConfigFieldType::Integer,
        required: false,
        default_value: Some("10"),
        min_value: Some(1),
        max_value: Some(50),
    }],
    max_duration_seconds: 3600,
};

#[derive(Debug, Deserialize)]
struct GoldenConfig {
    #[serde(default = "default_max_cases")]
    max_cases: usize,
}

fn default_max_cases() -> usize {
    10
}

impl Default for GoldenConfig {
    fn default() -> Self {
        Self {
            max_cases: default_max_cases(),
        }
    }
}

pub struct Task028LlmGoldenSetManager;

impl Task028LlmGoldenSetManager {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Task028LlmGoldenSetManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Результат одного прогнанного кейса — вход для сверки.
struct CaseRun {
    case_id: String,
    chat_id: String,
    question: String,
    expect: Vec<String>,
    notes: String,
    answer: String,
}

#[async_trait]
impl TaskManager for Task028LlmGoldenSetManager {
    fn task_type(&self) -> &'static str {
        "task028_llm_golden_set"
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
        let config: GoldenConfig =
            serde_json::from_str(&task.config_json).unwrap_or_else(|_| GoldenConfig::default());

        let cases = golden_set::load_cases().map_err(|error| anyhow::anyhow!(error))?;
        if cases.is_empty() {
            logger.write_log(
                session_id,
                "Каталог эталонных кейсов пуст — прогон пропущен.",
            )?;
            return Ok(TaskRunOutcome::completed());
        }

        let planned = cases.len().min(config.max_cases.clamp(1, 50));
        logger.write_log(
            session_id,
            &format!(
                "Голден-сет: кейсов найдено {}, прогоняем {planned}",
                cases.len()
            ),
        )?;

        let mut runs: Vec<CaseRun> = Vec::new();
        for case in cases.into_iter().take(planned) {
            match run_case(&case, session_id).await {
                Ok(run) => runs.push(run),
                Err(error) => {
                    // Падение одного кейса не должно отменять весь прогон: остальные
                    // всё ещё дают сравнимую с прошлым разом картину.
                    logger.write_log(
                        session_id,
                        &format!("Кейс {} не отработал: {error}", case.id),
                    )?;
                }
            }
        }

        if runs.is_empty() {
            anyhow::bail!("Ни один кейс не отработал — см. лог сессии");
        }

        let recorded = judge_runs(&runs, session_id).await?;

        logger.write_log(
            session_id,
            &format!(
                "Голден-сет завершён: прогнано {}, вердиктов записано {recorded}",
                runs.len()
            ),
        )?;

        if recorded == 0 {
            anyhow::bail!(
                "Судья не записал ни одного вердикта при {} прогнанных кейсах",
                runs.len()
            );
        }

        Ok(TaskRunOutcome::completed())
    }

    fn get_progress(&self, _session_id: &str) -> Option<TaskProgress> {
        None
    }
}

/// Задать вопрос кейса живому сотруднику нужной специализации.
async fn run_case(case: &golden_set::GoldenCase, session_id: &str) -> Result<CaseRun> {
    let employee = a017_llm_agent::service::ensure_employee_for(case.agent_type.clone()).await?;

    let chat_id = a018_llm_chat::service::create(
        a018_llm_chat::service::LlmChatDto {
            id: None,
            code: Some(format!("GOLDEN-{}-{}", session_id, case.id)),
            description: format!("Голден-кейс {}", case.title),
            comment: Some("Служебный чат task028_llm_golden_set".to_string()),
            agent_id: employee.base.id.as_string(),
            model_name: Some(employee.model_name.clone()),
        },
        None,
    )
    .await?;

    // Вопрос задаётся дословно: любая обвязка вокруг него сделала бы прогон
    // несравнимым с предыдущим и с реальным пользовательским запросом.
    let response = a018_llm_chat::service::send_message(
        &chat_id.to_string(),
        a018_llm_chat::service::SendMessageRequest {
            content: case.question.clone(),
            model_name: Some(employee.model_name.clone()),
            attachment_ids: Vec::new(),
            request_id: None,
        },
        None,
        None,
    )
    .await?;

    Ok(CaseRun {
        case_id: case.id.clone(),
        chat_id: chat_id.to_string(),
        question: case.question.clone(),
        expect: case.expect.clone(),
        notes: case.notes.clone(),
        answer: response.content,
    })
}

/// Отдать судье все прогнанные кейсы одним заходом и вернуть число записанных вердиктов.
async fn judge_runs(runs: &[CaseRun], session_id: &str) -> Result<i64> {
    let judge = a017_llm_agent::service::ensure_employee_for(AgentType::CoordinatorAdmin).await?;

    let chat_id = a018_llm_chat::service::create(
        a018_llm_chat::service::LlmChatDto {
            id: None,
            code: Some(format!("LLM-JUDGE-GOLDEN-{}", session_id)),
            description: format!("Сверка голден-сета {}", session_id),
            comment: Some("Служебный чат task028_llm_golden_set (сверка)".to_string()),
            agent_id: judge.base.id.as_string(),
            model_name: Some(judge.model_name.clone()),
        },
        None,
    )
    .await?;

    let mut body = String::new();
    for (index, run) in runs.iter().enumerate() {
        body.push_str(&format!(
            "\n### Кейс {} (case_id={}, chat_id={})\n\
             Вопрос: {}\n\
             Ожидания: {}\n",
            index + 1,
            run.case_id,
            run.chat_id,
            run.question,
            run.expect.join("; ")
        ));
        if !run.notes.is_empty() {
            body.push_str(&format!("Пояснение к кейсу: {}\n", run.notes));
        }
        body.push_str(&format!("Ответ агента:\n{}\n", truncate(&run.answer, 3000)));
    }

    let trigger = format!(
        "Режим: llm_quality_review.\n\
         Активируй навык llm-quality-review через use_skill, если он ещё не активен.\n\
         Это прогон эталонного набора: по каждому кейсу есть вопрос, список ожиданий \
         и фактический ответ агента. Сверь ответ с ожиданиями.\n\
         Вердикт solved — выполнены все ожидания; partial — часть; failed — ответ им \
         не соответствует. В reason назови конкретное невыполненное ожидание.\n\
         Ответы уже приведены ниже — get_chat_digest вызывать не нужно.\n\
         Одним вызовом record_chat_verdicts запиши вердикты по всем кейсам, указав для \
         каждого source='golden', его chat_id и case_id, и judge_session_id='{session_id}'.\n\
         После записи инструменты не вызывай: дай короткий отчёт по регрессии.\n\
         {body}"
    );

    a018_llm_chat::service::send_message(
        &chat_id.to_string(),
        a018_llm_chat::service::SendMessageRequest {
            content: trigger,
            model_name: Some(judge.model_name.clone()),
            attachment_ids: Vec::new(),
            request_id: None,
        },
        None,
        None,
    )
    .await?;

    Ok(
        crate::shared::llm::verdicts::verdicts_by_session(session_id)
            .await
            .unwrap_or(0),
    )
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let cut: String = text.chars().take(max_chars).collect();
    format!("{cut}… [ответ обрезан]")
}
