# UseCase u501: Импорт из УТ 11 - Руководство пользователя

## Быстрый старт

### 1. Запуск backend

```bash
cargo run --bin backend
```

Backend запустится на `http://localhost:3000`

### 2. Создание подключения к 1С УТ 11

**Через тестовые данные:**
```bash
curl -X POST http://localhost:3000/api/connection_1c/testdata
```

**Или вручную:**
```bash
curl -X POST http://localhost:3000/api/connection_1c \
  -H "Content-Type: application/json" \
  -d '{
    "description": "УТ 11 Production",
    "url": "http://your-1c-server/ut11/odata/standard.odata",
    "login": "admin",
    "password": "password",
    "isPrimary": true
  }'
```

### 3. Получить ID подключения

```bash
curl http://localhost:3000/api/connection_1c
```

Скопируйте `id` из ответа.

### 4. Запустить импорт организаций

```bash
curl -X POST http://localhost:3000/api/u501/import/start \
  -H "Content-Type: application/json" \
  -d '{
    "connection_id": "ваш-uuid-здесь",
    "target_aggregates": ["a002_organization"]
  }'
```

Ответ:
```json
{
  "session_id": "session-uuid",
  "status": "started",
  "message": "Импорт запущен"
}
```

### 5. Отслеживание прогресса

```bash
# Замените SESSION_ID на полученный session_id
curl http://localhost:3000/api/u501/import/SESSION_ID/progress
```

Повторяйте запрос каждые 2-3 секунды до завершения импорта.

### 6. Проверка результатов

```bash
curl http://localhost:3000/api/organization
```

## Структура ответа прогресса

```json
{
  "session_id": "uuid",
  "status": "running",  // running | completed | completed_with_errors | failed
  "started_at": "2025-01-15T10:00:00Z",
  "completed_at": null,
  "aggregates": [
    {
      "aggregate_index": "a002_organization",
      "aggregate_name": "Организации",
      "status": "running",  // pending | running | completed | failed
      "processed": 150,
      "total": 200,
      "inserted": 50,
      "updated": 100,
      "errors": 0,
      "current_item": null
    }
  ],
  "total_processed": 150,
  "total_inserted": 50,
  "total_updated": 100,
  "total_errors": 0,
  "errors": []
}
```

## Поддерживаемые агрегаты

На данный момент реализован импорт:
- ✅ `a002_organization` - Организации из `Catalog_Организации`

В разработке:
- 🚧 `a003_product` - Номенклатура
- 🚧 `a004_counterparty` - Контрагенты

## Требования к 1С УТ 11

1. **Версия**: 1С:Управление торговлей 11.5+
2. **OData**: Должен быть включен OData интерфейс
3. **Доступ**: Пользователь должен иметь права на чтение справочников
4. **URL**: Формат `http://server:port/база/odata/standard.odata`

### Проверка доступности OData

```bash
curl http://your-1c-server/ut11/odata/standard.odata/Catalog_Организации \
  -u admin:password
```

Должен вернуться JSON с метаданными коллекции.

## Обработка ошибок

### Ошибка подключения

```json
{
  "status": "failed",
  "errors": [
    {
      "message": "OData request failed with status 401",
      "occurred_at": "2025-01-15T10:05:00Z"
    }
  ]
}
```

**Решение**: Проверьте URL, логин и пароль в подключении.

### Ошибки валидации

```json
{
  "status": "completed_with_errors",
  "total_errors": 5,
  "errors": [
    {
      "aggregate_index": "a002_organization",
      "message": "Failed to process organization ORG-001",
      "details": "ИНН должен содержать 10 или 12 цифр",
      "occurred_at": "2025-01-15T10:10:00Z"
    }
  ]
}
```

**Решение**: Проверьте данные в 1С, исправьте и повторите импорт.

## Производительность

- **Скорость**: ~100-500 записей/мин (зависит от сервера 1С)
- **Batch size**: 100 записей за запрос
- **Timeout**: 30 секунд на каждый HTTP-запрос

## Логирование

Backend выводит подробные логи в консоль:

```
INFO Starting import for session: uuid
INFO Fetching OData from: http://server/ut11/odata/standard.odata/Catalog_Организации?$top=100&$skip=0
INFO Organizations import completed: processed=200, inserted=50, updated=150
INFO Import completed for session: uuid
```

Для детальных логов установите `RUST_LOG=debug`:

```bash
RUST_LOG=debug cargo run --bin backend
```

## Частые вопросы

### Q: Как отменить импорт?

A: На данный момент отмена не реализована. Импорт завершится автоматически или по ошибке.

### Q: Можно ли импортировать несколько агрегатов одновременно?

A: Да, просто перечислите их в `target_aggregates`:
```json
{
  "target_aggregates": ["a002_organization", "a003_product"]
}
```

### Q: Что происходит при повторном импорте?

A: Система делает **upsert**: обновляет существующие записи и вставляет новые. Удаления не происходит.

### Q: Как настроить фоновый импорт?

A: Фоновый режим будет реализован в будущей версии. Пока используйте `mode: "interactive"`.

## Архитектура

Подробности реализации см. в документах:
- [naming-conventions.md](memory-bank/naming-conventions.md) - Соглашения об именовании
- [usecase-u501-import-from-ut.md](memory-bank/usecase-u501-import-from-ut.md) - Техническая документация

## Разработка

### Добавление нового агрегата для импорта

1. Создайте `from_ut_odata.rs` в `backend/src/domain/aXXX_your_aggregate/`
2. Определите OData модель и маппинг
3. Добавьте метод импорта в `executor.rs`
4. Зарегистрируйте в match statement

Пример см. в [a002_organization/from_ut_odata.rs](crates/backend/src/domain/a002_organization/from_ut_odata.rs)

## Поддержка

При возникновении проблем:
1. Проверьте логи backend
2. Убедитесь что 1С доступна и OData работает
3. Проверьте права пользователя в 1С

## Лицензия

Часть проекта Integrator (MPI).
