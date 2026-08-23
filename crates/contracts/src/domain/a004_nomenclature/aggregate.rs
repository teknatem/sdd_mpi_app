use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Нулевой UUID, который не считается корректной ссылкой
const ZERO_UUID: &str = "00000000-0000-0000-0000-000000000000";

// ============================================================================
// ID Type
// ============================================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NomenclatureId(pub Uuid);

impl NomenclatureId {
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

impl AggregateId for NomenclatureId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }

    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(NomenclatureId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

// ============================================================================
// Aggregate Root
// ============================================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nomenclature {
    #[serde(flatten)]
    pub base: BaseAggregate<NomenclatureId>,

    #[serde(rename = "fullDescription")]
    pub full_description: String,

    #[serde(rename = "isFolder", default)]
    pub is_folder: bool,

    #[serde(rename = "parentId")]
    pub parent_id: Option<String>,

    #[serde(rename = "article")]
    pub article: String,

    #[serde(rename = "mpRefCount", default)]
    pub mp_ref_count: i32,

    // РР·РјРµСЂРµРЅРёСЏ (классификация)
    #[serde(rename = "dim1Category", default)]
    pub dim1_category: String,

    #[serde(rename = "dim2Line", default)]
    pub dim2_line: String,

    #[serde(rename = "dim3Model", default)]
    pub dim3_model: String,

    #[serde(rename = "dim4Format", default)]
    pub dim4_format: String,

    #[serde(rename = "dim5Sink", default)]
    pub dim5_sink: String,

    #[serde(rename = "dim6Size", default)]
    pub dim6_size: String,

    #[serde(rename = "isAssembly", default)]
    pub is_assembly: bool,

    #[serde(rename = "baseNomenclatureRef")]
    pub base_nomenclature_ref: Option<String>,

    #[serde(rename = "alternativeCostSourceRef")]
    pub alternative_cost_source_ref: Option<String>,

    #[serde(rename = "kitVariantRef")]
    pub kit_variant_ref: Option<String>,

    #[serde(rename = "isDerivative", default)]
    pub is_derivative: bool,
}

impl Nomenclature {
    /// Вычисление признака производной номенклатуры
    pub fn compute_is_derivative(&self) -> bool {
        self.base_nomenclature_ref
            .as_ref()
            .map_or(false, |s| !s.is_empty() && s != ZERO_UUID)
    }

    pub fn new_for_insert(
        code: String,
        description: String,
        full_description: String,
        is_folder: bool,
        parent_id: Option<String>,
        article: String,
        comment: Option<String>,
    ) -> Self {
        let mut base = BaseAggregate::new(NomenclatureId::new_v4(), code, description);
        base.comment = comment;

        Self {
            base,
            full_description,
            is_folder,
            parent_id,
            article,
            mp_ref_count: 0,
            dim1_category: String::new(),
            dim2_line: String::new(),
            dim3_model: String::new(),
            dim4_format: String::new(),
            dim5_sink: String::new(),
            dim6_size: String::new(),
            is_assembly: false,
            base_nomenclature_ref: None,
            alternative_cost_source_ref: None,
            kit_variant_ref: None,
            is_derivative: false,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_id(
        id: NomenclatureId,
        code: String,
        description: String,
        full_description: String,
        is_folder: bool,
        parent_id: Option<String>,
        article: String,
        comment: Option<String>,
    ) -> Self {
        let mut base = BaseAggregate::new(id, code, description);
        base.comment = comment;

        Self {
            base,
            full_description,
            is_folder,
            parent_id,
            article,
            mp_ref_count: 0,
            dim1_category: String::new(),
            dim2_line: String::new(),
            dim3_model: String::new(),
            dim4_format: String::new(),
            dim5_sink: String::new(),
            dim6_size: String::new(),
            is_assembly: false,
            base_nomenclature_ref: None,
            alternative_cost_source_ref: None,
            kit_variant_ref: None,
            is_derivative: false,
        }
    }

    pub fn to_string_id(&self) -> String {
        self.base.id.as_string()
    }

    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    pub fn update(&mut self, dto: &NomenclatureDto) {
        self.base.code = dto.code.clone().unwrap_or_default();
        self.base.description = dto.description.clone();
        self.base.comment = dto.comment.clone();
        self.full_description = dto.full_description.clone().unwrap_or_default();
        self.is_folder = dto.is_folder;
        self.parent_id = dto.parent_id.clone();
        self.article = dto.article.clone().unwrap_or_default();
        // mp_ref_count обновляется только автоматически при сопоставлении

        // Обновление измерений
        self.dim1_category = dto.dim1_category.clone().unwrap_or_default();
        self.dim2_line = dto.dim2_line.clone().unwrap_or_default();
        self.dim3_model = dto.dim3_model.clone().unwrap_or_default();
        self.dim4_format = dto.dim4_format.clone().unwrap_or_default();
        self.dim5_sink = dto.dim5_sink.clone().unwrap_or_default();
        self.dim6_size = dto.dim6_size.clone().unwrap_or_default();

        // Обновление новых полей
        if let Some(is_assembly) = dto.is_assembly {
            self.is_assembly = is_assembly;
        }
        if dto.base_nomenclature_ref.is_some() {
            self.base_nomenclature_ref = dto.base_nomenclature_ref.clone();
        }
        if dto.alternative_cost_source_ref.is_some() {
            self.alternative_cost_source_ref = dto.alternative_cost_source_ref.clone();
        }
        if dto.kit_variant_ref.is_some() {
            self.kit_variant_ref = dto.kit_variant_ref.clone();
        }

        // РРіРЅРѕСЂРёСЂСѓРµРј dto.is_derivative - вычисляем автоматически на основе base_nomenclature_ref
        // Автоматический пересчет признака производной номенклатуры
        self.is_derivative = self.compute_is_derivative();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.base.description.trim().is_empty() {
            return Err("Описание не может быть пустым".into());
        }
        if self.base.code.trim().is_empty() {
            return Err("Код не может быть пустым".into());
        }

        // Валидация длины измерений
        if self.dim1_category.len() > 40 {
            return Err(
                "Категория не должна превышать 40 символов"
                    .into(),
            );
        }
        if self.dim2_line.len() > 40 {
            return Err(
                "Линейка не должна превышать 40 символов".into(),
            );
        }
        if self.dim3_model.len() > 80 {
            return Err(
                "Модель не должна превышать 80 символов".into(),
            );
        }
        if self.dim4_format.len() > 40 {
            return Err(
                "Формат не должен превышать 20 символов".into(),
            );
        }
        if self.dim5_sink.len() > 40 {
            return Err(
                "Р Р°РєРѕРІРёРЅР° не должна превышать 40 символов".into(),
            );
        }
        if self.dim6_size.len() > 20 {
            return Err(
                "Р Р°Р·РјРµСЂ не должен превышать 20 символов".into(),
            );
        }

        Ok(())
    }

    pub fn before_write(&mut self) {
        self.touch_updated();
    }
}

impl AggregateRoot for Nomenclature {
    type Id = NomenclatureId;

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
        "a004"
    }

    fn collection_name() -> &'static str {
        "nomenclature"
    }

    fn element_name() -> &'static str {
        "Номенклатура"
    }

    fn list_name() -> &'static str {
        "Номенклатура"
    }

    fn origin() -> Origin {
        Origin::C1
    }
}

// ============================================================================
// DTO
// ============================================================================
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NomenclatureDto {
    pub id: Option<String>,
    pub code: Option<String>,
    pub description: String,
    #[serde(rename = "fullDescription")]
    pub full_description: Option<String>,
    #[serde(rename = "isFolder", default)]
    pub is_folder: bool,
    #[serde(rename = "parentId")]
    pub parent_id: Option<String>,
    pub article: Option<String>,
    pub comment: Option<String>,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(rename = "mpRefCount", default)]
    pub mp_ref_count: i32,

    // РР·РјРµСЂРµРЅРёСЏ (классификация)
    #[serde(rename = "dim1Category")]
    pub dim1_category: Option<String>,
    #[serde(rename = "dim2Line")]
    pub dim2_line: Option<String>,
    #[serde(rename = "dim3Model")]
    pub dim3_model: Option<String>,
    #[serde(rename = "dim4Format")]
    pub dim4_format: Option<String>,
    #[serde(rename = "dim5Sink")]
    pub dim5_sink: Option<String>,
    #[serde(rename = "dim6Size")]
    pub dim6_size: Option<String>,

    #[serde(rename = "isAssembly")]
    pub is_assembly: Option<bool>,
    #[serde(rename = "baseNomenclatureRef")]
    pub base_nomenclature_ref: Option<String>,
    #[serde(rename = "alternativeCostSourceRef")]
    pub alternative_cost_source_ref: Option<String>,
    #[serde(rename = "kitVariantRef")]
    pub kit_variant_ref: Option<String>,
    #[serde(rename = "isDerivative", default)]
    pub is_derivative: Option<bool>,
}
