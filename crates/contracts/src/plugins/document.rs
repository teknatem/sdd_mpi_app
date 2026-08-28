//! Редактируемый документ плагина: поле с хранимыми данными, которое плагин
//! читает и пишет обратно.
//!
//! Появилось ради редактора графов (ReactFlow), но специфики графа здесь нет:
//! содержимое — произвольный JSON, а смысл поля знает только его владелец.
//!
//! **Куда писать, решает хост, а не плагин.** [`PluginDocumentTarget`] приходит
//! в `PluginFrame` отдельным сигналом и в аргументах `host.saveDocument` не
//! участвует: плагин передаёт только содержимое. Иначе скрипт в iframe выбирал
//! бы себе цель записи сам, а весь смысл посредничества родителя — в том, что
//! право на запись держит хост.
//!
//! Плагину цель видна (её проекция кладётся в `context` при `plugin_init`) —
//! чтобы показать, что именно редактируется, — но видимость не есть право.

use serde::{Deserialize, Serialize};

/// Адрес редактируемого поля.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDocumentTarget {
    /// Тип владельца поля (например, код Процесса).
    pub doc_type: String,
    pub doc_id: String,
    /// Имя поля с хранимыми данными.
    pub field: String,
}

/// Что хост держит про открытый документ: адрес плюс версия, которую он видел
/// последней.
///
/// Версия живёт здесь, а не у плагина, потому что она — предмет оптимистичной
/// блокировки: плагин может её показать, но подставить в запрос обязан хост.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDocumentBinding {
    pub target: PluginDocumentTarget,
    /// `None` — документ ещё не читался в этой сессии.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
}

impl PluginDocumentBinding {
    pub fn new(target: PluginDocumentTarget) -> Self {
        Self {
            target,
            version: None,
        }
    }
}

/// Ответ на чтение документа.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDocumentResponse {
    #[serde(default)]
    pub content: serde_json::Value,
    pub version: i64,
}

/// Запрос на запись.
///
/// `expected_version` — версия, от которой плагин отталкивался. Сервер обязан
/// отклонить запись (409), если поле успело уйти вперёд: иначе редактор молча
/// затрёт чужие правки, а с автогенерацией схем на сервере это не краевой
/// случай, а норма.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDocumentSaveRequest {
    pub target: PluginDocumentTarget,
    pub content: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<i64>,
}

/// Результат записи.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDocumentSaveResponse {
    /// Версия после записи — её хост запоминает, чтобы следующее сохранение
    /// прошло без перезагрузки страницы.
    pub version: i64,
}
