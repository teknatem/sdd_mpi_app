use super::api;
use contracts::domain::a005_marketplace::aggregate::MarketplaceDto;
use contracts::domain::common::AggregateId;
use leptos::prelude::*;
use std::rc::Rc;

/// ViewModel for Marketplace details form
#[derive(Clone)]
pub struct MarketplaceDetailsViewModel {
    pub form: RwSignal<MarketplaceDto>,
    pub error: RwSignal<Option<String>>,
}

impl MarketplaceDetailsViewModel {
    pub fn new() -> Self {
        Self {
            form: RwSignal::new(MarketplaceDto::default()),
            error: RwSignal::new(None),
        }
    }

    pub fn is_edit_mode(&self) -> impl Fn() -> bool + '_ {
        move || self.form.get().id.is_some()
    }

    pub fn is_form_valid(&self) -> impl Fn() -> bool + '_ {
        move || Self::validate_form(&self.form.get()).is_ok()
    }

    fn validate_form(dto: &MarketplaceDto) -> Result<(), &'static str> {
        if dto.description.trim().is_empty() {
            return Err("Наименование обязательно для заполнения");
        }
        if dto.url.trim().is_empty() {
            return Err("URL обязателен для заполнения");
        }
        if !dto.url.starts_with("http://") && !dto.url.starts_with("https://") {
            return Err("URL должен начинаться с http:// или https://");
        }
        Ok(())
    }

    /// Load form data from server if ID is provided
    pub fn load_if_needed(&self, id: Option<String>) {
        let Some(existing_id) = id else {
            return;
        };
        let form = self.form;
        let error = self.error;
        wasm_bindgen_futures::spawn_local(async move {
            let result = api::fetch_by_id(existing_id).await;
            if let Err(e) = result {
                error.set(Some(format!("Ошибка загрузки: {}", e)));
                return;
            }

            let aggregate = result.unwrap();
            let dto = MarketplaceDto {
                id: Some(aggregate.base.id.as_string()),
                code: Some(aggregate.base.code),
                description: aggregate.base.description,
                url: aggregate.url,
                logo_path: aggregate.logo_path,
                marketplace_type: aggregate.marketplace_type,
                comment: aggregate.base.comment,
                acquiring_fee_pro: aggregate.acquiring_fee_pro,
            };
            form.set(dto);
        });
    }

    /// Save form data to server
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
}
