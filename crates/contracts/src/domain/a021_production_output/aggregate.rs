use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ID типа для документа Выпуск продукции
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProductionOutputId(pub Uuid);

impl ProductionOutputId {
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }
    pub fn value(&self) -> Uuid {
        self.0
    }
}

impl AggregateId for ProductionOutputId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(ProductionOutputId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

/// Документ Выпуск продукции (агрегат a021)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionOutput {
    #[serde(flatten)]
    pub base: BaseAggregate<ProductionOutputId>,

    /// Номер документа (напр. "Ф-00000084")
    pub document_no: String,

    /// Дата производства (YYYY-MM-DD)
    pub document_date: String,

    /// Артикул продукта (для сопоставления с a004)
    pub article: String,

    /// Количество произведённых единиц
    pub count: i64,

    /// Сумма себестоимости итого
    pub amount: f64,

    /// Себестоимость на 1 шт (amount / count)
    pub cost_of_production: Option<f64>,

    /// Ссылка на номенклатуру 1С (a004_nomenclature)
    pub nomenclature_ref: Option<String>,

    /// ID подключения к 1С (a001_connection_1c)
    pub connection_id: String,

    /// Дата и время загрузки/обновления документа
    pub fetched_at: DateTime<Utc>,
}

impl ProductionOutput {
    pub fn new_from_api(
        id: Uuid,
        document_no: String,
        document_date: String,
        description: String,
        article: String,
        count: i64,
        amount: f64,
        connection_id: String,
    ) -> Self {
        let cost = if count > 0 {
            Some(amount / count as f64)
        } else {
            None
        };
        let base = BaseAggregate::new(
            ProductionOutputId::new(id),
            document_no.clone(),
            description,
        );
        Self {
            base,
            document_no,
            document_date,
            article,
            count,
            amount,
            cost_of_production: cost,
            nomenclature_ref: None,
            connection_id,
            fetched_at: Utc::now(),
        }
    }

    pub fn to_string_id(&self) -> String {
        self.base.id.as_string()
    }

    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.document_no.trim().is_empty() {
            return Err("Номер документа не может быть пустым".into());
        }
        if self.document_date.trim().is_empty() {
            return Err("Дата документа не может быть пустой".into());
        }
        if self.connection_id.trim().is_empty() {
            return Err("Подключение к 1С обязательно".into());
        }
        Ok(())
    }
}

impl AggregateRoot for ProductionOutput {
    type Id = ProductionOutputId;

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
        "a021"
    }

    fn collection_name() -> &'static str {
        "production_output"
    }

    fn element_name() -> &'static str {
        "Выпуск продукции"
    }

    fn list_name() -> &'static str {
        "Выпуск продукции"
    }

    fn origin() -> Origin {
        Origin::C1
    }
}
