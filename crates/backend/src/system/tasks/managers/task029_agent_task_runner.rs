//! Исполнение очереди поручений между AI-сотрудниками (a042).
//!
//! Отличие от task014/015/021/027, где задача сама формулирует задание модели:
//! здесь задание приходит от другого агента строкой в очереди, а регламент —
//! только исполнитель. Отсюда три вещи, которых у образцов нет:
//!
//!   * атомарный захват записи (`try_claim`) — регламентный и ручной прогоны
//!     видят одну и ту же `pending`-строку, и read-then-write отдал бы её обоим;
//!   * пер-итемный таймаут — у task021/027 его нет, и один разогнавшийся агентный
//!     цикл съедает весь бюджет прогона, а остальные записи молча не исполняются;
//!   * развёртка брошенных прогонов — иначе упавший воркер оставляет запись в
//!     `processing` навсегда (болезнь a031, где нет ни колонки ошибки, ни развёртки).

use anyhow::Result;
use async_trait::async_trait;
use contracts::domain::a042_agent_task::aggregate::AgentTask;
use contracts::domain::common::AggregateId;
use contracts::system::tasks::aggregate::ScheduledTask;
use contracts::system::tasks::metadata::{TaskConfigField, TaskConfigFieldType, TaskMetadata};
use contracts::system::tasks::progress::TaskProgress;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

use crate::domain::{a017_llm_agent, a018_llm_chat, a042_agent_task};
use crate::shared::llm::router;
use crate::system::tasks::logger::TaskLogger;
use crate::system::tasks::manager::{TaskManager, TaskRunOutcome};

static METADATA: TaskMetadata = TaskMetadata {
    task_type: "task029_agent_task_runner",
    // Те же таблицы чат-пайплайна, что у остальных LLM-заданий: координатор
    // ресурсов сериализует их между собой, чтобы они не делили один пайплайн
    // и один лимит провайдера одновременно.
    write_tables: &[
        "a042_agent_task",
        "a018_llm_chat",
        "a018_llm_chat_message",
        "sys_tool_trace",
    ],
    display_name: "Поручения AI-сотрудникам — исполнение очереди",
    description: "Берёт поручения из очереди a042, прогоняет каждое через служебный чат от лица \
        сотрудника заказанной специализации и записывает результат в поручение.",
    external_apis: &[],
    constraints: &[
        "Каждое поручение — полный агентный прогон с реальными вызовами модели",
        "Требует активного сотрудника заказанной специализации (создаётся автоматически)",
        "Провал одного поручения не прерывает остальные в пачке",
        "Порог развёртки должен превышать лимит на одно поручение",
    ],
    config_fields: &[
        TaskConfigField {
            key: "max_tasks",
            label: "Максимум поручений за прогон",
            hint: "Верхняя граница: каждое поручение — это вызовы модели",
            field_type: TaskConfigFieldType::Integer,
            required: false,
            default_value: Some("3"),
            min_value: Some(1),
            max_value: Some(10),
        },
        TaskConfigField {
            key: "stale_minutes",
            label: "Порог зависания (минут)",
            hint: "Через сколько вернуть в очередь поручение, брошенное упавшим прогоном",
            field_type: TaskConfigFieldType::Integer,
            required: false,
            default_value: Some("45"),
            min_value: Some(10),
            max_value: Some(240),
        },
        TaskConfigField {
            key: "item_timeout_seconds",
            label: "Лимит на одно поручение (сек)",
            hint: "Обрыв затянувшегося агентного цикла, чтобы он не съел весь прогон",
            field_type: TaskConfigFieldType::Integer,
            required: false,
            default_value: Some("600"),
            min_value: Some(60),
            max_value: Some(1800),
        },
    ],
    max_duration_seconds: 3600,
};

#[derive(Debug, Deserialize)]
struct RunnerConfig {
    #[serde(default = "default_max_tasks")]
    max_tasks: u64,
    #[serde(default = "default_stale_minutes")]
    stale_minutes: i64,
    #[serde(default = "default_item_timeout")]
    item_timeout_seconds: u64,
}

fn default_max_tasks() -> u64 {
    3
}

fn default_stale_minutes() -> i64 {
    45
}

fn default_item_timeout() -> u64 {
    600
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            max_tasks: default_max_tasks(),
            stale_minutes: default_stale_minutes(),
            item_timeout_seconds: default_item_timeout(),
        }
    }
}

/// Ошибка исполнения одного поручения с признаком «стоит ли повторять».
///
/// Разделение проведено по источнику, а не по тексту ошибки: пре-флайт
/// (нет сотрудника, не создался чат) — это конфигурация, повтор её не вылечит;
/// всё, что пришло из прогона модели, включая таймаут, — транзиент.
struct ExecError {
    message: String,
    retryable: bool,
}

impl ExecError {
    fn fatal(e: impl std::fmt::Display) -> Self {
        Self {
            message: e.to_string(),
            retryable: false,
        }
    }
    fn retry(e: impl std::fmt::Display) -> Self {
        Self {
            message: e.to_string(),
            retryable: true,
        }
    }
}

pub struct Task029AgentTaskRunnerManager;

impl Task029AgentTaskRunnerManager {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Task029AgentTaskRunnerManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TaskManager for Task029AgentTaskRunnerManager {
    fn task_type(&self) -> &'static str {
        "task029_agent_task_runner"
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
        let config: RunnerConfig =
            serde_json::from_str(&task.config_json).unwrap_or_else(|_| RunnerConfig::default());
        let max_tasks = config.max_tasks.clamp(1, 10);
        let item_timeout = config.item_timeout_seconds.clamp(60, 1800);
        // Порог развёртки обязан превышать лимит на одно поручение, иначе
        // развёртка отбирает записи у живого прогона и запускает их повторно.
        let min_stale = (item_timeout as i64 / 60) + 5;
        let stale_minutes = config.stale_minutes.clamp(10, 240).max(min_stale);

        // Порядок обязателен: развёрнутая запись, у которой кончились попытки,
        // должна быть добита в этом же тике, иначе она целый цикл проживёт
        // невидимым `pending`, не попадающим в выборку.
        let released = a042_agent_task::service::release_stale(stale_minutes).await?;
        if released > 0 {
            logger.write_log(
                session_id,
                &format!("Развёрнуто зависших поручений: {released}"),
            )?;
        }
        let exhausted = a042_agent_task::service::fail_exhausted().await?;
        if exhausted > 0 {
            logger.write_log(
                session_id,
                &format!("Исчерпали попытки и переведены в «Ошибка»: {exhausted}"),
            )?;
        }

        let batch = a042_agent_task::service::list_claimable(max_tasks).await?;
        if batch.is_empty() {
            logger.write_log(session_id, "Очередь поручений пуста — исполнять нечего.")?;
            return Ok(TaskRunOutcome::completed());
        }
        logger.write_log(
            session_id,
            &format!("К исполнению поручений: {}", batch.len()),
        )?;

        let mut done = 0usize;
        let mut failed = 0usize;

        for rec in batch {
            let id = rec.to_string_id();

            // Захват через match, а не `?`: чужой захват — штатная ситуация
            // (ручной прогон параллельно регламентному), а не отказ пачки.
            match a042_agent_task::service::try_claim(&id, session_id).await {
                Ok(true) => {}
                Ok(false) => {
                    logger.write_log(
                        session_id,
                        &format!("{}: захвачено другим прогоном — пропуск", rec.base.code),
                    )?;
                    continue;
                }
                Err(e) => {
                    logger.write_log(
                        session_id,
                        &format!("{}: ошибка захвата: {e}", rec.base.code),
                    )?;
                    continue;
                }
            }

            // Изоляция по записи: одно упавшее поручение не роняет пачку.
            // Ровно этим ломается task015_kb_post, где `?` внутри цикла обрывает
            // публикацию всех оставшихся правок.
            match self.execute_one(&rec, session_id, &logger, item_timeout).await {
                Ok(()) => done += 1,
                Err(ExecError { message, retryable }) => {
                    failed += 1;
                    if let Err(e) = a042_agent_task::service::mark_failed(&id, &message, retryable)
                        .await
                    {
                        logger.write_log(
                            session_id,
                            &format!("{}: не удалось записать провал: {e}", rec.base.code),
                        )?;
                    }
                    logger.write_log(
                        session_id,
                        &format!(
                            "{}: провал ({}): {message}",
                            rec.base.code,
                            if retryable { "повторим" } else { "окончательно" }
                        ),
                    )?;
                }
            }
        }

        logger.write_log(
            session_id,
            &format!("Прогон очереди завершён: выполнено {done}, провалено {failed}"),
        )?;

        // Не `bail!`: он выбросил бы итог прогона и спрятал то, что успело
        // исполниться. Жёлтый статус задачи здесь — верный сигнал.
        if failed > 0 {
            Ok(TaskRunOutcome::completed_with_errors())
        } else {
            Ok(TaskRunOutcome::completed())
        }
    }

    fn get_progress(&self, _session_id: &str) -> Option<TaskProgress> {
        None
    }
}

impl Task029AgentTaskRunnerManager {
    async fn execute_one(
        &self,
        rec: &AgentTask,
        session_id: &str,
        logger: &Arc<TaskLogger>,
        item_timeout: u64,
    ) -> std::result::Result<(), ExecError> {
        let task_id = rec.to_string_id();

        let employee = a017_llm_agent::service::ensure_employee_for(rec.target_agent_type.clone())
            .await
            .map_err(ExecError::fatal)?;
        let employee_id = employee.base.id.as_string();

        // Номер попытки в коде обязателен: `a018_llm_chat.code` — NOT NULL UNIQUE,
        // и без него повтор упал бы на constraint уже ПОСЛЕ захвата, сжигая
        // попытку на ошибке вставки вместо реального прогона.
        let chat_code = format!(
            "AGENT-TASK-{}-{}",
            rec.base.code.trim_start_matches("AT-"),
            rec.attempts
        );

        let chat_id = a018_llm_chat::service::create(
            a018_llm_chat::service::LlmChatDto {
                id: None,
                code: Some(chat_code),
                description: format!("Поручение: {}", rec.base.description),
                comment: Some(format!(
                    "Исполнение поручения {} ({})",
                    rec.base.code,
                    rec.target_agent_type.display_name()
                )),
                agent_id: employee_id.clone(),
                model_name: Some(employee.model_name.clone()),
            },
            // Владелец — человек-заказчик: иначе чат исполнения остаётся ничейным
            // системным и тот, чей вопрос его породил, не найдёт его у себя.
            rec.requested_by_user_ref.clone(),
        )
        .await
        .map_err(ExecError::fatal)?;

        // Ссылку сохраняем ДО прогона: если процесс умрёт в цикле инструментов,
        // диалог найдётся по самой записи, а не только по логу сессии.
        a042_agent_task::service::attach_execution_chat(
            &task_id,
            &chat_id.to_string(),
            &employee_id,
        )
        .await
        .map_err(ExecError::retry)?;

        let response = tokio::time::timeout(
            Duration::from_secs(item_timeout),
            a018_llm_chat::service::send_message(
                &chat_id.to_string(),
                a018_llm_chat::service::SendMessageRequest {
                    content: build_prompt(rec),
                    model_name: Some(employee.model_name.clone()),
                    attachment_ids: Vec::new(),
                    request_id: None,
                },
                None, // прогресс не отслеживаем
                None, // фоновая задача: собеседника-человека нет
            ),
        )
        .await
        .map_err(|_| ExecError::retry(format!("превышен лимит исполнения {item_timeout} с")))?
        .map_err(ExecError::retry)?;

        let text = response.content.trim().to_string();
        if text.is_empty() {
            return Err(ExecError::retry("исполнитель вернул пустой ответ"));
        }

        a042_agent_task::service::mark_done(
            &task_id,
            text,
            Some(response.id.to_string()),
            response.artifact_id.map(|a| a.as_string()),
        )
        .await
        .map_err(ExecError::retry)?;

        logger
            .write_log(
                session_id,
                &format!(
                    "{}: исполнено ({}), чат {}",
                    rec.base.code,
                    rec.target_agent_type.display_name(),
                    chat_id
                ),
            )
            .map_err(ExecError::retry)?;
        Ok(())
    }
}

/// Собрать задание для исполнителя.
///
/// Служебный маркер режима поднимает навык специализации детерминированно —
/// иначе роутер классифицировал бы чужую формулировку по ключевым словам и мог
/// увести поручение в посторонний навык.
fn build_prompt(rec: &AgentTask) -> String {
    let mut prompt = String::new();

    if let Some(intent) = router::default_intent_for_agent_type(&rec.target_agent_type) {
        prompt.push_str(&format!("Режим: {intent}.\n"));
    }

    prompt.push_str(
        "Это поручение от другого AI-сотрудника, а не от человека. Диалог односторонний: \
         уточняющих вопросов задавать некому и второго хода не будет. Если данных не хватает — \
         так и напиши, чего именно недостаёт, и не выдумывай недостающее.\n\n",
    );

    prompt.push_str(&format!(
        "Задание ({}): {}\n\n{}\n",
        rec.base.code,
        rec.base.description,
        rec.request_text.trim()
    ));

    if let Some(context) = rec.payload_json.as_deref().filter(|c| !c.trim().is_empty()) {
        prompt.push_str(&format!(
            "\nСтруктурированный контекст:\n```json\n{context}\n```\n"
        ));
    }

    prompt.push_str(
        "\nВ конце дай ЗАКОНЧЕННЫЙ ответ по существу: он целиком уйдёт заказчику как результат \
         поручения. Не ссылайся на «см. выше», не обещай продолжить позже и не пересказывай \
         ход работы вместо вывода.\n",
    );

    prompt
}
