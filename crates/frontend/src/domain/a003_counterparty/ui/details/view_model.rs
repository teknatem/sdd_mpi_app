use super::api;
use contracts::domain::a003_counterparty::aggregate::CounterpartyDto;
use contracts::domain::common::AggregateId;
use leptos::prelude::*;
use std::rc::Rc;

/// ViewModel for Counterparty details form
#[derive(Clone)]
pub struct CounterpartyDetailsViewModel {
    pub form: RwSignal<CounterpartyDto>,
    pub error: RwSignal<Option<String>>,
}

impl CounterpartyDetailsViewModel {
    pub fn new() -> Self {
        Self {
            form: RwSignal::new(CounterpartyDto::default()),
            error: RwSignal::new(None),
        }
    }

    pub fn is_edit_mode(&self) -> impl Fn() -> bool + '_ {
        move || self.form.get().id.is_some()
    }

    pub fn is_form_valid(&self) -> impl Fn() -> bool + '_ {
        move || Self::validate_form(&self.form.get()).is_ok()
    }

    fn validate_form(dto: &CounterpartyDto) -> Result<(), &'static str> {
        if dto.description.trim().is_empty() {
            return Err("Наименование обязательно для заполнения");
        }
        Ok(())
    }

    /// Load form data from server if ID is provided
    pub fn load_if_needed(&self, id: Option<String>) {
        let Some(existing_id) = id else {
            return;
        };

        let this = self.clone();
        leptos::task::spawn_local(async move {
            match super::api::fetch_by_id(existing_id).await {
                Ok(item) => {
                    this.form.update(|f| {
                        f.id = Some(item.base.id.as_string());
                        f.code = Some(item.base.code);
                        f.description = item.base.description;
                        f.comment = item.base.comment;
                        f.is_folder = item.is_folder;
                        f.parent_id = item.parent_id;
                        f.inn = Some(item.inn);
                        f.kpp = Some(item.kpp);
                        f.updated_at = Some(item.base.metadata.updated_at);
                    });
                }
                Err(e) => this.error.set(Some(e)),
            }
        });
    }

    pub fn save_command(&self, on_saved: Rc<dyn Fn(())>) -> impl Fn() + '_ {
        move || {
            let this = self.clone();
            let dto = this.form.get();
            let on_saved_cb = on_saved.clone();
            leptos::task::spawn_local(async move {
                match api::save_form(dto).await {
                    Ok(_) => on_saved_cb(()),
                    Err(e) => this.error.set(Some(e)),
                }
            });
        }
    }
}
