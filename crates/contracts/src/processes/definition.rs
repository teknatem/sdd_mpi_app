//! Хранимое определение: версия, статус, отпечаток.
//!
//! Определения Процессов и Этапов живут в БД, а не файлами (ADR-0011 п.6):
//! решающий довод — транзакционность пина версии с экземпляром, который тоже в
//! БД. Отсюда два следствия, ради которых этот модуль и существует.
//!
//! Первое: **версия — это строка, а не поле**. Каждая публикация заводит новую
//! строку `(code, version)`, а не переписывает старую; живые экземпляры
//! доживают на своей версии (п.7), поэтому опубликованное не редактируется и не
//! удаляется никогда.
//!
//! Второе: **определения вне git**, поэтому «что изменилось» и «что сейчас
//! активно» можно узнать только в приложении. Diff здесь — не украшение
//! интерфейса, а замена `git diff`, которого у этих артефактов нет.

use serde::{Deserialize, Serialize};

use super::ProcessCriticality;

/// Состояние версии определения.
///
/// Три состояния, и переходы между ними односторонние: `Draft → Active →
/// Archived`. Обратно версия не возвращается — «откатиться» означает
/// активировать другую версию, а не переписать историю этой.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefinitionStatus {
    /// Черновик: правится и удаляется свободно, экземпляры на нём не стартуют.
    Draft,
    /// Активная версия. По коду она ровно одна: новые экземпляры стартуют на
    /// ней.
    Active,
    /// Была активной. Не удаляется: на ней могут доживать экземпляры, а её
    /// прогоны уже записаны в журнале эффектов.
    Archived,
}

impl DefinitionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "active" => Self::Active,
            "archived" => Self::Archived,
            _ => Self::Draft,
        }
    }

    /// Опубликована ли версия — то есть перестала ли она быть частной правкой
    /// автора. Опубликованное неизменяемо и неудаляемо.
    pub fn is_published(&self) -> bool {
        matches!(self, Self::Active | Self::Archived)
    }
}

/// Хранимая версия определения — общая обвязка для Процесса и Этапа.
///
/// Обвязка одна на оба вида намеренно: правило «версия не удаляется, пока на
/// неё ссылаются» действует на обоих уровнях (ADR-0011 п.7), и если бы уровни
/// разъехались реализацией, разъехалось бы и правило.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefinitionRecord<T> {
    /// Локальный идентификатор строки. Идентичность определения — `code`:
    /// UUID меняется при обновлении БД из боевой копии, код нет.
    pub id: String,
    pub code: String,
    pub version: i32,
    pub status: DefinitionStatus,
    /// Отпечаток содержимого: по нему опознаётся «то же самое определение».
    pub digest: String,
    pub created_at: String,
    #[serde(default)]
    pub created_by: Option<String>,
    pub definition: T,
}

/// Строка списка версий: то, что видно в истории до открытия самой версии.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefinitionVersion {
    pub code: String,
    pub version: i32,
    pub title: String,
    pub status: DefinitionStatus,
    pub digest: String,
    pub created_at: String,
    #[serde(default)]
    pub created_by: Option<String>,
}

/// Что изменилось между двумя версиями одного определения.
///
/// Список человекочитаемых строк, а не структурный дифф: читатель здесь —
/// человек перед активацией, и ему нужно «выход „расхождение“ исчез», а не
/// дерево различий JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefinitionDiff {
    pub code: String,
    pub title: String,
    /// Версия, с которой сравниваем. `None` — сравнивать не с чем: активной
    /// версии ещё не было.
    #[serde(default)]
    pub from_version: Option<i32>,
    pub to_version: i32,
    pub changes: Vec<String>,
}

impl DefinitionDiff {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

/// Всё, что человек обязан увидеть перед активацией версии Процесса.
///
/// Diff двухуровневый (ADR-0011 п.7): Этапы лежат в глобальном каталоге со
/// своими версиями, поэтому «Процесс не менялся» ничего не значит — под ним
/// мог поменяться Этап.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivationPlan {
    pub process: DefinitionDiff,
    /// Diff по каждому Этапу графа: что изменилось в нём с версии, на которой
    /// работает нынешняя активная версия Процесса.
    pub stages: Vec<DefinitionDiff>,
    /// Версии Этапов, которые запинит экземпляр, стартовавший после активации.
    pub pinned_stages: Vec<StagePin>,
    pub criticality: ProcessCriticality,
    /// Причины, по которым активация не состоится. Пустой список — можно.
    pub problems: Vec<String>,
}

impl ActivationPlan {
    pub fn is_allowed(&self) -> bool {
        self.problems.is_empty()
    }
}

/// Пин версии Этапа: пара, которую экземпляр фиксирует на старте и не меняет
/// до конца своей жизни.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagePin {
    pub code: String,
    pub version: i32,
    pub digest: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrips_through_column() {
        for status in [
            DefinitionStatus::Draft,
            DefinitionStatus::Active,
            DefinitionStatus::Archived,
        ] {
            assert_eq!(DefinitionStatus::from_str(status.as_str()), status);
        }
    }

    /// Порча значения в колонке не должна превращать архив в черновик так,
    /// чтобы его стало можно удалить: неизвестное читается как черновик, но
    /// удаление опирается на явную проверку, а не на этот разбор.
    #[test]
    fn published_is_the_pair_that_cannot_be_edited() {
        assert!(!DefinitionStatus::Draft.is_published());
        assert!(DefinitionStatus::Active.is_published());
        assert!(DefinitionStatus::Archived.is_published());
    }
}
