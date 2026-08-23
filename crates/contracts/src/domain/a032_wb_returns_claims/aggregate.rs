use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ID типа для заявки покупателя на возврат WB
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WbReturnsClaimsId(pub Uuid);

impl WbReturnsClaimsId {
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

impl AggregateId for WbReturnsClaimsId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(WbReturnsClaimsId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Заявка покупателя на возврат товара WB (агрегат)
///
/// Источник: GET https://feedbacks-api.wildberries.ru/api/v1/claims
/// Уникальный ключ: (claim_id, connection_id)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbReturnsClaims {
    #[serde(flatten)]
    pub base: BaseAggregate<WbReturnsClaimsId>,

    /// ID подключения МП (a006_connection_mp.id)
    #[serde(rename = "connectionId")]
    pub connection_id: String,

    /// ID организации (a002_organization.id)
    #[serde(rename = "organizationId")]
    pub organization_id: String,

    /// ID маркетплейса (a005_marketplace.id)
    #[serde(rename = "marketplaceId")]
    pub marketplace_id: String,

    /// ID заявки из WB API (поле `id` в ответе)
    #[serde(rename = "claimId")]
    pub claim_id: String,

    /// Тип заявки (числовой код WB)
    #[serde(rename = "claimType")]
    pub claim_type: Option<i32>,

    /// Статус заявки (числовой код WB)
    pub status: Option<i32>,

    /// Расширенный статус заявки
    #[serde(rename = "statusEx")]
    pub status_ex: Option<i32>,

    /// nmId — числовой идентификатор номенклатуры WB
    #[serde(rename = "nmId")]
    pub nm_id: i64,

    /// Название товара из WB
    #[serde(rename = "imtName")]
    pub imt_name: Option<String>,

    /// Комментарий покупателя
    #[serde(rename = "userComment")]
    pub user_comment: Option<String>,

    /// Комментарий WB для покупателя
    #[serde(rename = "wbComment")]
    pub wb_comment: Option<String>,

    /// Дата создания заявки
    pub dt: DateTime<Utc>,

    /// Дата заказа
    #[serde(rename = "orderDt")]
    pub order_dt: Option<DateTime<Utc>>,

    /// Дата последнего обновления заявки
    #[serde(rename = "dtUpdate")]
    pub dt_update: Option<DateTime<Utc>>,

    /// Дата доставки возврата
    #[serde(rename = "deliveryDt")]
    pub delivery_dt: Option<DateTime<Utc>>,

    /// Цена товара в заявке
    pub price: Option<f64>,

    /// Код валюты (ISO 4217)
    #[serde(rename = "currencyCode")]
    pub currency_code: Option<String>,

    /// srid — уникальный ID строки заказа WB
    pub srid: Option<String>,

    /// Доп. информация об исходном ID (IMEI и т.п.)
    #[serde(rename = "originIdInfo")]
    pub origin_id_info: Option<String>,

    /// Возможные действия продавца (JSON-массив строк)
    pub actions: Option<String>,

    /// Признак нахождения в архиве
    #[serde(rename = "isArchive")]
    pub is_archive: bool,
}

impl WbReturnsClaims {
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_insert(
        code: String,
        description: String,
        connection_id: String,
        organization_id: String,
        marketplace_id: String,
        claim_id: String,
        claim_type: Option<i32>,
        status: Option<i32>,
        status_ex: Option<i32>,
        nm_id: i64,
        imt_name: Option<String>,
        user_comment: Option<String>,
        wb_comment: Option<String>,
        dt: DateTime<Utc>,
        order_dt: Option<DateTime<Utc>>,
        dt_update: Option<DateTime<Utc>>,
        delivery_dt: Option<DateTime<Utc>>,
        price: Option<f64>,
        currency_code: Option<String>,
        srid: Option<String>,
        origin_id_info: Option<String>,
        actions: Option<String>,
        is_archive: bool,
    ) -> Self {
        let base = BaseAggregate::new(WbReturnsClaimsId::new_v4(), code, description);
        Self {
            base,
            connection_id,
            organization_id,
            marketplace_id,
            claim_id,
            claim_type,
            status,
            status_ex,
            nm_id,
            imt_name,
            user_comment,
            wb_comment,
            dt,
            order_dt,
            dt_update,
            delivery_dt,
            price,
            currency_code,
            srid,
            origin_id_info,
            actions,
            is_archive,
        }
    }

    pub fn to_string_id(&self) -> String {
        self.base.id.as_string()
    }

    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    pub fn before_write(&mut self) {
        self.touch_updated();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.claim_id.trim().is_empty() {
            return Err("ID заявки не может быть пустым".into());
        }
        if self.connection_id.trim().is_empty() {
            return Err("Подключение обязательно".into());
        }
        Ok(())
    }

    /// Преобразовать в ListDTO для frontend
    pub fn to_list_dto(&self) -> WbReturnsClaimsListDto {
        WbReturnsClaimsListDto {
            id: self.base.id.as_string(),
            claim_id: self.claim_id.clone(),
            nm_id: self.nm_id,
            imt_name: self.imt_name.clone(),
            status: self.status,
            dt: self.dt.to_rfc3339(),
            order_dt: self.order_dt.map(|d| d.to_rfc3339()),
            dt_update: self.dt_update.map(|d| d.to_rfc3339()),
            price: self.price,
            currency_code: self.currency_code.clone(),
            srid: self.srid.clone(),
            is_archive: self.is_archive,
            user_comment: self.user_comment.clone(),
            org_name: None,
        }
    }

    /// Преобразовать в DetailDTO для frontend
    pub fn to_detail_dto(&self) -> WbReturnsClaimsDetailDto {
        WbReturnsClaimsDetailDto {
            id: self.base.id.as_string(),
            code: self.base.code.clone(),
            description: self.base.description.clone(),
            connection_id: self.connection_id.clone(),
            organization_id: self.organization_id.clone(),
            marketplace_id: self.marketplace_id.clone(),
            claim_id: self.claim_id.clone(),
            claim_type: self.claim_type,
            status: self.status,
            status_ex: self.status_ex,
            nm_id: self.nm_id,
            imt_name: self.imt_name.clone(),
            user_comment: self.user_comment.clone(),
            wb_comment: self.wb_comment.clone(),
            dt: self.dt.to_rfc3339(),
            order_dt: self.order_dt.map(|d| d.to_rfc3339()),
            dt_update: self.dt_update.map(|d| d.to_rfc3339()),
            delivery_dt: self.delivery_dt.map(|d| d.to_rfc3339()),
            price: self.price,
            currency_code: self.currency_code.clone(),
            srid: self.srid.clone(),
            origin_id_info: self.origin_id_info.clone(),
            actions: self.actions.clone(),
            is_archive: self.is_archive,
            metadata: WbReturnsClaimsMetadataDto {
                created_at: self.base.metadata.created_at.to_rfc3339(),
                updated_at: self.base.metadata.updated_at.to_rfc3339(),
                is_deleted: self.base.metadata.is_deleted,
                is_posted: self.base.metadata.is_posted,
                version: self.base.metadata.version,
            },
        }
    }
}

impl AggregateRoot for WbReturnsClaims {
    type Id = WbReturnsClaimsId;

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
        "a032"
    }

    fn collection_name() -> &'static str {
        "wb_returns_claims"
    }

    fn element_name() -> &'static str {
        "Заявка на возврат WB"
    }

    fn list_name() -> &'static str {
        "Заявки на возврат WB"
    }

    fn origin() -> Origin {
        Origin::Marketplace
    }
}

// =============================================================================
// List DTO
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbReturnsClaimsListDto {
    pub id: String,
    #[serde(rename = "claimId")]
    pub claim_id: String,
    #[serde(rename = "nmId")]
    pub nm_id: i64,
    #[serde(rename = "imtName")]
    pub imt_name: Option<String>,
    pub status: Option<i32>,
    pub dt: String,
    #[serde(rename = "orderDt")]
    pub order_dt: Option<String>,
    #[serde(rename = "dtUpdate")]
    pub dt_update: Option<String>,
    pub price: Option<f64>,
    #[serde(rename = "currencyCode")]
    pub currency_code: Option<String>,
    pub srid: Option<String>,
    #[serde(rename = "isArchive")]
    pub is_archive: bool,
    #[serde(rename = "userComment")]
    pub user_comment: Option<String>,
    /// Наименование организации (resolves organization_id → a002_organization.full_name)
    #[serde(rename = "orgName")]
    pub org_name: Option<String>,
}

// =============================================================================
// Detail DTO
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbReturnsClaimsDetailDto {
    pub id: String,
    pub code: String,
    pub description: String,
    #[serde(rename = "connectionId")]
    pub connection_id: String,
    #[serde(rename = "organizationId")]
    pub organization_id: String,
    #[serde(rename = "marketplaceId")]
    pub marketplace_id: String,
    #[serde(rename = "claimId")]
    pub claim_id: String,
    #[serde(rename = "claimType")]
    pub claim_type: Option<i32>,
    pub status: Option<i32>,
    #[serde(rename = "statusEx")]
    pub status_ex: Option<i32>,
    #[serde(rename = "nmId")]
    pub nm_id: i64,
    #[serde(rename = "imtName")]
    pub imt_name: Option<String>,
    #[serde(rename = "userComment")]
    pub user_comment: Option<String>,
    #[serde(rename = "wbComment")]
    pub wb_comment: Option<String>,
    pub dt: String,
    #[serde(rename = "orderDt")]
    pub order_dt: Option<String>,
    #[serde(rename = "dtUpdate")]
    pub dt_update: Option<String>,
    #[serde(rename = "deliveryDt")]
    pub delivery_dt: Option<String>,
    pub price: Option<f64>,
    #[serde(rename = "currencyCode")]
    pub currency_code: Option<String>,
    pub srid: Option<String>,
    #[serde(rename = "originIdInfo")]
    pub origin_id_info: Option<String>,
    pub actions: Option<String>,
    #[serde(rename = "isArchive")]
    pub is_archive: bool,
    pub metadata: WbReturnsClaimsMetadataDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbReturnsClaimsMetadataDto {
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(rename = "isDeleted")]
    pub is_deleted: bool,
    #[serde(rename = "isPosted")]
    pub is_posted: bool,
    pub version: i32,
}
