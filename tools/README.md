# tools/ — то, что исполняет процесс разработки

Здесь живёт то, что запускается **во время работы над кодом**: генераторы карт и
метрик, гейты, git-хук, цикл сборки, замер её стоимости, доступ к API локального
dev-экземпляра. Ни один из этих скриптов не знает ни про боевую БД, ни про
сервер, ни про службу.

Второй каталог скриптов — [`scripts/`](../scripts/README.md) — про операции над
средой и данными: БД, релиз, деплой, служба, распространяемые артефакты. Граница
проходит по тому, кто и когда скрипт запускает, а не по языку и не по размеру.

Правило записано в `architecture.toml` (`[conventions.script_homes]`) и
**проверяется** правилами `tools_dir_manifest` / `scripts_dir_manifest`:
состав обоих каталогов перечислен явно, и `tools/check_architecture.ps1` сверяет
перечень с деревом. Новый скрипт не положить молча — его придётся
классифицировать явно.

## Состав

| Файл | Назначение |
|---|---|
| `gen_architecture.ps1` | `ARCHITECTURE.md` — каталог объектов из кода |
| `gen_ui_registry.ps1` | `UI_REGISTRY.md` — реестр BEM-блоков из CSS + `.rs` |
| `gen_code_metrics.ps1` | `codebase_metrics.json` — метрики (запускать **после** двух генераторов выше) |
| `check_architecture.ps1` | Валидатор: сверяет дерево с `[[rules]]` из `architecture.toml` |
| `check_health.ps1` | Храповик метрик: валит коммит на регрессиях (`SKIP_HEALTH=1` — осознанный размен) |
| `check_text_encoding.py` | Ловит текст, испорченный кодировкой (UTF-8, прочитанный как CP1251), `--fix` чинит |
| `build_frontend.ps1` | Сборка фронта в `dist/` (`-CssOnly`, `-Client`, `-Release`) |
| `run_backend.ps1` | Снять занятый `backend.exe` и поднять свежую сборку (`cargo run` уже включает сборку) |
| `dev_restart.ps1` | Полный цикл: фронт → сборка бэкенда → перезапуск бэкенда в своём окне консоли (`-CssOnly`, `-SkipFrontend`, `-Foreground`, `-Detached`) |
| `dev_client.ps1` | Сборка + поднятие/перезагрузка десктоп-клиента, снятие логов консоли |
| `measure_build.ps1` | 8 замеров стоимости сборки → `build_timings.json` (10–20 мин, в хук не входит) |
| `dev_token.ps1` | JWT для локального dev-экземпляра |
| `ask_internal_chat.ps1` | Запрос во внутренний чат (a018) через API |
| `hooks/` | Версионируемые git-хуки; включаются `git config core.hooksPath tools/hooks` |

## Что запускать для сборки

| Что поменял | Команда |
|---|---|
| Бэкенд | `cargo run -p backend` — соберёт и поднимет :3000 |
| …а `backend.exe` занят | `powershell -File tools/run_backend.ps1` |
| Фронт | `powershell -File tools/build_frontend.ps1`, затем перезагрузить страницу |
| Только CSS | `powershell -File tools/build_frontend.ps1 -CssOnly` |
| Фронт **и** бэкенд разом | `powershell -File tools/dev_restart.ps1` — соберёт фронт, пересоберёт и поднимет бэкенд |
| Фронт + десктоп-клиент | `powershell -File tools/dev_client.ps1` |
| `contracts` | задевает оба крейта — обе команды |

Релиз собирается не отсюда: `scripts/build-release.ps1` строит **и бэкенд, и
фронт** сразу и пакует `deploy/` — см. [scripts/README.md](../scripts/README.md).

`hooks/` — не «жилец» каталога в смысле правила: это git-хуки, их адрес задаёт
`core.hooksPath`. Манифест смотрит только на файлы верхнего уровня.
