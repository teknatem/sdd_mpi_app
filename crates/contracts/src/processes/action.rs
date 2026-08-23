//! Действие — операция ядра с побочным эффектом.
//!
//! Действие живёт в Rust и адресуется именем; кода вида `pr0001`/`st0001` у него
//! нет, потому что оно не хранится в БД и не версионируется вместе с
//! определениями (ADR-0011 п.14). Наружу — в mjs Этапа — оно попадает как
//! `host.actions.<camelCaseName>`, а право на вызов выдаётся `capability`
//! `action:<name>`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Режим исполнения. Оба режима обязаны быть у каждого Действия с первого дня:
/// допуск процесса в работу — это просмотр плана эффектов человеком (ADR-0011
/// п.8), а план получается только сухим прогоном.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionMode {
    /// Исполнить по-настоящему.
    Execute,
    /// Записать намерение и вернуть план, ничего не меняя.
    DryRun,
}

impl ActionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Execute => "execute",
            Self::DryRun => "dry_run",
        }
    }
}

/// Кто инициировал эффект. Журнал должен отвечать на «кто это сделал» и тогда,
/// когда процессов ещё нет, поэтому вариант `Manual` — не заглушка, а штатный
/// путь для ручного вызова из UI или теста.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionActor {
    /// Экземпляр процесса на конкретном Этапе.
    Process {
        instance_id: String,
        stage_code: String,
    },
    /// Человек через интерфейс — в том числе через LLM-чат.
    ///
    /// Провенанс здесь шире одного `user_id` ровно по той же причине, по которой
    /// `Process` несёт экземпляр и Этап: у эффекта бывает адресат, и обратная
    /// ссылка обязана пережить вызов. Поручение, поставленное из чата, должно
    /// помнить диалог, агента и своё место в цепочке делегирования — иначе
    /// результат некуда вернуть.
    ///
    /// Это провенанс, а не вход Действия: схема входа остаётся одинаковой для
    /// обеих оболочек, и `create_agent_task` не приобретает полей «для чата».
    User {
        user_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chat_ref: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_ref: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_task_ref: Option<String>,
        #[serde(default)]
        depth: i32,
    },
    /// Ручной вызов без пользовательской сессии.
    Manual,
}

impl ActionActor {
    /// Представление для колонки `actor` журнала.
    pub fn as_token(&self) -> String {
        match self {
            Self::Process { instance_id, .. } => format!("process:{instance_id}"),
            Self::User { user_id, .. } => format!("user:{user_id}"),
            Self::Manual => "manual".to_string(),
        }
    }

    pub fn instance_id(&self) -> Option<&str> {
        match self {
            Self::Process { instance_id, .. } => Some(instance_id),
            _ => None,
        }
    }

    pub fn stage_code(&self) -> Option<&str> {
        match self {
            Self::Process { stage_code, .. } => Some(stage_code),
            _ => None,
        }
    }

    pub fn user_id(&self) -> Option<&str> {
        match self {
            Self::User { user_id, .. } => Some(user_id),
            _ => None,
        }
    }

    pub fn chat_ref(&self) -> Option<&str> {
        match self {
            Self::User { chat_ref, .. } => chat_ref.as_deref(),
            _ => None,
        }
    }

    pub fn agent_ref(&self) -> Option<&str> {
        match self {
            Self::User { agent_ref, .. } => agent_ref.as_deref(),
            _ => None,
        }
    }

    pub fn parent_task_ref(&self) -> Option<&str> {
        match self {
            Self::User {
                parent_task_ref, ..
            } => parent_task_ref.as_deref(),
            _ => None,
        }
    }

    /// Глубина в цепочке делегирования. У Процесса и ручного вызова цепочки нет.
    pub fn depth(&self) -> i32 {
        match self {
            Self::User { depth, .. } => *depth,
            _ => 0,
        }
    }
}

/// Один вызов Действия.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionCall {
    /// Имя Действия: `repost_documents`, `post_document`, …
    pub action: String,
    /// Вход, проверяемый по `ActionInfo::input_schema`.
    pub input: Value,
    /// Смысловой ключ идемпотентности, а не случайный: он строится из того, что
    /// делает эффект уникальным, и отвечает на вопрос «это уже делали?».
    pub idempotency_key: String,
    pub mode: ActionMode,
    pub actor: ActionActor,
}

/// Состояние записи журнала эффектов.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectStatus {
    /// Сухой прогон: эффекта не было, в `result` — план.
    Planned,
    /// Исполнение начато и не завершилось. После перезапуска это неизвестность,
    /// а не повод повторить: повтор уводит экземпляр в карантин (ADR-0011 п.10).
    InProgress,
    /// Эффект состоялся.
    Executed,
    /// Исполнение упало; повтор с тем же ключом разрешён.
    Failed,
}

impl EffectStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::InProgress => "in_progress",
            Self::Executed => "executed",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "planned" => Self::Planned,
            "executed" => Self::Executed,
            "failed" => Self::Failed,
            _ => Self::InProgress,
        }
    }
}

/// Строка журнала эффектов.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectRecord {
    pub id: String,
    pub idempotency_key: String,
    pub action_name: String,
    pub mode: ActionMode,
    pub status: EffectStatus,
    pub input: Value,
    pub result: Option<Value>,
    pub error_text: Option<String>,
    pub actor: String,
    pub process_instance_ref: Option<String>,
    pub stage_code: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
}

/// Итог вызова Действия.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionOutcome {
    pub effect_id: String,
    pub status: EffectStatus,
    /// Результат исполнения либо план сухого прогона.
    pub result: Value,
    /// Истина, если вернули записанный результат прошлого вызова, а не
    /// исполняли заново. Для вызывающего это успех, но знать разницу он должен.
    pub replayed: bool,
    pub duration_ms: i64,
}

/// Паспорт Действия для каталога и для гейта прав.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionInfo {
    pub name: &'static str,
    /// Как называется в mjs: `host.actions.<method>`.
    pub method: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// Capability, без которой Этап не может это вызвать: `action:<name>`.
    pub capability: &'static str,
    /// Обратимо ли Действие отдельным обратным эффектом. Влияет на то, что
    /// человек видит в плане перед допуском процесса в работу.
    pub reversible: bool,
    /// Таблицы, в которые Действие пишет.
    ///
    /// Тот же смысл, что у `TaskMetadata::write_tables`, и та же цель:
    /// координатор ресурсов не даёт двум писателям одной таблицы работать
    /// одновременно. Список объявляется здесь, а не выводится из кода, потому
    /// что Действие зовёт доменный сервис, а тот — ещё несколько.
    ///
    /// Читается только из каталога в Rust, поэтому при разборе JSON поле
    /// пропускается: паспорт Действия наружу отдаётся, но обратно не
    /// принимается — иначе список писателей задавал бы вызывающий.
    #[serde(default, skip_deserializing)]
    pub write_tables: &'static [&'static str],
    /// JSON Schema входа.
    pub input_schema: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_token_carries_subject() {
        assert_eq!(ActionActor::Manual.as_token(), "manual");
        let user = ActionActor::User {
            user_id: "u1".into(),
            chat_ref: Some("chat-1".into()),
            agent_ref: Some("agent-1".into()),
            parent_task_ref: None,
            depth: 1,
        };
        // Токен журнала называет субъекта, а не весь провенанс: остальное
        // читается акцессорами, иначе колонка `actor` перестанет группироваться.
        assert_eq!(user.as_token(), "user:u1");
        assert_eq!(user.chat_ref(), Some("chat-1"));
        assert_eq!(user.depth(), 1);
        assert_eq!(user.parent_task_ref(), None);
        let process = ActionActor::Process {
            instance_id: "i1".into(),
            stage_code: "st0001".into(),
        };
        assert_eq!(process.as_token(), "process:i1");
        assert_eq!(process.stage_code(), Some("st0001"));
    }

    #[test]
    fn effect_status_roundtrips_through_column() {
        for status in [
            EffectStatus::Planned,
            EffectStatus::InProgress,
            EffectStatus::Executed,
            EffectStatus::Failed,
        ] {
            assert_eq!(EffectStatus::from_str(status.as_str()), status);
        }
    }

    /// Неизвестное значение колонки не должно молча превратиться в «исполнено»:
    /// худшее, что может случиться при порче данных, — повторный эффект.
    #[test]
    fn unknown_status_falls_back_to_in_progress() {
        assert_eq!(EffectStatus::from_str("мусор"), EffectStatus::InProgress);
    }
}
