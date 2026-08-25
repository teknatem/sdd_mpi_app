use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ID типа для агрегата Вариант комплектации
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KitVariantId(pub Uuid);

impl KitVariantId {
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }
    pub fn value(&self) -> Uuid {
        self.0
    }
    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }
}

impl AggregateId for KitVariantId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(KitVariantId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Строка состава набора (десериализуется из goods_json)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoodsItem {
    /// UUID номенклатуры-компонента (a004)
    pub nomenclature_ref: String,
    /// Количество
    pub quantity: f64,
}

/// Вариант комплектации номенклатуры (агрегат a022)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KitVariant {
    #[serde(flatten)]
    pub base: BaseAggregate<KitVariantId>,

    /// UUID номенклатуры-владельца (производимая номенклатура, a004)
    pub owner_ref: Option<String>,

    /// JSON-массив состава набора: [{nomenclature_ref, quantity}]
    pub goods_json: Option<String>,

    /// ID подключения к 1С (a001_connection_1c)
    pub connection_id: String,

    /// Дата и время загрузки из 1С
    pub fetched_at: DateTime<Utc>,
}

impl KitVariant {
    pub fn new_from_odata(
        id: Uuid,
        code: String,
        description: String,
        owner_ref: Option<String>,
        goods_json: Option<String>,
        connection_id: String,
    ) -> Self {
        let base = BaseAggregate::new(KitVariantId::new(id), code, description);
        Self {
            base,
            owner_ref,
            goods_json,
            connection_id,
            fetched_at: Utc::now(),
        }
    }

    pub fn to_string_id(&self) -> String {
        self.base.id.as_string()
    }

    /// Десериализовать goods_json в вектор GoodsItem
    pub fn parse_goods(&self) -> Vec<GoodsItem> {
        self.goods_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }
}

impl AggregateRoot for KitVariant {
    type Id = KitVariantId;

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
        "a022"
    }

    fn collection_name() -> &'static str {
        "kit_variant"
    }

    fn element_name() -> &'static str {
        "Вариант комплектации"
    }

    fn list_name() -> &'static str {
        "Варианты комплектации"
    }

    fn origin() -> Origin {
        Origin::C1
    }
}
