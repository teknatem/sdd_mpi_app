# Active Context

_Последнее обновление: 2026-05-15_

## 🎯 Текущий фокус

Добавлена закладка «Атрибуция» в карточку `a012_wb_sales` — расшифровка GL-проводки `advert_expense` через reserve-строки `p913_wb_advert_order_attr` со ссылками на исходные документы a026. Система стабильна. Актуальные следующие шаги — в разделе ниже.

## 📝 Ключевые реализованные системы

- **a012 «Атрибуция» tab** (2026-05-15): новая закладка в `a012_wb_sales_details` после «Связи». Бэкенд `GET /api/a012/wb-sales/:id/advert-attribution` возвращает все reserve-строки p913 с `order_key=srid`, обогащённые артикулом из a004 и `(document_no, document_date)` исходного a026 по `registrator_ref`; вычисляет Σ amount, GL `advert_expense` и `is_match`. Фронт: новый таб с двумя CardAnimated (Сводка + Таблица), бейджи totals/совпадения, ссылки на a026/a004, предупреждение для возвратов покупателей, подсказка для непроведённых документов. Бейдж счётчика в TabBar обновляется автоматически; reload после post/unpost.

- **a033_wb_day_close** (2026-05-14): снапшот «Закрытие дня WB» по кабинету. 1 активный + N архивных per `(connection_id, business_date)`. 10 колонок (Реализация, Реклама, Логистика, Эквайринг, Комиссия, Штрафы, Прочее, Результат, ЦенаДилер, Сравнение) из p903/p913/a012. Детектор проблем рекламной атрибуции + действие «Перепровести проблемные a012». Контракты, миграция 0108, backend domain (lines_builder, problem_detectors, repository, service), API (list/get/create/recalculate/repost/archive/compare), Frontend (список + детали с таблицей 10 колонок + панель проблем), 23 unit-теста (без DB).

- **YM Orders scheduled polling** (2026-05-05): добавлен тип регламентного задания `task013_ym_orders_polling` — адаптивный поллер заказов Яндекс.Маркета по аналогии с WB `task001`; использует `u503_import_from_yandex` только для `a013_ym_order`, отдельный progress tracker и две seed-задачи для двух YM-кабинетов.
- **Yandex Market auth/info** (2026-05-05): `a006_connection_mp` для Яндекс.Маркета использует выбранный `ТипАвторизации` (`API Key` → `Api-Key`, `OAuth 2.0` → `Authorization: Bearer`); кнопка "Информация" показывает сводку по кабинетам/магазинам через Partner API и данные API-Key токена, если доступно.
- **WB Orders → WB Supply link** (2026-05-04): в карточке заказа WB (`a015_wb_orders`) блок связанных объектов показывает ссылку на поставку WB (`a029_wb_supply`), найденную по списку заказов поставки; при отсутствии связи выводится "Поставка не найдена".
- **WB Advert Campaigns a030** (2026-04-26): отдельный справочник рекламных кампаний WB (`a030_wb_advert_campaign`) с сырым `info_json` из Advert API; `task012_wb_advert_campaigns` обновляет его ежедневно, `task011_wb_advert` берёт `advert_id` из a030 для `fullstats`. Внутри задач WB Advert используются локальные паузы между запросами (`task012`: 250 мс после списка и 3 сек между info-чанками; `task011`: 21 сек между fullstats-чанками), при `429` пишется диагностика с X-Ratelimit и дальнейшие info-чанки в запуске не отправляются. Добавлен UI: список и детали в сайдбаре `Справочники`.
- **DataView** (2026-03-12): `dv001_revenue`, самодостаточный, LLM tool `list_data_views`
- **Формальные миграции БД** (2026-02-18): `migrations/`, `sqlx::migrate::Migrator`, `_sqlx_migrations`
- **Field Metadata System** (POC): `a001_connection_1c`, JSON → `build.rs` → `metadata_gen.rs`
- **General Ledger** (2026-03): независимая система в `crates/*/src/general_ledger/`
- **Scheduled Tasks**: tokio background worker, file-based logging
- **LLM Chat**: a017-a019, tool_executor, knowledge base, default_agent.md
- **Read-only Knowledge Base UI** (2026-05-10): добавлен отдельный scope `knowledge_base`, backend API `/api/kb/stats|tree|articles/:id`, frontend workspace “База знаний”, вкладки статей `kb_article_{id}` и кликабельные внутренние ссылки `kb://article/{id}` в чате.
- **Разделение KB слоёв** (2026-05-11): `data/knowledge/` закреплён как Obsidian-база только для бизнес-знаний организации; технические сведения приложения перенесены во встроенные `llm.md`/embedded docs и отображаются во вкладке “Документация приложения”.

## 🔄 Следующие шаги

- Расширение Field Metadata на a002-a016
- Новые DataView (dv002+)
- Экспорт данных (Excel, CSV)
- Инструмент `create_bi_indicator` для LLM

## ⚠️ Критические правила (не нарушать)

- **General Ledger** — отдельная система: `crates/*/src/general_ledger/`, НЕ в `domain/`
- **Shell**: PowerShell, никогда не использовать `&&`, только `;`
- **Knowledge Base**: Obsidian (`data/knowledge/`) — только бизнес-процессы организации; SQL, DataView, агрегаты, API и tool usage должны быть embedded-документацией приложения.

## 📚 Полезные документы

- `architecture/data-view-system.md` — DataView архитектура
- `architecture/metadata-system.md` — Field Metadata система
- `architecture/list-standard.md` — стандарт списков
- `runbooks/` — пошаговые инструкции
- `known-issues/` — известные ограничения Thaw
