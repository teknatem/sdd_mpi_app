//! Семь осей классификации знания плюс роль кода и семейство единицы.
//!
//! Оси заданы нормативным документом `memory-bank/architecture/knowledge-inventory.md`
//! (§1 и §3). Здесь они превращены из таблицы в типы, и это не украшение: код
//! оси уходит в колонку `sys_knowledge_unit`, в JSON API и в фильтр на странице.
//! Строка в трёх местах разъехалась бы молча, enum — не разъедется.
//!
//! **Почему в контрактах, а не в бэкенде.** Фронт фильтрует таблицу по осям и
//! подписывает фасеты. Отдавать ему коды строками значит завести второй список
//! подписей и следить за ним руками; отдавать типами — не значит ничего, кроме
//! `use`.
//!
//! **Как фиксируются.** Три вещи разом:
//!
//! 1. `CLASSIFIER_VERSION` — растёт при любом изменении состава кодов;
//! 2. `golden_manifest()` + `classifiers_golden.txt` — тест не даст добавить,
//!    убрать или переименовать код, не правя золотой файл в том же коммите;
//! 3. `as_str()` отделён от имени варианта: переименование варианта в Rust не
//!    трогает код в БД, а переименование кода видно в золотом файле.
//!
//! Классификаторы меняются — но консервативно и на виду.

use serde::{Deserialize, Serialize};

/// Версия состава классификаторов.
///
/// Пишется в каждый снимок. Снимки разных версий сравнивать поразрезно нельзя:
/// разреза, которого в старой версии не было, задним числом не существует, и
/// дельта по нему была бы выдумкой. Страница показывает версию явно.
///
/// **Растёт при любом изменении состава кодов** — добавлении, удалении,
/// переименовании. Тест `golden_matches_file` не даст забыть.
pub const CLASSIFIER_VERSION: u16 = 1;

/// Генерирует ось: enum + `ALL` + стабильный код + подпись + разбор.
///
/// Код (`as_str`) намеренно задаётся литералом, а не выводится из имени
/// варианта: имя — дело Rust, код — контракт с базой и API.
macro_rules! classifier {
    (
        $(#[$meta:meta])*
        $name:ident, $axis:literal {
            $(
                $(#[$vmeta:meta])*
                $variant:ident => $code:literal, $label:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub enum $name {
            $(
                $(#[$vmeta])*
                #[serde(rename = $code)]
                $variant,
            )+
        }

        impl $name {
            /// Имя оси. Префикс строки в золотом файле и ключ фасета в API.
            pub const AXIS: &'static str = $axis;

            /// Все значения в порядке показа в UI.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// Стабильный код: колонка в БД, поле в JSON, строка золотого файла.
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $code),+ }
            }

            /// Подпись для интерфейса.
            pub const fn label(self) -> &'static str {
                match self { $(Self::$variant => $label),+ }
            }

            /// Разбор кода. `None` — код не из этой оси (например, из снимка
            /// старой версии классификатора).
            pub fn from_code(code: &str) -> Option<Self> {
                match code { $($code => Some(Self::$variant),)+ _ => None }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

classifier! {
    /// §1. Хранимое и вычисляемое складывать в одно число нельзя: у первого есть
    /// себестоимость на диске и полный размер, у второго — только цена
    /// конкретного ответа. Поэтому сводок всегда две.
    UnitFamily, "family" {
        /// Статья, карта, навык, промпт, бандл плагина, определение Процесса.
        Stored => "stored", "Хранимая",
        /// Схема сущности, источник данных, план счетов, прогон проверки.
        Computed => "computed", "Вычисляемая",
    }
}

classifier! {
    /// Ось A. Где источник правды.
    Origin, "origin" {
        /// Вшито в бинарь через `include_str!`.
        CodeEmbedded => "code_embedded", "Вшито в бинарь",
        /// Rust-константы и структуры (`ActionInfo`, `SCOPE_CATALOG`, `ALL_ENTITIES`).
        CodeRegistry => "code_registry", "Реестр в коде",
        /// Собрано из БД в файл при старте.
        DbGenerated => "db_generated", "Сгенерировано из БД",
        /// Читается из БД по запросу.
        DbLive => "db_live", "Живое из БД",
        /// Файлы вне репозитория, правит человек.
        FileCurated => "file_curated", "Курируемый файл",
        /// Ответ внешнего API; знанием не хранится, только описывается статьёй.
        ExternalApi => "external_api", "Внешний API",
    }
}

classifier! {
    /// Ось B. Форма хранения — определяет, чем единицу вообще можно обнаружить (§4).
    StorageForm, "storage_form" {
        Markdown => "markdown", "Markdown",
        RustConst => "rust_const", "Rust-константа",
        DbRow => "db_row", "Строка БД",
        JsModule => "js_module", "JS-модуль",
        JsonSchema => "json_schema", "JSON Schema",
    }
}

classifier! {
    /// Ось C. Кто правит. Без неё инвентаризация посоветует править то, куда
    /// запись запрещена и вернёт ошибку.
    Editor, "editor" {
        /// Руками нельзя: каталоги `app/` и `generated/`.
        Application => "application", "Приложение",
        Curator => "curator", "Куратор",
        Developer => "developer", "Разработчик",
        Llm => "llm", "LLM",
    }
}

classifier! {
    /// Ось D. Достижимость внутренним чатом — центральная ось.
    ///
    /// В отличие от остальных, **вычисляется**, а не объявляется: из `TOOL_YIELD`
    /// (какой инструмент отдаёт поверхность) и `CORE_TOOLS` (доступен ли он без
    /// активации навыка).
    Reachability, "reachability" {
        /// Находится обычным `search_knowledge`.
        DefaultSearch => "default_search", "Обычный поиск",
        /// Только `get_knowledge(id)` или явный запрос корпуса «generated».
        ByIdOnly => "by_id_only", "Только по id",
        /// Отдаётся лишь конкретным инструментом, но тот доступен всегда.
        ToolGated => "tool_gated", "Через инструмент",
        /// Инструмент есть, но только внутри навыка.
        SkillGated => "skill_gated", "Через навык",
        /// Инструмент не нужен: знание вкладывается в контекст безусловно.
        ///
        /// Уровня нет в исходной таблице §3, и он добавлен по факту: промпт ядра
        /// не достаётся ни одним инструментом, но и недостижимым не является.
        /// Без этого уровня он попадал бы в `unreachable_surfaces` и вечно
        /// изображал дефект.
        AlwaysInContext => "always_in_context", "Всегда в контексте",
        /// Ни один инструмент эту поверхность не отдаёт.
        Unreachable => "unreachable", "Недостижимо",
    }
}

classifier! {
    /// Ось E. Жизненный цикл.
    Lifecycle, "lifecycle" {
        Active => "active", "Активно",
        Draft => "draft", "Черновик",
        Deprecated => "deprecated", "Устарело",
        /// Истёк TTL.
        Stale => "stale", "Просрочено",
        /// Строка статистики без живой статьи.
        Orphaned => "orphaned", "Осиротело",
    }
}

classifier! {
    /// Ось F. Для какого контекста знание истинно.
    ///
    /// Не совпадает с происхождением: схема сущности хранится в коде и имеет
    /// область `application`, а результат запроса к ней вычисляется для `instance`.
    Scope, "scope" {
        /// Одинаково для всех экземпляров приложения.
        Application => "application", "Приложение",
        /// Относится к конкретной БД, конфигурации или набору данных.
        Instance => "instance", "Экземпляр",
        /// Общая схема или правило с экземплярными значениями.
        Mixed => "mixed", "Смешанная",
        /// Описание внешней системы или API.
        External => "external", "Внешняя",
    }
}

classifier! {
    /// Ось G. Кому знание доступно без чтения исходного кода.
    ///
    /// Не заменяет ось достижимости: `external_api` может быть доступен внешнему
    /// клиенту, но недоступен чату, а `tool_gated` — доступен чату без внешнего API.
    ExposureChannel, "channel" {
        /// Существует в исходном коде, но не отдаётся никаким прикладным каналом.
        SourceOnly => "source_only", "Только исходники",
        /// Доступно внутреннему чату или служебному инструменту.
        InternalRuntime => "internal_runtime", "Внутренний рантайм",
        /// Доступно клиенту через API без доступа к репозиторию.
        ExternalApi => "external_api", "Внешний API",
        Both => "both", "Оба канала",
        /// Существует, но намеренно не раскрывается.
        Unexposed => "unexposed", "Не раскрывается",
    }
}

classifier! {
    /// Роль исходного кода. Сырой код не считается доступным знанием
    /// автоматически: разработчик его прочитает, чат и внешний API получат
    /// только то, что прошло через канал раскрытия.
    CodeRole, "code_role" {
        /// Код — источник истины для поведения, правил, прав и побочных эффектов.
        Authoritative => "code_authoritative", "Источник истины",
        /// Из кода построен каталог, схема или документ, доступный через рантайм.
        Extracted => "code_extracted", "Извлечено из кода",
        /// Смысл существует в коде, но не выведен в прикладной канал.
        Undocumented => "code_undocumented", "Не выведено наружу",
    }
}

/// Одно значение оси, подготовленное для UI: код, подпись, порядок.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxisValueDto {
    pub code: String,
    pub label: String,
    pub order: u16,
}

/// Ось целиком — фронт строит из неё фильтр, ничего не зная о кодах.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxisDto {
    pub axis: String,
    pub label: String,
    pub values: Vec<AxisValueDto>,
}

/// Описание всех осей для интерфейса — единственный источник подписей фильтров.
pub fn axes() -> Vec<AxisDto> {
    macro_rules! axis_of {
        ($ty:ty, $label:literal) => {
            AxisDto {
                axis: <$ty>::AXIS.to_string(),
                label: $label.to_string(),
                values: <$ty>::ALL
                    .iter()
                    .enumerate()
                    .map(|(index, value)| AxisValueDto {
                        code: value.as_str().to_string(),
                        label: value.label().to_string(),
                        order: index as u16,
                    })
                    .collect(),
            }
        };
    }

    vec![
        axis_of!(UnitFamily, "Семейство"),
        axis_of!(Origin, "Происхождение"),
        axis_of!(StorageForm, "Форма хранения"),
        axis_of!(Editor, "Кто правит"),
        axis_of!(Reachability, "Достижимость чатом"),
        axis_of!(Lifecycle, "Жизненный цикл"),
        axis_of!(Scope, "Область"),
        axis_of!(ExposureChannel, "Канал раскрытия"),
        axis_of!(CodeRole, "Роль кода"),
    ]
}

/// Плоский снимок состава классификаторов — то, что сверяется с золотым файлом.
///
/// Формат намеренно скучный и отсортированный: diff по нему читается глазами,
/// а перестановка строк в исходнике его не меняет.
pub fn golden_manifest() -> String {
    let mut lines: Vec<String> = Vec::new();
    for axis in axes() {
        for value in axis.values {
            lines.push(format!("{}:{}", axis.axis, value.code));
        }
    }
    lines.sort();
    let mut out = format!("version {CLASSIFIER_VERSION}\n");
    for line in lines {
        out.push_str(&line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Главный сторож: состав классификаторов не меняется молча.
    ///
    /// Падение — не баг, а напоминание: обнови `classifiers_golden.txt` и подними
    /// `CLASSIFIER_VERSION`, иначе снимки разных сборок окажутся несравнимы, и
    /// понять это будет уже не по чему.
    #[test]
    fn golden_matches_file() {
        let expected = include_str!("classifiers_golden.txt");
        assert_eq!(
            golden_manifest(),
            expected,
            "состав классификаторов разошёлся с classifiers_golden.txt — \
             обнови файл и подними CLASSIFIER_VERSION"
        );
    }

    #[test]
    fn codes_are_unique_within_axis() {
        for axis in axes() {
            let mut seen = BTreeSet::new();
            for value in &axis.values {
                assert!(
                    seen.insert(value.code.clone()),
                    "ось {}: код {} повторяется",
                    axis.axis,
                    value.code
                );
                assert!(
                    !value.label.trim().is_empty(),
                    "ось {}: у кода {} нет подписи",
                    axis.axis,
                    value.code
                );
            }
        }
    }

    #[test]
    fn axis_names_are_unique() {
        let mut seen = BTreeSet::new();
        for axis in axes() {
            assert!(
                seen.insert(axis.axis.clone()),
                "ось {} задана дважды",
                axis.axis
            );
        }
    }

    /// `as_str` и `from_code` обратны друг другу, иначе снимок не прочитается.
    #[test]
    fn codes_round_trip() {
        macro_rules! round_trip {
            ($ty:ty) => {
                for value in <$ty>::ALL {
                    assert_eq!(
                        <$ty>::from_code(value.as_str()),
                        Some(*value),
                        "{}: код {} не разбирается обратно",
                        <$ty>::AXIS,
                        value.as_str()
                    );
                }
                assert_eq!(<$ty>::from_code("нет такого кода"), None);
            };
        }
        round_trip!(UnitFamily);
        round_trip!(Origin);
        round_trip!(StorageForm);
        round_trip!(Editor);
        round_trip!(Reachability);
        round_trip!(Lifecycle);
        round_trip!(Scope);
        round_trip!(ExposureChannel);
        round_trip!(CodeRole);
    }

    /// Код в JSON — это `as_str`, а не имя варианта. Проверяем на самом коварном
    /// месте: `CodeRole::Authoritative` сериализуется в `code_authoritative`, и
    /// никакой `rename_all` этого бы не дал.
    #[test]
    fn serde_uses_stable_codes() {
        assert_eq!(
            serde_json::to_string(&CodeRole::Authoritative).unwrap(),
            "\"code_authoritative\""
        );
        assert_eq!(
            serde_json::from_str::<Origin>("\"file_curated\"").unwrap(),
            Origin::FileCurated
        );
    }
}
