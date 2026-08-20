# _archive

Исторический слой memory-bank: записи завершённых работ, которые больше не являются
активным референсом, но сохранены для истории (Obsidian-вики-ссылки `[[...]]` на них
продолжают резолвиться по имени из остальной части базы).

- `debriefs/` — ретроспективы прошлых рабочих сессий (point-in-time, не живой референс).
- `decisions/` — ADR, снятые с активного каталога `memory-bank/decisions/` (решение осталось верным, но как ADR запись бесполезна: микро-решение либо конвенция, живущая в другом месте):
  - `thaw-transparent-background` (был ADR-0001) — прозрачный фон Thaw для forest-темы; механика в активном `runbooks/RB-thaw-css-variables-v1.md` и `lessons/LL-css-variable-timing-2025-12-20.md`
  - `shared-utilities-organization` (был ADR-0003) — правило про `frontend/src/shared/` поднято в `code-standards/code-quality-rules.md`
- `runbooks/` — пошаговые инструкции одноразовых **завершённых** миграций:
  - `RB__modal-migration-to-modalstack__v1` — переезд модалок на ModalStack (выполнено)
  - `RB__vsa-module-rename__v1` — переименование модулей под VSA (выполнено)
  - `RB__frontend-module-refactor__v1` — рефакторинг структуры модулей фронта (выполнено)
  - `RB__generate-aggregate-qc-json__v1` — расширение `domain_analisys.py` до QC-отчёта; скрипт удалён 2026-08-19, роль заняли `quality/checks/` и `codebase_metrics.json`

Активные стандарты, гайды и известные ограничения (в т.ч. по Thaw) остались в обычных
каталогах `memory-bank/`. Актуальная структура и карта объектов — в корневых
`CLAUDE.md` и `ARCHITECTURE.md`.
