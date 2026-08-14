# docs — гайды по фичам и планы

Разбор **отдельных** тем: как работает конкретная подсистема, как ей пользоваться, что
собираемся строить. Общая картина — в других местах:

| Нужно | Идти в |
|---|---|
| Как собрать, где что лежит, конвенции | `../CLAUDE.md` |
| Найти объект (`a0XX`, `p9XX`, роут, раздел UI) | `../ARCHITECTURE.md` |
| Что означает термин домена | `../CONTEXT.md` |
| ADR, UI-стандарты, runbook'и, грабли | `../memory-bank/` |

Статус в таблицах ниже проставлен по состоянию на **2026-08-14** сверкой с кодом.
При расхождении документа с кодом прав код.

## Подсистемы

| Документ | О чём | Статус |
|---|---|---|
| [DATASETS_TRANSFER.md](DATASETS_TRANSFER.md) | Наборы данных и перенос между экземплярами через S3 (`system/datasets/`) | актуально |
| [DEPLOYMENT_WINDOWS_SERVER.md](DEPLOYMENT_WINDOWS_SERVER.md) | Развёртывание на Windows Server | актуально |
| [database-config-system.md](database-config-system.md) | Конфигурация БД и путей данных (`[data].root`) | актуально |
| [quality-checks.md](quality-checks.md) | Quality-checks: популяция / нарушения / доля | актуально |
| [llm-quality.md](llm-quality.md) | Измеримость качества LLM: судья, голден-сет, дашборд d407 | актуально |
| [user-guide-kb-format.md](user-guide-kb-format.md) | Формат статей базы знаний (для авторов) | актуально |
| [ext-bi-wb-funnel.md](ext-bi-wb-funnel.md) | Внешний BI-доступ к воронке WB; пример для 1С — [ext-api-1c-example.txt](ext-api-1c-example.txt) | актуально |
| [general-ledger-and-analytics-projections.md](general-ledger-and-analytics-projections.md) | Связь GL с аналитическими проекциями p909–p911, инварианты | актуально; глубже — `crates/backend/src/general_ledger/llm.md` |

## Данные маркетплейсов

| Документ | О чём | Статус |
|---|---|---|
| [wb-advert-daily.md](wb-advert-daily.md) | Ежедневная статистика рекламы WB (a026) | актуально |
| [wildberries_api_investigation.md](wildberries_api_investigation.md) | Разбор поведения API WB | справочное, разовое исследование |
| [company-brief.md](company-brief.md) | Краткая справка о бизнесе: каналы, кабинеты, измерения аналитики | актуально |

## UI-гайды

| Документ | О чём | Статус |
|---|---|---|
| [list-search-sort-guide.md](list-search-sort-guide.md) | Поиск и сортировка в списках | актуально |
| [date-range-picker-guide.md](date-range-picker-guide.md) | Компонент выбора диапазона дат | актуально |
| [date-period-filtering.md](date-period-filtering.md) | Фильтрация по периоду | актуально |
| [excel-export-guide.md](excel-export-guide.md) | Экспорт в Excel | актуально |
| [d401-dashboard-implementation.md](d401-dashboard-implementation.md) | Разбор дашборда d401 WB Finance | описывает реализацию, сверяйся с кодом |

> Стандарты UI (как *должна* выглядеть страница) живут в `../memory-bank/architecture/`,
> а не здесь. В `docs/` — гайды по конкретным компонентам и фичам.

## Планы

Замыслы, кода за которыми ещё нет. У каждого в шапке — статус и дата проверки.

| План | Статус |
|---|---|
| [plans/TAURI_CLIENT_PLAN.md](plans/TAURI_CLIENT_PLAN.md) | не начато — Tauri в репозитории нет |
| [plans/DB_BACKUP_RESTORE_PLAN.md](plans/DB_BACKUP_RESTORE_PLAN.md) | заменён «Наборами данных»; перенос БД — их фаза 2 |

## `_archive/`

Завершённое и устаревшее. Читать для справки можно, опираться — нет. Туда же уезжают
реализованные планы (`_archive/plans/`) с пометкой о том, где искать актуальное поведение.

## Что куда класть

- Гайд по фиче или подсистеме → сюда, со ссылкой из этого индекса
- Стандарт «как делать страницы/таблицы/модалки» → `../memory-bank/architecture/`
- Почему выбрано трудно-обратимое решение → `../memory-bank/decisions/`
- План на будущее → `plans/`, со статусом в шапке; реализовался → `_archive/plans/`
