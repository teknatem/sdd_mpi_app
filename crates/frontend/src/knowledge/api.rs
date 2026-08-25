//! Запросы страницы инвентаризации знаний.
//!
//! DTO не переопределяются: они приходят из `contracts::knowledge`, как и оси
//! классификации. Второй список кодов на фронте разъехался бы с первым молча —
//! ровно та беда, ради которой оси вообще сделаны типами.

use contracts::knowledge::{InventoryCollectReportDto, InventoryResponseDto};

use crate::shared::api_utils::{get_json, post_json};

/// Последний снимок целиком: паспорт, сводка, оси, фасеты, реестр и единицы.
pub async fn get_inventory() -> Result<InventoryResponseDto, String> {
    get_json("/api/knowledge/inventory").await
}

/// Пересобрать снимок. Дорого не бывает: реестры в памяти, из БД читаются три
/// небольшие выборки, профиль данных не пересчитывается.
pub async fn collect_now() -> Result<InventoryCollectReportDto, String> {
    post_json("/api/knowledge/inventory/collect", &()).await
}
