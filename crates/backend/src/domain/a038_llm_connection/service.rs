use super::repository;
use contracts::domain::a038_llm_connection::aggregate::{LlmConnection, LlmProviderType};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConnectionDto {
    pub id: Option<String>,
    pub code: Option<String>,
    pub description: String,
    pub comment: Option<String>,
    pub provider_type: String,
    pub api_endpoint: String,
    pub api_key: String,
    pub model_name: String,
    pub temperature: f64,
    pub max_tokens: i32,
    #[serde(default = "contracts::domain::a038_llm_connection::aggregate::default_context_window")]
    pub context_window: i32,
    pub is_primary: bool,
    pub available_models: Option<String>,
    /// Курируемый короткий список разрешённых моделей (JSON-массив model_id).
    pub allowed_models: Option<String>,
    #[serde(default)]
    pub image_input_models: Option<String>,
    /// Прайс за миллион токенов. Пусто = стоимость прогонов не считается.
    #[serde(default)]
    pub price_in_per_mtok: Option<f64>,
    #[serde(default)]
    pub price_out_per_mtok: Option<f64>,
    #[serde(default)]
    pub price_cached_per_mtok: Option<f64>,
    #[serde(default)]
    pub currency: Option<String>,
}

/// Создание нового подключения LLM
pub async fn create(dto: LlmConnectionDto) -> anyhow::Result<Uuid> {
    let code = dto
        .code
        .clone()
        .unwrap_or_else(|| format!("LLM-{}", Uuid::new_v4()));

    let provider_type = LlmProviderType::from_str(&dto.provider_type)
        .map_err(|e| anyhow::anyhow!("Invalid provider type: {}", e))?;

    let mut aggregate = LlmConnection::new_for_insert(
        code,
        dto.description,
        provider_type,
        dto.api_endpoint,
        dto.api_key,
        dto.model_name,
        dto.temperature,
        dto.max_tokens,
        dto.context_window,
        dto.is_primary,
        dto.available_models,
        dto.allowed_models,
        dto.image_input_models,
    );
    aggregate.price_in_per_mtok = dto.price_in_per_mtok;
    aggregate.price_out_per_mtok = dto.price_out_per_mtok;
    aggregate.price_cached_per_mtok = dto.price_cached_per_mtok;
    aggregate.currency = dto.currency.filter(|s| !s.trim().is_empty());

    aggregate
        .validate()
        .map_err(|e| anyhow::anyhow!("Validation failed: {}", e))?;

    aggregate.before_write();

    // Бизнес-логика: обеспечение единственности primary
    if aggregate.is_primary {
        repository::clear_all_primary().await?;
    }

    let id = aggregate.base.id.0;
    repository::insert(&aggregate).await?;

    Ok(id)
}

/// Обновление существующего подключения
pub async fn update(dto: LlmConnectionDto) -> anyhow::Result<()> {
    let id_str = dto
        .id
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("ID is required"))?;

    let mut aggregate: LlmConnection = repository::find_by_id(id_str)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Connection not found"))?;

    if let Some(code) = dto.code {
        aggregate.base.code = code;
    }
    aggregate.base.description = dto.description;
    aggregate.base.comment = dto.comment;

    aggregate.provider_type = LlmProviderType::from_str(&dto.provider_type)
        .map_err(|e| anyhow::anyhow!("Invalid provider type: {}", e))?;
    aggregate.api_endpoint = dto.api_endpoint;
    aggregate.api_key = dto.api_key;
    aggregate.model_name = dto.model_name;
    aggregate.temperature = dto.temperature;
    aggregate.max_tokens = dto.max_tokens;
    aggregate.context_window = dto.context_window;
    aggregate.is_primary = dto.is_primary;
    aggregate.allowed_models = dto.allowed_models;
    aggregate.image_input_models = dto.image_input_models;
    aggregate.price_in_per_mtok = dto.price_in_per_mtok;
    aggregate.price_out_per_mtok = dto.price_out_per_mtok;
    aggregate.price_cached_per_mtok = dto.price_cached_per_mtok;
    aggregate.currency = dto.currency.filter(|s| !s.trim().is_empty());
    // available_models не обновляется через update, только через fetch_models endpoint

    aggregate
        .validate()
        .map_err(|e| anyhow::anyhow!("Validation failed: {}", e))?;

    aggregate.before_write();

    if aggregate.is_primary {
        repository::clear_all_primary().await?;
    }

    repository::update(&aggregate).await
}

/// Мягкое удаление подключения
pub async fn delete(id: &str) -> anyhow::Result<()> {
    repository::soft_delete(id).await
}

/// Получение подключения по ID
pub async fn get_by_id(id: &str) -> anyhow::Result<Option<LlmConnection>> {
    repository::find_by_id(id).await
}

/// Получение списка всех подключений
pub async fn list_all() -> anyhow::Result<Vec<LlmConnection>> {
    repository::list_all().await
}

/// Получение пагинированного списка
pub async fn list_paginated(
    limit: u64,
    offset: u64,
    sort_by: &str,
    sort_desc: bool,
) -> anyhow::Result<(Vec<LlmConnection>, u64)> {
    repository::list_paginated(limit, offset, sort_by, sort_desc).await
}

/// Получение основного подключения
pub async fn get_primary() -> anyhow::Result<Option<LlmConnection>> {
    repository::find_primary().await
}
