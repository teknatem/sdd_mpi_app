use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ID типа для записи продаж маркетплейсов
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarketplaceSalesId(pub Uuid);

impl MarketplaceSalesId {
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }
    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn value(&self) -> Uuid {
        self.0
    }
}

impl AggregateId for MarketplaceSalesId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(MarketplaceSalesId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Продажа на маркетплейсе (агрегат)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceSales {
    #[serde(flatten)]
    pub base: BaseAggregate<MarketplaceSalesId>,

    /// Ссылка на подключение МП (a006_connection_mp.id)
    #[serde(rename = "connectionId")]
    pub connection_id: String,

    /// Ссылка на организацию (a002_organization.id)
    #[serde(rename = "organizationId")]
    pub organization_id: String,

    /// Ссылка на маркетплейс (a005_marketplace.id)
    #[serde(rename = "marketplaceId")]
    pub marketplace_id: String,

    /// Дата начисления (YYYY-MM-DD)
    #[serde(with = "serde_date")]
    #[serde(rename = "accrualDate")]
    pub accrual_date: chrono::NaiveDate,

    /// Ссылка на товар маркетплейса (a007_marketplace_product.id)
    #[serde(rename = "productId")]
    pub product_id: String,

    /// Количество
    pub quantity: i32,

    /// Выручка (RUB)
    pub revenue: f64,

    /// Тип операции в источнике (например: sale, return, commission)
    #[serde(rename = "operationType")]
    pub operation_type: String,
}

impl MarketplaceSales {
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_insert(
        code: String,
        description: String,
        connection_id: String,
        organization_id: String,
        marketplace_id: String,
        accrual_date: chrono::NaiveDate,
        product_id: String,
        quantity: i32,
        revenue: f64,
        operation_type: String,
        comment: Option<String>,
    ) -> Self {
        let mut base = BaseAggregate::new(MarketplaceSalesId::new_v4(), code, description);
        base.comment = comment;
        Self {
            base,
            connection_id,
            organization_id,
            marketplace_id,
            accrual_date,
            product_id,
            quantity,
            revenue,
            operation_type,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_id(
        id: MarketplaceSalesId,
        code: String,
        description: String,
        connection_id: String,
        organization_id: String,
        marketplace_id: String,
        accrual_date: chrono::NaiveDate,
        product_id: String,
        quantity: i32,
        revenue: f64,
        operation_type: String,
        comment: Option<String>,
    ) -> Self {
        let mut base = BaseAggregate::new(id, code, description);
        base.comment = comment;
        Self {
            base,
            connection_id,
            organization_id,
            marketplace_id,
            accrual_date,
            product_id,
            quantity,
            revenue,
            operation_type,
        }
    }

    pub fn to_string_id(&self) -> String {
        self.base.id.as_string()
    }
    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    pub fn update(&mut self, dto: &MarketplaceSalesDto) {
        self.base.code = dto.code.clone().unwrap_or_default();
        self.base.description = dto.description.clone();
        self.base.comment = dto.comment.clone();
        self.connection_id = dto.connection_id.clone();
        self.organization_id = dto.organization_id.clone();
        self.marketplace_id = dto.marketplace_id.clone();
        self.accrual_date = dto.accrual_date;
        self.product_id = dto.product_id.clone();
        self.quantity = dto.quantity;
        self.revenue = dto.revenue;
        self.operation_type = dto.operation_type.clone();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.base.description.trim().is_empty() {
            return Err("Описание не может быть пустым".into());
        }
        if self.base.code.trim().is_empty() {
            return Err("Код не может быть пустым".into());
        }
        if self.connection_id.trim().is_empty() {
            return Err("Подключение обязательно".into());
        }
        if self.organization_id.trim().is_empty() {
            return Err("Организация обязательна".into());
        }
        if self.marketplace_id.trim().is_empty() {
            return Err("Маркетплейс обязателен".into());
        }
        if self.product_id.trim().is_empty() {
            return Err("Позиция обязательна".into());
        }
        if self.quantity < 0 {
            return Err("Количество не может быть отрицательным".into());
        }
        // Доход может быть отрицательным (возвраты)
        if self.operation_type.trim().is_empty() {
            return Err("Тип операции обязателен".into());
        }
        Ok(())
    }

    pub fn before_write(&mut self) {
        self.touch_updated();
    }
}

impl AggregateRoot for MarketplaceSales {
    type Id = MarketplaceSalesId;
    fn id(&self) -> Self::Id {
        self.base.id
    }
    fn code(&self) -> &str {
        &self.base.code
    }
    fn description(&self) -> &str {
        &self.base.description
    }
    fn metadata(&self) -> &EntityMetadata {
        &self.base.metadata
    }
    fn metadata_mut(&mut self) -> &mut EntityMetadata {
        &mut self.base.metadata
    }
    fn aggregate_index() -> &'static str {
        "a008"
    }
    fn collection_name() -> &'static str {
        "marketplace_sales"
    }
    fn element_name() -> &'static str {
        "Продажа маркетплейса"
    }
    fn list_name() -> &'static str {
        "Продажи маркетплейсов"
    }
    fn origin() -> Origin {
        Origin::Marketplace
    }
}

// =============================================================================
// DTO
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MarketplaceSalesDto {
    pub id: Option<String>,
    pub code: Option<String>,
    pub description: String,
    #[serde(rename = "connectionId")]
    pub connection_id: String,
    #[serde(rename = "organizationId")]
    pub organization_id: String,
    #[serde(rename = "marketplaceId")]
    pub marketplace_id: String,
    #[serde(with = "serde_date")]
    #[serde(rename = "accrualDate")]
    pub accrual_date: chrono::NaiveDate,
    #[serde(rename = "productId")]
    pub product_id: String,
    pub quantity: i32,
    pub revenue: f64,
    #[serde(rename = "operationType")]
    pub operation_type: String,
    pub comment: Option<String>,
}

// Local serde helper for NaiveDate as YYYY-MM-DD
mod serde_date {
    use chrono::NaiveDate;
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%d";

    pub fn serialize<S>(date: &NaiveDate, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = date.format(FORMAT).to_string();
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        NaiveDate::parse_from_str(&s, FORMAT).map_err(serde::de::Error::custom)
    }
}
