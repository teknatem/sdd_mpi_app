//! Экземпляр процесса — один прогон по графу со своим долговечным состоянием.
//!
//! Первокласснен по ADR-0011 п.2: у него курсор, попытки и ожидания, и он
//! переживает перезапуск сервера. Этим он отличается от прогона регламентного
//! задания, который живёт ровно один запуск.
//!
//! Три вещи здесь стоит прочитать внимательно, потому что от них зависит
//! корректность эффектов:
//!
//! - **`visit`** — номер захода в Этап. Он входит в ключ идемпотентности
//!   четвёртой частью, и без него цикл в графе (st0004 → st0001) на втором
//!   заходе вернул бы `replayed` и не сделал бы ничего — ровно того, ради чего
//!   в цикл и возвращались.
//! - **`claim_session_id`** — аренда. Пока экземпляр арендован, второй воркер
//!   его не возьмёт; поэтому незавершённая запись в журнале эффектов означает
//!   «кто-то умер», а не «кто-то работает прямо сейчас».
//! - **`wait`** — ожидание как состояние экземпляра, а не отдельная подсистема
//!   (п.9, п.13): инбокс есть список экземпляров в этом состоянии.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{CorrelationKey, EdgeTarget};

/// Где экземпляр находится.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    /// Есть что исполнять: курсор стоит на Этапе.
    Running,
    /// Ждёт доменное событие или дедлайн.
    Waiting,
    /// Дошёл до терминала.
    Done,
    /// Дефект Этапа или исчерпанные попытки: дальше нужен человек. Повтор
    /// бессмыслен до правки кода, поэтому воркер такой экземпляр не трогает.
    Quarantined,
}

impl InstanceStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Done => "done",
            Self::Quarantined => "quarantined",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "running" => Self::Running,
            "waiting" => Self::Waiting,
            "done" => Self::Done,
            _ => Self::Quarantined,
        }
    }

    /// Жив ли экземпляр — то есть занимает ли он ключ корреляции. Живой на
    /// процесс и ключ ровно один: повторное событие про тот же день не должно
    /// заводить второй прогон.
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Running | Self::Waiting)
    }
}

/// Чего ждёт экземпляр.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceWait {
    /// Имя события из каталога.
    pub event: String,
    /// Канонический токен ключа корреляции: событие сводится с ожиданием по
    /// равенству токенов, а не по похожести.
    pub token: String,
    /// События с номером не больше этого уже были в момент постановки в
    /// ожидание и не считаются: иначе экземпляр проснулся бы от собственного
    /// прошлого.
    pub since_seq: i64,
    /// Дедлайн эскалации. Ожидание без дедлайна запрещено (ADR-0011 п.9).
    pub deadline_at: String,
    /// Куда идти по дедлайну. `None` — эскалация: экземпляр остаётся человеку.
    #[serde(default)]
    pub on_timeout: Option<EdgeTarget>,
}

/// Строка состояния одного прогона.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessInstance {
    pub id: String,
    pub process_code: String,
    /// Версия Процесса, на которой экземпляр стартовал и доживёт (ADR-0011
    /// п.7). Версии Этапов приходят вместе с ней — они запинены активацией.
    pub process_version: i32,
    pub correlation: CorrelationKey,
    pub correlation_token: String,
    pub status: InstanceStatus,
    /// Курсор: Этап, который исполняется следующим. `None` у завершённого.
    #[serde(default)]
    pub stage_code: Option<String>,
    /// Номер захода в текущий Этап, начиная с 1.
    pub visit: i32,
    /// Вход текущего Этапа: ключ корреляции плюс данные выхода предыдущего.
    #[serde(default)]
    pub input: Value,
    /// Сколько раз текущий Этап падал временным сбоем.
    pub attempts: i32,
    #[serde(default)]
    pub next_attempt_at: Option<String>,
    #[serde(default)]
    pub wait: Option<InstanceWait>,
    #[serde(default)]
    pub last_outcome: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub claim_session_id: Option<String>,
    pub started_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub finished_at: Option<String>,
}

/// Один отработавший шаг экземпляра — строка журнала шагов.
///
/// Отвечает на вопрос «как мы сюда попали», на который состояние экземпляра
/// ответить не может: оно знает только последний выход. Журнал эффектов тоже не
/// заменяет: там записаны изменения мира, а Этап без эффектов — обычное дело,
/// хотя его выход и определил маршрут.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceStep {
    pub id: String,
    pub instance_ref: String,
    pub stage_code: String,
    pub visit: i32,
    /// Класс исхода: `outcome` | `temporary_failure` | `defect`.
    pub verdict: String,
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub data: Option<Value>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub logs: Vec<String>,
    #[serde(default)]
    pub effect_ids: Vec<String>,
    pub duration_ms: i64,
    pub created_at: String,
}

/// Экземпляр целиком — для экрана разбора: где стоим, как сюда пришли, что
/// изменили.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceDetails {
    pub instance: ProcessInstance,
    pub steps: Vec<InstanceStep>,
    pub effects: Vec<super::EffectRecord>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrips_and_knows_who_is_alive() {
        for status in [
            InstanceStatus::Running,
            InstanceStatus::Waiting,
            InstanceStatus::Done,
            InstanceStatus::Quarantined,
        ] {
            assert_eq!(InstanceStatus::from_str(status.as_str()), status);
        }
        assert!(InstanceStatus::Running.is_live());
        assert!(InstanceStatus::Waiting.is_live());
        assert!(!InstanceStatus::Done.is_live());
        assert!(!InstanceStatus::Quarantined.is_live());
    }

    /// Порча значения в колонке не должна превращать карантин в «running»:
    /// худшее, что может случиться, — воркер снова возьмёт сломанный прогон и
    /// повторит эффект.
    #[test]
    fn unknown_status_falls_back_to_quarantine() {
        assert_eq!(
            InstanceStatus::from_str("мусор"),
            InstanceStatus::Quarantined
        );
    }
}
