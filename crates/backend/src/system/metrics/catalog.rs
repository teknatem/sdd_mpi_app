//! Каталог метрик: что означает ключ, как его подписать и когда красить.
//!
//! Живёт в коде, а не в БД, по той же причине, что `SCOPE_CATALOG` и план
//! счетов: набор метрик — часть версии приложения. Экземпляр не должен уметь
//! завести свою метрику, иначе две базы перестанут быть сравнимыми.
//!
//! Ключ, которого нет здесь, в БД писаться может (сборщик его не фильтрует), но
//! на страницу не попадёт. Это сознательно: добавить измерение и решить, как его
//! показывать, — два разных действия.
//!
//! **Пороги.** Выставлены по состоянию на момент внедрения и намеренно
//! консервативны: задача первой версии — зафиксировать текущий уровень как базу,
//! а не покрасить всё красным. Главный сигнал страницы — дельта, а не цвет.

use contracts::system::metrics::{MetricDirection, MetricGroupDto, MetricStatus};

/// Описание одной метрики.
pub struct MetricDef {
    pub key: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub unit: &'static str,
    pub precision: u8,
    pub direction: MetricDirection,
    /// Порог «жёлтого». `None` — метрика справочная, цвета у неё нет.
    pub warn: Option<f64>,
    /// Порог «красного».
    pub bad: Option<f64>,
    pub hint: Option<&'static str>,
}

impl MetricDef {
    /// Оценка значения по порогам с учётом направления.
    pub fn status(&self, value: f64) -> MetricStatus {
        let (Some(warn), Some(bad)) = (self.warn, self.bad) else {
            return MetricStatus::Neutral;
        };
        match self.direction {
            MetricDirection::Lower => {
                if value >= bad {
                    MetricStatus::Bad
                } else if value >= warn {
                    MetricStatus::Warn
                } else {
                    MetricStatus::Ok
                }
            }
            MetricDirection::Higher => {
                if value <= bad {
                    MetricStatus::Bad
                } else if value <= warn {
                    MetricStatus::Warn
                } else {
                    MetricStatus::Ok
                }
            }
            MetricDirection::Neutral => MetricStatus::Neutral,
        }
    }
}

/// Группы = блоки страницы, в порядке показа.
pub const METRIC_GROUPS: &[(&str, &str)] = &[
    ("code", "Размер кода"),
    ("build", "Стоимость сборки"),
    ("domain", "Домен"),
    ("api", "Границы и контракты"),
    ("tests", "Тесты и запахи"),
    ("ui", "UI-стандарт"),
    ("db", "База данных"),
    ("data_quality", "Качество данных"),
    ("access", "Целостность доступа"),
    ("activity", "Активность"),
];

pub fn groups() -> Vec<MetricGroupDto> {
    METRIC_GROUPS
        .iter()
        .enumerate()
        .map(|(index, (code, label))| MetricGroupDto {
            code: (*code).to_string(),
            label: (*label).to_string(),
            order: index as u16,
        })
        .collect()
}

pub fn find(key: &str) -> Option<&'static MetricDef> {
    METRIC_CATALOG.iter().find(|def| def.key == key)
}

/// Короткий конструктор: у большинства метрик нет ни порогов, ни подсказки.
const fn def(
    key: &'static str,
    label: &'static str,
    group: &'static str,
    unit: &'static str,
    precision: u8,
    direction: MetricDirection,
) -> MetricDef {
    MetricDef {
        key,
        label,
        group,
        unit,
        precision,
        direction,
        warn: None,
        bad: None,
        hint: None,
    }
}

const fn with_limits(mut d: MetricDef, warn: f64, bad: f64) -> MetricDef {
    d.warn = Some(warn);
    d.bad = Some(bad);
    d
}

const fn with_hint(mut d: MetricDef, hint: &'static str) -> MetricDef {
    d.hint = Some(hint);
    d
}

use MetricDirection::{Higher, Lower, Neutral};

pub static METRIC_CATALOG: &[MetricDef] = &[
    // --- Размер кода -------------------------------------------------------
    def(
        "code.lines.total",
        "Строк кода",
        "code",
        "строк",
        0,
        Neutral,
    ),
    def("code.lines.backend", "backend", "code", "строк", 0, Neutral),
    def(
        "code.lines.frontend",
        "frontend",
        "code",
        "строк",
        0,
        Neutral,
    ),
    def(
        "code.lines.contracts",
        "contracts",
        "code",
        "строк",
        0,
        Neutral,
    ),
    def("code.files.total", "Файлов .rs", "code", "шт", 0, Neutral),
    with_hint(
        def("code.avg_lines", "Средний файл", "code", "строк", 0, Lower),
        "Строк кода / число файлов",
    ),
    with_hint(
        with_limits(
            def(
                "code.files_over_1000",
                "Файлов > 1000 строк",
                "code",
                "шт",
                0,
                Lower,
            ),
            30.0,
            60.0,
        ),
        "Прямая мера сложности: такой файл уже не читается целиком",
    ),
    with_hint(
        def(
            "code.top10_share",
            "Доля топ-10 файлов",
            "code",
            "%",
            1,
            Lower,
        ),
        "Какую часть кода занимают 10 самых больших файлов",
    ),
    with_hint(
        with_limits(
            def(
                "code.files_over_2000",
                "Файлов > 2000 строк",
                "code",
                "шт",
                0,
                Lower,
            ),
            8.0,
            15.0,
        ),
        "Порог, за которым файл перестаёт быть большим и становится хабом",
    ),
    with_hint(
        with_limits(
            def("code.max_file_lines", "Самый большой файл", "code", "строк", 0, Lower),
            2500.0,
            5000.0,
        ),
        "Один файл, который держит рекорд. Разбор god-файлов виден именно здесь",
    ),
    with_hint(
        with_limits(
            def(
                "code.orphan_files",
                "Файлов вне дерева модулей",
                "code",
                "шт",
                0,
                Lower,
            ),
            1.0,
            10.0,
        ),
        "Ни один `mod` их не подключает: компилятор их не видит, cargo fmt не форматирует, а в строках кода они посчитаны",
    ),
    // --- Стоимость сборки --------------------------------------------------
    // Пороги намеренно не заданы: секунды сравнимы только с секундами той же
    // машины. `machine` в build_timings.json фиксирует, чья это была сборка;
    // до появления второго замера красить нечего, а направление уже записано.
    with_hint(
        def(
            "build.incr_backend_sec",
            "backend после правки среза",
            "build",
            "с",
            1,
            Lower,
        ),
        "cargo check -p backend после изменения одного файла агрегата",
    ),
    with_hint(
        def(
            "build.incr_frontend_sec",
            "frontend после правки среза",
            "build",
            "с",
            1,
            Lower,
        ),
        "cargo check -p frontend --target wasm32 после изменения одного файла агрегата",
    ),
    // `check` останавливается перед кодогенерацией, и потому дёшев. Настоящее
    // ожидание — эти два: сборка и линковка тест-бинаря и wasm-артефакта.
    // Разрыв между ними и `check` — не деталь округления, а то, на основании
    // чего вообще решается вопрос о разбиении на крейты.
    with_hint(
        def(
            "build.bin_backend_sec",
            "Бинарь backend",
            "build",
            "с",
            1,
            Lower,
        ),
        "cargo build --bin backend после правки одного файла агрегата — путь «правка → работающее приложение». check, test и build --bin держат ТРИ независимых набора артефактов и не греют друг друга",
    ),
    with_hint(
        def(
            "build.test_backend_sec",
            "Тест-бинарь backend",
            "build",
            "с",
            1,
            Lower,
        ),
        "cargo test -p backend --no-run после правки одного файла агрегата: кодогенерация и линковка без запуска тестов",
    ),
    // Полный конвейер trunk, а не только cargo: wasm-bindgen работает над уже
    // слинкованным .wasm и пропорционален его размеру, поэтому профиль решает
    // больше, чем что-либо в коде. dev даёт 197 МБ и 36 с, wasm-dev — 66 МБ и 25 с.
    with_hint(
        def(
            "build.trunk_dev_edit_sec",
            "Цикл фронта, профиль dev",
            "build",
            "с",
            1,
            Lower,
        ),
        "trunk build после правки одного файла агрегата на профиле dev",
    ),
    with_hint(
        def(
            "build.trunk_wasmdev_edit_sec",
            "Цикл фронта, профиль wasm-dev",
            "build",
            "с",
            1,
            Lower,
        ),
        "то же на профиле wasm-dev — рабочий режим, см. команду запуска в CLAUDE.md",
    ),
    with_hint(
        def(
            "build.wasm_frontend_sec",
            "Сборка wasm",
            "build",
            "с",
            1,
            Lower,
        ),
        "cargo build -p frontend --target wasm32 после правки одного файла агрегата",
    ),
    with_hint(
        def(
            "build.contracts_ripple_sec",
            "Волна от contracts",
            "build",
            "с",
            1,
            Lower,
        ),
        "Одна правка contracts задевает 424 файла backend и 243 frontend — оба крейта пересобираются целиком",
    ),
    def(
        "build.full_backend_sec",
        "backend с нуля",
        "build",
        "с",
        1,
        Lower,
    ),
    def(
        "build.full_frontend_sec",
        "frontend с нуля",
        "build",
        "с",
        1,
        Lower,
    ),
    with_hint(
        def("arch.crates", "Крейтов в workspace", "build", "шт", 0, Neutral),
        "Единица инкрементальной компиляции: rustc пересобирает крейт целиком",
    ),
    // --- Домен -------------------------------------------------------------
    def(
        "arch.aggregates",
        "Агрегаты a0XX",
        "domain",
        "шт",
        0,
        Neutral,
    ),
    def(
        "arch.projections",
        "Проекции p9XX",
        "domain",
        "шт",
        0,
        Neutral,
    ),
    def(
        "arch.usecases",
        "Use-cases u5XX",
        "domain",
        "шт",
        0,
        Neutral,
    ),
    def(
        "arch.data_views",
        "DataView dvXXX",
        "domain",
        "шт",
        0,
        Neutral,
    ),
    def(
        "arch.data_schemes",
        "Схемы таблиц dsXX",
        "domain",
        "шт",
        0,
        Neutral,
    ),
    def(
        "arch.tasks",
        "Регламентные задания",
        "domain",
        "шт",
        0,
        Neutral,
    ),
    def("arch.routes", "API-роутов", "domain", "шт", 0, Neutral),
    def("arch.ui_scopes", "UI-scopes", "domain", "шт", 0, Neutral),
    def("arch.migrations", "Миграций БД", "domain", "шт", 0, Neutral),
    def(
        "docs.memory_bank",
        "Статей memory-bank",
        "domain",
        "шт",
        0,
        Neutral,
    ),
    def("docs.adr", "ADR", "domain", "шт", 0, Neutral),
    with_hint(
        with_limits(
            def(
                "arch.sdd_findings",
                "Замечаний анализатора",
                "domain",
                "шт",
                0,
                Lower,
            ),
            40.0,
            60.0,
        ),
        "Findings внешнего SDD Studio. Каталог .sdd gitignore'ится — на свежем клоне метрики просто нет",
    ),
    with_hint(
        with_limits(
            def(
                "arch.docs_coverage",
                "Документированность",
                "domain",
                "%",
                0,
                Higher,
            ),
            40.0,
            20.0,
        ),
        "Доля объектов с рукописным llm.md рядом с кодом",
    ),
    // --- Соответствие стандарту --------------------------------------------
    // architecture.toml описывает правила проекта, tools/check_architecture.ps1
    // их исполняет. Пороги здесь стоят намеренно жёсткие: правило заводится
    // только после того, как дерево ему уже соответствует, поэтому нормой
    // является ноль, а не «немного».
    with_hint(
        with_limits(
            def(
                "arch.naming_violations",
                "Нарушений стандарта",
                "domain",
                "шт",
                0,
                Lower,
            ),
            1.0,
            5.0,
        ),
        "Расхождения дерева исходников с architecture.toml: имена ролей, индексы каталогов, регистр, пути внутри include_str!",
    ),
    with_hint(
        with_limits(
            def("arch.waived_rules", "Waiver'ов в стандарте", "domain", "шт", 0, Lower),
            5.0,
            15.0,
        ),
        "Точечные разрешения нарушать правило. Каждый несёт причину, но растущая куча — это то, как стандарт выхолащивается",
    ),
    // --- Границы и контракты -----------------------------------------------
    // Три счётчика одной и той же течи. Крейт `contracts` существует ради
    // compile-time сцепки фронта и бэка; каждый хендлер, отвечающий голым
    // `serde_json::Value`, каждая ошибка, схлопнутая до `StatusCode`, и каждый
    // файл фронта, который сам ходит по HTTP, обходят его стороной.
    with_hint(
        with_limits(
            def(
                "api.untyped_handlers",
                "Хендлеров без DTO",
                "api",
                "шт",
                0,
                Lower,
            ),
            60.0,
            100.0,
        ),
        "Возвращают serde_json::Value — контракт держится на строковых ключах, а не на типах",
    ),
    with_hint(
        with_limits(
            def(
                "api.status_only_errors",
                "Ошибок без тела",
                "api",
                "шт",
                0,
                Lower,
            ),
            60.0,
            120.0,
        ),
        "Err = StatusCode: причина теряется, и фронт вынужден различать её по подстроке",
    ),
    with_hint(
        with_limits(
            def(
                "fe.raw_fetch_files",
                "Файлов фронта мимо api_utils",
                "api",
                "шт",
                0,
                Lower,
            ),
            40.0,
            100.0,
        ),
        "Свой fetch или gloo_net: авторизация, 401 и разбор ошибок решаются заново в каждом",
    ),
    with_hint(
        with_limits(
            def(
                "api.global_db_files",
                "Файлов на глобальном соединении",
                "api",
                "шт",
                0,
                Lower,
            ),
            60.0,
            150.0,
        ),
        "Берут базу из синглтона db::get_connection() вместо AppState. Мост Фазы 3: пока счётчик не ноль, второй экземпляр приложения в процессе невозможен",
    ),
    // --- Тесты и запахи ----------------------------------------------------
    def("tests.total", "Тестов", "tests", "шт", 0, Neutral),
    with_hint(
        with_limits(
            def(
                "tests.integration",
                "Интеграционных тестов",
                "tests",
                "шт",
                0,
                Higher,
            ),
            10.0,
            1.0,
        ),
        "Тесты в crates/backend/tests, поднимающие БД. Красный по построению: глобальный DB_CONN не оставляет тесту способа открыть свою базу",
    ),
    with_hint(
        with_limits(
            def(
                "tests.density.backend",
                "Плотность тестов backend",
                "tests",
                "на 1k строк",
                2,
                Higher,
            ),
            2.0,
            1.0,
        ),
        "Тестов на 1000 строк кода крейта",
    ),
    with_limits(
        def(
            "tests.density.frontend",
            "Плотность тестов frontend",
            "tests",
            "на 1k строк",
            2,
            Higher,
        ),
        1.0,
        0.5,
    ),
    with_limits(
        def(
            "tests.density.contracts",
            "Плотность тестов contracts",
            "tests",
            "на 1k строк",
            2,
            Higher,
        ),
        1.0,
        0.5,
    ),
    with_hint(
        with_limits(
            def("smells.unwrap", ".unwrap()", "tests", "шт", 0, Lower),
            500.0,
            800.0,
        ),
        "Каждый — потенциальная паника в рантайме",
    ),
    def("smells.todo_fixme", "TODO / FIXME", "tests", "шт", 0, Lower),
    // --- UI-стандарт -------------------------------------------------------
    def("ui.block_roots", "BEM-блоков", "ui", "шт", 0, Neutral),
    with_hint(
        with_limits(
            def(
                "ui.unregistered_blocks",
                "Блоков вне allowlist",
                "ui",
                "шт",
                0,
                Lower,
            ),
            1.0,
            5.0,
        ),
        "Правило UI-010: новый блок заводится вместе с записью в allowlist",
    ),
    with_limits(
        def(
            "ui.dead_classes",
            "Классов без ссылок",
            "ui",
            "шт",
            0,
            Lower,
        ),
        250.0,
        450.0,
    ),
    with_limits(
        def("ui.inline_styles", "Инлайн-стилей", "ui", "шт", 0, Lower),
        3000.0,
        5000.0,
    ),
    with_hint(
        with_limits(
            def("ui.hardcoded_hex", "Хардкод цвета", "ui", "шт", 0, Lower),
            250.0,
            450.0,
        ),
        "Правило UI-020: цвета живут в темах",
    ),
    with_limits(
        def("ui.raw_px", "Сырых px", "ui", "шт", 0, Lower),
        1500.0,
        2500.0,
    ),
    with_hint(
        with_limits(
            def(
                "ui.duplicate_blocks",
                "Блоков в нескольких файлах",
                "ui",
                "шт",
                0,
                Lower,
            ),
            20.0,
            40.0,
        ),
        "Правило UI-011: у блока один дом",
    ),
    with_hint(
        with_limits(
            def(
                "ui.broken_tokens",
                "Сломанных токенов",
                "ui",
                "шт",
                0,
                Lower,
            ),
            1.0,
            3.0,
        ),
        "var(--x) без определения и без fallback — это баг, а не стилистика",
    ),
    // --- База данных -------------------------------------------------------
    def("db.file_mb", "Размер файла БД", "db", "МБ", 0, Neutral),
    with_hint(
        with_limits(def("db.wal_mb", "WAL", "db", "МБ", 1, Lower), 200.0, 500.0),
        "Разросшийся WAL — признак давно не выполнявшегося checkpoint",
    ),
    with_hint(
        with_limits(
            def("db.reclaimable_mb", "Освобождаемо", "db", "МБ", 0, Lower),
            300.0,
            800.0,
        ),
        "Столько вернёт VACUUM",
    ),
    def(
        "db.rows_total",
        "Строк во всех таблицах",
        "db",
        "шт",
        0,
        Neutral,
    ),
    def(
        "db.tables_profiled",
        "Таблиц в профиле",
        "db",
        "шт",
        0,
        Neutral,
    ),
    def(
        "db.raw_storage_mb",
        "Сырой JSON-архив",
        "db",
        "МБ",
        0,
        Lower,
    ),
    with_hint(
        with_limits(
            def(
                "db.profile_age_hours",
                "Возраст профиля",
                "db",
                "ч",
                1,
                Lower,
            ),
            48.0,
            168.0,
        ),
        "Когда последний раз пересчитывались row_count по таблицам",
    ),
    // --- Качество данных ---------------------------------------------------
    def(
        "quality.checks",
        "Проверок",
        "data_quality",
        "шт",
        0,
        Neutral,
    ),
    with_hint(
        with_limits(
            def(
                "quality.min_compliance",
                "Худшая проверка",
                "data_quality",
                "%",
                2,
                Higher,
            ),
            99.0,
            95.0,
        ),
        "Минимальная доля соответствия среди всех проверок",
    ),
    with_limits(
        def(
            "quality.failing_checks",
            "С нарушениями",
            "data_quality",
            "шт",
            0,
            Lower,
        ),
        1.0,
        3.0,
    ),
    with_hint(
        with_limits(
            def(
                "quality.never_run",
                "Ни разу не запускались",
                "data_quality",
                "шт",
                0,
                Lower,
            ),
            1.0,
            3.0,
        ),
        "Проверка без прогона ничего не гарантирует",
    ),
    with_limits(
        def(
            "quality.stalest_run_days",
            "Самый старый прогон",
            "data_quality",
            "дн",
            1,
            Lower,
        ),
        7.0,
        30.0,
    ),
    // --- Целостность доступа -----------------------------------------------
    with_hint(
        with_limits(
            def(
                "access.violations",
                "Нарушений policy",
                "access",
                "шт",
                0,
                Lower,
            ),
            1.0,
            5.0,
        ),
        "Отчёт /api/system/audit/violations",
    ),
    def(
        "access.unscoped_routes",
        "Роутов без scope",
        "access",
        "шт",
        0,
        Lower,
    ),
    def(
        "access.open_routes",
        "Публичных роутов",
        "access",
        "шт",
        0,
        Lower,
    ),
    def(
        "access.scoped_routes",
        "Роутов со scope",
        "access",
        "шт",
        0,
        Neutral,
    ),
    with_hint(
        def(
            "access.routes_total",
            "Роутов в реестре policy",
            "access",
            "шт",
            0,
            Neutral,
        ),
        "ROUTE_REGISTRY; `API-роутов` в блоке «Домен» считает только api/routes.rs",
    ),
    // --- Активность --------------------------------------------------------
    def(
        "git.commits_30d",
        "Коммитов за 30 дней",
        "activity",
        "шт",
        0,
        Neutral,
    ),
    def(
        "git.added_30d",
        "Добавлено строк за 30 дней",
        "activity",
        "строк",
        0,
        Neutral,
    ),
    def(
        "git.deleted_30d",
        "Удалено строк за 30 дней",
        "activity",
        "строк",
        0,
        Neutral,
    ),
    def(
        "git.commits_total",
        "Коммитов всего",
        "activity",
        "шт",
        0,
        Neutral,
    ),
    with_limits(
        def(
            "tasks.failed_24h",
            "Упавших заданий за 24 ч",
            "activity",
            "шт",
            0,
            Lower,
        ),
        1.0,
        5.0,
    ),
    def(
        "tasks.enabled",
        "Включённых заданий",
        "activity",
        "шт",
        0,
        Neutral,
    ),
    with_hint(
        def(
            "instance.restarts_7d",
            "Рестартов за 7 дней",
            "activity",
            "шт",
            0,
            Neutral,
        ),
        "Считается по самой таблице снимков",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn keys_are_unique() {
        let mut seen = HashSet::new();
        for def in METRIC_CATALOG {
            assert!(seen.insert(def.key), "дубль ключа метрики: {}", def.key);
        }
    }

    #[test]
    fn every_metric_belongs_to_a_known_group() {
        let groups: HashSet<&str> = METRIC_GROUPS.iter().map(|(code, _)| *code).collect();
        for def in METRIC_CATALOG {
            assert!(
                groups.contains(def.group),
                "метрика {} ссылается на несуществующую группу {}",
                def.key,
                def.group
            );
        }
    }

    #[test]
    fn thresholds_come_in_pairs_and_face_the_right_way() {
        for def in METRIC_CATALOG {
            assert_eq!(
                def.warn.is_some(),
                def.bad.is_some(),
                "у {} задан только один порог",
                def.key
            );
            if let (Some(warn), Some(bad)) = (def.warn, def.bad) {
                match def.direction {
                    // «Меньше — лучше»: красный порог должен быть выше жёлтого.
                    MetricDirection::Lower => assert!(bad > warn, "{}: bad <= warn", def.key),
                    MetricDirection::Higher => assert!(bad < warn, "{}: bad >= warn", def.key),
                    MetricDirection::Neutral => {
                        panic!("{}: пороги у нейтральной метрики не сработают", def.key)
                    }
                }
            }
        }
    }

    #[test]
    fn status_follows_direction() {
        let lower = with_limits(def("x", "x", "code", "", 0, Lower), 10.0, 20.0);
        assert_eq!(lower.status(5.0), MetricStatus::Ok);
        assert_eq!(lower.status(15.0), MetricStatus::Warn);
        assert_eq!(lower.status(25.0), MetricStatus::Bad);

        let higher = with_limits(def("y", "y", "code", "", 0, Higher), 10.0, 5.0);
        assert_eq!(higher.status(15.0), MetricStatus::Ok);
        assert_eq!(higher.status(7.0), MetricStatus::Warn);
        assert_eq!(higher.status(3.0), MetricStatus::Bad);

        assert_eq!(
            def("z", "z", "code", "", 0, Neutral).status(1.0),
            MetricStatus::Neutral
        );
    }
}
