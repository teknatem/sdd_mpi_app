# Система метаданных полей агрегатов

**Дата создания**: 2025-10-09
**Статус**: ✅ **РЕАЛИЗОВАНО (POC)** — 2025-12-26
**Документация**: `memory-bank/architecture/metadata-system.md`

> **Примечание**: Этот документ сохранён как исторический.
> POC реализован на `a001_connection_1c`.
> Актуальная документация: `memory-bank/architecture/metadata-system.md`

---

## Проблема

В текущей архитектуре агрегатов отсутствуют метаданные о полях:

- Русские наименования полей для UI
- Комментарии/описания полей
- Флаги обязательности для валидации
- Типы полей для построения форм
- Информация о вложенных структурах и таблицах

**Требование**: Создать систему, которая позволит:

1. Декларативно описывать метаданные полей в JSON
2. Автоматически генерировать Rust код с метаданными
3. Использовать метаданные для валидации, генерации UI, документации
4. Парсить метаданные внешними инструментами для построения дерева данных

---

## Решение: Генерация metadata.rs из metadata.json через build.rs

### Архитектура

```
crates/contracts/
├── build.rs                                   # Build script - сканер и запуск генератора
├── codegen/                                   # Библиотека генерации (опционально - отдельный крейт)
│   ├── schema.rs                             # Типы для парсинга metadata.json
│   ├── generator.rs                          # Логика генерации metadata.rs
│   └── tests.rs                              # Тесты генератора
└── src/
    └── domain/
        ├── common/
        │   ├── field_metadata.rs             # FieldMetadata, FieldType
        │   └── aggregate_root.rs             # Трейт с методом field_metadata()
        └── a001_connection_1c/
            ├── mod.rs
            ├── aggregate.rs                  # Ручной код (struct, impl)
            ├── metadata.json                 # ⭐ Исходные метаданные (декларация)
            └── metadata.rs                   # ⭐ 100% сгенерированный код
```

### Принцип работы

1. **Разработчик создает/изменяет `metadata.json`** - описывает поля агрегата
2. **Cargo запускает `build.rs`** перед компиляцией (автоматически при изменении JSON)
3. **build.rs сканирует** `src/domain/a*/metadata.json`
4. **Генератор создает** `metadata.rs` в той же папке с константами метаданных
5. **Основной код** подключает `mod metadata;` и использует `metadata::FIELDS`
6. **Внешние инструменты** читают те же `metadata.json` для анализа структуры данных

---

## Типы полей

Система поддерживает следующие типы полей:

### 1. Примитивные типы (Primitive)

```json
{
  "name": "url",
  "field_type": "primitive",
  "rust_type": "String",
  "ru_name": "URL",
  "comment": "HTTP(S) адрес базы данных",
  "required": true
}
```

**Rust код**:

```rust
pub url: String,
```

### 2. Опциональные поля (Optional)

```json
{
  "name": "comment",
  "field_type": "primitive",
  "rust_type": "String",
  "ru_name": "Комментарий",
  "comment": "Дополнительная информация",
  "required": false
}
```

**Rust код**:

```rust
pub comment: Option<String>,
```

### 3. Перечисления (Enum)

```json
{
  "name": "status",
  "field_type": "enum",
  "rust_type": "OrderStatus",
  "enum_values": ["New", "Processing", "Shipped", "Delivered", "Cancelled"],
  "ru_name": "Статус",
  "comment": "Текущий статус заказа",
  "required": true
}
```

**Rust код**:

```rust
pub status: OrderStatus,
```

### 4. Ссылки на другие агрегаты (AggregateRef)

```json
{
  "name": "organization_id",
  "field_type": "aggregate_ref",
  "rust_type": "OrganizationId",
  "ref_aggregate": "a002_organization",
  "ru_name": "Организация",
  "comment": "Ссылка на организацию-продавца",
  "required": true
}
```

**Rust код**:

```rust
pub organization_id: OrganizationId,
```

### 5. Вложенные структуры (NestedStruct)

```json
{
  "name": "contact_info",
  "field_type": "nested_struct",
  "rust_type": "ContactInfo",
  "ru_name": "Контактная информация",
  "comment": "Данные для связи с покупателем",
  "required": false,
  "nested_fields": [
    {
      "name": "phone",
      "rust_type": "String",
      "ru_name": "Телефон",
      "comment": "Контактный телефон",
      "required": true
    },
    {
      "name": "email",
      "rust_type": "String",
      "ru_name": "Email",
      "comment": "Электронная почта",
      "required": false
    }
  ]
}
```

**Rust код**:

```rust
// Сгенерированная структура
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactInfo {
    pub phone: String,
    pub email: Option<String>,
}

// Поле в агрегате
pub contact_info: Option<ContactInfo>,
```

### 6. Вложенные таблицы (NestedTable)

```json
{
  "name": "items",
  "field_type": "nested_table",
  "rust_type": "Vec<OrderItem>",
  "ru_name": "Товарные позиции",
  "comment": "Список товаров в заказе",
  "required": true,
  "nested_fields": [
    {
      "name": "nomenclature_id",
      "field_type": "aggregate_ref",
      "rust_type": "NomenclatureId",
      "ref_aggregate": "a004_nomenclature",
      "ru_name": "Номенклатура",
      "comment": "Товар",
      "required": true
    },
    {
      "name": "quantity",
      "field_type": "primitive",
      "rust_type": "i32",
      "ru_name": "Количество",
      "comment": "Количество товара",
      "required": true
    },
    {
      "name": "price",
      "field_type": "primitive",
      "rust_type": "f64",
      "ru_name": "Цена",
      "comment": "Цена за единицу",
      "required": true
    }
  ]
}
```

**Rust код**:

```rust
// Сгенерированная структура
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItem {
    pub nomenclature_id: NomenclatureId,
    pub quantity: i32,
    pub price: f64,
}

// Поле в агрегате
pub items: Vec<OrderItem>,
```

---

## Пример metadata.json (полный)

```json
{
  "aggregate": "Order",
  "aggregate_index": "a007",
  "collection_name": "order",
  "element_name": "Заказ",
  "list_name": "Заказы",
  "fields": [
    {
      "name": "number",
      "field_type": "primitive",
      "rust_type": "String",
      "ru_name": "Номер заказа",
      "comment": "Уникальный номер заказа в системе",
      "required": true
    },
    {
      "name": "date",
      "field_type": "primitive",
      "rust_type": "chrono::NaiveDate",
      "ru_name": "Дата",
      "comment": "Дата оформления заказа",
      "required": true
    },
    {
      "name": "status",
      "field_type": "enum",
      "rust_type": "OrderStatus",
      "enum_values": ["New", "Processing", "Shipped", "Delivered", "Cancelled"],
      "ru_name": "Статус",
      "comment": "Текущий статус заказа",
      "required": true
    },
    {
      "name": "organization_id",
      "field_type": "aggregate_ref",
      "rust_type": "OrganizationId",
      "ref_aggregate": "a002_organization",
      "ru_name": "Организация",
      "comment": "Организация-продавец",
      "required": true
    },
    {
      "name": "counterparty_id",
      "field_type": "aggregate_ref",
      "rust_type": "CounterpartyId",
      "ref_aggregate": "a003_counterparty",
      "ru_name": "Контрагент",
      "comment": "Покупатель",
      "required": true
    },
    {
      "name": "contact_info",
      "field_type": "nested_struct",
      "rust_type": "ContactInfo",
      "ru_name": "Контактная информация",
      "comment": "Данные для связи с покупателем",
      "required": false,
      "nested_fields": [
        {
          "name": "phone",
          "rust_type": "String",
          "ru_name": "Телефон",
          "comment": "Контактный телефон",
          "required": true
        },
        {
          "name": "email",
          "rust_type": "String",
          "ru_name": "Email",
          "comment": "Электронная почта",
          "required": false
        }
      ]
    },
    {
      "name": "items",
      "field_type": "nested_table",
      "rust_type": "Vec<OrderItem>",
      "ru_name": "Товарные позиции",
      "comment": "Список товаров в заказе",
      "required": true,
      "nested_fields": [
        {
          "name": "nomenclature_id",
          "field_type": "aggregate_ref",
          "rust_type": "NomenclatureId",
          "ref_aggregate": "a004_nomenclature",
          "ru_name": "Номенклатура",
          "comment": "Товар",
          "required": true
        },
        {
          "name": "quantity",
          "field_type": "primitive",
          "rust_type": "i32",
          "ru_name": "Количество",
          "comment": "Количество товара",
          "required": true,
          "validation": {
            "min": 1
          }
        },
        {
          "name": "price",
          "field_type": "primitive",
          "rust_type": "f64",
          "ru_name": "Цена",
          "comment": "Цена за единицу",
          "required": true,
          "validation": {
            "min": 0.0
          }
        },
        {
          "name": "discount_percent",
          "field_type": "primitive",
          "rust_type": "f64",
          "ru_name": "Процент скидки",
          "comment": "Скидка на позицию в процентах",
          "required": false,
          "validation": {
            "min": 0.0,
            "max": 100.0
          }
        }
      ]
    },
    {
      "name": "delivery_address",
      "field_type": "primitive",
      "rust_type": "String",
      "ru_name": "Адрес доставки",
      "comment": "Полный адрес доставки заказа",
      "required": false
    },
    {
      "name": "notes",
      "field_type": "primitive",
      "rust_type": "String",
      "ru_name": "Примечания",
      "comment": "Дополнительные примечания к заказу",
      "required": false
    }
  ]
}
```

---

## Сгенерированный metadata.rs (пример)

```rust
// ⚠️ АВТОМАТИЧЕСКИ СГЕНЕРИРОВАН из metadata.json
// НЕ РЕДАКТИРОВАТЬ ВРУЧНУЮ!
//
// Последняя генерация: 2025-10-09 15:30:00 UTC
// Исходный файл: metadata.json

use serde::{Deserialize, Serialize};
use crate::domain::common::field_metadata::{FieldMetadata, FieldType};
use crate::domain::a002_organization::aggregate::OrganizationId;
use crate::domain::a003_counterparty::aggregate::CounterpartyId;
use crate::domain::a004_nomenclature::aggregate::NomenclatureId;

// ============================================================================
// Вложенные структуры
// ============================================================================

/// Контактная информация
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactInfo {
    /// Контактный телефон
    pub phone: String,

    /// Электронная почта
    pub email: Option<String>,
}

/// Товарная позиция заказа
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItem {
    /// Товар
    pub nomenclature_id: NomenclatureId,

    /// Количество товара
    pub quantity: i32,

    /// Цена за единицу
    pub price: f64,

    /// Скидка на позицию в процентах
    pub discount_percent: Option<f64>,
}

// ============================================================================
// Метаданные вложенных структур
// ============================================================================

const CONTACT_INFO_FIELDS: &[FieldMetadata] = &[
    FieldMetadata {
        field_name: "phone",
        rust_type: "String",
        field_type: FieldType::Primitive,
        ru_name: "Телефон",
        comment: "Контактный телефон",
        required: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
    FieldMetadata {
        field_name: "email",
        rust_type: "String",
        field_type: FieldType::Primitive,
        ru_name: "Email",
        comment: "Электронная почта",
        required: false,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
];

const ORDER_ITEM_FIELDS: &[FieldMetadata] = &[
    FieldMetadata {
        field_name: "nomenclature_id",
        rust_type: "NomenclatureId",
        field_type: FieldType::AggregateRef,
        ru_name: "Номенклатура",
        comment: "Товар",
        required: true,
        nested_fields: None,
        ref_aggregate: Some("a004_nomenclature"),
        enum_values: None,
    },
    FieldMetadata {
        field_name: "quantity",
        rust_type: "i32",
        field_type: FieldType::Primitive,
        ru_name: "Количество",
        comment: "Количество товара",
        required: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
    FieldMetadata {
        field_name: "price",
        rust_type: "f64",
        field_type: FieldType::Primitive,
        ru_name: "Цена",
        comment: "Цена за единицу",
        required: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
    FieldMetadata {
        field_name: "discount_percent",
        rust_type: "f64",
        field_type: FieldType::Primitive,
        ru_name: "Процент скидки",
        comment: "Скидка на позицию в процентах",
        required: false,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
];

// ============================================================================
// Метаданные агрегата
// ============================================================================

pub const FIELDS: &[FieldMetadata] = &[
    FieldMetadata {
        field_name: "number",
        rust_type: "String",
        field_type: FieldType::Primitive,
        ru_name: "Номер заказа",
        comment: "Уникальный номер заказа в системе",
        required: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
    FieldMetadata {
        field_name: "date",
        rust_type: "chrono::NaiveDate",
        field_type: FieldType::Primitive,
        ru_name: "Дата",
        comment: "Дата оформления заказа",
        required: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
    FieldMetadata {
        field_name: "status",
        rust_type: "OrderStatus",
        field_type: FieldType::Enum,
        ru_name: "Статус",
        comment: "Текущий статус заказа",
        required: true,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: Some(&["New", "Processing", "Shipped", "Delivered", "Cancelled"]),
    },
    FieldMetadata {
        field_name: "organization_id",
        rust_type: "OrganizationId",
        field_type: FieldType::AggregateRef,
        ru_name: "Организация",
        comment: "Организация-продавец",
        required: true,
        nested_fields: None,
        ref_aggregate: Some("a002_organization"),
        enum_values: None,
    },
    FieldMetadata {
        field_name: "counterparty_id",
        rust_type: "CounterpartyId",
        field_type: FieldType::AggregateRef,
        ru_name: "Контрагент",
        comment: "Покупатель",
        required: true,
        nested_fields: None,
        ref_aggregate: Some("a003_counterparty"),
        enum_values: None,
    },
    FieldMetadata {
        field_name: "contact_info",
        rust_type: "ContactInfo",
        field_type: FieldType::NestedStruct,
        ru_name: "Контактная информация",
        comment: "Данные для связи с покупателем",
        required: false,
        nested_fields: Some(CONTACT_INFO_FIELDS),
        ref_aggregate: None,
        enum_values: None,
    },
    FieldMetadata {
        field_name: "items",
        rust_type: "Vec<OrderItem>",
        field_type: FieldType::NestedTable,
        ru_name: "Товарные позиции",
        comment: "Список товаров в заказе",
        required: true,
        nested_fields: Some(ORDER_ITEM_FIELDS),
        ref_aggregate: None,
        enum_values: None,
    },
    FieldMetadata {
        field_name: "delivery_address",
        rust_type: "String",
        field_type: FieldType::Primitive,
        ru_name: "Адрес доставки",
        comment: "Полный адрес доставки заказа",
        required: false,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
    FieldMetadata {
        field_name: "notes",
        rust_type: "String",
        field_type: FieldType::Primitive,
        ru_name: "Примечания",
        comment: "Дополнительные примечания к заказу",
        required: false,
        nested_fields: None,
        ref_aggregate: None,
        enum_values: None,
    },
];
```

---

## Использование в коде агрегата

```rust
// crates/contracts/src/domain/a007_order/mod.rs
pub mod aggregate;
mod metadata;  // ⭐ Подключаем сгенерированный модуль

// crates/contracts/src/domain/a007_order/aggregate.rs
use super::metadata::{self, ContactInfo, OrderItem};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    #[serde(flatten)]
    pub base: BaseAggregate<OrderId>,

    // Специфичные поля агрегата
    pub number: String,
    pub date: chrono::NaiveDate,
    pub status: OrderStatus,
    pub organization_id: OrganizationId,
    pub counterparty_id: CounterpartyId,
    pub contact_info: Option<ContactInfo>,
    pub items: Vec<OrderItem>,
    pub delivery_address: Option<String>,
    pub notes: Option<String>,
}

impl AggregateRoot for Order {
    type Id = OrderId;

    // ... существующие методы ...

    fn field_metadata() -> &'static [FieldMetadata] {
        metadata::FIELDS  // ⭐ Используем сгенерированные метаданные
    }
}

impl Order {
    /// Валидация данных с использованием метаданных
    pub fn validate(&self) -> Result<(), String> {
        // Автоматическая валидация обязательных полей
        for field_meta in Self::field_metadata() {
            if field_meta.required {
                // Проверка обязательности через рефлексию или явно
            }
        }

        // Специфичная бизнес-валидация
        if self.items.is_empty() {
            return Err("Заказ должен содержать хотя бы одну позицию".into());
        }

        Ok(())
    }
}
```

---

## FieldMetadata - определение типов

```rust
// crates/contracts/src/domain/common/field_metadata.rs

/// Метаданные поля агрегата
#[derive(Debug, Clone)]
pub struct FieldMetadata {
    /// Имя поля в Rust коде
    pub field_name: &'static str,

    /// Тип поля в Rust коде (например, "String", "Option<i32>", "Vec<OrderItem>")
    pub rust_type: &'static str,

    /// Категория типа поля
    pub field_type: FieldType,

    /// Русское наименование для UI
    pub ru_name: &'static str,

    /// Комментарий/описание поля
    pub comment: &'static str,

    /// Обязательность поля (true = не может быть пустым/None)
    pub required: bool,

    /// Метаданные вложенных полей (для NestedStruct и NestedTable)
    pub nested_fields: Option<&'static [FieldMetadata]>,

    /// Ссылка на агрегат (для AggregateRef) - индекс агрегата типа "a002_organization"
    pub ref_aggregate: Option<&'static str>,

    /// Возможные значения (для Enum)
    pub enum_values: Option<&'static [&'static str]>,
}

/// Категория типа поля
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    /// Примитивный тип (String, i32, bool, f64, chrono::NaiveDate, etc.)
    Primitive,

    /// Перечисление (enum)
    Enum,

    /// Ссылка на другой агрегат через ID
    AggregateRef,

    /// Вложенная структура (не Vec)
    NestedStruct,

    /// Вложенная таблица (Vec<T>)
    NestedTable,
}

impl FieldMetadata {
    /// Получить метаданные вложенных полей (для NestedStruct/NestedTable)
    pub fn nested(&self) -> Option<&'static [FieldMetadata]> {
        self.nested_fields
    }

    /// Проверка, является ли поле опциональным
    pub fn is_optional(&self) -> bool {
        !self.required
    }

    /// Получить имя связанного агрегата (для AggregateRef)
    pub fn referenced_aggregate(&self) -> Option<&'static str> {
        self.ref_aggregate
    }
}
```

---

## Расширение AggregateRoot trait

```rust
// crates/contracts/src/domain/common/aggregate_root.rs

use super::field_metadata::FieldMetadata;

pub trait AggregateRoot {
    type Id;

    // ... существующие методы ...

    /// Получить метаданные полей агрегата
    ///
    /// Возвращает статический массив метаданных всех полей агрегата.
    /// Используется для:
    /// - Валидации данных
    /// - Генерации UI форм
    /// - Построения документации
    /// - Анализа структуры данных внешними инструментами
    fn field_metadata() -> &'static [FieldMetadata];
}
```

---

## Build.rs - логика генерации

```rust
// crates/contracts/build.rs

use std::fs;
use std::path::{Path, PathBuf};
use serde::Deserialize;

fn main() {
    println!("cargo:rerun-if-changed=src/domain");

    // Сканируем все домены
    let domains_dir = PathBuf::from("src/domain");

    for entry in fs::read_dir(&domains_dir).unwrap() {
        let domain_path = entry.unwrap().path();

        // Пропускаем не-директории и common
        if !domain_path.is_dir() || domain_path.file_name().unwrap() == "common" {
            continue;
        }

        let metadata_json = domain_path.join("metadata.json");

        if metadata_json.exists() {
            // ⭐ Отслеживаем изменения JSON
            println!("cargo:rerun-if-changed={}", metadata_json.display());

            // Генерируем metadata.rs в ту же папку
            let output_rs = domain_path.join("metadata.rs");

            match generate_metadata_rs(&metadata_json, &output_rs) {
                Ok(_) => {
                    println!("cargo:warning=Generated: {}", output_rs.display());
                }
                Err(e) => {
                    panic!("Failed to generate {}: {}", output_rs.display(), e);
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct Metadata {
    aggregate: String,
    aggregate_index: Option<String>,
    collection_name: Option<String>,
    element_name: Option<String>,
    list_name: Option<String>,
    fields: Vec<Field>,
}

#[derive(Debug, Deserialize)]
struct Field {
    name: String,
    field_type: String,
    rust_type: String,
    ru_name: String,
    comment: String,
    required: bool,

    // Для вложенных структур/таблиц
    nested_fields: Option<Vec<Field>>,

    // Для ссылок на агрегаты
    ref_aggregate: Option<String>,

    // Для перечислений
    enum_values: Option<Vec<String>>,
}

fn generate_metadata_rs(json_path: &Path, output_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let json = fs::read_to_string(json_path)?;
    let metadata: Metadata = serde_json::from_str(&json)?;

    let mut code = String::new();

    // Заголовок
    code.push_str(&format!(
        "// ⚠️ АВТОМАТИЧЕСКИ СГЕНЕРИРОВАН из metadata.json\n\
         // НЕ РЕДАКТИРОВАТЬ ВРУЧНУЮ!\n\
         //\n\
         // Последняя генерация: {}\n\
         // Исходный файл: metadata.json\n\n",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    ));

    // Импорты
    code.push_str("use serde::{Deserialize, Serialize};\n");
    code.push_str("use crate::domain::common::field_metadata::{FieldMetadata, FieldType};\n");

    // Собираем все необходимые импорты для ссылок на агрегаты
    let mut imports = std::collections::HashSet::new();
    collect_imports(&metadata.fields, &mut imports);
    for import in imports {
        code.push_str(&format!("use crate::domain::{}::aggregate::{};\n",
            import.aggregate, import.type_name));
    }
    code.push_str("\n");

    // Генерируем вложенные структуры
    code.push_str("// ============================================================================\n");
    code.push_str("// Вложенные структуры\n");
    code.push_str("// ============================================================================\n\n");

    for field in &metadata.fields {
        if field.field_type == "nested_struct" || field.field_type == "nested_table" {
            generate_nested_struct(&mut code, field)?;
        }
    }

    // Генерируем метаданные вложенных структур
    code.push_str("// ============================================================================\n");
    code.push_str("// Метаданные вложенных структур\n");
    code.push_str("// ============================================================================\n\n");

    for field in &metadata.fields {
        if field.field_type == "nested_struct" || field.field_type == "nested_table" {
            generate_nested_metadata(&mut code, field)?;
        }
    }

    // Генерируем основные метаданные
    code.push_str("// ============================================================================\n");
    code.push_str("// Метаданные агрегата\n");
    code.push_str("// ============================================================================\n\n");
    code.push_str("pub const FIELDS: &[FieldMetadata] = &[\n");

    for field in &metadata.fields {
        generate_field_metadata(&mut code, field)?;
    }

    code.push_str("];\n");

    // Записываем результат
    fs::write(output_path, code)?;

    Ok(())
}

fn generate_nested_struct(code: &mut String, field: &Field) -> Result<(), Box<dyn std::error::Error>> {
    let type_name = extract_type_name(&field.rust_type);

    code.push_str(&format!("/// {}\n", field.ru_name));
    code.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
    code.push_str(&format!("pub struct {} {{\n", type_name));

    if let Some(nested_fields) = &field.nested_fields {
        for nested in nested_fields {
            code.push_str(&format!("    /// {}\n", nested.comment));

            let rust_type = if nested.required {
                nested.rust_type.clone()
            } else {
                format!("Option<{}>", nested.rust_type)
            };

            code.push_str(&format!("    pub {}: {},\n", nested.name, rust_type));
            code.push_str("\n");
        }
    }

    code.push_str("}\n\n");
    Ok(())
}

fn generate_nested_metadata(code: &mut String, field: &Field) -> Result<(), Box<dyn std::error::Error>> {
    let type_name = extract_type_name(&field.rust_type);
    let const_name = to_screaming_snake_case(&type_name) + "_FIELDS";

    code.push_str(&format!("const {}: &[FieldMetadata] = &[\n", const_name));

    if let Some(nested_fields) = &field.nested_fields {
        for nested in nested_fields {
            generate_field_metadata(code, nested)?;
        }
    }

    code.push_str("];\n\n");
    Ok(())
}

fn generate_field_metadata(code: &mut String, field: &Field) -> Result<(), Box<dyn std::error::Error>> {
    code.push_str("    FieldMetadata {\n");
    code.push_str(&format!("        field_name: \"{}\",\n", field.name));
    code.push_str(&format!("        rust_type: \"{}\",\n", field.rust_type));

    let field_type_variant = match field.field_type.as_str() {
        "primitive" => "Primitive",
        "enum" => "Enum",
        "aggregate_ref" => "AggregateRef",
        "nested_struct" => "NestedStruct",
        "nested_table" => "NestedTable",
        _ => "Primitive",
    };
    code.push_str(&format!("        field_type: FieldType::{},\n", field_type_variant));

    code.push_str(&format!("        ru_name: \"{}\",\n", field.ru_name));
    code.push_str(&format!("        comment: \"{}\",\n", field.comment));
    code.push_str(&format!("        required: {},\n", field.required));

    // nested_fields
    if field.field_type == "nested_struct" || field.field_type == "nested_table" {
        let type_name = extract_type_name(&field.rust_type);
        let const_name = to_screaming_snake_case(&type_name) + "_FIELDS";
        code.push_str(&format!("        nested_fields: Some({}),\n", const_name));
    } else {
        code.push_str("        nested_fields: None,\n");
    }

    // ref_aggregate
    if let Some(ref_agg) = &field.ref_aggregate {
        code.push_str(&format!("        ref_aggregate: Some(\"{}\"),\n", ref_agg));
    } else {
        code.push_str("        ref_aggregate: None,\n");
    }

    // enum_values
    if let Some(enum_vals) = &field.enum_values {
        code.push_str("        enum_values: Some(&[");
        for (i, val) in enum_vals.iter().enumerate() {
            if i > 0 { code.push_str(", "); }
            code.push_str(&format!("\"{}\"", val));
        }
        code.push_str("]),\n");
    } else {
        code.push_str("        enum_values: None,\n");
    }

    code.push_str("    },\n");

    Ok(())
}

// Вспомогательные функции
fn extract_type_name(rust_type: &str) -> &str {
    // Извлекает "OrderItem" из "Vec<OrderItem>"
    if rust_type.starts_with("Vec<") {
        &rust_type[4..rust_type.len()-1]
    } else if rust_type.starts_with("Option<") {
        &rust_type[7..rust_type.len()-1]
    } else {
        rust_type
    }
}

fn to_screaming_snake_case(s: &str) -> String {
    // Конвертирует "OrderItem" в "ORDER_ITEM"
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_ascii_uppercase());
    }
    result
}

struct Import {
    aggregate: String,
    type_name: String,
}

fn collect_imports(fields: &[Field], imports: &mut std::collections::HashSet<String>) {
    for field in fields {
        if field.field_type == "aggregate_ref" {
            if let Some(ref_agg) = &field.ref_aggregate {
                imports.insert(format!("{}::{}", ref_agg, field.rust_type));
            }
        }

        if let Some(nested) = &field.nested_fields {
            collect_imports(nested, imports);
        }
    }
}
```

---

## Использование метаданных

### 1. Автоматическая валидация

```rust
impl Order {
    pub fn validate(&self) -> Result<(), String> {
        // Проверка обязательных полей через метаданные
        for field_meta in Self::field_metadata() {
            if field_meta.required {
                // Можно реализовать через макросы или явную проверку
                match field_meta.field_name {
                    "number" => {
                        if self.number.trim().is_empty() {
                            return Err(format!("{} не может быть пустым", field_meta.ru_name));
                        }
                    }
                    "items" => {
                        if self.items.is_empty() {
                            return Err(format!("{} не может быть пустым", field_meta.ru_name));
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }
}
```

### 2. Генерация UI форм (frontend)

```rust
// Frontend Leptos
#[component]
pub fn AggregateForm<T: AggregateRoot>(aggregate: T) -> impl IntoView {
    let metadata = T::field_metadata();

    view! {
        <form>
            {metadata.iter().map(|field_meta| {
                let label = field_meta.ru_name;
                let required_marker = if field_meta.required { "*" } else { "" };

                view! {
                    <div class="form-field">
                        <label>{label}{required_marker}</label>
                        <input
                            type={get_input_type(field_meta)}
                            placeholder={field_meta.comment}
                            required={field_meta.required}
                        />
                    </div>
                }
            }).collect::<Vec<_>>()}
        </form>
    }
}
```

### 3. Построение дерева данных (внешний инструмент)

```rust
// Отдельное приложение для анализа структуры данных
use serde::{Deserialize, Serialize};
use std::fs;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct AggregateMetadata {
    aggregate: String,
    aggregate_index: String,
    fields: Vec<FieldInfo>,
}

#[derive(Debug, Deserialize)]
struct FieldInfo {
    name: String,
    field_type: String,
    rust_type: String,
    ru_name: String,
    comment: String,
    required: bool,
    nested_fields: Option<Vec<FieldInfo>>,
    ref_aggregate: Option<String>,
}

fn main() {
    // Читаем все metadata.json
    let mut aggregates: HashMap<String, AggregateMetadata> = HashMap::new();

    for entry in fs::read_dir("crates/contracts/src/domain").unwrap() {
        let path = entry.unwrap().path();
        if !path.is_dir() {
            continue;
        }

        let metadata_file = path.join("metadata.json");
        if metadata_file.exists() {
            let json = fs::read_to_string(&metadata_file).unwrap();
            let metadata: AggregateMetadata = serde_json::from_str(&json).unwrap();
            aggregates.insert(metadata.aggregate_index.clone(), metadata);
        }
    }

    // Строим дерево зависимостей
    println!("=== Структура данных системы ===\n");

    for (index, aggregate) in &aggregates {
        println!("[{}] {}", index, aggregate.aggregate);

        for field in &aggregate.fields {
            print_field(field, 1, &aggregates);
        }

        println!();
    }
}

fn print_field(field: &FieldInfo, indent: usize, aggregates: &HashMap<String, AggregateMetadata>) {
    let prefix = "  ".repeat(indent);
    let req_marker = if field.required { "[*]" } else { "[ ]" };

    println!("{}{} {} ({}) - {}",
        prefix,
        req_marker,
        field.ru_name,
        field.rust_type,
        field.comment
    );

    // Если это ссылка на агрегат - показываем связь
    if let Some(ref_agg) = &field.ref_aggregate {
        if let Some(ref_aggregate) = aggregates.get(ref_agg) {
            println!("{}  └─> Ссылка на: [{}] {}",
                prefix,
                ref_agg,
                ref_aggregate.aggregate
            );
        }
    }

    // Если есть вложенные поля - рекурсивно выводим
    if let Some(nested) = &field.nested_fields {
        for nested_field in nested {
            print_field(nested_field, indent + 1, aggregates);
        }
    }
}
```

**Вывод программы**:

```
=== Структура данных системы ===

[a007] Order
  [*] Номер заказа (String) - Уникальный номер заказа в системе
  [*] Дата (chrono::NaiveDate) - Дата оформления заказа
  [*] Статус (OrderStatus) - Текущий статус заказа
  [*] Организация (OrganizationId) - Организация-продавец
    └─> Ссылка на: [a002] Organization
  [*] Контрагент (CounterpartyId) - Покупатель
    └─> Ссылка на: [a003] Counterparty
  [ ] Контактная информация (ContactInfo) - Данные для связи с покупателем
    [*] Телефон (String) - Контактный телефон
    [ ] Email (String) - Электронная почта
  [*] Товарные позиции (Vec<OrderItem>) - Список товаров в заказе
    [*] Номенклатура (NomenclatureId) - Товар
      └─> Ссылка на: [a004] Nomenclature
    [*] Количество (i32) - Количество товара
    [*] Цена (f64) - Цена за единицу
    [ ] Процент скидки (f64) - Скидка на позицию в процентах
  [ ] Адрес доставки (String) - Полный адрес доставки заказа
  [ ] Примечания (String) - Дополнительные примечания к заказу
```

---

## Git и версионирование

### Рекомендация: Коммитить metadata.rs в Git

**Преимущества**:

- ✅ Проект работает сразу после клонирования
- ✅ IDE сразу видит типы и метаданные
- ✅ Diff показывает изменения в сгенерированном коде
- ✅ Code review может проверить корректность генерации

**Как обеспечить актуальность**:

1. **Pre-commit hook**:

```bash
#!/bin/bash
# .git/hooks/pre-commit

echo "Checking if metadata.json files changed..."

# Проверяем, изменялись ли metadata.json файлы
if git diff --cached --name-only | grep -q "metadata.json"; then
    echo "metadata.json changed, regenerating metadata.rs..."

    # Запускаем build.rs вручную через cargo check
    cargo check --package contracts

    # Добавляем все измененные metadata.rs в commit
    git add crates/contracts/src/domain/*/metadata.rs

    echo "metadata.rs files updated and staged"
fi
```

2. **CI проверка** (GitHub Actions):

```yaml
# .github/workflows/check-metadata.yml
name: Check Metadata Generation

on: [pull_request]

jobs:
  check-metadata:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Generate metadata
        run: cargo check --package contracts

      - name: Check for uncommitted changes
        run: |
          if [ -n "$(git status --porcelain)" ]; then
            echo "Error: metadata.rs files are out of sync with metadata.json"
            echo "Please run 'cargo check' and commit the changes"
            git status --porcelain
            exit 1
          fi
```

### .gitignore - НЕ игнорируем metadata.rs

```gitignore
# НЕ добавляем в .gitignore:
# **/metadata.rs  ← НЕТ!

# Обычные игнорируемые файлы
/target/
/dist/
Cargo.lock
```

---

## План поэтапной реализации

### Этап 1: Базовая инфраструктура (1-2 дня)

1. **Создать типы для метаданных**:

   - [ ] `contracts/src/domain/common/field_metadata.rs`
     - `FieldMetadata` struct
     - `FieldType` enum
     - Вспомогательные методы
   - [ ] Обновить `contracts/src/domain/common/mod.rs` для экспорта

2. **Создать схему для metadata.json**:
   - [ ] `contracts/codegen/schema.rs` (или прямо в build.rs)
     - `Metadata` struct для парсинга JSON
     - `Field` struct
     - Serde derive для десериализации

### Этап 2: Генератор и Build Script (2-3 дня)

3. **Создать генератор кода**:

   - [ ] `contracts/codegen/generator.rs` (или в build.rs)
     - `generate_metadata_rs()` - главная функция
     - `generate_nested_struct()` - генерация вложенных структур
     - `generate_nested_metadata()` - метаданные для вложенных
     - `generate_field_metadata()` - метаданные полей
     - Вспомогательные функции (extract_type_name, etc.)

4. **Создать build.rs**:

   - [ ] `contracts/build.rs`
     - Сканирование `src/domain/a*/metadata.json`
     - Вызов генератора для каждого файла
     - Запись результата в `metadata.rs`
     - `cargo:rerun-if-changed` для автоперегенерации

5. **Обновить Cargo.toml**:
   - [ ] `contracts/Cargo.toml`
     - Добавить `[build-dependencies]`: serde, serde_json, chrono

### Этап 3: Обновление AggregateRoot (1 день)

6. **Расширить trait AggregateRoot**:
   - [ ] `contracts/src/domain/common/aggregate_root.rs`
     - Добавить метод `fn field_metadata() -> &'static [FieldMetadata]`
     - Документация с примерами использования

### Этап 4: Пример на Connection1C (1 день)

7. **Создать metadata.json для Connection1C**:

   - [ ] `contracts/src/domain/a001_connection_1c/metadata.json`
     - Описать поля: url, login, password, is_primary
     - Все простые типы (Primitive)

8. **Обновить Connection1CDatabase aggregate**:
   - [ ] `contracts/src/domain/a001_connection_1c/mod.rs`
     - Добавить `mod metadata;`
   - [ ] `contracts/src/domain/a001_connection_1c/aggregate.rs`
     - Имплементировать `field_metadata()` -> `metadata::FIELDS`

### Этап 5: Тестирование (1 день)

9. **Проверить работу**:
   - [ ] `cargo build` - проверить генерацию metadata.rs
   - [ ] Изменить metadata.json - проверить автоперегенерацию
   - [ ] Использовать метаданные в коде (вывод в лог/endpoint)
   - [ ] Добавить unit-тесты для генератора

### Этап 6: Документация (0.5 дня)

10. **Обновить CLAUDE.md**:
    - [ ] Описание системы метаданных
    - [ ] Примеры для разных типов полей
    - [ ] Инструкция добавления нового агрегата с metadata.json

---

## Возможные расширения (будущее)

### 1. Валидация на уровне метаданных

```json
{
  "name": "quantity",
  "field_type": "primitive",
  "rust_type": "i32",
  "validation": {
    "min": 1,
    "max": 999999,
    "error_message": "Количество должно быть от 1 до 999999"
  }
}
```

### 2. UI hints

```json
{
  "name": "date",
  "ui": {
    "widget": "date-picker",
    "default_value": "today",
    "format": "dd.MM.yyyy"
  }
}
```

### 3. Database mapping

```json
{
  "name": "full_description",
  "database": {
    "column_name": "full_desc",
    "index": true,
    "searchable": true
  }
}
```

### 4. Генерация миграций БД

Build.rs может также генерировать SQL миграции на основе metadata.json:

```sql
-- Сгенерировано из metadata.json
CREATE TABLE IF NOT EXISTS a007_order (
    id TEXT PRIMARY KEY,
    number TEXT NOT NULL,  -- Номер заказа: Уникальный номер заказа в системе
    date TEXT NOT NULL,     -- Дата: Дата оформления заказа
    ...
);
```

### 5. Генерация OpenAPI/Swagger документации

Автоматическое создание OpenAPI спецификации API на основе метаданных.

---

## Преимущества решения

1. **Single Source of Truth**: metadata.json - единственное место определения метаданных
2. **Простой парсинг**: Любое приложение (на любом языке) может читать JSON
3. **Автосинхронизация**: Изменения в JSON автоматически попадают в код при сборке
4. **Типобезопасность**: Rust компилятор проверяет корректность сгенерированного кода
5. **Расширяемость**: Можно добавлять новые поля в JSON без изменения генератора
6. **Инструментарий**: Метаданные доступны для валидации, UI, документации, анализа

## Ограничения

1. **Сложность генератора**: build.rs становится нетривиальным (решение: вынести в отдельный крейт)
2. **Дублирование struct**: Struct в aggregate.rs нужно писать вручную (частично решается через include!)
3. **Ограничения build.rs**: Не может использовать типы из основного крейта (решение: String вместо типизированных ID)
4. **Отладка**: Ошибки в генераторе могут быть неочевидными (решение: хорошие error messages)

## Альтернативные решения (отклонены)

### Вариант A: Процедурный макрос

- ❌ Сложнее реализовать
- ❌ Парсинг через syn требует знания Rust AST
- ✅ Метаданные рядом с полями (через атрибуты)

### Вариант B: Трейт с const функциями

- ❌ Требует ручного дублирования метаданных в коде
- ❌ Сложный парсинг для внешних инструментов
- ✅ Простота реализации

### Вариант C: Только конфигурация (без генерации)

- ❌ Дублирование структуры данных
- ❌ Риск рассинхронизации JSON и кода
- ✅ Простота парсинга

---

## Заключение

Система генерации метаданных через build.rs позволяет:

- ✅ Декларативно описывать структуру агрегатов
- ✅ Автоматически поддерживать актуальность кода
- ✅ Использовать метаданные в runtime
- ✅ Анализировать структуру данных внешними инструментами
- ✅ Генерировать UI, валидацию, документацию

**Рекомендация**: Начать с простого примера (Connection1C с примитивными типами), затем постепенно добавлять поддержку вложенных структур и таблиц.

**Следующие шаги**:

1. Создать PoC (proof of concept) на одном агрегате
2. Проверить работу build.rs и перегенерации
3. Расширить на остальные агрегаты
4. Добавить поддержку вложенных структур
5. Создать внешний инструмент анализа
