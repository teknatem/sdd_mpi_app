//! LLM Connection Details - ViewModel
//!
//! Reactive state management for LLM Connection details form

use leptos::prelude::*;

/// Колонки таблицы моделей, по которым допустима сортировка.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModelSortCol {
    Id,
    Provider,
    Name,
    Context,
    /// Стоимость входящих (prompt) токенов.
    PriceIn,
    /// Стоимость исходящих (completion) токенов.
    PriceOut,
}

/// ViewModel for LLM Connection Details form
#[derive(Clone, Copy)]
pub struct LlmConnectionDetailsVm {
    // Basic fields
    pub code: RwSignal<String>,
    pub description: RwSignal<String>,
    pub comment: RwSignal<String>,

    // Provider configuration
    pub provider_type: RwSignal<String>,
    pub api_endpoint: RwSignal<String>,
    pub api_key: RwSignal<String>,

    // Model configuration
    pub model_name: RwSignal<String>,
    pub temperature: RwSignal<String>,
    pub max_tokens: RwSignal<String>,
    /// Окно контекста модели в токенах: из него считается бюджет истории чата.
    pub context_window: RwSignal<String>,

    /// Прайс за миллион токенов. Пустая строка = ставка не задана: стоимость
    /// прогонов по этому подключению не считается (а не считается нулевой).
    pub price_in_per_mtok: RwSignal<String>,
    pub price_out_per_mtok: RwSignal<String>,
    pub price_cached_per_mtok: RwSignal<String>,
    pub currency: RwSignal<String>,

    /// Курируемый короткий список разрешённых моделей (id). Именно из него
    /// можно выбирать модель в чате. Подмножество available_models.
    pub allowed_models: RwSignal<Vec<String>>,
    pub image_input_models: RwSignal<Vec<String>>,

    /// Мини-фильтр таблицы моделей (подстрока по id/name).
    pub model_filter: RwSignal<String>,

    /// Фильтр по провайдеру (пусто = все провайдеры).
    pub provider_filter: RwSignal<String>,

    /// Сортировка таблицы моделей: (колонка, по возрастанию).
    pub model_sort: RwSignal<(ModelSortCol, bool)>,

    // Flags
    pub is_primary: RwSignal<bool>,

    // State signals
    pub error: Signal<Option<String>>,
    pub set_error: WriteSignal<Option<String>>,

    // Test connection state
    pub test_result: Signal<Option<(bool, String)>>,
    pub set_test_result: WriteSignal<Option<(bool, String)>>,
    pub is_testing: Signal<bool>,
    pub set_is_testing: WriteSignal<bool>,

    // Fetch models state
    pub available_models: Signal<Vec<serde_json::Value>>,
    pub set_available_models: WriteSignal<Vec<serde_json::Value>>,
    pub is_fetching_models: Signal<bool>,
    pub set_is_fetching_models: WriteSignal<bool>,
    pub fetch_models_result: Signal<Option<(bool, String)>>,
    pub set_fetch_models_result: WriteSignal<Option<(bool, String)>>,
}

impl LlmConnectionDetailsVm {
    /// Create new ViewModel with default values
    pub fn new() -> Self {
        let (error, set_error) = signal::<Option<String>>(None);
        let (test_result, set_test_result) = signal::<Option<(bool, String)>>(None);
        let (is_testing, set_is_testing) = signal(false);
        let (available_models, set_available_models) = signal::<Vec<serde_json::Value>>(Vec::new());
        let (is_fetching_models, set_is_fetching_models) = signal(false);
        let (fetch_models_result, set_fetch_models_result) = signal::<Option<(bool, String)>>(None);

        Self {
            code: RwSignal::new(String::new()),
            description: RwSignal::new(String::new()),
            comment: RwSignal::new(String::new()),
            provider_type: RwSignal::new("OpenRouter".to_string()),
            api_endpoint: RwSignal::new("https://openrouter.ai/api/v1".to_string()),
            api_key: RwSignal::new(String::new()),
            model_name: RwSignal::new("openai/gpt-4o".to_string()),
            temperature: RwSignal::new("0.7".to_string()),
            max_tokens: RwSignal::new("4096".to_string()),
            context_window: RwSignal::new("160000".to_string()),
            price_in_per_mtok: RwSignal::new(String::new()),
            price_out_per_mtok: RwSignal::new(String::new()),
            price_cached_per_mtok: RwSignal::new(String::new()),
            currency: RwSignal::new(String::new()),
            allowed_models: RwSignal::new(Vec::new()),
            image_input_models: RwSignal::new(Vec::new()),
            model_filter: RwSignal::new(String::new()),
            provider_filter: RwSignal::new(String::new()),
            model_sort: RwSignal::new((ModelSortCol::Id, true)),
            is_primary: RwSignal::new(false),
            error: error.into(),
            set_error,
            test_result: test_result.into(),
            set_test_result,
            is_testing: is_testing.into(),
            set_is_testing,
            available_models: available_models.into(),
            set_available_models,
            is_fetching_models: is_fetching_models.into(),
            set_is_fetching_models,
            fetch_models_result: fetch_models_result.into(),
            set_fetch_models_result,
        }
    }

    /// Отсортированный и отфильтрованный список id моделей для рендера таблицы.
    /// Каталог (`available_models`) объединяется с уже отмеченными моделями, которых
    /// в каталоге нет (чтобы не потерять их), затем фильтруется и сортируется.
    pub fn visible_model_rows(&self) -> Vec<serde_json::Value> {
        use std::collections::HashSet;

        let mut rows: Vec<serde_json::Value> = self.available_models.get();
        let present: HashSet<String> = rows
            .iter()
            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect();
        // Добавить отмеченные модели, отсутствующие в каталоге, как минимальные строки.
        for a in self.allowed_models.get() {
            if !present.contains(&a) {
                rows.push(serde_json::json!({ "id": a }));
            }
        }

        let filter = self.model_filter.get().to_lowercase();
        let filter = filter.trim();
        if !filter.is_empty() {
            rows.retain(|m| {
                let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("");
                id.to_lowercase().contains(filter) || name.to_lowercase().contains(filter)
            });
        }

        let provider = self.provider_filter.get();
        if !provider.is_empty() {
            rows.retain(|m| model_provider(m) == provider);
        }

        let (col, asc) = self.model_sort.get();
        rows.sort_by(|a, b| {
            let ord = match col {
                ModelSortCol::Id => str_field(a, "id").cmp(&str_field(b, "id")),
                ModelSortCol::Provider => model_provider(a)
                    .to_lowercase()
                    .cmp(&model_provider(b).to_lowercase()),
                ModelSortCol::Name => str_field(a, "name").cmp(&str_field(b, "name")),
                ModelSortCol::Context => num_field(a, "context_length")
                    .partial_cmp(&num_field(b, "context_length"))
                    .unwrap_or(std::cmp::Ordering::Equal),
                ModelSortCol::PriceIn => price_field(a, "prompt")
                    .partial_cmp(&price_field(b, "prompt"))
                    .unwrap_or(std::cmp::Ordering::Equal),
                ModelSortCol::PriceOut => price_field(a, "completion")
                    .partial_cmp(&price_field(b, "completion"))
                    .unwrap_or(std::cmp::Ordering::Equal),
            };
            if asc {
                ord
            } else {
                ord.reverse()
            }
        });
        rows
    }

    /// Отсортированный список уникальных провайдеров из каталога (+ отмеченных моделей).
    pub fn available_providers(&self) -> Vec<String> {
        use std::collections::BTreeSet;
        let mut set: BTreeSet<String> = BTreeSet::new();
        for m in self.available_models.get() {
            let p = model_provider(&m);
            if !p.is_empty() {
                set.insert(p);
            }
        }
        for a in self.allowed_models.get() {
            let p = model_provider(&serde_json::json!({ "id": a }));
            if !p.is_empty() {
                set.insert(p);
            }
        }
        set.into_iter().collect()
    }

    /// Переключить сортировку по колонке: тот же столбец — инвертирует порядок,
    /// иначе — выбирает столбец по возрастанию.
    pub fn toggle_sort(&self, col: ModelSortCol) {
        self.model_sort.update(|(c, asc)| {
            if *c == col {
                *asc = !*asc;
            } else {
                *c = col;
                *asc = true;
            }
        });
    }

    /// Get temperature as f64
    pub fn get_temperature(&self) -> f64 {
        self.temperature.get().parse().unwrap_or(0.7)
    }

    /// Get max_tokens as i32
    pub fn get_max_tokens(&self) -> i32 {
        self.max_tokens.get().parse().unwrap_or(4096)
    }

    /// Окно контекста как i32. Дефолт совпадает с дефолтом колонки в БД, чтобы
    /// пустое/испорченное поле не меняло бюджет компакции облачных подключений.
    pub fn get_context_window(&self) -> i32 {
        self.context_window.get().parse().unwrap_or(160_000)
    }

    /// Ставка прайса как `Option<f64>`. Пустое или нечисловое поле — это `None`
    /// («не задано»), а не 0.0: нулевая ставка означала бы бесплатную модель и
    /// молча обнулила бы стоимость всех прогонов подключения.
    fn parse_price(raw: String) -> Option<f64> {
        let trimmed = raw.trim().replace(',', ".");
        if trimmed.is_empty() {
            return None;
        }
        trimmed.parse::<f64>().ok().filter(|v| *v >= 0.0)
    }

    pub fn get_price_in(&self) -> Option<f64> {
        Self::parse_price(self.price_in_per_mtok.get())
    }

    pub fn get_price_out(&self) -> Option<f64> {
        Self::parse_price(self.price_out_per_mtok.get())
    }

    pub fn get_price_cached(&self) -> Option<f64> {
        Self::parse_price(self.price_cached_per_mtok.get())
    }

    /// Переключить принадлежность модели к allowed_models.
    /// Если снимаем галочку с модели, которая сейчас основная — сбрасываем основную
    /// (пустой model_name), чтобы `validate_models` поймал это перед записью.
    pub fn toggle_allowed(&self, model_id: &str) {
        let mut removed = false;
        self.allowed_models.update(|list| {
            if let Some(pos) = list.iter().position(|m| m == model_id) {
                list.remove(pos);
                removed = true;
            } else {
                list.push(model_id.to_string());
            }
        });
        if removed && self.model_name.get_untracked() == model_id {
            self.model_name.set(String::new());
        }
        if removed {
            self.image_input_models
                .update(|list| list.retain(|item| item != model_id));
        }
    }

    pub fn toggle_image_input(&self, model_id: &str) {
        if !self
            .allowed_models
            .get_untracked()
            .iter()
            .any(|item| item == model_id)
        {
            return;
        }
        self.image_input_models.update(|list| {
            if let Some(pos) = list.iter().position(|item| item == model_id) {
                list.remove(pos);
            } else {
                list.push(model_id.to_string());
            }
        });
    }

    /// Отметить модель основной (по умолчанию). Гарантирует, что она входит в
    /// allowed_models (авто-добавление). Основная всегда ровно одна.
    pub fn set_primary(&self, model_id: &str) {
        self.model_name.set(model_id.to_string());
        self.allowed_models.update(|list| {
            if !list.iter().any(|m| m == model_id) {
                list.push(model_id.to_string());
            }
        });
    }

    /// Проверка выбора моделей перед записью. Возвращает текст ошибки при нарушении.
    pub fn validate_models(&self) -> Result<(), String> {
        let allowed = self.allowed_models.get_untracked();
        if allowed.is_empty() {
            return Err("Отметьте хотя бы одну разрешённую модель в таблице.".into());
        }
        let primary = self.model_name.get_untracked();
        if primary.trim().is_empty() {
            return Err("Отметьте основную модель звёздочкой в таблице.".into());
        }
        if !allowed.iter().any(|m| m == &primary) {
            return Err("Основная модель должна входить в список разрешённых.".into());
        }
        Ok(())
    }

    /// Build save DTO from current values
    pub fn build_save_dto(&self, id: Option<String>) -> serde_json::Value {
        let allowed = self.allowed_models.get();
        let allowed_json: Option<String> = if allowed.is_empty() {
            None
        } else {
            serde_json::to_string(&allowed).ok()
        };
        let image_models = self.image_input_models.get();
        let image_models_json = if image_models.is_empty() {
            None
        } else {
            serde_json::to_string(&image_models).ok()
        };
        serde_json::json!({
            "id": id,
            "code": self.code.get(),
            "description": self.description.get(),
            "comment": if self.comment.get().is_empty() { None } else { Some(self.comment.get()) },
            "provider_type": self.provider_type.get(),
            "api_endpoint": self.api_endpoint.get(),
            "api_key": self.api_key.get(),
            "model_name": self.model_name.get(),
            "temperature": self.get_temperature(),
            "max_tokens": self.get_max_tokens(),
            "context_window": self.get_context_window(),
            "is_primary": self.is_primary.get(),
            "allowed_models": allowed_json,
            "image_input_models": image_models_json,
            "price_in_per_mtok": self.get_price_in(),
            "price_out_per_mtok": self.get_price_out(),
            "price_cached_per_mtok": self.get_price_cached(),
            "currency": if self.currency.get().trim().is_empty() { None } else { Some(self.currency.get().trim().to_uppercase()) },
        })
    }
}

impl Default for LlmConnectionDetailsVm {
    fn default() -> Self {
        Self::new()
    }
}

/// Провайдер модели: `owned_by` (OpenAI) либо префикс id до «/» (OpenRouter,
/// напр. `openai/gpt-4o` → `openai`). Пусто, если определить нельзя.
pub fn model_provider(m: &serde_json::Value) -> String {
    if let Some(owner) = m.get("owned_by").and_then(|v| v.as_str()) {
        if !owner.trim().is_empty() {
            return owner.to_string();
        }
    }
    m.get("id")
        .and_then(|v| v.as_str())
        .and_then(|id| id.split_once('/'))
        .map(|(prefix, _)| prefix.to_string())
        .unwrap_or_default()
}

fn str_field(m: &serde_json::Value, key: &str) -> String {
    m.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase()
}

fn num_field(m: &serde_json::Value, key: &str) -> f64 {
    m.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

/// Цена токенов (для сортировки) из `pricing.<key>` (`prompt`/`completion`).
/// Строкой или числом; некорректные/отсутствующие данные считаются 0.0, чтобы
/// сохранить полный порядок при сортировке.
fn price_field(m: &serde_json::Value, key: &str) -> f64 {
    m.get("pricing")
        .and_then(|p| p.get(key))
        .map(|v| match v {
            serde_json::Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
            serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
            _ => 0.0,
        })
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}
