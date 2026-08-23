use super::{EntityMetadata, Origin};
use crate::shared::metadata::{EntityMetadataInfo, FieldMetadata};

/// Трейт для корня агрегата
///
/// Определяет обязательные методы и метаданные для всех агрегатов системы
pub trait AggregateRoot {
    /// Тип идентификатора агрегата
    type Id;

    // ============================================================================
    // Методы экземпляра (данные конкретной записи)
    // ============================================================================

    /// Получить ID записи
    fn id(&self) -> Self::Id;

    /// Получить бизнес-код записи (например, "ORD-2025-001")
    fn code(&self) -> &str;

    /// Получить описание/название записи
    fn description(&self) -> &str;

    /// Получить метаданные жизненного цикла
    fn metadata(&self) -> &EntityMetadata;

    /// Получить изменяемые метаданные
    fn metadata_mut(&mut self) -> &mut EntityMetadata;

    // ============================================================================
    // Метаданные класса агрегата (статические данные)
    // ============================================================================

    /// Индекс агрегата в системе (например, "a001")
    fn aggregate_index() -> &'static str;

    /// Имя коллекции для БД (например, "connection_1c")
    fn collection_name() -> &'static str;

    /// Имя элемента для UI (единственное число, например, "Подключение 1С")
    fn element_name() -> &'static str;

    /// Имя списка для UI (множественное число, например, "Подключения 1С")
    fn list_name() -> &'static str;

    /// Источник данных агрегата
    fn origin() -> Origin;

    // ============================================================================
    // Расширенные метаданные (из metadata.json)
    // ============================================================================

    /// Получить полные метаданные сущности (из сгенерированного кода)
    ///
    /// Возвращает compile-time константу с нулевыми runtime затратами.
    /// Содержит UI метаданные, AI контекст и информацию о полях.
    ///
    /// # Пример
    /// ```rust,ignore
    /// let meta = Connection1CDatabase::entity_metadata_info();
    /// println!("Entity: {}", meta.ui.element_name);
    /// println!("AI: {}", meta.ai.description);
    /// ```
    fn entity_metadata_info() -> Option<&'static EntityMetadataInfo> {
        None // Default implementation for aggregates without metadata.json
    }

    /// Получить метаданные полей агрегата (из сгенерированного кода)
    ///
    /// Возвращает статический срез с определениями всех полей.
    /// Используется для валидации, генерации UI и AI контекста.
    ///
    /// # Пример
    /// ```rust,ignore
    /// for field in Connection1CDatabase::field_metadata() {
    ///     if field.validation.required {
    ///         println!("Required: {}", field.ui.label);
    ///     }
    /// }
    /// ```
    fn field_metadata() -> Option<&'static [FieldMetadata]> {
        None // Default implementation for aggregates without metadata.json
    }

    // ============================================================================
    // Методы с реализацией по умолчанию
    // ============================================================================

    /// Полное имя агрегата для системы (например, "a001_connection_1c")
    fn full_name() -> String {
        format!("{}_{}", Self::aggregate_index(), Self::collection_name())
    }

    /// Префикс для таблиц БД (например, "a001_connection_1c_")
    fn table_prefix() -> String {
        format!("{}_", Self::full_name())
    }
}
