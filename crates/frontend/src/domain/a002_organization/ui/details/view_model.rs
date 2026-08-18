use super::api;
use contracts::domain::a002_organization::aggregate::OrganizationDto;
use contracts::domain::common::AggregateId;
use leptos::prelude::*;
use std::rc::Rc;

/// ViewModel for Organization details form
#[derive(Clone)]
pub struct OrganizationDetailsViewModel {
    pub form: RwSignal<OrganizationDto>,
    pub error: RwSignal<Option<String>>,
}

impl OrganizationDetailsViewModel {
    pub fn new() -> Self {
        Self {
            form: RwSignal::new(OrganizationDto::default()),
            error: RwSignal::new(None),
        }
    }

    pub fn is_edit_mode(&self) -> impl Fn() -> bool + '_ {
        move || self.form.get().id.is_some()
    }

    pub fn is_form_valid(&self) -> impl Fn() -> bool + '_ {
        move || Self::validate_form(&self.form.get()).is_ok()
    }

    fn validate_form(dto: &OrganizationDto) -> Result<(), &'static str> {
        if dto.description.trim().is_empty() {
            return Err("Наименование обязательно для заполнения");
        }
        if dto.full_name.trim().is_empty() {
            return Err("Полное наименование обязательно для заполнения");
        }
        if dto.inn.trim().is_empty() {
            return Err("ИНН обязателен для заполнения");
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
            let dto = OrganizationDto {
                id: Some(aggregate.base.id.as_string()),
                code: Some(aggregate.base.code),
                description: aggregate.base.description,
                full_name: aggregate.full_name,
                inn: aggregate.inn,
                kpp: aggregate.kpp,
                comment: aggregate.base.comment,
                entity_ref: aggregate.entity_ref,
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
