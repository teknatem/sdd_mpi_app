//! Режим технического обслуживания: приложение закрыто для всех, кроме админов.
//!
//! Нужен там, где работа пользователей и операция администратора несовместимы.
//! Главный случай — восстановление базы данных: подмена файла происходит при
//! следующем запуске, и всё, что люди успеют записать между подготовкой и
//! перезапуском, уедет в архив вместе со старой базой. Молча потерять эти
//! записи хуже, чем на несколько минут закрыть вход.

use serde::{Deserialize, Serialize};

/// Кто включил режим. Различие важно для UI: автоматический режим снимется сам
/// (или после перезапуска), ручной ждёт админа.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceTrigger {
    /// Включён администратором вручную.
    Manual,
    /// Включён операцией переноса, затрагивающей базу данных.
    Automatic,
}

impl MaintenanceTrigger {
    pub fn label_ru(&self) -> &'static str {
        match self {
            Self::Manual => "включён вручную",
            Self::Automatic => "включён операцией переноса",
        }
    }
}

/// Состояние режима. Отдаётся без авторизации: страницу-заглушку надо показать
/// и тому, кто ещё не вошёл.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MaintenanceStatusDto {
    pub active: bool,
    /// Что именно происходит — показывается пользователю как есть.
    pub reason: Option<String>,
    pub trigger: Option<MaintenanceTrigger>,
    /// RFC 3339, момент включения.
    pub since: Option<String>,
    /// `user:<id> (<login>)` либо `auto:<операция>`.
    pub started_by: Option<String>,
    /// Требуется ли перезапуск бэкенда, чтобы работа продолжилась (подготовлена
    /// подмена базы). UI показывает это отдельной строкой.
    pub requires_restart: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetMaintenanceRequest {
    pub reason: Option<String>,
}

/// Машинный признак в теле 503: отличает «закрыто на работы» от любого другого
/// временного отказа.
pub const MAINTENANCE_ERROR_CODE: &str = "maintenance";

/// Тело ответа 503, которым приложение отказывает из-за режима обслуживания.
/// Одно на всех отказывающих: и гейт, и обработчик входа отдают именно его —
/// иначе клиент показывает человеку голый код статуса вместо причины.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceUnavailableDto {
    /// Всегда [`MAINTENANCE_ERROR_CODE`].
    pub error: String,
    /// Всегда `true`. Оставлено ради клиентов, которые проверяют флаг, а не код.
    pub maintenance: bool,
    pub reason: Option<String>,
    /// RFC 3339, момент включения.
    pub since: Option<String>,
    pub requires_restart: bool,
}

impl From<MaintenanceStatusDto> for MaintenanceUnavailableDto {
    fn from(status: MaintenanceStatusDto) -> Self {
        Self {
            error: MAINTENANCE_ERROR_CODE.to_string(),
            maintenance: true,
            reason: status.reason,
            since: status.since,
            requires_restart: status.requires_restart,
        }
    }
}

/// Причина, которую администратор написал сам.
///
/// `None` — не написал ничего содержательного. Подставлять вместо неё
/// [`DEFAULT_MAINTENANCE_REASON`] нельзя: сообщения начинаются с этой же фразы,
/// и получается «Идут технические работы: Идут технические работы». Точку в
/// конце снимаем — её ставит шаблон сообщения.
pub fn custom_reason(reason: Option<&str>) -> Option<&str> {
    reason
        .map(str::trim)
        .filter(|reason| !reason.is_empty() && *reason != DEFAULT_MAINTENANCE_REASON)
        .map(|reason| reason.trim_end_matches(['.', ' ']))
}

impl MaintenanceUnavailableDto {
    /// См. [`custom_reason`].
    pub fn custom_reason(&self) -> Option<&str> {
        custom_reason(self.reason.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_empty_reasons_are_not_shown_twice() {
        // Всё это уже сказано началом сообщения — повторять нечего.
        assert_eq!(custom_reason(None), None);
        assert_eq!(custom_reason(Some("   ")), None);
        assert_eq!(custom_reason(Some(DEFAULT_MAINTENANCE_REASON)), None);
    }

    #[test]
    fn authored_reason_survives_without_trailing_period() {
        assert_eq!(
            custom_reason(Some("  перенос базы на новый сервер.  ")),
            Some("перенос базы на новый сервер")
        );
    }
}

/// Текст по умолчанию, если админ не указал причину.
pub const DEFAULT_MAINTENANCE_REASON: &str = "Идут технические работы";
