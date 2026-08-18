# EditDetails MVVM Standard

Стандарт для **редактируемых** форм details в проекте Integrator (MPI).

> **Область действия.** Этот документ дополняет `detail-page-standard.md`
> (просмотровые detail-страницы) частью про редактирование: ViewModel с
> двусторонней привязкой, вложенные таблицы, lazy loading вкладок.
> Общая структура страницы, TabBar и раскладка карточек берутся из
> `detail-page-standard.md` и `ui-standard.md` — при расхождении правы они.

## Обзор

Паттерн MVVM для редактируемых форм с:

- Разделением UI по вкладкам (tabs)
- Максимальным использованием THAW компонентов
- Поддержкой вложенных таблиц
- Lazy loading для данных вкладок

## Структура файлов

```
details/
├── mod.rs              # pub mod + pub use XxxDetails
├── model.rs            # API: fetch, save, delete + DTOs
├── view_model.rs       # ViewModel: RwSignals для полей, команды, валидация
├── page.rs             # Главный компонент: header + tabs + tab routing
├── dimension_input.rs  # (опционально) Специфичные input компоненты
└── tabs/               # UI вкладок (основное дробление)
    ├── mod.rs          # pub mod + pub use
    ├── general.rs      # GeneralTab - основные поля
    ├── dimensions.rs   # DimensionsTab - измерения (если есть)
    └── barcodes.rs     # BarcodesTab - вложенные таблицы
```

## model.rs - API слой

```rust
// DTOs для API
pub struct XxxBarcodeDto {
    pub barcode: String,
    pub source: String,
    pub is_active: bool,
    // ...
}

// API функции
pub async fn fetch_by_id(id: String) -> Result<Xxx, String>;
pub async fn save_form(dto: XxxFormDto) -> Result<String, String>;
pub async fn delete_by_id(id: String) -> Result<(), String>;

// Вспомогательные загрузки
pub async fn fetch_barcodes(id: String) -> Result<Vec<XxxBarcodeDto>, String>;
pub async fn fetch_dimension_values() -> Result<DimensionValuesResponse, String>;
```

## view_model.rs - ViewModel

**Ключевое**: поля формы как отдельные `RwSignal` (не один `RwSignal<DTO>`).
Это нужно для двухсторонней привязки с THAW Input/Textarea.

```rust
#[derive(Clone)]
pub struct NomenclatureDetailsVm {
    // === Поля формы (отдельные RwSignals для THAW) ===
    pub id: RwSignal<Option<String>>,
    pub description: RwSignal<String>,
    pub article: RwSignal<String>,
    // ... остальные поля

    // === Вложенные данные (таблицы) ===
    pub barcodes: RwSignal<Vec<BarcodeDto>>,
    pub barcodes_loaded: RwSignal<bool>,
    pub barcodes_loading: RwSignal<bool>,

    // === Reference data (dropdown options) ===
    pub dimension_options: RwSignal<Option<DimensionValuesResponse>>,

    // === UI State ===
    pub active_tab: RwSignal<&'static str>,
    pub loading: RwSignal<bool>,
    pub saving: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
    pub success: RwSignal<Option<String>>,
}

impl NomenclatureDetailsVm {
    pub fn new() -> Self { ... }

    // === Derived signals ===
    pub fn is_edit_mode(&self) -> Signal<bool>;
    pub fn is_valid(&self) -> Signal<bool>;
    pub fn is_save_disabled(&self) -> Signal<bool>;

    // === Загрузка ===
    pub fn load(&self, id: String);
    pub fn load_barcodes(&self);  // lazy по active_tab
    pub fn load_dimension_options(&self);

    // === Валидация ===
    pub fn validate(&self) -> Result<(), String>;

    // === Команды ===
    pub fn save(&self, on_saved: Callback<()>);
    pub fn reset(&self);

    // === Tab helpers ===
    pub fn set_tab(&self, tab: &'static str);
    pub fn get_dim_options(&self, dim: &str) -> Signal<Vec<String>>;

    // === Private ===
    fn to_dto(&self) -> XxxFormDto;
    fn from_aggregate(&self, agg: &XxxAggregate);
}
```

## page.rs - Главный компонент

Тонкая обертка с tab routing и lazy loading:

```rust
#[component]
pub fn NomenclatureDetails(
    id: Option<String>,
    #[prop(into)] on_saved: Callback<()>,
    #[prop(into)] on_cancel: Callback<()>,
) -> impl IntoView {
    let vm = NomenclatureDetailsVm::new();

    // Load data
    vm.load_dimension_options();
    if let Some(existing_id) = id {
        vm.load(existing_id);
    }

    // Lazy loading по active_tab
    Effect::new({
        let vm = vm.clone();
        move || {
            if vm.active_tab.get() == "barcodes" && !vm.barcodes_loaded.get() {
                vm.load_barcodes();
            }
        }
    });

    view! {
        <div class="details-container">
            <Header vm=vm.clone() on_saved=on_saved on_cancel=on_cancel />
            <div class="modal-body">
                <ErrorDisplay vm=vm.clone() />
                <TabBar vm=vm.clone() />
                <TabContent vm=vm.clone() />
            </div>
        </div>
    }
}
```

## TabBar - переключатель вкладок

> **УСТАРЕЛО.** Раньше здесь предписывался Thaw `Flex` + `Button` с инлайн-стилем
> на контейнере. Действующее правило — **UI-003**: TabBar это `div.page__tabs` +
> нативные `button.page__tab`, расположенный **между** header и `page__content`.
> Шаблон — `detail-page-standard.md`, правило — `ui-standard.md`.
> Остальная часть этого документа (ViewModel, двусторонняя привязка к THAW-инпутам,
> вложенные таблицы, lazy loading вкладок) актуальна.

## tabs/\*.rs - UI вкладок

Каждая вкладка - отдельный файл, получает `vm: XxxDetailsVm`:

```rust
// tabs/general.rs
#[component]
pub fn GeneralTab(vm: NomenclatureDetailsVm) -> impl IntoView {
    view! {
        <div class="details-section">
            <h4 class="details-section__title">"Основные поля"</h4>
            <div class="details-grid--3col">
                <div class="form__group" style="grid-column: 1 / -1;">
                    <label class="form__label">"Наименование *"</label>
                    <Input value=vm.description placeholder="Введите наименование" />
                </div>
                // ... остальные поля
            </div>
        </div>
    }
}

// tabs/barcodes.rs - с вложенной таблицей
#[component]
pub fn BarcodesTab(vm: NomenclatureDetailsVm) -> impl IntoView {
    view! {
        <Show when=move || vm.barcodes_loading.get()>
            <Spinner />
        </Show>
        <Show when=move || !vm.barcodes_loading.get()>
            <Table>
                // ... THAW Table с данными
            </Table>
        </Show>
    }
}
```

## THAW компоненты

| Элемент             | THAW компонент                                    | Примечание              |
| ------------------- | ------------------------------------------------- | ----------------------- |
| Текстовое поле      | `Input value=RwSignal<String>`                    | Двухсторонняя привязка  |
| Readonly поле       | `Input value=RwSignal<String> attr:readonly=true` | Выделение/копирование ✓ |
| Многострочный текст | `Textarea value=RwSignal<String>`                 | resize=Vertical         |
| Чекбокс             | `Checkbox checked=RwSignal<bool>`                 | label="Текст"           |
| Кнопки              | `Button appearance=Primary/Secondary`             | icon prefix             |
| Карточка            | `Card`                                            | Группировка полей       |
| Layout              | `Flex vertical=true gap=FlexGap::Medium`          | Вместо div+styles       |
| Метка               | `Label`                                           | Для полей формы         |
| Таблица             | `Table/TableHeader/TableBody/TableRow/TableCell`  | Вложенные данные        |
| Spinner             | `Spinner`                                         | Загрузка                |
| Badge               | `Badge appearance=Tint color=Brand`               | Статусы                 |
| Tab bar             | `Flex` + `Button` с dynamic appearance            | Segmented control style |

> **readonly vs disabled**: Для read-only форм используйте `attr:readonly=true` вместо `disabled=true`.
> Readonly позволяет выделять и копировать текст, disabled - нет.
> CSS стили для readonly в `thaw-patches.css`.

## Shared utilities

### clipboard.rs

```rust
use crate::shared::clipboard::copy_to_clipboard;

// Использование
copy_to_clipboard(&some_text);
```

### api_utils.rs

```rust
use crate::shared::api_utils::api_base;

// Использование
let url = format!("{}/api/nomenclature/{}", api_base(), id);
```

## Lazy loading для нескольких таблиц

```rust
// В view_model.rs
pub barcodes: RwSignal<Vec<BarcodeDto>>,
pub barcodes_loaded: RwSignal<bool>,

pub prices: RwSignal<Vec<PriceDto>>,
pub prices_loaded: RwSignal<bool>,

pub stocks: RwSignal<Vec<StockDto>>,
pub stocks_loaded: RwSignal<bool>,

// В page.rs
Effect::new(move || {
    match vm.active_tab.get() {
        "barcodes" if !vm.barcodes_loaded.get() => vm.load_barcodes(),
        "prices" if !vm.prices_loaded.get() => vm.load_prices(),
        "stocks" if !vm.stocks_loaded.get() => vm.load_stocks(),
        _ => {}
    }
});
```

## Референсная реализация

Эталонная реализация: `crates/frontend/src/domain/a004_nomenclature/ui/details/`

## Тиражирование

При создании новой формы details:

1. Скопировать структуру из `a004_nomenclature/ui/details/`
2. Адаптировать:
   - Поля в `view_model.rs`
   - Вкладки в `tabs/`
   - API в `model.rs`
3. Обновить `mod.rs`
