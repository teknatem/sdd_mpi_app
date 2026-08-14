# Detail Page Standard — v2.1

**Дата:** 2026-02-28  
**Версия:** 2.1  
**Статус:** ✅ Актуальный стандарт  
**Эталоны:** `a015_wb_orders`, `a004_nomenclature`

> Предыдущий стандарт: `../_archive/architecture/detail-form-standard.md` (v1.0, 2025-12-19) — устарел, в архиве, использует `.detail-form`, `.field-row` паттерны.  
> v2.0 → v2.1 (2026-02-28): TabBar переведён с Thaw `Flex + Button` на нативные `div.page__tabs` + `button.page__tab`; удалён "особый случай" a004 (теперь полноценный эталон); добавлен `page__tab:disabled`.

---

## Архитектурный паттерн: MVVM

Каждая detail-страница строится по паттерну **MVVM**:

```
page.rs         — PageFrame + Header + TabBar + TabContent
view_model.rs   — MyDetailsVm (RwSignal, команды)
model.rs        — DTO-структуры + API-вызовы
tabs/           — отдельный файл на каждую вкладку
  general.rs
  json.rs
  ...
```

---

## Визуальная структура

```
┌──────────────────────────────────────────────────────────────┐
│ page__header (sticky)                                        │
│  [Заголовок + Badge]               [Кнопки] [Закрыть]       │
├──────────────────────────────────────────────────────────────┤
│ page__tabs (32px, фон = заголовок)                           │
│  [Общие] [Подробно] [JSON] ...                               │
├──────────────────────────────────────────────────────────────┤
│ page__content                                                │
│  ┌─ detail-grid ─────────────────────────────────────────┐  │
│  │  ┌─ detail-grid__col ──┐  ┌─ detail-grid__col ──────┐ │  │
│  │  │ CardAnimated        │  │ CardAnimated             │ │  │
│  │  │ CardAnimated        │  │ CardAnimated             │ │  │
│  │  └─────────────────────┘  └──────────────────────────┘ │  │
│  └───────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

---

## page.rs — корневой файл страницы

```rust
use crate::layout::global_context::AppGlobalContext;
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use leptos::prelude::*;
use thaw::*;

#[component]
pub fn MyDetail(id: String, #[prop(into)] on_close: Callback<()>) -> impl IntoView {
    let vm = MyDetailsVm::new();
    vm.load(id);

    // Lazy loading для тяжёлых вкладок
    Effect::new({
        let vm = vm.clone();
        move || match vm.active_tab.get() {
            "json" if !vm.raw_json_loaded.get() => vm.load_raw_json(),
            _ => {}
        }
    });

    let vm_header  = vm.clone();
    let vm_tabs    = vm.clone();
    let vm_content = vm.clone();

    view! {
        <PageFrame page_id="aXXX_entity--detail" category="detail">
            <Header vm=vm_header on_close=on_close />

            <TabBar vm=vm_tabs />

            <div class="page__content">
                {move || {
                    if vm.loading.get() {
                        view! {
                            <Flex gap=FlexGap::Small style="align-items: center; padding: var(--spacing-4xl); justify-content: center;">
                                <Spinner />
                                <span>"Загрузка..."</span>
                            </Flex>
                        }.into_any()
                    } else if let Some(err) = vm.error.get() {
                        view! {
                            <div style="padding: var(--spacing-lg); background: var(--color-error-50); border: 1px solid var(--color-error-100); border-radius: var(--radius-sm); color: var(--color-error); margin: var(--spacing-lg);">
                                <strong>"Ошибка: "</strong>{err}
                            </div>
                        }.into_any()
                    } else if vm.data.get().is_some() {
                        view! { <TabContent vm=vm_content.clone() /> }.into_any()
                    } else {
                        view! { <div>"Нет данных"</div> }.into_any()
                    }
                }}
            </div>
        </PageFrame>
    }
}
```

> **Ключевое правило:** `<TabBar>` находится **вне** `page__content`, прямо между `<Header>` и `<div class="page__content">`. Это обеспечивает визуальное слияние полосы вкладок с заголовком страницы.

### Header

```rust
#[component]
fn Header(vm: MyDetailsVm, on_close: Callback<()>) -> impl IntoView {
    view! {
        <div class="page__header">
            <div class="page__header-left">
                <h2>{move || format!("Заголовок {}", vm.title.get())}</h2>
                // опционально: Badge со статусом
            </div>
            <div class="page__header-right">
                // опционально: кнопки действий (Save, Post/Unpost)
                <Button appearance=ButtonAppearance::Secondary on_click=move |_| on_close.run(())>
                    {icon("x")} "Закрыть"
                </Button>
            </div>
        </div>
    }
}
```

### TabBar — нативные кнопки (не Thaw Button)

```rust
#[component]
fn TabBar(vm: MyDetailsVm) -> impl IntoView {
    let active_tab = vm.active_tab;

    view! {
        <div class="page__tabs">
            <button
                class="page__tab"
                class:page__tab--active=move || active_tab.get() == "general"
                on:click={ let vm = vm.clone(); move |_| vm.set_tab("general") }
            >
                {icon("file-text")} "Основная"
            </button>

            <button
                class="page__tab"
                class:page__tab--active=move || active_tab.get() == "json"
                on:click=move |_| vm.set_tab("json")
            >
                {icon("code")} "JSON"
            </button>
        </div>
    }
}
```

**Правила TabBar:**
- Контейнер — `div.page__tabs`, нативные `button.page__tab`
- Активная вкладка — `class:page__tab--active=move || active_tab.get() == "name"`
- Иконка + текст — `{icon("name")} "Текст"` (CSS обеспечивает `gap: 0.5em`)
- Нет Thaw `<Button>`, нет `<Flex>`, нет inline `background`/`border-radius`

### Disabled-вкладки

Для вкладок, недоступных в определённых состояниях (например, до сохранения записи):

```rust
<button
    class="page__tab"
    class:page__tab--active=move || active_tab.get() == "barcodes"
    disabled=move || !is_edit_mode.get()
    on:click={ let vm = vm.clone(); move |_| vm.set_tab("barcodes") }
>
    {icon("barcode")} "Штрихкоды"
</button>
```

CSS (определён в `layout.css`):
```css
.page__tab:disabled {
  opacity: 0.35;
  cursor: not-allowed;
  pointer-events: none;
}
```

### TabContent

```rust
#[component]
fn TabContent(vm: MyDetailsVm) -> impl IntoView {
    let active_tab = vm.active_tab;
    let vm_general = vm.clone();
    let vm_json    = vm.clone();

    view! {
        {move || match active_tab.get() {
            "general" => view! { <GeneralTab vm=vm_general.clone() /> }.into_any(),
            "json"    => view! { <JsonTab    vm=vm_json.clone()    /> }.into_any(),
            _         => view! { <GeneralTab vm=vm_general.clone() /> }.into_any(),
        }}
    }
}
```

---

## Вкладка с карточками: detail-grid

### CSS-классы (layout.css)

```css
/* 2-колоночная сетка, центрирована, колонки 600–900px */
.detail-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(600px, 900px));
  gap: var(--spacing-md);
  justify-content: center;
  align-items: start;
  align-content: start;
  padding: var(--spacing-sm);
}

/* Адаптив: 1 колонка на узких экранах */
@media (max-width: 1300px) {
  .detail-grid {
    grid-template-columns: minmax(600px, 900px);
  }
}

/* Колонка внутри detail-grid */
.detail-grid__col {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
}
```

### Шаблон вкладки (general.rs)

```rust
use crate::shared::components::card_animated::CardAnimated;

#[component]
pub fn GeneralTab(vm: MyDetailsVm) -> impl IntoView {
    view! {
        {move || {
            let Some(data) = vm.data.get() else {
                return view! { <div>"Нет данных"</div> }.into_any();
            };
            // извлечь значения из data...

            view! {
                <div class="detail-grid">
                    // Левая колонка
                    <div class="detail-grid__col">
                        <CardAnimated delay_ms=0>
                            <h4 class="details-section__title">"Блок 1"</h4>
                            // ...поля...
                        </CardAnimated>

                        <CardAnimated delay_ms=80>
                            <h4 class="details-section__title">"Блок 2"</h4>
                        </CardAnimated>
                    </div>

                    // Правая колонка
                    <div class="detail-grid__col">
                        <CardAnimated delay_ms=40>
                            <h4 class="details-section__title">"Блок 3"</h4>
                        </CardAnimated>

                        <CardAnimated delay_ms=120>
                            <h4 class="details-section__title">"Блок 4"</h4>
                        </CardAnimated>
                    </div>
                </div>
            }.into_any()
        }}
    }
}
```

### Рекомендуемые задержки (stagger-эффект)

| Позиция в сетке | delay_ms |
|---|---|
| Лев. колонка, карточка 1 | 0 |
| Прав. колонка, карточка 1 | 40 |
| Лев. колонка, карточка 2 | 80 |
| Прав. колонка, карточка 2 | 120 |
| Лев. колонка, карточка 3 | 160 |
| Прав. колонка, карточка 3 | 200 |

Шахматный паттерн (0/40/80/120...) создаёт каскадное появление обеих колонок.

---

## Компонент CardAnimated

**Файл:** `crates/frontend/src/shared/components/card_animated.rs`

```rust
use crate::shared::components::card_animated::CardAnimated;

// Базовое использование
<CardAnimated delay_ms=0>
    <h4 class="details-section__title">"Заголовок"</h4>
    // ...содержимое...
</CardAnimated>

// С дополнительными стилями (нестандартная ширина и т.д.)
<CardAnimated delay_ms=0 style="max-width: 400px;">
    // ...
</CardAnimated>
```

**Замена:** `<Card attr:style="width: 600px; margin: 0;">` → `<CardAnimated delay_ms=N>`

---

## Вкладка JSON

Все три состояния (loading / данные / пусто) оборачиваются в `detail-grid`.  
Card занимает **левую колонку**, правая остаётся пустой.

```rust
use crate::shared::components::card_animated::CardAnimated;
use crate::shared::json_viewer::widget::JsonViewer;

#[component]
pub fn JsonTab(vm: MyDetailsVm) -> impl IntoView {
    view! {
        {move || {
            if vm.raw_json_loading.get() {
                return view! {
                    <div class="detail-grid">
                        <CardAnimated delay_ms=0>
                            <Flex gap=FlexGap::Small style="align-items: center; justify-content: center; padding: var(--spacing-xl);">
                                <Spinner />
                                <span>"Загрузка JSON..."</span>
                            </Flex>
                        </CardAnimated>
                    </div>
                }.into_any();
            }

            if let Some(json) = vm.raw_json.get() {
                view! {
                    <div class="detail-grid">
                        <CardAnimated delay_ms=0 style="padding: var(--spacing-sm);">
                            <h4 class="details-section__title">"JSON данные"</h4>
                            <div style="margin-bottom: var(--spacing-sm); color: var(--color-text-secondary);">
                                "Исходный ответ API для этого документа."
                            </div>
                            <div style="max-height: calc(100vh - 290px); overflow: auto;">
                                <JsonViewer json_content=json title="Raw JSON".to_string() />
                            </div>
                        </CardAnimated>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="detail-grid">
                        <CardAnimated delay_ms=0>
                            <h4 class="details-section__title">"JSON данные"</h4>
                            <div style="color: var(--color-text-secondary);">"Данные не загружены"</div>
                        </CardAnimated>
                    </div>
                }.into_any()
            }
        }}
    }
}
```

---

## Критическое правило: Flex vs div для колонок

**НЕЛЬЗЯ** использовать `<Flex>` как контейнер для колонок внутри `move ||` замыкания:

```rust
// ОШИБКА: FnOnce closure! Компилятор: "expected FnMut, found FnOnce"
view! {
    <div class="detail-grid">
        <Flex vertical=true gap=FlexGap::Medium>  // ← Flex.children = Box<dyn FnOnce()>
            <CardAnimated delay_ms=0>...</CardAnimated>
            <SomeComponent vm=vm.clone() />  // ← vm перемещается в FnOnce
        </Flex>
    </div>
}
```

**НУЖНО** использовать `<div class="detail-grid__col">`:

```rust
// ПРАВИЛЬНО: div — HTML-элемент, дети рендерятся напрямую без FnOnce
view! {
    <div class="detail-grid">
        <div class="detail-grid__col">
            <CardAnimated delay_ms=0>...</CardAnimated>
            <SomeComponent vm=vm.clone() />  // ← работает корректно
        </div>
    </div>
}
```

**Причина:** Thaw-компоненты (Card, Flex и т.д.) принимают `children: Children = Box<dyn FnOnce()>`.
Когда внутрь FnOnce-замыкания попадает `vm` (через `vm.clone()` или прямое использование),
внешний реактивный `move ||` становится `FnOnce` вместо `FnMut`. Leptos требует `FnMut`.

HTML-элементы (`<div>`, `<span>` и т.д.) этой проблемы **не создают**.

---

## Одиночная карточка (single-card таб)

Для вкладок, где одна карточка на всю ширину (таблицы, списки):

```rust
// Карточка идёт в левую колонку, правая пустая
<div class="detail-grid">
    <CardAnimated delay_ms=0>
        <h4 class="details-section__title">"Штрихкоды"</h4>
        // ...таблица...
    </CardAnimated>
</div>
```

---

## Поля внутри Card: form__group

```rust
<CardAnimated delay_ms=0>
    <h4 class="details-section__title">"Документ"</h4>

    // 2 поля в строку
    <div style="display: grid; grid-template-columns: 1fr 1fr; gap: var(--spacing-sm);">
        <div class="form__group">
            <label class="form__label">"Номер"</label>
            <Input value=RwSignal::new(doc_no) attr:readonly=true />
        </div>
        <div class="form__group">
            <label class="form__label">"Дата"</label>
            <Input value=RwSignal::new(date) attr:readonly=true />
        </div>
    </div>

    // Полная ширина
    <div class="form__group">
        <label class="form__label">"Описание"</label>
        <Input value=RwSignal::new(description) attr:readonly=true />
    </div>
</CardAnimated>
```

---

## Чеклист при создании/рефакторинге detail-страницы

### Структура файлов
- [ ] `page.rs` — PageFrame + Header + TabBar + TabContent
- [ ] `view_model.rs` — VM с RwSignal
- [ ] `model.rs` — DTO + API
- [ ] `tabs/mod.rs` — реэкспорт
- [ ] `tabs/general.rs`, `tabs/json.rs`, ... — по одному файлу на вкладку

### Rust
- [ ] `use crate::shared::components::card_animated::CardAnimated`
- [ ] `use crate::shared::icons::icon`
- [ ] `use crate::shared::page_frame::PageFrame`
- [ ] `<TabBar>` — между `<Header>` и `<div class="page__content">`, **не** внутри `page__content`
- [ ] TabBar — `div.page__tabs` + нативные `button.page__tab`, **не** Thaw `<Button>`
- [ ] `class:page__tab--active=move || active_tab.get() == "name"` для активной вкладки
- [ ] Disabled-вкладки: `disabled=move || !condition.get()`, **не** `opacity` вручную
- [ ] Колонки — `<div class="detail-grid__col">`, **не** `<Flex>`
- [ ] Нет `<Card attr:style="width: ...">` — только `<CardAnimated delay_ms=N>`
- [ ] Нет `<div style="display: grid; grid-template-columns: ...">` на уровне вкладки — только `<div class="detail-grid">`
- [ ] JSON-вкладка по шаблону выше (все 3 состояния в `detail-grid`)

### CSS
- [ ] Нет inline-стилей для ширины карточек (управляется через `detail-grid`)
- [ ] Используются `var(--spacing-*)`, `var(--color-*)` вместо hardcode
- [ ] `details-section__title` для заголовков секций внутри карточки

### UX
- [ ] Stagger-задержки по шахматному паттерну (0/40/80/120 мс)
- [ ] Spinner + "Загрузка..." пока `loading.get()` == true
- [ ] Блок ошибки при `error.get().is_some()`
- [ ] Lazy loading для тяжёлых вкладок (JSON, связанные данные) через `Effect`

---

## Эталонные реализации

| Файл | Особенность |
|---|---|
| `a015_wb_orders/ui/details/` | Полный эталон: 5 вкладок, Post/Unpost, lazy load, только чтение |
| `a004_nomenclature/ui/details/` | Эталон редактируемой формы: Save/Cancel, disabled-вкладки, слияние вкладок |

---

## Связанные документы

- `memory-bank/_archive/architecture/detail-form-standard.md` — v1 (устарел, field-row паттерн)
- `memory-bank/architecture/ui-standard.md` — **норматив UI** (правила UI-0XX)
- `UI_REGISTRY.md` (корень) — фактический реестр классов и токенов
- `memory-bank/architecture/edit-details-mvvm-standard.md` — редактируемые формы
- `memory-bank/architecture/modal-ui-standard.md` — стандарт модальных окон
- `memory-bank/architecture/css-page-structure.md` — BEM DOM-иерархия, page__tabs
- `crates/frontend/src/shared/components/card_animated.rs` — компонент
- `crates/frontend/static/themes/core/layout.css` — CSS классы detail-grid, page__tabs
