> **АРХИВ (2026-08-14).** Документ устарел и выведен из состава стандартов.
> Действующий норматив — `memory-bank/architecture/ui-standard.md`,
> фактическое состояние — `UI_REGISTRY.md` в корне.
> Оставлен для истории; на него нельзя ссылаться как на стандарт.

# Стандарт списков документов List-Standard (List Standard)

## Обзор

Единый стандарт для всех списков документов в системе. Эталонная реализация: **WB Sales (A012)**.

Все списки документов должны использовать серверную пагинацию и следовать этому стандарту.

---

## Типы таблиц в проекте (Simple vs Complex)

В проекте используется **2 вида таблиц**:

### 1) SimpleTable (простые таблицы на Thaw)

**Когда использовать:**
- Малый объём данных (обычно **1–5 строк**, иногда до десятков)
- Таблица **полностью загружает данные с сервера** одним запросом (fetch-all)
- Дальше таблица **автономно управляется на клиенте**: сортировка, выбор строк, диалоги
- **Без пагинации**

**Технические особенности:**
- `thaw::Table` + `TableHeaderCell resizable=true` (+ `min_width/max_width` при необходимости)
- Сортировка: клиентская (через `Sortable` + `Effect` как в `a006`)

**Примеры в коде:**
- `crates/frontend/src/domain/a006_connection_mp/ui/list/mod.rs` (эталон SimpleTable)
- `crates/frontend/src/domain/a001_connection_1c/ui/list/view.rs` (SimpleTable по образцу a006)

### 2) ComplexTable (сложные списки по List-Standard)

**Когда использовать:**
- Большие объёмы данных
- Нужны фильтры/периоды/итоги/массовые операции
- Обязательна **серверная пагинация (offset/limit)** и часто **persist ширин колонок**

**Примеры в коде:**
- `crates/frontend/src/domain/a012_wb_sales/ui/list/mod.rs` (эталон ComplexTable)

## Эталонные файлы

| Механизм            | Файл                                                                  | Строки    |
| ------------------- | --------------------------------------------------------------------- | --------- |
| Полный List         | `crates/frontend/src/domain/a012_wb_sales/ui/list/mod.rs`             | весь файл |
| State с пагинацией  | `crates/frontend/src/domain/a012_wb_sales/ui/list/state.rs`           | весь файл |
| Resize колонок      | `crates/frontend/src/domain/a012_wb_sales/ui/list/mod.rs`             | 191-320   |
| Пагинация UI        | `crates/frontend/src/domain/a012_wb_sales/ui/list/mod.rs`             | 907-994   |
| Итоги в header      | `crates/frontend/src/projections/p904_sales_data/ui/list/mod.rs`      | 826-846   |
| Detail с проекциями | `crates/frontend/src/domain/a014_ozon_transactions/ui/details/mod.rs` | весь файл |

---

## Структура файлов

```
domain/aXXX_feature/
├── mod.rs
└── ui/
    ├── mod.rs
    ├── list/
    │   ├── mod.rs           # List компонент
    │   └── state.rs         # Состояние списка
    └── details/
        └── mod.rs           # Detail компонент
```

---

## State (state.rs)

### Обязательные поля

```rust
use super::ItemDto;
use chrono::{Datelike, Utc};
use leptos::prelude::*;

#[derive(Clone, Debug)]
pub struct FeatureState {
    // Данные
    pub items: Vec<ItemDto>,

    // Фильтры
    pub date_from: String,
    pub date_to: String,
    pub selected_organization_id: Option<String>,  // если применимо

    // Сортировка
    pub sort_field: String,
    pub sort_ascending: bool,

    // Множественный выбор
    pub selected_ids: Vec<String>,

    // Флаг загрузки
    pub is_loaded: bool,

    // Серверная пагинация (ОБЯЗАТЕЛЬНО для всех списков)
    pub page: usize,
    pub page_size: usize,
    pub total_count: usize,
    pub total_pages: usize,
}

impl Default for FeatureState {
    fn default() -> Self {
        // Период по умолчанию: текущий месяц
        let now = Utc::now().date_naive();
        let year = now.year();
        let month = now.month();
        let month_start = chrono::NaiveDate::from_ymd_opt(year, month, 1)
            .expect("Invalid month start date");
        let month_end = if month == 12 {
            chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
                .map(|d| d - chrono::Duration::days(1))
                .expect("Invalid month end date")
        } else {
            chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
                .map(|d| d - chrono::Duration::days(1))
                .expect("Invalid month end date")
        };

        Self {
            items: Vec::new(),
            date_from: month_start.format("%Y-%m-%d").to_string(),
            date_to: month_end.format("%Y-%m-%d").to_string(),
            selected_organization_id: None,
            sort_field: "date".to_string(),
            sort_ascending: false,  // новые сначала
            selected_ids: Vec::new(),
            is_loaded: false,
            // Пагинация
            page: 0,
            page_size: 100,
            total_count: 0,
            total_pages: 0,
        }
    }
}

pub fn create_state() -> RwSignal<FeatureState> {
    RwSignal::new(FeatureState::default())
}
```

---

## List компонент

### Структура UI

```
┌─────────────────────────────────────────────────────────────────────┐
│ Header Row 1 (gradient background)                                   │
│ ┌──────────┬─────────────────────────┬───────────────────────────┐  │
│ │ Заголовок│ ⏮◀ 1/N (total) ▶⏭ [100]│ Post(n) Unpost(n) │ 📊 💾🔄│  │
│ └──────────┴─────────────────────────┴───────────────────────────┘  │
├─────────────────────────────────────────────────────────────────────┤
│ Header Row 2 (белый фон)                                            │
│ Период: [____] — [____] [-1M][0M][⋯]  Организация: [▼]  [Обновить] │
├─────────────────────────────────────────────────────────────────────┤
│ Table                                                                │
│ ┌───┬──────────┬────────────┬─────────────────────────────────────┐ │
│ │ ☐ │ Дата ▼   │ Номер ▼    │ ... остальные колонки ...          │ │
│ ├───┼──────────┼────────────┼─────────────────────────────────────┤ │
│ │   │ Итого: N │ Сумма: X   │ ... итоги по колонкам ...          │ │
│ ├───┼──────────┼────────────┼─────────────────────────────────────┤ │
│ │ ☐ │ 01.12.25 │ DOC-001    │ ...                                 │ │
│ │ ☐ │ 01.12.25 │ DOC-002    │ ...                                 │ │
│ └───┴──────────┴────────────┴─────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

### Обязательные элементы

#### 1. Header Row 1

- Заголовок списка с иконкой
- **Пагинация**: `⏮ ◀ "1 / N (total)" ▶ ⏭` + select page_size
- Кнопки Post/Unpost с счётчиком выбранных: `✓ Post (n)` / `✗ Unpost (n)`
- Кнопка Excel: `📊 Excel`
- Кнопки настроек: `🔄` (восстановить) + `💾` (сохранить)

#### 2. Header Row 2

- Фильтр периода: `DateInput` + `DateInput` + `MonthSelector`
- Дополнительные фильтры (организация, тип и т.п.)
- Кнопка "Обновить"

#### 3. Таблица

- **Колонка checkbox** (первая, class="checkbox-cell")
- **Колонка Дата** (сортируемая)
- **Колонка Номер** (сортируемая)
- Остальные колонки данных
- **Строка итогов в thead** (sticky, под заголовками)
- **Изменение ширины колонок** (class="resizable" + resize-handle)
- Переход в Detail по клику на строку

---

## CSS классы

### Таблица (tables.css)

```css
/* Колонка checkbox */
.checkbox-cell {
  width: 40px;
  text-align: center;
  padding: 4px !important;
}

/* Resizable колонки */
th.resizable {
  position: relative;
}

.resize-handle {
  position: absolute;
  right: 0;
  top: 0;
  bottom: 0;
  width: 6px;
  cursor: col-resize;
  background: transparent;
}

.resize-handle:hover {
  background: rgba(255, 255, 255, 0.4);
}

/* Строка итогов в header */
.totals-header-row td {
  font-weight: 600;
  background: #f0f0f0;
  border-bottom: 2px solid #ddd;
}
```

### Кнопки (buttons.css)

Использовать существующие классы:

- `.btn` - базовый
- `.btn-success` - Post
- `.btn-warning` - Unpost
- `.btn-excel` - Excel экспорт
- `.btn-icon-transparent` - иконки в header

---

## Изменение ширины колонок

Функция `init_column_resize` из `shared/table_utils.rs`:

```rust
/// Инициализирует изменение ширины для всех колонок с классом "resizable"
pub fn init_column_resize(table_id: &str) {
    // 1. Восстановить сохранённые ширины из localStorage
    // 2. Добавить resize-handle к каждой th.resizable
    // 3. Обработчики mousedown/mousemove/mouseup
    // 4. Сохранить ширины в localStorage при изменении
}
```

Использование:

```rust
Effect::new(move |_| {
    init_column_resize("my-table-id");
});
```

---

## Серверная пагинация

### Backend API

```
GET /api/feature/list?limit=100&offset=0&sort_by=date&sort_desc=true&date_from=...&date_to=...
```

Response:

```json
{
    "items": [...],
    "total": 5000,
    "page": 0,
    "page_size": 100,
    "total_pages": 50
}
```

### Frontend State

```rust
pub page: usize,           // текущая страница (0-based)
pub page_size: usize,      // записей на странице
pub total_count: usize,    // всего записей
pub total_pages: usize,    // всего страниц
```

### UI навигации

```rust
// Кнопки навигации
<button on:click=|_| go_to_page(0)>"⏮"</button>                    // первая
<button on:click=|_| go_to_page(page - 1)>"◀"</button>             // предыдущая
<span>"{page + 1} / {total_pages} ({total_count})"</span>          // инфо
<button on:click=|_| go_to_page(page + 1)>"▶"</button>             // следующая
<button on:click=|_| go_to_page(total_pages - 1)>"⏭"</button>      // последняя

// Выбор размера страницы
<select on:change=|ev| change_page_size(...)>
    <option value="50">"50"</option>
    <option value="100">"100"</option>
    <option value="200">"200"</option>
    <option value="500">"500"</option>
</select>
```

---

## Detail форма

### Обязательные элементы

1. **Header**

   - Заголовок с номером документа
   - Статус-бейдж (Проведен/Не проведен)
   - Кнопки Post/Unpost
   - Кнопка "Закрыть"

2. **Закладки (tabs)**
   - "Общие данные" - основные поля документа
   - "Товары/Строки" - табличная часть
   - "Raw JSON" - исходные данные (если применимо)
   - **"Проекции"** - записи в p900/p902/p904 (если документ проведён)

### CSS классы

- `.detail-form` - контейнер формы
- `.detail-form-header` - заголовок
- `.detail-tabs` - контейнер закладок
- `.detail-tab` - кнопка закладки
- `.detail-tab.active` - активная закладка
- `.status-badge` - статус-бейдж
- `.status-badge-posted` - проведён
- `.status-badge-not-posted` - не проведён

---

## Стандартные компоненты

### DateInput

```rust
use crate::shared::components::date_input::DateInput;

<DateInput
    value=Signal::derive(move || state.get().date_from)
    on_change=move |val| state.update(|s| s.date_from = val)
/>
```

### MonthSelector

```rust
use crate::shared::components::month_selector::MonthSelector;

<MonthSelector
    on_select=Callback::new(move |(from, to)| {
        state.update(|s| {
            s.date_from = from;
            s.date_to = to;
        });
    })
/>
```

---

## Чеклист соответствия стандарту

- [ ] Файл `state.rs` с полями пагинации
- [ ] Header Row 1 с пагинацией и кнопками Post/Unpost
- [ ] Header Row 2 с фильтрами и DateInput/MonthSelector
- [ ] Колонка checkbox (первая)
- [ ] Колонки Дата и Номер документа
- [ ] Сортировка колонок с индикаторами
- [ ] Строка итогов в thead
- [ ] Изменение ширины колонок (resizable)
- [ ] Переход в Detail по клику на строку
- [ ] Экспорт в Excel
- [ ] Сохранение/загрузка настроек из БД
- [ ] Detail форма с закладкой "Проекции"
