# known-issues — грабли, на которые уже наступали

Записи о том, что ломается и почему. Ищи **по симптому**, а не по имени файла.
Уроки более общего характера — в `../lessons/`, пошаговые сценарии — в `../runbooks/`.

Имя файла: `KI-<суть>-<дата>.md` (встречаются старые варианты с `__` — не заводить новые).
В шапке `status:` — `documented` / `active` / `resolved` / `workaround-available`.

## Индекс по симптому

### Leptos / WASM

| Симптом | Запись | Статус |
|---|---|---|
| Замыкание вызвано после `drop` при закрытии модалки | [`KI-wasm-closure-dropped-on-modal-close-2025-12-27`](KI-wasm-closure-dropped-on-modal-close-2025-12-27.md) | documented |
| `FnOnce` вместо `Fn` в реактивном контексте | [`KI__leptos-closure-fnonce-fn__2025-01-12`](KI__leptos-closure-fnonce-fn__2025-01-12.md) | pattern-established |
| Ошибки владения в замыканиях Leptos | [`KI-leptos-closure-ownership-2025-01-26`](KI-leptos-closure-ownership-2025-01-26.md) | documented |
| Проверка `Option` после перемещения значения | [`KI-rust-ownership-option-check-2025-01-29`](KI-rust-ownership-option-check-2025-01-29.md) | documented |
| Кончается место при компиляции WASM (Windows) | [`KI_disk-space-wasm-windows_2026-01-19`](KI_disk-space-wasm-windows_2026-01-19.md) | documented |

### Thaw UI

| Симптом | Запись | Статус |
|---|---|---|
| Не стилизуется таблица Thaw | [`KI-thaw-table-style-limitations-2025-12-21`](KI-thaw-table-style-limitations-2025-12-21.md) | documented |
| У Thaw Checkbox нет `disabled` | [`KI__thaw-checkbox-no-disabled__2025-01-12`](KI__thaw-checkbox-no-disabled__2025-01-12.md) | workaround-available |
| Пути к CSS в доке разошлись с реальными | [`KI__ui-standards-css-paths-drift__2025-12-29`](KI__ui-standards-css-paths-drift__2025-12-29.md) | documented |

### Бэкенд и интеграции

| Симптом | Запись | Статус |
|---|---|---|
| `JSON "EOF while parsing"` — обычно пустой ответ бэка | [`KI-json-eof-empty-response-2025-01-27`](KI-json-eof-empty-response-2025-01-27.md) | documented |
| Ошибки соединения с API Wildberries | [`KI-wb-api-connection-errors-2025-12-22`](KI-wb-api-connection-errors-2025-12-22.md) | documented |
| Несовпадение типов logprobs в `async-openai` | [`KI_openai-logprobs-type-mismatch_2026-01-17`](KI_openai-logprobs-type-mismatch_2026-01-17.md) | resolved |
| Domain-слой зависит от DTO хендлеров | [`KI__domain-depends-on-handlers__2026-01-11`](KI__domain-depends-on-handlers__2026-01-11.md) | resolved |
| Паттерны DTO на фронте | [`KI__frontend-dto-patterns__2026-01-11`](KI__frontend-dto-patterns__2026-01-11.md) | documented |

### Инструменты

| Симптом | Запись | Статус |
|---|---|---|
| StrReplace промахивается при нечётком совпадении | [`KI-strreplace-fuzzy-matching-2025-01-13`](KI-strreplace-fuzzy-matching-2025-01-13.md) | active |
| Cursor не сохранил правку на диск | [`KI__cursor-file-not-saved__2026-01-16`](KI__cursor-file-not-saved__2026-01-16.md) | documented |

---

Записи со статусом `resolved` оставлены намеренно: они объясняют, почему код выглядит так,
как выглядит. Если проблема ушла вместе с кодом — файл переезжает в `../_archive/`.
