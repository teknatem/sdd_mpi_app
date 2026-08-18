use crate::shared::api_utils::api_base;
use contracts::domain::a006_connection_mp::{
    AuthorizationType, ConnectionMP, ConnectionMPDto, ConnectionTestResult,
};

/// Загрузить подключение по ID
pub async fn fetch_by_id(id: String) -> Result<ConnectionMP, String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let url = format!("{}/api/connection_mp/{}", api_base(), id);
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if resp.status() == 404 {
        return Err("Not found".to_string());
    }
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
    let data: ConnectionMP = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;
    Ok(data)
}

/// Сохранить подключение (создать или обновить)
pub async fn save_form(dto: &ConnectionMPDto) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let json_data = serde_json::to_string(&dto).map_err(|e| format!("{e}"))?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    let body = wasm_bindgen::JsValue::from_str(&json_data);
    opts.set_body(&body);

    let url = format!("{}/api/connection_mp", api_base());
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

/// Получить информацию о продавце (WB seller-info)
pub async fn fetch_seller_info(dto: &ConnectionMPDto) -> Result<ConnectionTestResult, String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let json_data = serde_json::to_string(&dto).map_err(|e| format!("{e}"))?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    let body = wasm_bindgen::JsValue::from_str(&json_data);
    opts.set_body(&body);

    let url = format!("{}/api/connection_mp/seller_info", api_base());
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
    let result: ConnectionTestResult = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;
    Ok(result)
}

/// Тестировать подключение
pub async fn test_connection(dto: &ConnectionMPDto) -> Result<ConnectionTestResult, String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let json_data = serde_json::to_string(&dto).map_err(|e| format!("{e}"))?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    let body = wasm_bindgen::JsValue::from_str(&json_data);
    opts.set_body(&body);

    let url = format!("{}/api/connection_mp/test", api_base());
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
    let result: ConnectionTestResult = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;
    Ok(result)
}

/// Информация о маркетплейсе
#[derive(Clone, Debug)]
pub struct MarketplaceInfo {
    pub code: String,
    pub name: String,
}

/// Загрузить информацию о маркетплейсе
pub async fn fetch_marketplace_info(id: &str) -> Result<MarketplaceInfo, String> {
    use contracts::domain::a005_marketplace::aggregate::Marketplace;
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let url = format!("{}/api/marketplace/{}", api_base(), id);
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
    let marketplace: Marketplace = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;

    Ok(MarketplaceInfo {
        code: marketplace.base.code,
        name: marketplace.base.description,
    })
}

/// DTO для работы с формой (используется в ViewModel)
#[derive(Clone, Debug)]
pub struct ConnectionMPFormDto {
    pub id: Option<String>,
    pub code: Option<String>,
    pub description: String,
    pub comment: Option<String>,
    pub marketplace_id: String,
    pub organization_ref: String,
    pub api_key: String,
    pub supplier_id: Option<String>,
    pub application_id: Option<String>,
    pub is_used: bool,
    pub business_account_id: Option<String>,
    pub api_key_stats: Option<String>,
    pub test_mode: bool,
    pub planned_commission_percent: Option<f64>,
    pub planned_acquiring_percent: Option<f64>,
    pub authorization_type: AuthorizationType,
}

impl Default for ConnectionMPFormDto {
    fn default() -> Self {
        Self {
            id: None,
            code: None,
            description: String::new(),
            comment: None,
            marketplace_id: String::new(),
            organization_ref: String::new(),
            api_key: String::new(),
            supplier_id: None,
            application_id: None,
            is_used: false,
            business_account_id: None,
            api_key_stats: None,
            test_mode: false,
            planned_commission_percent: None,
            planned_acquiring_percent: None,
            authorization_type: AuthorizationType::default(),
        }
    }
}

impl From<ConnectionMP> for ConnectionMPFormDto {
    fn from(conn: ConnectionMP) -> Self {
        use contracts::domain::common::AggregateId;
        Self {
            id: Some(conn.base.id.as_string()),
            code: Some(conn.base.code),
            description: conn.base.description,
            comment: conn.base.comment,
            marketplace_id: conn.marketplace_id,
            organization_ref: conn.organization_ref,
            api_key: conn.api_key,
            supplier_id: conn.supplier_id,
            application_id: conn.application_id,
            is_used: conn.is_used,
            business_account_id: conn.business_account_id,
            api_key_stats: conn.api_key_stats,
            test_mode: conn.test_mode,
            planned_commission_percent: conn.planned_commission_percent,
            planned_acquiring_percent: conn.planned_acquiring_percent,
            authorization_type: conn.authorization_type,
        }
    }
}

impl From<ConnectionMPFormDto> for ConnectionMPDto {
    fn from(form: ConnectionMPFormDto) -> Self {
        Self {
            id: form.id,
            code: form.code,
            description: form.description,
            comment: form.comment,
            marketplace_id: form.marketplace_id,
            organization_ref: form.organization_ref,
            api_key: form.api_key,
            supplier_id: form.supplier_id,
            application_id: form.application_id,
            is_used: form.is_used,
            business_account_id: form.business_account_id,
            api_key_stats: form.api_key_stats,
            test_mode: form.test_mode,
            planned_commission_percent: form.planned_commission_percent,
            planned_acquiring_percent: form.planned_acquiring_percent,
            authorization_type: form.authorization_type,
        }
    }
}

/// Загрузить наименование организации по UUID
pub async fn fetch_organization_name(id: &str) -> Result<String, String> {
    use contracts::domain::a002_organization::aggregate::Organization;
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let normalized_id = id.trim().trim_matches('"').to_string();
    if normalized_id.is_empty() {
        return Ok(String::new());
    }

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let url = format!("{}/api/organization/{}", api_base(), normalized_id);
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
    let org: Organization = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;
    Ok(org.base.description)
}
