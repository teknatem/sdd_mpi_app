use super::api;
use contracts::domain::a004_nomenclature::aggregate::Nomenclature;
use contracts::domain::a005_marketplace::aggregate::Marketplace;
use contracts::domain::a007_marketplace_product::aggregate::MarketplaceProductDto;
use contracts::domain::common::AggregateId;
use contracts::enums::marketplace_type::MarketplaceType;
use leptos::prelude::*;
use std::rc::Rc;

#[derive(Clone)]
pub struct MarketplaceProductDetailsViewModel {
    pub form: RwSignal<MarketplaceProductDto>,
    pub error: RwSignal<Option<String>>,
    pub success_message: RwSignal<Option<String>>,
    pub marketplace_name: RwSignal<String>,
    pub marketplace_type: RwSignal<Option<MarketplaceType>>,
    pub connection_name: RwSignal<String>,
    pub nomenclature_name: RwSignal<String>,
    pub nomenclature_code: RwSignal<String>,
    pub nomenclature_article: RwSignal<String>,
    pub show_picker: RwSignal<bool>,
    pub search_results: RwSignal<Option<Vec<Nomenclature>>>,
}

impl MarketplaceProductDetailsViewModel {
    pub fn new() -> Self {
        Self {
            form: RwSignal::new(MarketplaceProductDto::default()),
            error: RwSignal::new(None),
            success_message: RwSignal::new(None),
            marketplace_name: RwSignal::new(String::new()),
            marketplace_type: RwSignal::new(None),
            connection_name: RwSignal::new(String::new()),
            nomenclature_name: RwSignal::new(String::new()),
            nomenclature_code: RwSignal::new(String::new()),
            nomenclature_article: RwSignal::new(String::new()),
            show_picker: RwSignal::new(false),
            search_results: RwSignal::new(None),
        }
    }

    pub fn is_edit_mode(&self) -> impl Fn() -> bool + '_ {
        move || self.form.get().id.is_some()
    }

    pub fn is_form_valid(&self) -> impl Fn() -> bool + '_ {
        move || Self::validate_form(&self.form.get()).is_ok()
    }

    fn validate_form(dto: &MarketplaceProductDto) -> Result<(), &'static str> {
        if dto.description.trim().is_empty() {
            return Err("Описание обязательно для заполнения");
        }
        if dto.marketplace_ref.trim().is_empty() {
            return Err("Маркетплейс обязателен для заполнения");
        }
        if dto.marketplace_sku.trim().is_empty() {
            return Err("SKU обязателен для заполнения");
        }
        if dto.article.trim().is_empty() {
            return Err("Артикул обязателен для заполнения");
        }
        Ok(())
    }

    pub fn load_if_needed(&self, id: Option<String>) {
        let Some(existing_id) = id else {
            return;
        };
        let form = self.form;
        let error = self.error;
        let marketplace_name = self.marketplace_name;
        let marketplace_type = self.marketplace_type;
        let connection_name = self.connection_name;
        let nomenclature_name = self.nomenclature_name;
        let nomenclature_code = self.nomenclature_code;
        let nomenclature_article = self.nomenclature_article;

        wasm_bindgen_futures::spawn_local(async move {
            let result = api::fetch_by_id(existing_id).await;
            if let Err(e) = result {
                error.set(Some(format!("Ошибка загрузки: {}", e)));
                return;
            }

            let aggregate = result.unwrap();
            let dto = MarketplaceProductDto {
                id: Some(aggregate.base.id.as_string()),
                code: Some(aggregate.base.code),
                description: aggregate.base.description.clone(),
                marketplace_ref: aggregate.marketplace_ref.clone(),
                connection_mp_ref: aggregate.connection_mp_ref.clone(),
                marketplace_sku: aggregate.marketplace_sku,
                barcode: aggregate.barcode,
                article: aggregate.article,
                brand: aggregate.brand,
                category_id: aggregate.category_id,
                category_name: aggregate.category_name,
                last_update: aggregate.last_update,
                nomenclature_ref: aggregate.nomenclature_ref.clone(),
                comment: aggregate.base.comment,
            };
            form.set(dto);

            if let Ok(mp) = api::fetch_marketplace(&aggregate.marketplace_ref).await {
                let resolved_marketplace_type = Self::resolve_marketplace_type(&mp);
                marketplace_name.set(mp.base.description);
                marketplace_type.set(resolved_marketplace_type);
            }
            if let Ok(conn) = api::fetch_connection_mp(&aggregate.connection_mp_ref).await {
                connection_name.set(conn.base.description);
            }
            if let Some(ref nom_id) = aggregate.nomenclature_ref {
                if let Ok(nom) = api::fetch_nomenclature(nom_id).await {
                    nomenclature_name.set(nom.base.description);
                    nomenclature_code.set(nom.base.code);
                    nomenclature_article.set(nom.article);
                }
            }
        });
    }

    pub fn save_command(&self, on_saved: Rc<dyn Fn(())>) {
        let current = self.form.get();

        if let Err(msg) = Self::validate_form(&current) {
            self.error.set(Some(msg.to_string()));
            return;
        }

        let on_saved_cb = on_saved.clone();
        let error = self.error;
        wasm_bindgen_futures::spawn_local(async move {
            match api::save_form(&current).await {
                Ok(()) => (on_saved_cb)(()),
                Err(e) => error.set(Some(e)),
            }
        });
    }

    pub fn search_nomenclature_by_article(&self) {
        let article = self.form.get().article.trim().to_string();
        if article.is_empty() {
            self.error.set(Some(
                "Для автоподбора заполните артикул товара маркетплейса".to_string(),
            ));
            return;
        }

        let error = self.error;
        let success = self.success_message;
        let form = self.form;
        let nomenclature_name = self.nomenclature_name;
        let nomenclature_code = self.nomenclature_code;
        let nomenclature_article = self.nomenclature_article;
        let show_picker = self.show_picker;
        let search_results = self.search_results;

        wasm_bindgen_futures::spawn_local(async move {
            match api::search_nomenclature_by_article(&article).await {
                Ok(results) => match results.len() {
                    0 => {
                        error.set(Some(format!(
                            "Автоподбор не нашел позицию 1С для артикула: {}",
                            article
                        )));
                        success.set(None);
                    }
                    1 => {
                        let nom = &results[0];
                        form.update(|f| f.nomenclature_ref = Some(nom.base.id.as_string()));
                        nomenclature_name.set(nom.base.description.clone());
                        nomenclature_code.set(nom.base.code.clone());
                        nomenclature_article.set(nom.article.clone());
                        success.set(Some(format!(
                            "Связь с 1С УТ обновлена автоматически: {}",
                            nom.base.description
                        )));
                        error.set(None);
                    }
                    n => {
                        error.set(None);
                        success.set(Some(format!(
                            "Автоподбор нашел {} вариантов. Выберите нужную позицию вручную.",
                            n
                        )));
                        search_results.set(Some(results));
                        show_picker.set(true);
                    }
                },
                Err(e) => {
                    error.set(Some(format!("Ошибка поиска: {}", e)));
                    success.set(None);
                }
            }
        });
    }

    pub fn search_nomenclature_by_barcode(&self) {
        let barcode = self
            .form
            .get()
            .barcode
            .map(|b| b.trim().to_string())
            .unwrap_or_default();
        if barcode.is_empty() {
            self.error.set(Some(
                "Для автоподбора заполните штрихкод товара маркетплейса".to_string(),
            ));
            return;
        }

        let error = self.error;
        let success = self.success_message;
        let form = self.form;
        let nomenclature_name = self.nomenclature_name;
        let nomenclature_code = self.nomenclature_code;
        let nomenclature_article = self.nomenclature_article;
        let show_picker = self.show_picker;
        let search_results = self.search_results;

        wasm_bindgen_futures::spawn_local(async move {
            match api::search_nomenclature_by_barcode(&barcode).await {
                Ok(results) => match results.len() {
                    0 => {
                        error.set(Some(format!(
                            "Автоподбор не нашел позицию 1С по штрихкоду: {}",
                            barcode
                        )));
                        success.set(None);
                    }
                    1 => {
                        let nom = &results[0];
                        form.update(|f| f.nomenclature_ref = Some(nom.base.id.as_string()));
                        nomenclature_name.set(nom.base.description.clone());
                        nomenclature_code.set(nom.base.code.clone());
                        nomenclature_article.set(nom.article.clone());
                        success.set(Some(format!(
                            "Связь с 1С УТ обновлена по штрихкоду: {}",
                            nom.base.description
                        )));
                        error.set(None);
                    }
                    n => {
                        error.set(None);
                        success.set(Some(format!(
                            "По штрихкоду найдено {} вариантов. Выберите нужную позицию вручную.",
                            n
                        )));
                        search_results.set(Some(results));
                        show_picker.set(true);
                    }
                },
                Err(e) => {
                    error.set(Some(format!("Ошибка поиска по штрихкоду: {}", e)));
                    success.set(None);
                }
            }
        });
    }

    pub fn clear_nomenclature(&self) {
        self.form.update(|f| f.nomenclature_ref = None);
        self.nomenclature_name.set(String::new());
        self.nomenclature_code.set(String::new());
        self.nomenclature_article.set(String::new());
        self.success_message
            .set(Some("Связь с 1С УТ очищена".to_string()));
    }

    pub fn open_picker(&self) {
        self.search_results.set(None);
        self.show_picker.set(true);
    }

    /// Является ли товар товаром Wildberries (для блока «Быстрый доступ»).
    pub fn is_wildberries(&self) -> bool {
        matches!(
            self.marketplace_type.get(),
            Some(MarketplaceType::Wildberries)
        )
    }

    pub fn marketplace_product_url(&self) -> Option<String> {
        let sku = self.form.get().marketplace_sku.trim().to_string();
        if sku.is_empty() {
            return None;
        }

        match self.marketplace_type.get() {
            Some(MarketplaceType::Wildberries) => Some(format!(
                "https://www.wildberries.ru/catalog/{sku}/detail.aspx?targetUrl=MI"
            )),
            _ => None,
        }
    }

    fn resolve_marketplace_type(marketplace: &Marketplace) -> Option<MarketplaceType> {
        if let Some(marketplace_type) = marketplace.marketplace_type {
            return Some(marketplace_type);
        }

        let code = marketplace.base.code.to_lowercase();
        let description = marketplace.base.description.to_lowercase();
        let url = marketplace.url.to_lowercase();

        if code.contains("wb")
            || description.contains("wildberries")
            || url.contains("wildberries.ru")
        {
            Some(MarketplaceType::Wildberries)
        } else if code.contains("ozon") || description.contains("ozon") || url.contains("ozon.ru") {
            Some(MarketplaceType::Ozon)
        } else if code.contains("ym")
            || description.contains("яндекс")
            || description.contains("yandex")
            || url.contains("market.yandex")
        {
            Some(MarketplaceType::YandexMarket)
        } else if code.contains("kuper")
            || description.contains("купер")
            || description.contains("kuper")
        {
            Some(MarketplaceType::Kuper)
        } else if code.contains("lemana")
            || description.contains("лемана")
            || description.contains("lemana")
        {
            Some(MarketplaceType::LemanaPro)
        } else {
            None
        }
    }
}
