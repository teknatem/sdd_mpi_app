//! Процесс — граф Этапов с триггером и версией.
//!
//! Единственная по-настоящему новая сущность механизма: у quality-проверок и у
//! регламентных заданий графа нет, поэтому «после какого исхода куда идём»
//! сегодня нигде не записано. Здесь оно записано данными.
//!
//! Граф намеренно плоский — список рёбер `(Этап, выход) → цель`, а не дерево.
//! Причина в том, что читать его будет не только человек: экземпляр процесса
//! двигается по одному ребру за раз, и поиск следующего шага обязан быть
//! прямым обращением, а не обходом структуры.
//!
//! Этапы в графе адресуются **кодом, а не версией**: конкретные версии
//! фиксируются в момент активации версии Процесса (ADR-0011 п.7, уточнение Б3)
//! и хранятся рядом с ней. Поэтому граф переживает правку Этапа, а работающий
//! Процесс не меняет поведение молча.

use serde::{Deserialize, Serialize};

/// Триггер Процесса: доменное событие из каталога.
///
/// Ключа корреляции здесь нет намеренно — он берётся из каталога событий
/// (`DomainEventKind::correlation_fields`). Ключ отвечает на вопрос «про что
/// этот факт» и потому является свойством факта, а не подписки: объяви его
/// подписчик, два Процесса разошлись бы в том, что считать «тем же самым днём».
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessTrigger {
    /// Имя доменного события: `import.day.completed`. Каталог событий —
    /// типизированный и в Rust (ADR-0011 п.5); здесь имя, а не описание.
    pub event: String,
}

impl ProcessTrigger {
    pub fn on(event: impl Into<String>) -> Self {
        Self {
            event: event.into(),
        }
    }
}

/// Куда ведёт ребро графа.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EdgeTarget {
    /// Следующий Этап по коду.
    Stage { code: String },
    /// Экземпляр доработал. Терминал объявляется явно, а не выводится из
    /// отсутствия ребра: «выход никуда не ведёт» и «выход завершает процесс» —
    /// разные вещи, и первое почти всегда забытое ребро.
    Done,
}

impl EdgeTarget {
    pub fn stage(code: impl Into<String>) -> Self {
        Self::Stage { code: code.into() }
    }

    pub fn stage_code(&self) -> Option<&str> {
        match self {
            Self::Stage { code } => Some(code),
            Self::Done => None,
        }
    }
}

/// Ожидание доменного события перед переходом по ребру.
///
/// Ожидание — состояние экземпляра, а не отдельная подсистема (ADR-0011 п.9,
/// п.13): инбокс есть список экземпляров, ждущих человека. Условие пробуждения
/// — только событие с ключом корреляции; перепроверка произвольного SQL
/// отклонена ADR-ом, потому что она растаскивает определение фактов из ядра по
/// Этапам.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitSpec {
    /// Событие, которого ждём: `human.action.done`.
    pub event: String,
    /// Дедлайн в минутах. Ожидание без дедлайна запрещено: экземпляр, которого
    /// никто не разбудит, — это тихо потерянная работа.
    pub deadline_minutes: i64,
    /// Куда идти по дедлайну. `None` — эскалация: экземпляр остаётся человеку,
    /// а не уходит по графу дальше сам.
    #[serde(default)]
    pub on_timeout: Option<EdgeTarget>,
}

/// Одно ребро графа: «после выхода `outcome` Этапа `from` идём в `to`».
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessEdge {
    pub from: String,
    pub outcome: String,
    pub to: EdgeTarget,
    /// Ожидание перед переходом. `None` — переход немедленный.
    #[serde(default)]
    pub wait: Option<WaitSpec>,
}

/// Паспорт Процесса.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessManifest {
    /// Код вида `pr0001` — идентичность Процесса, стабильная между версиями.
    pub code: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub trigger: ProcessTrigger,
    /// Код Этапа, с которого начинается экземпляр.
    pub entry: String,
    pub edges: Vec<ProcessEdge>,
    /// Код парной quality-проверки. Критичный Процесс без неё не активируется
    /// (ADR-0011 п.4): пока ситуация не нормализована, она обязана быть видна
    /// как нарушение, а не только как экземпляр в середине графа.
    #[serde(default)]
    pub quality_check: Option<String>,
}

impl ProcessManifest {
    /// Коды Этапов, участвующих в графе: вход плюс все цели рёбер и все
    /// источники. Источники учитываются намеренно — ребро от Этапа, до которого
    /// нельзя дойти, это дефект графа, и находить его должен валидатор, а не
    /// рантайм.
    pub fn stage_codes(&self) -> Vec<String> {
        let mut codes = vec![self.entry.clone()];
        for edge in &self.edges {
            for code in [Some(edge.from.as_str()), edge.to.stage_code()]
                .into_iter()
                .flatten()
            {
                if !codes.iter().any(|known| known == code) {
                    codes.push(code.to_string());
                }
            }
            if let Some(code) = edge
                .wait
                .as_ref()
                .and_then(|wait| wait.on_timeout.as_ref())
                .and_then(EdgeTarget::stage_code)
            {
                if !codes.iter().any(|known| known == code) {
                    codes.push(code.to_string());
                }
            }
        }
        codes
    }

    /// Ребро, по которому уходит конкретный выход конкретного Этапа.
    pub fn edge(&self, from: &str, outcome: &str) -> Option<&ProcessEdge> {
        self.edges
            .iter()
            .find(|edge| edge.from == from && edge.outcome == outcome)
    }
}

/// Определение Процесса. Отдельный тип от манифеста — ради симметрии с Этапом
/// и ради отпечатка: экземпляр пинит версию, и «то же самое определение»
/// должно опознаваться сравнением строки.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessDefinition {
    pub manifest: ProcessManifest,
    /// SHA-256 манифеста.
    #[serde(default)]
    pub digest: String,
}

/// Насколько Процесс опасен — выводится из того, что запрашивают его Этапы, а
/// не декларируется автором (ADR-0011 п.4): субъект гейта не может выставлять
/// себе оценку.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessCriticality {
    /// Ни один Этап не просит Действий: Процесс только читает и решает.
    ReadOnly,
    /// Есть эффекты, но все обратимы отдельным обратным эффектом.
    Effectful,
    /// Есть необратимое Действие. Проведение документа не отменяется тем, что
    /// его «не собирались проводить».
    Irreversible,
}

impl ProcessCriticality {
    /// Требуется ли парная quality-проверка для активации.
    ///
    /// Порог стоит на первом же эффекте, а не на необратимости: рассогласование
    /// (ADR-0011 п.3) остаётся видимым нарушением независимо от того, можно ли
    /// эффект откатить.
    pub fn needs_quality_check(&self) -> bool {
        !matches!(self, Self::ReadOnly)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::Effectful => "effectful",
            Self::Irreversible => "irreversible",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> ProcessManifest {
        ProcessManifest {
            code: "pr0001".into(),
            title: "Закрытие дня WB".into(),
            description: String::new(),
            trigger: ProcessTrigger::on("import.day.completed"),
            entry: "st0001".into(),
            edges: vec![
                ProcessEdge {
                    from: "st0001".into(),
                    outcome: "пересчитан".into(),
                    to: EdgeTarget::stage("st0002"),
                    wait: None,
                },
                ProcessEdge {
                    from: "st0002".into(),
                    outcome: "сходится".into(),
                    to: EdgeTarget::Done,
                    wait: None,
                },
                ProcessEdge {
                    from: "st0002".into(),
                    outcome: "расхождение".into(),
                    to: EdgeTarget::stage("st0004"),
                    wait: None,
                },
                ProcessEdge {
                    from: "st0004".into(),
                    outcome: "позвали".into(),
                    to: EdgeTarget::stage("st0001"),
                    wait: Some(WaitSpec {
                        event: "human.action.done".into(),
                        deadline_minutes: 24 * 60,
                        on_timeout: None,
                    }),
                },
            ],
            quality_check: Some("wb_day_not_closed".into()),
        }
    }

    #[test]
    fn stage_codes_cover_the_whole_graph() {
        let codes = manifest().stage_codes();
        assert_eq!(codes, vec!["st0001", "st0002", "st0004"]);
    }

    #[test]
    fn edge_is_addressable_by_stage_and_outcome() {
        let manifest = manifest();
        let edge = manifest
            .edge("st0002", "расхождение")
            .expect("ребро объявлено");
        assert_eq!(edge.to.stage_code(), Some("st0004"));
        assert!(manifest.edge("st0002", "почти сходится").is_none());
    }

    /// Терминал обязан отличаться от отсутствующего ребра и после сериализации:
    /// «процесс закончился» и «забыли ребро» ведут к разным решениям воркера.
    #[test]
    fn done_target_survives_serde() {
        let raw = serde_json::to_string(&EdgeTarget::Done).unwrap();
        assert!(raw.contains("\"kind\":\"done\""), "{raw}");
        assert_eq!(
            serde_json::from_str::<EdgeTarget>(&raw).unwrap(),
            EdgeTarget::Done
        );
    }

    #[test]
    fn criticality_gate_starts_at_the_first_effect() {
        assert!(!ProcessCriticality::ReadOnly.needs_quality_check());
        assert!(ProcessCriticality::Effectful.needs_quality_check());
        assert!(ProcessCriticality::Irreversible.needs_quality_check());
        assert!(ProcessCriticality::Irreversible > ProcessCriticality::Effectful);
    }
}
