use super::api::{self, ConnectionMPFormDto};
use contracts::domain::a006_connection_mp::ConnectionTestResult;
use leptos::prelude::*;
use std::rc::Rc;

/// ViewModel для формы подключения к маркетплейсу
#[derive(Clone)]
pub struct ConnectionMPDetailsVm {
    pub form: RwSignal<ConnectionMPFormDto>,
    pub error: RwSignal<Option<String>>,
    pub test_result: RwSignal<Option<ConnectionTestResult>>,
    pub is_testing: RwSignal<bool>,
    pub seller_info_result: RwSignal<Option<ConnectionTestResult>>,
    pub is_fetching_info: RwSignal<bool>,
    pub marketplace_name: RwSignal<String>,
    pub marketplace_code: RwSignal<String>,
    pub organization_name: RwSignal<String>,
}

impl ConnectionMPDetailsVm {
    pub fn new(id: Option<String>) -> Self {
        let vm = Self {
            form: RwSignal::new(ConnectionMPFormDto::default()),
            error: RwSignal::new(None),
            test_result: RwSignal::new(None),
            is_testing: RwSignal::new(false),
            seller_info_result: RwSignal::new(None),
            is_fetching_info: RwSignal::new(false),
            marketplace_name: RwSignal::new(String::new()),
            marketplace_code: RwSignal::new(String::new()),
            organization_name: RwSignal::new(String::new()),
        };

        if let Some(id) = id {
            vm.load(id);
        }

        vm
    }

    /// Режим редактирования (есть ID)
    pub fn is_edit_mode(&self) -> impl Fn() -> bool + '_ {
        move || self.form.get().id.is_some()
    }

    /// Валидация формы
    pub fn is_form_valid(&self) -> impl Fn() -> bool + '_ {
        move || Self::validate_form(&self.form.get()).is_ok()
    }

    fn validate_form(dto: &ConnectionMPFormDto) -> Result<(), &'static str> {
        if dto.description.trim().is_empty() {
            return Err("Наименование обязательно для заполнения");
        }
        if dto.marketplace_id.is_empty() {
            return Err("Маркетплейс должен быть выбран");
        }
        if dto.organization_ref.is_empty() {
            return Err("Организация должна быть выбрана");
        }
        if dto.api_key.trim().is_empty() {
            return Err("API Key обязателен для заполнения");
        }
        // Валидация процента комиссии
        if let Some(percent) = dto.planned_commission_percent {
            if percent < 0.0 || percent > 100.0 {
                return Err("Процент комиссии должен быть от 0 до 100");
            }
        }
        // Валидация процента эквайринга
        if let Some(percent) = dto.planned_acquiring_percent {
            if percent < 0.0 || percent > 100.0 {
                return Err("Процент эквайринга должен быть от 0 до 100");
            }
        }
        Ok(())
    }

    /// Загрузить данные с сервера
    pub fn load(&self, id: String) {
        let form = self.form;
        let error = self.error;
        let marketplace_name = self.marketplace_name;
        let marketplace_code = self.marketplace_code;
        let organization_name = self.organization_name;

        wasm_bindgen_futures::spawn_local(async move {
            match api::fetch_by_id(id).await {
                Ok(conn) => {
                    let marketplace_id = conn.marketplace_id.clone();
                    let organization_ref = conn.organization_ref.clone();

                    form.set(ConnectionMPFormDto::from(conn));

                    // Загрузить информацию о маркетплейсе
                    if let Ok(mp_info) = api::fetch_marketplace_info(&marketplace_id).await {
                        marketplace_name.set(mp_info.name);
                        marketplace_code.set(mp_info.code);
                    }

                    // Загрузить название организации по UUID
                    match api::fetch_organization_name(&organization_ref).await {
                        Ok(name) if !name.is_empty() => organization_name.set(name),
                        _ => organization_name.set(organization_ref),
                    }
                }
                Err(e) => {
                    error.set(Some(format!("Ошибка загрузки: {}", e)));
                }
            }
        });
    }

    /// Сохранить данные на сервер
    pub fn save_command(&self, on_saved: Rc<dyn Fn(())>) {
        let current = self.form.get();

        if let Err(msg) = Self::validate_form(&current) {
            self.error.set(Some(msg.to_string()));
            return;
        }

        let dto = current.into();
        let on_saved_cb = on_saved.clone();
        let error = self.error;

        wasm_bindgen_futures::spawn_local(async move {
            match api::save_form(&dto).await {
                Ok(()) => (on_saved_cb)(()),
                Err(e) => error.set(Some(e)),
            }
        });
    }

    /// Тестировать подключение
    pub fn test_command(&self) {
        let current = self.form.get();

        // Базовая валидация перед тестом
        if current.marketplace_id.is_empty() || current.api_key.trim().is_empty() {
            self.error.set(Some(
                "Для теста необходимо указать маркетплейс и API Key".to_string(),
            ));
            return;
        }

        self.is_testing.set(true);
        self.test_result.set(None);
        self.error.set(None);

        let dto = current.into();
        let test_result = self.test_result;
        let is_testing = self.is_testing;
        let error = self.error;

        wasm_bindgen_futures::spawn_local(async move {
            match api::test_connection(&dto).await {
                Ok(result) => {
                    test_result.set(Some(result));
                    is_testing.set(false);
                }
                Err(e) => {
                    error.set(Some(format!("Ошибка теста: {}", e)));
                    is_testing.set(false);
                }
            }
        });
    }

    /// Получить информацию о продавце (WB seller-info)
    pub fn seller_info_command(&self) {
        let current = self.form.get();

        if current.marketplace_id.is_empty() || current.api_key.trim().is_empty() {
            self.error.set(Some(
                "Для получения информации необходимо указать маркетплейс и API Key".to_string(),
            ));
            return;
        }

        self.is_fetching_info.set(true);
        self.seller_info_result.set(None);
        self.error.set(None);

        let dto = current.into();
        let seller_info_result = self.seller_info_result;
        let is_fetching_info = self.is_fetching_info;
        let error = self.error;

        wasm_bindgen_futures::spawn_local(async move {
            match api::fetch_seller_info(&dto).await {
                Ok(result) => {
                    seller_info_result.set(Some(result));
                    is_fetching_info.set(false);
                }
                Err(e) => {
                    error.set(Some(format!("Ошибка запроса: {}", e)));
                    is_fetching_info.set(false);
                }
            }
        });
    }

    /// Обновить информацию о маркетплейсе
    pub fn update_marketplace_info(&self, mp_id: String, mp_name: String, mp_code: String) {
        self.form.update(|f| {
            f.marketplace_id = mp_id;
        });
        self.marketplace_name.set(mp_name);
        self.marketplace_code.set(mp_code);
    }

    /// Обновить информацию об организации
    pub fn update_organization_info(&self, org_id: String, org_name: String) {
        self.form.update(|f| {
            f.organization_ref = org_id;
        });
        self.organization_name.set(org_name);
    }
}
