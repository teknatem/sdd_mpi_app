# memory-bank — накопленные знания о проекте

Здесь лежит то, что **не выводится из кода**: почему решили так, как правильно делать
повторяющееся, на какие грабли уже наступали. Всё остальное — в других местах:

| Нужно | Идти в |
|---|---|
| Как собрать, где что лежит, конвенции | `../CLAUDE.md` |
| Найти объект (`a0XX`, `p9XX`, роут, раздел UI) | `../ARCHITECTURE.md` (генерируется из кода) |
| Что означает термин домена | `../CONTEXT.md` |
| Гайд по конкретной фиче, план | `../docs/` |

> **Осторожно с датами.** Значительная часть записей — конец 2025 / начало 2026, времена
> миграции фронта на Thaw UI. Решения в `decisions/` актуальны, стандарты в `architecture/`
> в основном тоже, но перед тем как опереться на конкретный файл — проверь, что упомянутые
> в нём пути и функции ещё существуют. Источник истины — код.

## Каталоги

### `decisions/` — ADR (11 записей)
Архитектурные решения: **почему** сделано так. Формат, критерий «когда ADR нужен» и полный
индекс — в [`decisions/README.md`](decisions/README.md). Это самый ухоженный раздел, читай
его конвенцию как образец.

### `architecture/` — стандарты UI и слоёв (21 файл)
Крупнейший раздел. Сюда идти за «как должна выглядеть страница», прежде чем грепать код.

- **Программы и планы**: [`ARCHITECTURE_EVOLUTION_PROGRAM.md`](architecture/ARCHITECTURE_EVOLUTION_PROGRAM.md),
  [`ARCHITECTURE_MODERNIZATION_PLAN.md`](architecture/ARCHITECTURE_MODERNIZATION_PLAN.md),
  [`ARCHITECTURE_STANDARD_HARDENING_PLAN.md`](architecture/ARCHITECTURE_STANDARD_HARDENING_PLAN.md).
  Предложения по будущей унификации инструкций разных AI-инструментов вынесены отдельно в
  [`AGENT_DOCUMENTATION_OPTIMIZATION_PROPOSAL.md`](architecture/AGENT_DOCUMENTATION_OPTIMIZATION_PROPOSAL.md)
  и не являются поручением на миграцию.

- **⭐ Норматив UI**: [`ui-standard.md`](architecture/ui-standard.md) — единственный
  нормативный документ фронта: нумерованные правила UI-0XX с пометкой, кто их проверяет.
  Начинать отсюда. Фактическое состояние (что за классы реально есть, где мёртвое,
  какие токены сломаны) — `UI_REGISTRY.md` в корне репозитория, он генерируется из кода.
- **Страницы**: [`detail-page-standard.md`](architecture/detail-page-standard.md) (detail-страницы:
  PageFrame + MVVM + CardAnimated),
  [`edit-details-mvvm-standard.md`](architecture/edit-details-mvvm-standard.md) (редактируемые формы),
  [`details-page-layout-standard.md`](architecture/details-page-layout-standard.md),
  [`frontend-page-standards.md`](architecture/frontend-page-standards.md),
  [`css-page-structure.md`](architecture/css-page-structure.md) (грамматика классов, слои CSS)
- **Компоненты**: [`table-standards.md`](architecture/table-standards.md) — с 2026-08-14
  **пояснительное приложение**, а не норматив: ценен разбором реактивных антипаттернов Leptos;
  [`modal-ui-standard.md`](architecture/modal-ui-standard.md) (см. ADR-0004),
  [`thaw-ui-standard.md`](architecture/thaw-ui-standard.md),
  [`document-general-ledger-tab-standard.md`](architecture/document-general-ledger-tab-standard.md)
- **Слои и объекты**: [`domain-layer-architecture.md`](architecture/domain-layer-architecture.md),
  [`aggregate-structure-final.md`](architecture/aggregate-structure-final.md),
  [`metadata-system.md`](architecture/metadata-system.md) (см. ADR-0001),
  [`data-view-system.md`](architecture/data-view-system.md),
  [`naming-conventions.md`](architecture/naming-conventions.md),
  [`tab-registry-reference.md`](architecture/tab-registry-reference.md)

> Выведены в `_archive/architecture/` (2026-08-14): `list-standard.md` — предписывал
> 5 классов, которых нет в коде; `UI_STANDARDS_README.md` — changelog миграции
> декабря 2025, противоречил `table-standards.md` по `.table__header-cell--right`.

### `runbooks/` — пошаговые сценарии (16 файлов)
Повторяющиеся задачи. **Загляни сюда прежде, чем делать вручную:**

| Задача | Runbook |
|---|---|
| Новый агрегат `a0XX` | [`RB_add-new-aggregate-sdd_v1.md`](runbooks/RB_add-new-aggregate-sdd_v1.md) |
| Миграция БД | [`RB_db-migration-workflow_v1.md`](runbooks/RB_db-migration-workflow_v1.md) |
| Добавить метаданные агрегату | [`RB__metadata-add-to-aggregate__v1.md`](runbooks/RB__metadata-add-to-aggregate__v1.md) |
| Новое регламентное задание | [`RB__scheduled-tasks-implementation__v1.md`](runbooks/RB__scheduled-tasks-implementation__v1.md) |
| Перенести DTO в contracts | [`RB__move-dto-to-contracts__v1.md`](runbooks/RB__move-dto-to-contracts__v1.md) |
| Рефакторинг api/handlers | [`RB__api-handlers-refactoring__v1.md`](runbooks/RB__api-handlers-refactoring__v1.md) |
| Quality-check по агрегату | [`RB__generate-aggregate-qc-json__v1.md`](runbooks/RB__generate-aggregate-qc-json__v1.md) |
| Кончилось место под сборки | [`RB_recover-disk-space-rust-builds_v1.md`](runbooks/RB_recover-disk-space-rust-builds_v1.md) |

Остальные (`RB-thaw-*`, `RB-page-refactoring-to-bem-thaw`, `RB-pivot-dashboard-pattern`,
`RB-code-duplication-detection`, `RB_llm-chat-enhancement`, `RB__details-form-design-pattern`)
— из периода миграции на Thaw UI, применимы точечно.

### `known-issues/` (15) и `lessons/` (22) — грабли
Что ломалось и чему научились: Leptos-замыкания и владение, особенности Thaw, WASM-сборка,
организация модулей. Имя файла = суть проблемы + дата. Полезно грепать по симптому
(`closure`, `ownership`, `thaw`, `wasm`), а не читать подряд.

### `code-standards/` — конвенции кодирования
[`code-quality-rules.md`](code-standards/code-quality-rules.md). Конвенции живут здесь,
а **не** в `decisions/` — ADR только для трудно-обратимых решений с реальным разменом.

### `features/` (5) и `templates/` (1)
Разбор отдельных фич (u501 импорт из 1С, picker агрегатов) и шаблоны страниц.

### `todo/` — незакрытые замыслы
Сейчас только `sales-register/`. Реализованное отсюда уезжает в `_archive/todo/`.

### `_archive/` — снято с обращения
Debrief'ы сессий, отменённые ADR (`_archive/decisions/`, они теряют номер — см. конвенцию),
устаревшие стандарты (`_archive/architecture/`), реализованные планы (`_archive/todo/`)
и статусные файлы memory-bank-шаблона (`_archive/status/`: activeContext, progress,
projectbrief, productContext, systemPatterns, techContext — велись до середины 2026,
их роль перешла к ARCHITECTURE.md и авто-памяти).

**Читать из `_archive/` для справки можно, опираться — нет.**

## Что куда класть

- Решил трудно-обратимое, с альтернативами → `decisions/` (по конвенции из его README)
- Описал, как делать повторяющееся → `runbooks/`
- Наступил на грабли → `known-issues/`, вынес урок → `lessons/`
- Договорился о стиле кода → `code-standards/`
- Файл перестал быть правдой → `_archive/`, а не правка задним числом

Не клади сюда то, что генерируется (каталог объектов, API-роуты, схемы таблиц) — это
ARCHITECTURE.md, он обновляется хуком и не расходится с кодом.
