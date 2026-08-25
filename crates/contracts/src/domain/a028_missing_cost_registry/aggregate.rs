use crate::domain::common::{AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MissingCostRegistryId(pub Uuid);

impl MissingCostRegistryId {
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

impl AggregateId for MissingCostRegistryId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(MissingCostRegistryId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MissingCostRegistryLine {
    pub nomenclature_ref: String,
    pub cost: Option<f64>,
    pub comment: Option<String>,
    pub detected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingCostRegistry {
    #[serde(flatten)]
    pub base: BaseAggregate<MissingCostRegistryId>,
    pub document_no: String,
    pub document_date: String,
    pub lines_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MissingCostRegistryUpdateDto {
    pub comment: Option<String>,
    pub lines: Vec<MissingCostRegistryLine>,
}

impl MissingCostRegistry {
    pub fn new_monthly(document_date: String) -> Self {
        let document_no = format!("MCR-{}", &document_date[..7]);
        let description = format!("Реестр отсутствующих цен от {}", document_date);
        let base = BaseAggregate::new(
            MissingCostRegistryId::new_v4(),
            document_no.clone(),
            description,
        );

        Self {
            base,
            document_no,
            document_date,
            lines_json: None,
        }
    }

    pub fn to_string_id(&self) -> String {
        self.base.id.as_string()
    }

    pub fn parse_lines(&self) -> Vec<MissingCostRegistryLine> {
        self.lines_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    pub fn set_lines(&mut self, lines: Vec<MissingCostRegistryLine>) {
        self.lines_json = if lines.is_empty() {
            None
        } else {
            serde_json::to_string(&lines).ok()
        };
    }

    pub fn update_from_dto(&mut self, dto: &MissingCostRegistryUpdateDto) {
        self.base.comment = dto.comment.clone().filter(|value| !value.trim().is_empty());
        self.set_lines(dto.lines.clone());
    }

    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    pub fn before_write(&mut self) {
        self.touch_updated();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.document_no.trim().is_empty() {
            return Err("Номер документа не может быть пустым".into());
        }
        if self.document_date.trim().is_empty() {
            return Err("Дата документа не может быть пустой".into());
        }

        let mut seen = HashSet::new();
        for line in self.parse_lines() {
            if line.nomenclature_ref.trim().is_empty() {
                return Err("В строке отсутствует номенклатура".into());
            }
            if !seen.insert(line.nomenclature_ref.clone()) {
                return Err(format!(
                    "Повторяющаяся номенклатура в документе: {}",
                    line.nomenclature_ref
                ));
            }
            if let Some(cost) = line.cost {
                if cost <= 0.0 {
                    return Err(format!(
                        "Себестоимость должна быть больше нуля для номенклатуры {}",
                        line.nomenclature_ref
                    ));
                }
            }
            if line.detected_at.trim().is_empty() {
                return Err(format!(
                    "Не заполнена дата обнаружения для номенклатуры {}",
                    line.nomenclature_ref
                ));
            }
        }

        Ok(())
    }
}

impl AggregateRoot for MissingCostRegistry {
    type Id = MissingCostRegistryId;

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
        "a028"
    }

    fn collection_name() -> &'static str {
        "missing_cost_registry"
    }

    fn element_name() -> &'static str {
        "Реестр отсутствующих цен"
    }

    fn list_name() -> &'static str {
        "Реестр отсутствующих цен"
    }

    fn origin() -> Origin {
        Origin::Self_
    }
}

impl Default for MissingCostRegistry {
    fn default() -> Self {
        let month_start = Utc::now().date_naive().format("%Y-%m-01").to_string();
        Self::new_monthly(month_start)
    }
}
