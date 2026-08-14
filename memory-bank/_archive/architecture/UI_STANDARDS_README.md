> **АРХИВ (2026-08-14).** Документ устарел и выведен из состава стандартов.
> Действующий норматив — `memory-bank/architecture/ui-standard.md`,
> фактическое состояние — `UI_REGISTRY.md` в корне.
> Оставлен для истории; на него нельзя ссылаться как на стандарт.

# UI Standards - Итоги внедрения

**Дата:** 2025-12-19  
**Версия:** 1.0  
**Статус:** ✅ Стандарты внедрены

---

## 📋 Что реализовано

### 1. Документация

Созданы ключевые документы:

- **[table-standards.md](./table-standards.md)** - Стандарты для простых и сложных таблиц
- **[detail-page-standard.md](./detail-page-standard.md)** ⭐ **v2 (актуальный)** - Detail-страницы: PageFrame + MVVM + detail-grid + CardAnimated
- ~~detail-form-standard.md~~ - v1, устарел и перенесён в `../_archive/architecture/`; актуальное — detail-page-standard.md
- **[thaw-ui-standard.md](./thaw-ui-standard.md)** - Использование компонентов Thaw UI (Leptos 0.8)

### 2. Backend компоненты

**Обновлённые handlers с серверными итогами:**

- ✅ `handlers/a016_ym_returns.rs` - функция `calculate_totals()`
  - Структура `YmReturnsTotals` с полями: total_records, sum_items, sum_amount, returns_count, unredeemed_count
  - Итоги рассчитываются по всему датасету с учётом фильтров
- ✅ `handlers/a012_wb_sales.rs` - функция `calculate_wb_sales_totals()`
  - Структура `WbSalesTotals` с полями: total_records, sum_quantity, sum_for_pay, sum_retail_amount
  - Итоги рассчитываются по всему датасету с учётом фильтров

**Обновлённые response structures:**

```rust
pub struct PaginatedResponse {
    pub items: Vec<T>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
    pub totals: Option<ServerTotals>,  // <- НОВОЕ
}
```

### 3. Frontend компоненты

**Созданы новые переиспользуемые компоненты:**

- ✅ **`TableCheckbox`** - `src/shared/components/table_checkbox.rs`

  - Единый стиль чекбоксов для всех таблиц
  - Фиксированная ширина 40px
  - Клик не открывает detail (stop_propagation)
  - BEM классы: `.table__cell--checkbox`, `.table__checkbox`

- ✅ **`TableTotalsRow`** - `src/shared/components/table_totals_row.rs`
  - Строка итогов через Leptos children slot
  - Легко включить/выключить
  - BEM класс: `.table__totals-row`

### 4. CSS обновления

**Файл:** `static/themes/core/components.css`

Добавлены/обновлены BEM классы:

```css
/* Чекбоксы */
.table__cell--checkbox {
}
.table__header-cell--checkbox {
}
.table__checkbox {
}

/* Итоги */
.table__totals-row {
}

/* Выравнивание */
.table__cell--right {
}
.table__header-cell--right {
}
```

**Все стили следуют:**

- ✅ BEM методологии (Block\_\_Element--Modifier)
- ✅ CSS-переменным вместо hardcode
- ✅ Нет inline-стилей (кроме динамических)

### 5. Обновлённые списки

**Простые таблицы (без пагинации, все записи):**

- ✅ `a002_organization` - Организации
- ✅ `a005_marketplace` - Маркетплейсы
- ✅ `a006_connection_mp` - Подключения к маркетплейсам
- ✅ `a001_connection_1c` - Подключения 1C
- ✅ `a007_marketplace_product` - Продукты маркетплейса

**Сложные таблицы (серверная пагинация + итоги):**

- ✅ `a016_ym_returns` - **ЭТАЛОН** для сложных таблиц
  - Серверные итоги через TableTotalsRow
  - TableCheckbox компонент
  - Все BEM классы
- ✅ `a012_wb_sales` - Продажи Wildberries
  - Серверные итоги (total_records, sum_quantity, sum_for_pay, sum_retail_amount)
  - TableCheckbox компонент

**Оставшиеся списки для обновления:**

По тому же паттерну можно обновить:

- `a009_ozon_returns` - Возвраты Ozon
- `a014_ozon_transactions` - Транзакции Ozon
- `a015_wb_orders` - Заказы Wildberries
- `a011_ozon_fbo_posting` - FBO поставки Ozon
- `a010_ozon_fbs_posting` - FBS поставки Ozon
- `a013_ym_order` - Заказы Яндекс

---

## 🚀 Как использовать стандарты

### Создание простой таблицы

```rust
use crate::shared::components::table_checkbox::TableCheckbox;

view! {
    <table class="table__data table--striped">
        <thead class="table__head">
            <tr>
                <th class="table__header-cell table__header-cell--checkbox">
                    <input type="checkbox" class="table__checkbox" on:change=toggle_all />
                </th>
                <th class="table__header-cell">"Название"</th>
            </tr>
        </thead>
        <tbody>
            {move || items.get().into_iter().map(|item| {
                view! {
                    <tr class="table__row" on:click=move |_| edit(item.id)>
                        <TableCheckbox
                            checked=Signal::derive(move || selected.contains(&item.id))
                            on_change=Callback::new(move |checked| toggle(item.id, checked))
                        />
                        <td class="table__cell">{item.name}</td>
                    </tr>
                }
            }).collect_view()}
        </tbody>
    </table>
}
```

### Создание сложной таблицы с итогами

```rust
use crate::shared::components::{
    table_checkbox::TableCheckbox,
    table_totals_row::TableTotalsRow,
};

view! {
    <table class="table__data">
        <thead class="table__head">
            <tr>
                <th class="table__header-cell table__header-cell--checkbox">
                    <input type="checkbox" class="table__checkbox" on:change=toggle_all />
                </th>
                <th class="table__header-cell">"Дата"</th>
                <th class="table__header-cell table__header-cell--right">"Сумма"</th>
            </tr>

            // Строка итогов от сервера
            {move || {
                if let Some(totals) = state.get().server_totals {
                    view! {
                        <TableTotalsRow>
                            <td class="table__cell--checkbox"></td>
                            <td>{format!("Записей: {}", totals.total_records)}</td>
                            <td class="table__cell--right">{format_number(totals.sum_amount)}</td>
                        </TableTotalsRow>
                    }.into_any()
                } else {
                    view! { <></> }.into_any()
                }
            }}
        </thead>
        <tbody>
            {move || items.get().into_iter().map(|item| {
                view! {
                    <tr class="table__row" on:click=move |_| open_detail(item.id)>
                        <TableCheckbox
                            checked=Signal::derive(move || selected.contains(&item.id))
                            on_change=Callback::new(move |checked| toggle(item.id, checked))
                        />
                        <td class="table__cell">{format_date(&item.date)}</td>
                        <td class="table__cell table__cell--right">{format_number(item.amount)}</td>
                    </tr>
                }
            }).collect_view()}
        </tbody>
    </table>
}
```

### Как отключить строку итогов

```rust
// Вариант 1: if false
{move || {
    if false {  // <- поменять на false чтобы отключить
        view! { <TableTotalsRow>...</TableTotalsRow> }.into_any()
    } else {
        view! { <></> }.into_any()
    }
}}

// Вариант 2: закомментировать
// {move || {
//     if let Some(totals) = state.get().server_totals {
//         view! { <TableTotalsRow>...</TableTotalsRow> }.into_any()
//     } else {
//         view! { <></> }.into_any()
//     }
// }}
```

---

## 📊 BEM Чеклист

Перед созданием/обновлением таблицы проверьте:

### Структура классов ✓

- [ ] Все классы следуют формату `.table__element--modifier`
- [ ] Используется префикс `.table__` для всех табличных классов
- [ ] Модификаторы используются с базовым классом
- [ ] Нет глубокой вложенности (max 2 уровня)

### CSS ✓

- [ ] Используются CSS-переменные (var(--spacing-xs), var(--color-primary))
- [ ] Нет hardcode значений (4px → var(--spacing-xs))
- [ ] Нет inline-стилей (кроме динамических width для resize)

### Компоненты ✓

- [ ] Используется `TableCheckbox` для чекбоксов
- [ ] Используется `TableTotalsRow` для итогов (если нужно)
- [ ] Чекбокс в первой колонке (40px)
- [ ] Клик на чекбокс не открывает detail

---

## 🎯 Эталонные примеры

### Простая таблица

**Файл:** `crates/frontend/src/domain/a002_organization/ui/list/mod.rs`

Особенности:

- Без пагинации (все записи сразу)
- Клиентская сортировка
- TableCheckbox компонент
- Модальное окно для редактирования

### Сложная таблица

**Файл:** `crates/frontend/src/domain/a016_ym_returns/ui/list/mod.rs`

Особенности:

- Серверная пагинация
- Фильтр-панель с collapse
- TableCheckbox компонент
- TableTotalsRow с серверными итогами
- Resize колонок
- Post/Unpost batch операции
- Экспорт в Excel

---

## 📝 Следующие шаги

### Для новых списков

1. Определить тип: простая или сложная таблица
2. Следовать [table-standards.md](./table-standards.md)
3. Использовать эталонные примеры (a002_organization или a016_ym_returns)
4. Проверить BEM чеклист
5. Протестировать все функции

### Для существующих списков

Остальные списки можно обновить по тому же паттерну:

1. Добавить импорт `TableCheckbox`
2. Заменить чекбоксы на компонент
3. Если сложная таблица:
   - Добавить `server_totals` в state
   - Обновить backend handler с `calculate_totals()`
   - Использовать `TableTotalsRow`
4. Проверить BEM классы

---

## 🔗 Связанные документы

- [Table Standards](./table-standards.md) - Полный стандарт таблиц
- [Detail Page Standard](./detail-page-standard.md) - Стандарт detail-страниц
- [List Standard](./list-standard.md) - Оригинальный стандарт списков
- [Modal UI Standard](./modal-ui-standard.md) - Стандарт модальных окон
- `E:\dev\bolt\bolt-mpi-ui-redesign\BEM_MIGRATION_MAP.md` - Референс BEM

---

## ✅ Итоги

**Достигнуты цели:**

1. ✅ Два стандарта таблиц (простые/сложные)
2. ✅ Единые чекбоксы во всех таблицах (TableCheckbox)
3. ✅ Гибкая система итогов через slot (TableTotalsRow)
4. ✅ Серверные итоги по всему датасету
5. ✅ Строгое следование BEM методологии
6. ✅ CSS-переменные вместо hardcode
7. ✅ Практичная система без переусложнения

**Эталонные реализации:**

- **Простая:** a002_organization (организации)
- **Сложная:** a016_ym_returns (возвраты Яндекс)

**Система готова к развитию:** Новые таблицы могут быть созданы за 15-30 минут, следуя стандартам и используя готовые компоненты.
