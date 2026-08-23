//! Доменные события — то, по чему Процессы стартуют и просыпаются.
//!
//! Каталог **закрытый и типизированный** (ADR-0011 п.5): новое событие требует
//! правки Rust и пересборки. Универсальный поток «агрегат изменён» отклонён —
//! смысл пришлось бы вычислять подписчику, а объём на импортах неподъёмен. Это
//! осознанная граница: **ядро решает, что является фактом, а Процесс решает,
//! что с фактом делать.**
//!
//! У каждого события есть **ключ корреляции** — поля, отвечающие на вопрос «про
//! что этот факт»: кабинет и бизнес-дата для закрытия дня. Состав ключа задан
//! здесь, а не в манифесте Процесса: ключ — свойство факта, и если бы его
//! объявлял подписчик, два Процесса разошлись бы в том, что считать «тем же
//! самым днём».
//!
//! Ключ сводится к **токену** — строке `поле=значение;поле=значение` в порядке
//! объявления. По равенству токенов ожидающий экземпляр находит своё событие,
//! поэтому порядок обязан быть детерминированным, а не «как в словаре».
//!
//! Не путать с `domain::common::EventStore`: такой заглушки в кодовой базе
//! больше нет — двух разных «доменных событий» здесь быть не должно.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Каталог событий. Пять штук — ровно те, что нужны пилоту; шестое заводится
/// правкой этого перечисления, а не строкой в БД.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainEventKind {
    /// День кабинета импортирован полностью: данные за дату можно считать
    /// собранными.
    ImportDayCompleted,
    /// Документ проведён в Главную книгу.
    DocumentPosted,
    /// Quality-проверка нашла нарушения.
    QualityViolationRaised,
    /// Человек сделал то, о чём его попросили.
    HumanActionDone,
    /// Экземпляр процесса не дождался события к дедлайну.
    ProcessInstanceTimeout,
}

impl DomainEventKind {
    /// Все события каталога — для UI, валидации триггеров и тестов.
    pub const ALL: [DomainEventKind; 5] = [
        Self::ImportDayCompleted,
        Self::DocumentPosted,
        Self::QualityViolationRaised,
        Self::HumanActionDone,
        Self::ProcessInstanceTimeout,
    ];

    /// Имя события в манифесте Процесса и в журнале.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ImportDayCompleted => "import.day.completed",
            Self::DocumentPosted => "document.posted",
            Self::QualityViolationRaised => "quality.violation.raised",
            Self::HumanActionDone => "human.action.done",
            Self::ProcessInstanceTimeout => "process.instance.timeout",
        }
    }

    /// Разобрать имя. `None` — события нет в каталоге, и это ошибка автора, а
    /// не повод завести его на лету.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value.trim())
    }

    /// Поля ключа корреляции — в том порядке, в котором они входят в токен.
    ///
    /// Для `human.action.done` ключ один — `request_key`: событие общее для
    /// всех Процессов, а «про что оно» знает тот, кто просил человека, и он же
    /// кладёт в запрос токен своего экземпляра.
    pub fn correlation_fields(&self) -> &'static [&'static str] {
        match self {
            Self::ImportDayCompleted => &["connection_id", "business_date"],
            Self::DocumentPosted => &["aggregate", "document_id"],
            Self::QualityViolationRaised => &["check_id"],
            Self::HumanActionDone => &["request_key"],
            Self::ProcessInstanceTimeout => &["instance_id"],
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::ImportDayCompleted => "День импортирован",
            Self::DocumentPosted => "Документ проведён",
            Self::QualityViolationRaised => "Проверка нашла нарушения",
            Self::HumanActionDone => "Человек сделал",
            Self::ProcessInstanceTimeout => "Ожидание истекло",
        }
    }
}

/// Ключ корреляции: значения полей события, по которым оно сводится с
/// ожидающим экземпляром.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CorrelationKey(BTreeMap<String, String>);

impl CorrelationKey {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, field: impl Into<String>, value: impl Into<String>) -> Self {
        self.0.insert(field.into(), value.into());
        self
    }

    pub fn get(&self, field: &str) -> Option<&str> {
        self.0.get(field).map(String::as_str)
    }

    pub fn fields(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Проверить состав ключа против каталога и собрать токен.
    ///
    /// Состав обязан совпасть **точно**: лишнее поле изменило бы токен, и
    /// событие перестало бы сводиться с ожиданием — молча, потому что «похоже
    /// на нужное» здесь не считается.
    pub fn token(&self, kind: DomainEventKind) -> Result<String, String> {
        let expected = kind.correlation_fields();
        for field in expected {
            match self.0.get(*field) {
                None => {
                    return Err(format!(
                        "событию '{}' не хватает поля ключа корреляции '{field}'",
                        kind.as_str()
                    ))
                }
                Some(value) if value.trim().is_empty() => {
                    return Err(format!(
                        "у события '{}' пустое поле ключа корреляции '{field}'",
                        kind.as_str()
                    ))
                }
                Some(_) => {}
            }
        }
        if let Some(extra) = self.0.keys().find(|key| !expected.contains(&key.as_str())) {
            return Err(format!(
                "у события '{}' лишнее поле ключа корреляции '{extra}': \
                 всё, что не входит в ключ, кладётся в данные события",
                kind.as_str()
            ));
        }
        Ok(expected
            .iter()
            .map(|field| format!("{field}={}", self.0[*field].trim()))
            .collect::<Vec<_>>()
            .join(";"))
    }
}

/// Опубликованный факт.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainEvent {
    pub id: String,
    /// Порядковый номер в журнале: курсор воркера двигается по нему, а не по
    /// времени — время в SQLite не монотонно между процессами.
    pub seq: i64,
    pub kind: DomainEventKind,
    pub correlation: CorrelationKey,
    /// Канонический вид ключа: по нему ищется ожидающий экземпляр.
    pub correlation_token: String,
    /// Данные события сверх ключа.
    #[serde(default)]
    pub payload: Value,
    /// Кто опубликовал: `u504`, `a033`, `ui`, `worker`. Строкой — журналу
    /// достаточно, а перечислять всех издателей типом значило бы вести второй
    /// каталог.
    pub source: String,
    pub published_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_name_and_a_key() {
        for kind in DomainEventKind::ALL {
            assert!(!kind.as_str().is_empty());
            assert!(
                !kind.correlation_fields().is_empty(),
                "у события '{}' нет ключа корреляции: его нечем свести с ожиданием",
                kind.as_str()
            );
            assert_eq!(DomainEventKind::parse(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn unknown_event_is_not_invented_on_the_fly() {
        assert_eq!(DomainEventKind::parse("import.day.almost"), None);
    }

    /// Токен строится в порядке объявления полей, а не в порядке заполнения:
    /// иначе один и тот же факт давал бы два разных токена.
    #[test]
    fn token_order_comes_from_the_catalog() {
        let direct = CorrelationKey::new()
            .with("connection_id", "c-1")
            .with("business_date", "2026-08-21");
        let reversed = CorrelationKey::new()
            .with("business_date", "2026-08-21")
            .with("connection_id", "c-1");
        let kind = DomainEventKind::ImportDayCompleted;
        assert_eq!(
            direct.token(kind).unwrap(),
            "connection_id=c-1;business_date=2026-08-21"
        );
        assert_eq!(direct.token(kind), reversed.token(kind));
    }

    #[test]
    fn incomplete_or_padded_key_is_rejected() {
        let kind = DomainEventKind::ImportDayCompleted;
        let missing = CorrelationKey::new().with("connection_id", "c-1");
        assert!(missing.token(kind).unwrap_err().contains("не хватает"));

        let empty = CorrelationKey::new()
            .with("connection_id", "c-1")
            .with("business_date", "  ");
        assert!(empty.token(kind).unwrap_err().contains("пустое поле"));

        let extra = CorrelationKey::new()
            .with("connection_id", "c-1")
            .with("business_date", "2026-08-21")
            .with("warehouse", "главный");
        assert!(extra.token(kind).unwrap_err().contains("лишнее поле"));
    }
}
