# Стандарт форм детальных записей (Detail Form Standard)

## Обзор

Единый стандарт для всех форм просмотра и редактирования детальных записей.

**Дата создания:** 2025-12-19  
**Версия:** 1.0

---

## Структура UI

```
┌─────────────────────────────────────────────────────────────────┐
│ Header (fixed, gradient)                                         │
│ [Статус Badge] Документ №123 от 01.12.24    [✓] [✗] [✕]        │
├─────────────────────────────────────────────────────────────────┤
│ Tabs: [Общие] [Товары] [Raw JSON] [Проекции]                   │
├─────────────────────────────────────────────────────────────────┤
│ Content (scrollable)                                             │
│                                                                  │
│ [Содержимое активной закладки]                                  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## BEM Методология (ОБЯЗАТЕЛЬНО)

### Правила именования для форм

Все классы форм следуют BEM: `.detail-form__element--modifier`

```css
/* Блок detail-form */
.detail-form
.detail-form__header
.detail-form__content

/* Элементы tabs */
.detail-tabs
.detail-tabs__item
.detail-tabs__item--active

/* Элементы field-row */
.field-row
.field-row__label
.field-row__value
.field-row__value--nowrap
.field-row__value--mono;
```

### ❌ Запрещено

```css
.detailForm {
} /* camelCase */
.detail-header {
} /* kebab-case без BEM */
.form-header {
} /* без префикса блока */
```

### ✅ Правильно

```rust
<div class="detail-form">
<div class="detail-form__header">
<div class="field-row">
<span class="field-row__label">
<span class="field-row__value field-row__value--nowrap">
```

---

## Обязательные элементы

### 1. Header (фиксированный)

**Содержимое:**

- **Левая часть:**
  - Статус-badge (проведён/не проведён)
  - Заголовок с номером и датой документа
- **Правая часть:**
  - Кнопка "Post" (✓) - провести документ
  - Кнопка "Unpost" (✗) - отменить проведение
  - Кнопка "Закрыть" (✕)

**BEM классы:**

```css
.detail-form__header
  .detail-form__header-left
  .detail-form__header-right
  .detail-form__title;
```

### 2. Tabs (закладки)

**Стандартные закладки:**

1. **"Общие"** - основные поля документа
2. **"Табличная часть"** - товары/строки документа (если есть)
3. **"Raw JSON"** - исходные данные от API (если применимо)
4. **"Проекции"** - записи в p900/p902/p904 (если документ проведён)

**BEM классы:**

```css
.detail-tabs .detail-tabs__item .detail-tabs__item--active .detail-tabs__badge;
```

### 3. Content (прокручиваемый)

**Содержит:**

- Поля для просмотра/редактирования
- Используется `.field-row` для каждого поля
- Может содержать вложенные таблицы, json-preview и т.д.

**BEM классы:**

```css
.detail-form__content .field-row .field-row__label .field-row__value;
```

---

## Код компонента

### Базовая структура

```rust
use leptos::prelude::*;
use crate::shared::components::ui::badge::Badge;
use crate::shared::components::ui::button::Button;
use crate::shared::icons::icon;

#[component]
pub fn FeatureDetail(
    id: Signal<Option<String>>,
    on_close: Callback<()>,
) -> impl IntoView {
    let (active_tab, set_active_tab) = signal("general".to_string());
    let (item, set_item) = signal::<Option<FeatureDto>>(None);
    let (posting_in_progress, set_posting_in_progress) = signal(false);

    // Загрузить данные
    Effect::new(move |_| {
        if let Some(id_val) = id.get() {
            spawn_local(async move {
                match fetch_item(&id_val).await {
                    Ok(data) => set_item.set(Some(data)),
                    Err(e) => log!("Error: {}", e),
                }
            });
        }
    });

    // Post document
    let post_document = move |_| {
        if let Some(item_data) = item.get() {
            set_posting_in_progress.set(true);
            spawn_local(async move {
                match post_api_call(&item_data.id).await {
                    Ok(_) => {
                        // Reload data
                        // ...
                    }
                    Err(e) => log!("Post error: {}", e),
                }
                set_posting_in_progress.set(false);
            });
        }
    };

    view! {
        <div class="detail-form">
            // Header
            <div class="detail-form__header">
                <div class="detail-form__header-left">
                    {move || {
                        if let Some(item_data) = item.get() {
                            view! {
                                <>
                                    {if item_data.is_posted {
                                        view! {
                                            <Badge variant="success".to_string()>
                                                "Проведён"
                                            </Badge>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <Badge variant="neutral".to_string()>
                                                "Не проведён"
                                            </Badge>
                                        }.into_any()
                                    }}
                                    <h2 class="detail-form__title">
                                        {format!("Документ № {} от {}", item_data.number, format_date(&item_data.date))}
                                    </h2>
                                </>
                            }.into_any()
                        } else {
                            view! { <></> }.into_any()
                        }
                    }}
                </div>
                <div class="detail-form__header-right">
                    <Button
                        variant="success".to_string()
                        on_click=Callback::new(post_document)
                        disabled=posting_in_progress.get() || move || item.get().map(|i| i.is_posted).unwrap_or(false)
                    >
                        {icon("check")} "Post"
                    </Button>
                    <Button
                        variant="warning".to_string()
                        on_click=Callback::new(unpost_document)
                        disabled=posting_in_progress.get() || move || !item.get().map(|i| i.is_posted).unwrap_or(false)
                    >
                        {icon("x")} "Unpost"
                    </Button>
                    <Button
                        variant="ghost".to_string()
                        on_click=Callback::new(move |_| on_close.run(()))
                    >
                        {icon("x")} "Закрыть"
                    </Button>
                </div>
            </div>

            // Tabs
            <div class="detail-tabs">
                <button
                    class=move || if active_tab.get() == "general" {
                        "detail-tabs__item detail-tabs__item--active"
                    } else {
                        "detail-tabs__item"
                    }
                    on:click=move |_| set_active_tab.set("general".to_string())
                >
                    "Общие"
                </button>
                <button
                    class=move || if active_tab.get() == "items" {
                        "detail-tabs__item detail-tabs__item--active"
                    } else {
                        "detail-tabs__item"
                    }
                    on:click=move |_| set_active_tab.set("items".to_string())
                >
                    "Товары"
                    {move || {
                        if let Some(item_data) = item.get() {
                            if item_data.items.len() > 0 {
                                view! {
                                    <span class="detail-tabs__badge">
                                        {item_data.items.len()}
                                    </span>
                                }.into_any()
                            } else {
                                view! { <></> }.into_any()
                            }
                        } else {
                            view! { <></> }.into_any()
                        }
                    }}
                </button>
                <button
                    class=move || if active_tab.get() == "raw" {
                        "detail-tabs__item detail-tabs__item--active"
                    } else {
                        "detail-tabs__item"
                    }
                    on:click=move |_| set_active_tab.set("raw".to_string())
                >
                    "Raw JSON"
                </button>
                {move || {
                    if item.get().map(|i| i.is_posted).unwrap_or(false) {
                        view! {
                            <button
                                class=move || if active_tab.get() == "projections" {
                                    "detail-tabs__item detail-tabs__item--active"
                                } else {
                                    "detail-tabs__item"
                                }
                                on:click=move |_| set_active_tab.set("projections".to_string())
                            >
                                "Проекции"
                            </button>
                        }.into_any()
                    } else {
                        view! { <></> }.into_any()
                    }
                }}
            </div>

            // Content
            <div class="detail-form__content">
                {move || match active_tab.get().as_str() {
                    "general" => view! { <GeneralTab item=item /> }.into_any(),
                    "items" => view! { <ItemsTab item=item /> }.into_any(),
                    "raw" => view! { <RawJsonTab item=item /> }.into_any(),
                    "projections" => view! { <ProjectionsTab item=item /> }.into_any(),
                    _ => view! { <></> }.into_any(),
                }}
            </div>
        </div>
    }
}
```

---

## Закладка "Общие" (General Tab)

### Структура

Используем `.field-row` для каждого поля:

```rust
#[component]
fn GeneralTab(item: Signal<Option<FeatureDto>>) -> impl IntoView {
    view! {
        <div class="general-tab">
            {move || {
                if let Some(item_data) = item.get() {
                    view! {
                        <>
                            <div class="field-row">
                                <span class="field-row__label">"Номер документа:"</span>
                                <span class="field-row__value">{item_data.number}</span>
                            </div>

                            <div class="field-row">
                                <span class="field-row__label">"Дата:"</span>
                                <span class="field-row__value">{format_date(&item_data.date)}</span>
                            </div>

                            <div class="field-row">
                                <span class="field-row__label">"Организация:"</span>
                                <span class="field-row__value">{item_data.organization}</span>
                            </div>

                            <div class="field-row">
                                <span class="field-row__label">"Сумма:"</span>
                                <span class="field-row__value field-row__value--nowrap">
                                    {format_number(item_data.amount)}
                                </span>
                            </div>

                            <div class="field-row">
                                <span class="field-row__label">"ID:"</span>
                                <span class="field-row__value field-row__value--mono">
                                    {item_data.id}
                                </span>
                            </div>

                            <div class="field-row">
                                <span class="field-row__label">"Создано:"</span>
                                <span class="field-row__value">{format_datetime(&item_data.created_at)}</span>
                            </div>
                        </>
                    }.into_any()
                } else {
                    view! { <p>"Загрузка..."</p> }.into_any()
                }
            }}
        </div>
    }
}
```

### CSS для field-row

```css
/* BEM: Строка поля */
.field-row {
  display: grid;
  grid-template-columns: 180px 1fr;
  gap: var(--spacing-md);
  align-items: start;
  margin-bottom: var(--spacing-sm);
  font-size: var(--font-size-sm);
}

/* BEM: Label поля */
.field-row__label {
  font-weight: var(--font-weight-semibold);
  color: var(--color-text-secondary);
  text-align: right;
  padding-right: var(--spacing-sm);
}

/* BEM: Value поля */
.field-row__value {
  color: var(--color-text-primary);
}

/* BEM: Модификаторы value */
.field-row__value--nowrap {
  white-space: nowrap;
}

.field-row__value--mono {
  font-family: "Courier New", monospace;
  font-size: var(--font-size-sm);
  background: var(--color-bg-secondary);
  padding: var(--spacing-xs) var(--spacing-sm);
  border-radius: var(--radius-sm);
}

.field-row__value--mono-small {
  font-family: "Courier New", monospace;
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 200px;
}
```

---

## Закладка "Товары" (Items Tab)

### Табличная часть документа

```rust
#[component]
fn ItemsTab(item: Signal<Option<FeatureDto>>) -> impl IntoView {
    view! {
        <div class="items-tab">
            {move || {
                if let Some(item_data) = item.get() {
                    if item_data.items.is_empty() {
                        view! {
                            <p class="info-message">"Нет товарных позиций"</p>
                        }.into_any()
                    } else {
                        view! {
                            <div class="table">
                                <table class="table__data table--striped">
                                    <thead class="table__head">
                                        <tr>
                                            <th class="table__header-cell">"№"</th>
                                            <th class="table__header-cell">"Товар"</th>
                                            <th class="table__header-cell table__header-cell--right">"Кол-во"</th>
                                            <th class="table__header-cell table__header-cell--right">"Цена"</th>
                                            <th class="table__header-cell table__header-cell--right">"Сумма"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {item_data.items.iter().enumerate().map(|(idx, row)| {
                                            view! {
                                                <tr class="table__row">
                                                    <td class="table__cell">{idx + 1}</td>
                                                    <td class="table__cell">{row.product_name.clone()}</td>
                                                    <td class="table__cell table__cell--right">{row.quantity}</td>
                                                    <td class="table__cell table__cell--right">{format_number(row.price)}</td>
                                                    <td class="table__cell table__cell--right">{format_number(row.amount)}</td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                    <tfoot>
                                        <tr class="table__totals-row">
                                            <td class="table__cell" colspan="2">"Итого:"</td>
                                            <td class="table__cell table__cell--right">
                                                {item_data.items.iter().map(|i| i.quantity).sum::<i32>()}
                                            </td>
                                            <td class="table__cell"></td>
                                            <td class="table__cell table__cell--right">
                                                {format_number(item_data.items.iter().map(|i| i.amount).sum::<f64>())}
                                            </td>
                                        </tr>
                                    </tfoot>
                                </table>
                            </div>
                        }.into_any()
                    }
                } else {
                    view! { <p>"Загрузка..."</p> }.into_any()
                }
            }}
        </div>
    }
}
```

---

## Закладка "Raw JSON"

### Отображение исходных данных

```rust
#[component]
fn RawJsonTab(item: Signal<Option<FeatureDto>>) -> impl IntoView {
    view! {
        <div class="raw-json-tab">
            {move || {
                if let Some(item_data) = item.get() {
                    if let Some(ref raw_json) = item_data.raw_json {
                        view! {
                            <>
                                <div class="json__header">
                                    "Исходные данные от API"
                                </div>
                                <pre class="json__content">
                                    {raw_json}
                                </pre>
                            </>
                        }.into_any()
                    } else {
                        view! {
                            <p class="info-message">"Нет исходных данных"</p>
                        }.into_any()
                    }
                } else {
                    view! { <p>"Загрузка..."</p> }.into_any()
                }
            }}
        </div>
    }
}
```

### CSS для JSON

```css
/* BEM: JSON preview */
.json__header {
  padding: var(--spacing-md);
  background: var(--color-bg-secondary);
  border-radius: var(--radius-sm) var(--radius-sm) 0 0;
  font-size: var(--font-size-sm);
  font-weight: var(--font-weight-semibold);
  color: var(--color-text-primary);
}

.json__content {
  background: var(--color-code-bg);
  padding: var(--spacing-lg);
  border: 1px solid var(--color-border);
  border-radius: 0 0 var(--radius-sm) var(--radius-sm);
  overflow-x: auto;
  font-family: "Courier New", monospace;
  font-size: var(--font-size-xs);
  line-height: 1.5;
  max-height: 600px;
  overflow-y: auto;
  color: var(--color-text-primary);
  margin: 0;
}

.json__preview {
  font-family: "Courier New", monospace;
  font-size: var(--font-size-xs);
  background: var(--color-code-bg);
  padding: var(--spacing-sm);
  border-radius: var(--radius-sm);
  overflow-x: auto;
  margin: 0;
  color: var(--color-text-primary);
}
```

---

## Закладка "Проекции"

### Показывать только если документ проведён

```rust
#[component]
fn ProjectionsTab(item: Signal<Option<FeatureDto>>) -> impl IntoView {
    let (projections, set_projections) = signal::<Vec<ProjectionRow>>(Vec::new());
    let (loading, set_loading) = signal(false);

    // Загрузить проекции
    Effect::new(move |_| {
        if let Some(item_data) = item.get() {
            if item_data.is_posted {
                set_loading.set(true);
                spawn_local(async move {
                    match fetch_projections(&item_data.id).await {
                        Ok(data) => set_projections.set(data),
                        Err(e) => log!("Error loading projections: {}", e),
                    }
                    set_loading.set(false);
                });
            }
        }
    });

    view! {
        <div class="projections-tab">
            {move || {
                if loading.get() {
                    view! { <p>"Загрузка проекций..."</p> }.into_any()
                } else if projections.get().is_empty() {
                    view! {
                        <p class="info-message">"Нет записей в проекциях"</p>
                    }.into_any()
                } else {
                    view! {
                        <div class="table">
                            <table class="table__data table--striped">
                                <thead class="table__head">
                                    <tr>
                                        <th class="table__header-cell">"Регистр"</th>
                                        <th class="table__header-cell">"Дата"</th>
                                        <th class="table__header-cell">"Товар"</th>
                                        <th class="table__header-cell table__header-cell--right">"Количество"</th>
                                        <th class="table__header-cell table__header-cell--right">"Сумма"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {projections.get().into_iter().map(|proj| {
                                        view! {
                                            <tr class="table__row">
                                                <td class="table__cell">{proj.register_name}</td>
                                                <td class="table__cell">{format_date(&proj.date)}</td>
                                                <td class="table__cell">{proj.product}</td>
                                                <td class="table__cell table__cell--right">{proj.quantity}</td>
                                                <td class="table__cell table__cell--right">{format_number(proj.amount)}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }
            }}
        </div>
    }
}
```

---

## CSS классы (полный список)

### Detail Form

```css
/* Блок */
.detail-form {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--color-bg-primary);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-sm);
  min-height: 0;
}

/* Header */
.detail-form__header {
  background: var(--detail-form-header-bg);
  padding: var(--spacing-md) var(--spacing-xl);
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-shrink: 0;
  border-radius: var(--radius-lg) var(--radius-lg) 0 0;
}

.detail-form__header-left {
  display: flex;
  align-items: center;
  gap: var(--spacing-md);
}

.detail-form__header-right {
  display: flex;
  gap: var(--spacing-sm);
}

.detail-form__title {
  margin: 0;
  font-size: var(--font-size-lg);
  font-weight: var(--font-weight-semibold);
  color: var(--detail-form-header-text);
}

/* Content */
.detail-form__content {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
  padding: var(--spacing-lg);
  background: var(--color-bg-primary);
}
```

### Detail Tabs

```css
/* Tabs контейнер */
.detail-tabs {
  display: flex;
  gap: var(--spacing-xs);
  padding: var(--spacing-sm) var(--spacing-lg);
  background: var(--color-bg-secondary);
  border-bottom: 1px solid var(--color-border);
  position: sticky;
  top: 0;
  z-index: var(--z-header);
  flex-shrink: 0;
}

/* Tab элемент */
.detail-tabs__item {
  display: flex;
  align-items: center;
  gap: var(--spacing-xs);
  padding: var(--spacing-sm) var(--spacing-lg);
  border: none;
  border-radius: var(--radius-lg);
  cursor: pointer;
  font-weight: var(--font-weight-medium);
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
  background: transparent;
  transition: all var(--transition-fast);
}

.detail-tabs__item:hover {
  background: rgba(59, 130, 246, 0.1);
  color: var(--color-text-primary);
}

/* Активный tab */
.detail-tabs__item--active {
  background: var(--color-primary);
  color: white;
}

.detail-tabs__item--active:hover {
  background: var(--btn-primary-hover);
}

/* Badge в табе */
.detail-tabs__badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 18px;
  padding: 0 var(--spacing-xs);
  font-size: var(--font-size-xs);
  font-weight: var(--font-weight-semibold);
  border-radius: var(--radius-full);
  background: var(--color-border);
  color: var(--color-text-secondary);
}

.detail-tabs__item--active .detail-tabs__badge {
  background: rgba(255, 255, 255, 0.3);
  color: white;
}
```

### Field Row

```css
/* Строка поля */
.field-row {
  display: grid;
  grid-template-columns: 180px 1fr;
  gap: var(--spacing-md);
  align-items: start;
  margin-bottom: var(--spacing-sm);
  font-size: var(--font-size-sm);
}

/* Label */
.field-row__label {
  font-weight: var(--font-weight-semibold);
  color: var(--color-text-secondary);
  text-align: right;
  padding-right: var(--spacing-sm);
}

/* Value */
.field-row__value {
  color: var(--color-text-primary);
}

/* Модификаторы value */
.field-row__value--nowrap {
  white-space: nowrap;
}

.field-row__value--mono {
  font-family: "Courier New", monospace;
  font-size: var(--font-size-sm);
  background: var(--color-bg-secondary);
  padding: var(--spacing-xs) var(--spacing-sm);
  border-radius: var(--radius-sm);
}

.field-row__value--mono-small {
  font-family: "Courier New", monospace;
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 200px;
}
```

---

## Чеклист соответствия стандарту

### Обязательные элементы ✓

- [ ] Header с статус-badge и кнопками Post/Unpost/Закрыть
- [ ] Tabs для навигации между разделами
- [ ] Закладка "Общие" с field-row для каждого поля
- [ ] Закладка "Товары" (если есть табличная часть)
- [ ] Закладка "Raw JSON" (если есть исходные данные)
- [ ] Закладка "Проекции" (только для проведённых документов)

### BEM ✓

- [ ] Все классы следуют `.detail-form__element--modifier`
- [ ] Нет inline-стилей (кроме динамических)
- [ ] Используются CSS-переменные вместо hardcode
- [ ] Модификаторы используются с базовым классом
- [ ] Нет глубокой вложенности (max 2 уровня)

### UX ✓

- [ ] Header фиксированный (не прокручивается)
- [ ] Tabs sticky (остаются вверху при прокрутке)
- [ ] Content прокручивается
- [ ] Кнопки Post/Unpost disabled в зависимости от статуса
- [ ] Проекции показываются только для проведённых документов
- [ ] Loading состояния для асинхронных операций

---

## Эталонные примеры

- `a016_ym_returns/ui/details/` - Детали возврата Яндекс
- `a014_ozon_transactions/ui/details/` - Детали транзакции Ozon

---

## См. также

- [Table Standards](./table-standards.md) - Стандарты таблиц
- [Modal UI Standard](./modal-ui-standard.md) - Стандарт модальных окон
- `E:\dev\bolt\bolt-mpi-ui-redesign\BEM_MIGRATION_MAP.md` - Референс BEM
