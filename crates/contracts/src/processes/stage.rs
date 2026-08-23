//! Этап — единица исполнения Процесса.
//!
//! Устроен как quality-проверка: манифест плюс mjs-модуль, исполняемый в
//! QuickJS с ограниченным `host`. Отличий от проверки ровно два, и оба
//! существенные:
//!
//! 1. Этап возвращает **один из заранее объявленных выходов**, а не свободный
//!    JSON: по имени выхода Процесс выбирает следующий Этап, поэтому множество
//!    выходов — часть контракта, а не деталь реализации.
//! 2. Этапу разрешено вызывать **Действия**, то есть менять мир.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ActionMode;

/// Один именованный выход Этапа.
///
/// Имена доменные («сходится», «расхождение»), а не технические: по ним читается
/// граф Процесса, и они же попадают человеку в разбор прогона.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageOutput {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// JSON Schema поля `data` для этого выхода. `None` — данные не описаны и
    /// не проверяются; описанная схема проверяется в рантайме (ADR-0011 п.11).
    #[serde(default)]
    pub data_schema: Option<Value>,
}

/// Паспорт Этапа.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageManifest {
    /// Код вида `st0001` — идентичность Этапа, стабильная между версиями.
    pub code: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_entrypoint")]
    pub entrypoint: String,
    #[serde(default = "default_export")]
    pub export: String,
    /// JSON Schema входа.
    #[serde(default)]
    pub input_schema: Option<Value>,
    /// Объявленные выходы. Пустой список запрещён: Этап без выходов не может
    /// стоять в графе.
    pub outputs: Vec<StageOutput>,
    /// Права: `db:read:<table>` на чтение и `action:<name>` на эффекты.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

fn default_entrypoint() -> String {
    "stage.mjs".to_string()
}

fn default_export() -> String {
    "run".to_string()
}

impl StageManifest {
    /// Есть ли такой выход среди объявленных.
    pub fn output(&self, name: &str) -> Option<&StageOutput> {
        self.outputs.iter().find(|output| output.name == name)
    }

    /// Имена выходов — для сообщений об ошибке и для проверки рёбер графа.
    pub fn output_names(&self) -> Vec<&str> {
        self.outputs.iter().map(|o| o.name.as_str()).collect()
    }
}

/// Определение Этапа: паспорт плюс код.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageDefinition {
    pub manifest: StageManifest,
    pub script: String,
    /// SHA-256 манифеста и кода. Версия Этапа пинится экземпляром процесса,
    /// поэтому «тот же код» должен опознаваться дёшево и однозначно.
    #[serde(default)]
    pub digest: String,
}

/// Условия прогона Этапа.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageRunContext {
    /// Экземпляр процесса, которому принадлежит прогон. `None` — Этап запущен
    /// отдельно: на просмотр плана или из теста.
    #[serde(default)]
    pub instance_id: Option<String>,
    /// Номер захода в этот Этап у этого экземпляра.
    ///
    /// Входит в ключ идемпотентности, и это не украшение: в графе бывают циклы
    /// (в пилоте st0004 → st0001), и без номера захода второй проход вернул бы
    /// `replayed` — то есть не сделал бы ровно того, ради чего возвращались.
    /// Повтор одного и того же захода после сбоя номер не меняет, поэтому
    /// защита от двойного эффекта остаётся.
    #[serde(default)]
    pub visit: i32,
    /// Режим для **всех** Действий этого прогона. Сухой прогон Этапа не может
    /// исполнить настоящий эффект: режим задаётся снаружи и внутрь не
    /// прокидывается — иначе автор mjs мог бы его переопределить.
    pub mode: ActionMode,
}

impl StageRunContext {
    pub fn manual(mode: ActionMode) -> Self {
        Self {
            instance_id: None,
            visit: 0,
            mode,
        }
    }

    /// Прогон в составе экземпляра.
    pub fn for_instance(instance_id: impl Into<String>, visit: i32, mode: ActionMode) -> Self {
        Self {
            instance_id: Some(instance_id.into()),
            visit,
            mode,
        }
    }
}

/// Бизнес-исход Этапа: имя выхода и его данные.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageOutcome {
    pub outcome: String,
    #[serde(default)]
    pub data: Value,
}

/// Чем закончился прогон Этапа.
///
/// Три класса разделены механизмом, а не соглашением (ADR-0011 п.10). Единый
/// штатный выход `error` отклонён намеренно: он позволил бы автору Этапа
/// замаскировать дефект под штатную ветку, и тихая неправильность заменила бы
/// громкое падение.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StageVerdict {
    /// Штатный исход: именованный выход графа.
    Outcome(StageOutcome),
    /// Временный сбой: упало Действие, движок повторит Этап.
    TemporaryFailure { message: String },
    /// Дефект Этапа: исключение в mjs или выход не по контракту. Экземпляр
    /// уходит в карантин, повтор бессмысленен до правки кода.
    Defect { message: String },
}

impl StageVerdict {
    pub fn outcome_name(&self) -> Option<&str> {
        match self {
            Self::Outcome(outcome) => Some(&outcome.outcome),
            _ => None,
        }
    }

    pub fn is_outcome(&self) -> bool {
        matches!(self, Self::Outcome(_))
    }
}

/// Результат прогона вместе с тем, что Этап написал в лог.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageRun {
    pub stage_code: String,
    pub verdict: StageVerdict,
    #[serde(default)]
    pub logs: Vec<String>,
    pub duration_ms: i64,
    /// Идентификаторы записей журнала эффектов, созданных этим прогоном.
    #[serde(default)]
    pub effect_ids: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest() -> StageManifest {
        StageManifest {
            code: "st0002".into(),
            title: "Сверить с ГК".into(),
            description: String::new(),
            entrypoint: default_entrypoint(),
            export: default_export(),
            input_schema: None,
            outputs: vec![
                StageOutput {
                    name: "сходится".into(),
                    description: String::new(),
                    data_schema: None,
                },
                StageOutput {
                    name: "расхождение".into(),
                    description: String::new(),
                    data_schema: Some(json!({"type": "object"})),
                },
            ],
            capabilities: vec![],
        }
    }

    #[test]
    fn outputs_are_addressable_by_name() {
        let manifest = manifest();
        assert!(manifest.output("сходится").is_some());
        assert!(manifest.output("почти сходится").is_none());
        assert_eq!(manifest.output_names(), vec!["сходится", "расхождение"]);
    }

    /// Вердикт сериализуется тегированно: разбор прогона в UI различает три
    /// класса исхода, и они не должны схлопываться в «строку с текстом».
    #[test]
    fn verdict_keeps_its_class_through_serde() {
        let defect = StageVerdict::Defect {
            message: "выход не объявлен".into(),
        };
        let raw = serde_json::to_string(&defect).unwrap();
        assert!(raw.contains("\"kind\":\"defect\""), "{raw}");
        let back: StageVerdict = serde_json::from_str(&raw).unwrap();
        assert_eq!(back, defect);
        assert!(!back.is_outcome());
    }
}
